//! The server, exercised from the outside as a device would: over HTTP
//! on the loopback, which is the one place the clear is allowed, and
//! over the live channel.

use std::net::SocketAddr;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::net::TcpStream;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;
use zyr_broker::live::{FromDevice, FromServer};
use zyr_broker::proof::{Purpose, challenge_message};
use zyr_broker::rest::{
    Access, Challenge, ContactInfo, ContactRequest, ContactStatus, DeviceInfo, Link, LinkAnswer,
    Login, LoginAnswer, Permission, Register, Registration, Rename, ServerInfo, ShareInfo,
    ShareRequest, paths,
};
use zyr_broker::ticket::Grant;
use zyr_broker::{Code, PROTOCOL, Verifier, now};
use zyr_server::config::Config;
use zyr_transport::Identity;

/// Past this, an answer that should have come has not.
const PATIENCE: Duration = Duration::from_secs(5);

struct Server {
    running: zyr_server::Running,
    agent: ureq::Agent,
    folder: std::path::PathBuf,
}

impl Server {
    async fn start(policy: &str) -> Self {
        let folder = std::env::temp_dir().join(format!(
            "zyrdesk-server-test-{}",
            zyr_proto::random::alphanumeric_string(8)
        ));
        let config = Config::parse(&format!(
            r#"
name = "Essai"
data_dir = '{}'

[api]
listen = "127.0.0.1:0"
public_url = "https://essai.invalid"

[relay]
listen = "127.0.0.1:0"

[registration]
policy = "{policy}"

[limits]
login_attempts_per_minute = 1000
"#,
            folder.display()
        ))
        .unwrap();
        let running = zyr_server::start(config).await.unwrap();
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .build()
            .new_agent();
        Self {
            running,
            agent,
            folder,
        }
    }

    fn address(&self) -> SocketAddr {
        self.running.address
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.address())
    }

    fn get<T: DeserializeOwned>(&self, path: &str, token: Option<&str>) -> (u16, T) {
        let mut request = self.agent.get(self.url(path));
        if let Some(token) = token {
            request = request.header("Authorization", format!("Bearer {token}"));
        }
        let mut answer = request.call().unwrap();
        (
            answer.status().as_u16(),
            answer.body_mut().read_json().unwrap(),
        )
    }

    fn post<T: DeserializeOwned>(
        &self,
        path: &str,
        token: Option<&str>,
        body: &impl Serialize,
    ) -> (u16, T) {
        let mut request = self.agent.post(self.url(path));
        if let Some(token) = token {
            request = request.header("Authorization", format!("Bearer {token}"));
        }
        let mut answer = request.send_json(body).unwrap();
        (
            answer.status().as_u16(),
            answer.body_mut().read_json().unwrap(),
        )
    }

    fn post_empty(&self, path: &str, token: &str) -> u16 {
        self.agent
            .post(self.url(path))
            .header("Authorization", format!("Bearer {token}"))
            .send_empty()
            .unwrap()
            .status()
            .as_u16()
    }

    fn delete(&self, path: &str, token: &str) -> u16 {
        self.agent
            .delete(self.url(path))
            .header("Authorization", format!("Bearer {token}"))
            .call()
            .unwrap()
            .status()
            .as_u16()
    }

    fn info(&self) -> ServerInfo {
        self.get::<ServerInfo>(paths::SERVER, None).1
    }

    fn register(&self, username: &str) -> LoginAnswer {
        let (status, answer) = self.post::<LoginAnswer>(
            paths::ACCOUNTS,
            None,
            &Register {
                username: username.into(),
                password: "douze caractères".into(),
                email: None,
                invitation: None,
            },
        );
        assert_eq!(status, 200);
        answer
    }

    /// Attaches a brand new device to the account of that token.
    fn link(&self, account_token: &str, name: &str) -> (Identity, LinkAnswer) {
        let identity = Identity::generate().unwrap();
        let info = self.info();
        let (status, challenge) =
            self.post::<Challenge>(paths::CHALLENGE, None, &serde_json::json!({}));
        assert_eq!(status, 200);
        let signature = identity
            .sign(&challenge_message(
                &info.signing_key,
                &challenge.nonce,
                Purpose::Link,
            ))
            .unwrap();
        let (status, answer) = self.post::<LinkAnswer>(
            paths::DEVICES,
            Some(account_token),
            &Link {
                certificate: BASE64.encode(identity.certificate().as_ref()),
                nonce: challenge.nonce,
                signature: BASE64.encode(signature),
                name: name.into(),
            },
        );
        assert_eq!(status, 200);
        assert_eq!(answer.device.fingerprint, identity.fingerprint());
        (identity, answer)
    }

    async fn stop(self) {
        self.running.stop().await;
        let _ = std::fs::remove_dir_all(&self.folder);
    }
}

