//! Attaching this device to an account.
//!
//! In order: the server is asked what it is, over a connection the
//! device believes or refuses; the account is created or entered; the
//! device proves its key on a challenge; and the link is made of what
//! came back. A refusal at the first step for want of trust carries the
//! key presented, so the window can show it and ask.

use std::fmt;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use zyr_broker::proof::{Purpose, challenge_message};
use zyr_broker::rest::{Login, Register};
use zyr_broker::{Code, PROTOCOL};
use zyr_transport::Identity;
use zyr_transport::trust::{Trust, Untrusted};

use crate::link::Link;
use crate::rest::{Failure, Rest};

/// Creating the account on the way, rather than entering one.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Registering {
    pub email: Option<String>,
    pub invitation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credentials {
    pub username: String,
    pub password: String,
    /// Present when the account is to be created first.
    pub register: Option<Registering>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachError {
    /// The server could not be believed; for an unpinned key, the person
    /// may confirm it and try again with it pinned.
    Untrusted(Untrusted),
    /// The server speaks another version of the dialect.
    Version {
        server: u32,
    },
    Refused {
        code: Code,
        message: String,
    },
    Failed(Failure),
    /// This device's key could not sign.
    Signing(String),
}

impl fmt::Display for AttachError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AttachError::Untrusted(e) => write!(f, "{e}"),
            AttachError::Version { server } => write!(
                f,
                "ce serveur parle la version {server} du dialecte, cet appareil la version \
                 {PROTOCOL} : l'un des deux est à mettre à jour"
            ),
            AttachError::Refused { code, .. } => write!(f, "{}", code.explanation()),
            AttachError::Failed(e) => write!(f, "{e}"),
            AttachError::Signing(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for AttachError {}

impl From<Failure> for AttachError {
    fn from(failure: Failure) -> Self {
        match failure {
            Failure::Untrusted(why) => AttachError::Untrusted(why),
            Failure::Refused { code, message } => AttachError::Refused { code, message },
            other => AttachError::Failed(other),
        }
    }
}

/// Attaches this device, and hands back the link to keep.
pub async fn attach(
    server: &str,
    trust: Trust,
    identity: &Identity,
    credentials: &Credentials,
    device_name: &str,
) -> Result<Link, AttachError> {
    let rest = Rest::new(server, trust)?;
    let info = rest.server_info().await?;
    if info.protocol != PROTOCOL {
        return Err(AttachError::Version {
            server: info.protocol,
        });
    }
    let entered = match &credentials.register {
        Some(registering) => {
            rest.register(&Register {
                username: credentials.username.clone(),
                password: credentials.password.clone(),
                email: registering.email.clone(),
                invitation: registering.invitation.clone(),
            })
            .await?
        }
        None => {
            rest.login(&Login {
                username: credentials.username.clone(),
                password: credentials.password.clone(),
            })
            .await?
        }
    };
    let challenge = rest.challenge().await?;
    let signature = identity
        .sign(&challenge_message(
            &info.signing_key,
            &challenge.nonce,
            Purpose::Link,
        ))
        .map_err(|e| AttachError::Signing(e.to_string()))?;
    let answer = rest
        .link(
            &entered.token,
            &zyr_broker::rest::Link {
                certificate: BASE64.encode(identity.certificate().as_ref()),
                nonce: challenge.nonce,
                signature: BASE64.encode(signature),
                name: device_name.to_string(),
            },
        )
        .await?;
    Ok(Link {
        server: rest.server().to_string(),
        name: info.name,
        username: entered.username,
        device: answer.device.id,
        token: answer.token,
        pin: match trust {
            Trust::Pinned(pin) => Some(pin),
            Trust::PublicOnly => None,
        },
        signing_key: info.signing_key,
    })
}
