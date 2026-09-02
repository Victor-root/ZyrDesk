//! Asking the server, and reading what it answers.
//!
//! HTTPS, one request at a time, with the trust of `trust` and nothing
//! else. A refusal comes back as the code the server gave; a connection
//! that could not be trusted comes back as the reason the verifier
//! recorded, which is what the window needs to ask the person.

use std::fmt;
use std::sync::Arc;

use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::{Method, Request, StatusCode};
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use serde::Serialize;
use serde::de::DeserializeOwned;
use zyr_broker::Code;
use zyr_broker::rest::{
    Challenge, ContactInfo, ContactRequest, DeviceInfo, Link, LinkAnswer, Login, LoginAnswer,
    Register, Rename, ServerInfo, ShareInfo, ShareRequest, paths,
};

use crate::address::{BadAddress, normalized};
use crate::trust::{Trust, Untrusted, Verifier, client_config};

/// Why a request did not get its answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    Address(BadAddress),
    /// The server could not be believed.
    Untrusted(Untrusted),
    /// The server said no, with its code.
    Refused {
        code: Code,
        message: String,
    },
    /// The server could not be reached, or the connection broke.
    Transport(String),
    /// An answer that is not what the server was expected to say.
    Unreadable(String),
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Failure::Address(e) => write!(f, "{e}"),
            Failure::Untrusted(e) => write!(f, "{e}"),
            Failure::Refused { code, .. } => write!(f, "{}", code.explanation()),
            Failure::Transport(e) => write!(f, "le serveur n'a pas pu être joint : {e}"),
            Failure::Unreadable(e) => write!(f, "réponse du serveur illisible : {e}"),
        }
    }
}

impl std::error::Error for Failure {}

impl From<BadAddress> for Failure {
    fn from(e: BadAddress) -> Self {
        Failure::Address(e)
    }
}

/// The server's door, with the trust it is spoken to with.
pub struct Rest {
    client: Client<HttpsConnector<HttpConnector>, Full<Bytes>>,
    verifier: Arc<Verifier>,
    /// `https://host:port`.
    server: String,
}

impl Rest {
    pub fn new(server: &str, trust: Trust) -> Result<Self, Failure> {
        let server = normalized(server)?;
        let (config, verifier) = client_config(trust);
        let connector = hyper_rustls::HttpsConnectorBuilder::new()
            .with_tls_config(Arc::unwrap_or_clone(config))
            .https_only()
            .enable_http1()
            .build();
        Ok(Self {
            client: Client::builder(TokioExecutor::new()).build(connector),
            verifier,
            server,
        })
    }

    /// `https://host:port`, as the link keeps it.
    pub fn server(&self) -> &str {
        &self.server
    }