type Channel = WebSocketStream<TcpStream>;

/// The next thing the server says, pings left aside.
async fn next(channel: &mut Channel) -> FromServer {
    loop {
        let frame = tokio::time::timeout(PATIENCE, channel.next())
            .await
            .expect("le serveur devait dire quelque chose")
            .expect("le canal est fermé")
            .unwrap();
        match frame {
            Message::Text(text) => return serde_json::from_str(text.as_str()).unwrap(),
            Message::Close(_) => panic!("le canal s'est fermé"),
            _ => {}
        }
    }
}

async fn say(channel: &mut Channel, said: &FromDevice) {
    channel
        .send(Message::Text(serde_json::to_string(said).unwrap().into()))
        .await
        .unwrap();
}

/// Opens the live channel and gets through the challenge.
async fn open_channel(
    server: &Server,
    token: &str,
    identity: &Identity,
    signing_key: &zyr_broker::ServerPublicKey,
) -> (Channel, FromServer) {
    let mut channel = connect(server, token).await;
    let FromServer::Challenge { nonce } = next(&mut channel).await else {
        panic!("le serveur devait commencer par un défi");
    };
    let signature = identity
        .sign(&challenge_message(signing_key, &nonce, Purpose::Live))
        .unwrap();
    say(
        &mut channel,
        &FromDevice::Hello {
            protocol: PROTOCOL,
            build: "essai".into(),
            signature: BASE64.encode(signature),
        },
    )
    .await;
    let welcome = next(&mut channel).await;
    assert!(matches!(welcome, FromServer::Welcome { .. }), "{welcome:?}");
    (channel, welcome)
}

async fn connect(server: &Server, token: &str) -> Channel {
    let stream = TcpStream::connect(server.address()).await.unwrap();
    let request = tokio_tungstenite::tungstenite::http::Request::builder()
        .uri(format!("ws://{}{}", server.address(), paths::LIVE))
        .header("Authorization", format!("Bearer {token}"))
        .header("Host", server.address().to_string())
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header(
            "Sec-WebSocket-Key",
            tokio_tungstenite::tungstenite::handshake::client::generate_key(),
        )
        .body(())
        .unwrap();
    let (channel, _) = tokio_tungstenite::client_async(request, stream)
        .await
        .unwrap();
    channel
}