    async fn call<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        token: Option<&str>,
        body: Option<Vec<u8>>,
    ) -> Result<T, Failure> {
        let mut request = Request::builder()
            .method(method)
            .uri(format!("{}{path}", self.server))
            .header("Accept", "application/json");
        if let Some(token) = token {
            request = request.header("Authorization", format!("Bearer {token}"));
        }
        if body.is_some() {
            request = request.header("Content-Type", "application/json");
        }
        let request = request
            .body(Full::new(Bytes::from(body.unwrap_or_default())))
            .map_err(|e| Failure::Transport(e.to_string()))?;
        let response = match self.client.request(request).await {
            Ok(response) => response,
            Err(e) => {
                return Err(match self.verifier.why_refused() {
                    Some(why) => Failure::Untrusted(why),
                    None => Failure::Transport(e.to_string()),
                });
            }
        };
        let status = response.status();
        let bytes = response
            .into_body()
            .collect()
            .await
            .map_err(|e| Failure::Transport(e.to_string()))?
            .to_bytes();
        if status.is_success() {
            if status == StatusCode::NO_CONTENT || bytes.is_empty() {
                return serde_json::from_slice(b"null")
                    .map_err(|_| Failure::Unreadable("réponse vide".to_string()));
            }
            return serde_json::from_slice(&bytes).map_err(|e| Failure::Unreadable(e.to_string()));
        }
        match serde_json::from_slice::<zyr_broker::rest::Error>(&bytes) {
            Ok(error) => Err(Failure::Refused {
                code: error.error,
                message: error.message,
            }),
            Err(_) => Err(Failure::Unreadable(format!(
                "{status} sans code : {}",
                String::from_utf8_lossy(&bytes[..bytes.len().min(200)])
            ))),
        }
    }

    async fn get<T: DeserializeOwned>(&self, path: &str, token: &str) -> Result<T, Failure> {
        self.call(Method::GET, path, Some(token), None).await
    }

    async fn post<T: DeserializeOwned>(
        &self,
        path: &str,
        token: Option<&str>,
        body: &impl Serialize,
    ) -> Result<T, Failure> {
        let body = serde_json::to_vec(body).map_err(|e| Failure::Unreadable(e.to_string()))?;
        self.call(Method::POST, path, token, Some(body)).await
    }

    pub async fn server_info(&self) -> Result<ServerInfo, Failure> {
        self.call(Method::GET, paths::SERVER, None, None).await
    }

    pub async fn register(&self, register: &Register) -> Result<LoginAnswer, Failure> {
        self.post(paths::ACCOUNTS, None, register).await
    }

    pub async fn login(&self, login: &Login) -> Result<LoginAnswer, Failure> {
        self.post(paths::LOGIN, None, login).await
    }

    pub async fn challenge(&self) -> Result<Challenge, Failure> {
        self.call(Method::POST, paths::CHALLENGE, None, None).await
    }

    pub async fn link(&self, account_token: &str, link: &Link) -> Result<LinkAnswer, Failure> {
        self.post(paths::DEVICES, Some(account_token), link).await
    }

    pub async fn devices(&self, token: &str) -> Result<Vec<DeviceInfo>, Failure> {
        self.get(paths::DEVICES, token).await
    }

    pub async fn rename_device(
        &self,
        token: &str,
        device: &str,
        name: &str,
    ) -> Result<DeviceInfo, Failure> {
        let body = serde_json::to_vec(&Rename {
            name: name.to_string(),
        })
        .map_err(|e| Failure::Unreadable(e.to_string()))?;
        self.call(
            Method::PATCH,
            &format!("{}/{device}", paths::DEVICES),
            Some(token),
            Some(body),
        )
        .await
    }

    pub async fn revoke_device(&self, token: &str, device: &str) -> Result<DeviceInfo, Failure> {
        self.call(
            Method::DELETE,
            &format!("{}/{device}", paths::DEVICES),
            Some(token),
            None,
        )
        .await
    }

    pub async fn contacts(&self, token: &str) -> Result<Vec<ContactInfo>, Failure> {
        self.get(paths::CONTACTS, token).await
    }

    pub async fn ask_contact(&self, token: &str, username: &str) -> Result<ContactInfo, Failure> {
        self.post(
            paths::CONTACTS,
            Some(token),
            &ContactRequest {
                username: username.to_string(),
            },
        )
        .await
    }

    pub async fn answer_contact(
        &self,
        token: &str,
        contact: &str,
        accept: bool,
    ) -> Result<ContactInfo, Failure> {
        let word = if accept { "accept" } else { "decline" };
        self.call(
            Method::POST,
            &format!("{}/{contact}/{word}", paths::CONTACTS),
            Some(token),
            None,
        )
        .await
    }

    pub async fn remove_contact(&self, token: &str, contact: &str) -> Result<(), Failure> {
        self.call(
            Method::DELETE,
            &format!("{}/{contact}", paths::CONTACTS),
            Some(token),
            None,
        )
        .await
    }

    pub async fn shares(&self, token: &str) -> Result<Vec<ShareInfo>, Failure> {
        self.get(paths::SHARES, token).await
    }

    pub async fn give_share(
        &self,
        token: &str,
        share: &ShareRequest,
    ) -> Result<ShareInfo, Failure> {
        self.post(paths::SHARES, Some(token), share).await
    }

    pub async fn remove_share(&self, token: &str, share: &str) -> Result<(), Failure> {
        self.call(
            Method::DELETE,
            &format!("{}/{share}", paths::SHARES),
            Some(token),
            None,
        )
        .await
    }
}