#[tokio::test(flavor = "multi_thread")]
async fn accounts_devices_contacts_and_shares_from_the_outside() {
    let server = Server::start("open").await;
    let info = server.info();
    assert_eq!(info.name, "Essai");
    assert_eq!(info.registration, Registration::Open);
    assert_eq!(info.protocol, PROTOCOL);

    // Un compte, un mot de passe faux, un vrai.
    let victor = server.register("victor");
    let (status, error) = server.post::<zyr_broker::rest::Error>(
        paths::LOGIN,
        None,
        &Login {
            username: "victor".into(),
            password: "pas le bon".into(),
        },
    );
    assert_eq!((status, error.error), (401, Code::InvalidCredentials));
    let (status, again) = server.post::<LoginAnswer>(
        paths::LOGIN,
        None,
        &Login {
            username: "victor".into(),
            password: "douze caractères".into(),
        },
    );
    assert_eq!(status, 200);
    assert_eq!(again.username, "victor");

    // Deux appareils, dont l'un rattaché avec le jeton de compte fraîchement
    // obtenu ; un jeton d'appareil fait ensuite tous les gestes du compte.
    let (_, pc1) = server.link(&victor.token, "PC de Victor");
    let (_, pc2) = server.link(&again.token, "Portable");
    let (status, devices) = server.get::<Vec<DeviceInfo>>(paths::DEVICES, Some(&pc1.token));
    assert_eq!(status, 200);
    assert_eq!(devices.len(), 2);
    assert!(devices.iter().all(|device| !device.online));

    // Un jeton d'appareil ne rattache pas un autre appareil.
    let (status, error) = server.post::<zyr_broker::rest::Error>(
        paths::DEVICES,
        Some(&pc1.token),
        &serde_json::json!({}),
    );
    assert_eq!((status, error.error), (401, Code::Unauthorized));

    // Renommer, révoquer.
    let (status, renamed) = server
        .agent
        .patch(server.url(&format!("/v1/devices/{}", pc2.device.id)))
        .header("Authorization", format!("Bearer {}", pc1.token))
        .send_json(&Rename {
            name: "Portable de Victor".into(),
        })
        .map(|mut answer| {
            (
                answer.status().as_u16(),
                answer.body_mut().read_json::<DeviceInfo>().unwrap(),
            )
        })
        .unwrap();
    assert_eq!(status, 200);
    assert_eq!(renamed.name, "Portable de Victor");
    assert_eq!(
        server.delete(&format!("/v1/devices/{}", pc2.device.id), &pc1.token),
        200
    );
    let (_, devices) = server.get::<Vec<DeviceInfo>>(paths::DEVICES, Some(&pc1.token));
    assert_eq!(devices.len(), 1);
    let (status, error) = server.get::<zyr_broker::rest::Error>(paths::DEVICES, Some(&pc2.token));
    assert_eq!((status, error.error), (401, Code::Unauthorized));

    // Un contact, demandé et accepté, puis un partage.
    let ami = server.register("ami");
    let (_, portable) = server.link(&ami.token, "PC de l'ami");
    let (status, asked) = server.post::<ContactInfo>(
        paths::CONTACTS,
        Some(&pc1.token),
        &ContactRequest {
            username: "ami".into(),
        },
    );
    assert_eq!(status, 200);
    assert_eq!(asked.status, ContactStatus::Pending);
    assert!(asked.asked_by_me);
    let (_, seen_by_ami) = server.get::<Vec<ContactInfo>>(paths::CONTACTS, Some(&portable.token));
    assert_eq!(seen_by_ami[0].username, "victor");
    assert!(!seen_by_ami[0].asked_by_me);
    // Pas de partage avant l'accord.
    let (status, error) = server.post::<zyr_broker::rest::Error>(
        paths::SHARES,
        Some(&pc1.token),
        &ShareRequest {
            device: pc1.device.id.clone(),
            with: "ami".into(),
            permissions: Permission::ALL.to_vec(),
            expires: None,
        },
    );
    assert_eq!((status, error.error), (400, Code::NotAContact));
    assert_eq!(
        server.post_empty(
            &format!("/v1/contacts/{}/accept", asked.id),
            &portable.token
        ),
        200
    );
    let (status, share) = server.post::<ShareInfo>(
        paths::SHARES,
        Some(&pc1.token),
        &ShareRequest {
            device: pc1.device.id.clone(),
            with: "ami".into(),
            permissions: Permission::ALL.to_vec(),
            expires: None,
        },
    );
    assert_eq!(status, 200);
    assert_eq!(share.with, "ami");
    assert_eq!(share.device.id, pc1.device.id);
    let (_, received) = server.get::<Vec<ShareInfo>>(paths::SHARES, Some(&portable.token));
    assert_eq!(received.len(), 1);
    assert_eq!(
        server.delete(&format!("/v1/shares/{}", share.id), &portable.token),
        204
    );
    let (_, received) = server.get::<Vec<ShareInfo>>(paths::SHARES, Some(&portable.token));
    assert!(received.is_empty());

    server.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_live_channel_carries_presence_and_a_rendezvous() {
    let server = Server::start("open").await;
    let info = server.info();
    let victor = server.register("victor");
    let (pc_identity, pc) = server.link(&victor.token, "PC de Victor");
    let (other_identity, other) = server.link(&victor.token, "Portable");
    let ami = server.register("ami");
    let (friend_identity, friend) = server.link(&ami.token, "PC de l'ami");

    // Une preuve fausse ferme le canal avec la raison.
    let mut liar = connect(&server, &pc.token).await;
    let FromServer::Challenge { .. } = next(&mut liar).await else {
        panic!("un défi d'abord");
    };
    say(
        &mut liar,
        &FromDevice::Hello {
            protocol: PROTOCOL,
            build: "essai".into(),
            signature: BASE64.encode(b"pas une signature"),
        },
    )
    .await;
    assert!(matches!(
        next(&mut liar).await,
        FromServer::Bye {
            code: Code::ProofInvalid
        }
    ));

    // Le PC arrive : il se voit, et voit le portable hors ligne.
    let (mut pc_channel, welcome) =
        open_channel(&server, &pc.token, &pc_identity, &info.signing_key).await;
    let FromServer::Welcome { me, devices, .. } = welcome else {
        unreachable!()
    };
    assert_eq!(me, pc.device.id);
    let portable = devices.iter().find(|d| d.id == other.device.id).unwrap();
    assert!(!portable.online);

    // Le portable arrive : le PC l'apprend.
    let (mut other_channel, _) =
        open_channel(&server, &other.token, &other_identity, &info.signing_key).await;
    assert_eq!(
        next(&mut pc_channel).await,
        FromServer::Presence {
            device: other.device.id.clone(),
            online: true,
            access: Access::Off,
        }
    );
    // Le PC accepte l'accès distant : le portable l'apprend.
    say(
        &mut pc_channel,
        &FromDevice::State {
            access: Access::Ready,
        },
    )
    .await;
    assert_eq!(
        next(&mut other_channel).await,
        FromServer::Presence {
            device: pc.device.id.clone(),
            online: true,
            access: Access::Ready,
        }
    );

    // Le rendez-vous : le portable va vers le PC, les deux reçoivent le
    // ticket, chacun le lit de son côté.
    say(
        &mut other_channel,
        &FromDevice::SessionOpen {
            to: pc.device.id.clone(),
        },
    )
    .await;
    let FromServer::SessionStart {
        session,
        ticket,
        peer,
        relay,
    } = next(&mut other_channel).await
    else {
        panic!("le portable devait recevoir le début de session");
    };
    assert_eq!(peer.device, pc.device.id);
    assert_eq!(peer.fingerprint, pc_identity.fingerprint());
    assert_eq!(peer.account, "victor");
    assert!(relay.is_none());
    let FromServer::SessionStart {
        session: same,
        ticket: same_ticket,
        peer: opener,
        ..
    } = next(&mut pc_channel).await
    else {
        panic!("le PC devait recevoir le début de session");
    };
    assert_eq!(same, session);
    assert_eq!(same_ticket, ticket);
    assert_eq!(opener.device, other.device.id);
    let read_by_host = Verifier::new(info.signing_key)
        .ticket_for_host(&ticket, pc_identity.fingerprint(), now())
        .unwrap();
    assert_eq!(read_by_host.grant, Grant::Owner);
    assert_eq!(read_by_host.session, session);
    Verifier::new(info.signing_key)
        .ticket_for_client(&ticket, other_identity.fingerprint(), now())
        .unwrap();

    // Les candidats passent d'un bout à l'autre, et la fin aussi.
    let candidates = vec!["192.168.1.4:47000".parse().unwrap()];
    say(
        &mut other_channel,
        &FromDevice::SessionCandidates {
            session: session.clone(),
            candidates: candidates.clone(),
        },
    )
    .await;
    assert_eq!(
        next(&mut pc_channel).await,
        FromServer::SessionCandidates {
            session: session.clone(),
            candidates,
        }
    );
    say(
        &mut other_channel,
        &FromDevice::SessionEnd {
            session: session.clone(),
        },
    )
    .await;
    assert_eq!(
        next(&mut pc_channel).await,
        FromServer::SessionEnd { session }
    );

    // L'ami n'a aucun droit sur le PC tant que rien n'est partagé.
    let (mut friend_channel, _) =
        open_channel(&server, &friend.token, &friend_identity, &info.signing_key).await;
    say(
        &mut friend_channel,
        &FromDevice::SessionOpen {
            to: pc.device.id.clone(),
        },
    )
    .await;
    assert_eq!(
        next(&mut friend_channel).await,
        FromServer::SessionRefused {
            to: pc.device.id.clone(),
            code: Code::NoRight,
        }
    );

    // Révoqué depuis le PC, le portable est congédié avec la raison, et le
    // PC l'apprend.
    assert_eq!(
        server.delete(&format!("/v1/devices/{}", other.device.id), &pc.token),
        200
    );
    assert_eq!(
        next(&mut other_channel).await,
        FromServer::Bye {
            code: Code::DeviceRevoked
        }
    );
    assert_eq!(
        next(&mut pc_channel).await,
        FromServer::DeviceRevoked {
            device: other.device.id.clone()
        }
    );

    server.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn registration_by_invitation_takes_a_code_once() {
    let server = Server::start("invitation").await;
    assert_eq!(server.info().registration, Registration::Invitation);
    let (status, error) = server.post::<zyr_broker::rest::Error>(
        paths::ACCOUNTS,
        None,
        &Register {
            username: "victor".into(),
            password: "douze caractères".into(),
            email: None,
            invitation: None,
        },
    );
    assert_eq!((status, error.error), (400, Code::InvitationInvalid));
    let code = server.running.app.store.new_invitation(now()).unwrap();
    let invited = Register {
        username: "victor".into(),
        password: "douze caractères".into(),
        email: Some("victor@exemple.fr".into()),
        invitation: Some(code),
    };
    let (status, _) = server.post::<LoginAnswer>(paths::ACCOUNTS, None, &invited);
    assert_eq!(status, 200);
    let (status, error) = server.post::<zyr_broker::rest::Error>(
        paths::ACCOUNTS,
        None,
        &Register {
            username: "autre".into(),
            ..invited
        },
    );
    assert_eq!((status, error.error), (400, Code::InvitationInvalid));
    server.stop().await;
}
