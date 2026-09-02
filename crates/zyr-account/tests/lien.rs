//! The link, exercised against a real server on this machine, over TLS
//! with a certificate nobody vouches for: the road every self-hosted
//! server takes.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use zyr_account::{AttachError, Credentials, Event, Live, Registering, Rest, Trust, Untrusted};
use zyr_broker::live::FromDevice;
use zyr_broker::rest::{Access, Registration};
use zyr_broker::ticket::Grant;
use zyr_broker::{Code, Verifier, now};
use zyr_server::config::Config;
use zyr_transport::identity::public_key_fingerprint;
use zyr_transport::{Fingerprint, Identity};

/// Past this, something that should have happened has not.
const PATIENCE: Duration = Duration::from_secs(5);

struct Server {
    running: zyr_server::Running,
    folder: std::path::PathBuf,
    /// The fingerprint of the key of its certificate: what a person would
    /// read on the installation's summary.
    fingerprint: Fingerprint,
}

impl Server {
    async fn start() -> Self {
        let folder = std::env::temp_dir().join(format!(
            "zyrdesk-account-test-{}",
            zyr_proto::random::alphanumeric_string(8)
        ));
        std::fs::create_dir_all(&folder).unwrap();
        let generated = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
        let certificate = folder.join("server.crt");
        let key = folder.join("server.key");
        std::fs::write(&certificate, generated.cert.pem()).unwrap();
        std::fs::write(&key, generated.signing_key.serialize_pem()).unwrap();
        let fingerprint = public_key_fingerprint(generated.cert.der()).unwrap();
        let config = Config::parse(&format!(
            r#"
name = "Essai"
data_dir = "{}"

[api]
listen = "127.0.0.1:0"
tls_cert = "{}"
tls_key = "{}"
public_url = "https://essai.invalid"

[registration]
policy = "open"

[limits]
login_attempts_per_minute = 1000
"#,
            folder.display(),
            certificate.display(),
            key.display()
        ))
        .unwrap();
        let running = zyr_server::start(config).await.unwrap();
        Self {
            running,
            folder,
            fingerprint,
        }
    }

    fn address(&self) -> String {
        self.running.address.to_string()
    }

    async fn stop(self) {
        self.running.stop().await;
        let _ = std::fs::remove_dir_all(&self.folder);
    }
}

fn log() -> Arc<dyn Fn(&str) + Send + Sync> {
    Arc::new(|line: &str| println!("{line}"))
}

fn credentials(username: &str, register: bool) -> Credentials {
    Credentials {
        username: username.to_string(),
        password: "douze caractères".to_string(),
        register: register.then(Registering::default),
    }
}

async fn expect(events: &mut mpsc::UnboundedReceiver<Event>) -> Event {
    tokio::time::timeout(PATIENCE, events.recv())
        .await
        .expect("le canal devait dire quelque chose")
        .expect("le canal est fermé")
}

async fn until_connected(live: &Live) -> zyr_account::Snapshot {
    let deadline = tokio::time::Instant::now() + PATIENCE;
    loop {
        let snapshot = live.snapshot();
        if snapshot.connected {
            return snapshot;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "jamais connecté : {:?}",
            snapshot.trouble
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_self_signed_server_is_refused_until_its_key_is_pinned() {
    let server = Server::start().await;
    let identity = Identity::generate().unwrap();

    // Sans épinglage, refusé, avec l'empreinte à comparer.
    let refused = zyr_account::attach(
        &server.address(),
        Trust::PublicOnly,
        &identity,
        &credentials("victor", true),
        "PC",
    )
    .await
    .unwrap_err();
    assert_eq!(
        refused,
        AttachError::Untrusted(Untrusted::Unpinned {
            presented: server.fingerprint
        })
    );

    // Une autre clé épinglée, refusé aussi, en le disant.
    let other = Identity::generate().unwrap().fingerprint();
    let refused = zyr_account::attach(
        &server.address(),
        Trust::Pinned(other),
        &identity,
        &credentials("victor", true),
        "PC",
    )
    .await
    .unwrap_err();
    assert_eq!(
        refused,
        AttachError::Untrusted(Untrusted::Changed {
            pinned: other,
            presented: server.fingerprint
        })
    );

    // La bonne, et le lien est fait.
    let link = zyr_account::attach(
        &server.address(),
        Trust::Pinned(server.fingerprint),
        &identity,
        &credentials("victor", true),
        "PC de Victor",
    )
    .await
    .unwrap();
    assert_eq!(link.server, format!("https://{}", server.address()));
    assert_eq!(link.name, "Essai");
    assert_eq!(link.username, "victor");
    assert_eq!(link.pin, Some(server.fingerprint));

    // Le clair est refusé avant d'être essayé.
    assert!(matches!(
        zyr_account::attach(
            &format!("http://{}", server.address()),
            Trust::Pinned(server.fingerprint),
            &identity,
            &credentials("victor", false),
            "PC",
        )
        .await
        .unwrap_err(),
        AttachError::Failed(zyr_account::Failure::Address(_))
    ));

    // Un mauvais mot de passe est le code du serveur, en français.
    let refused = zyr_account::attach(
        &server.address(),
        Trust::Pinned(server.fingerprint),
        &identity,
        &Credentials {
            password: "pas le bon mot de passe".into(),
            ..credentials("victor", false)
        },
        "PC",
    )
    .await
    .unwrap_err();
    assert!(matches!(
        refused,
        AttachError::Refused {
            code: Code::InvalidCredentials,
            ..
        }
    ));
    assert_eq!(refused.to_string(), Code::InvalidCredentials.explanation());

    server.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_live_channel_serves_the_account_and_a_rendezvous() {
    let server = Server::start().await;
    let trust = Trust::Pinned(server.fingerprint);
    let pc_identity = Arc::new(Identity::generate().unwrap());
    let pc_link = zyr_account::attach(
        &server.address(),
        trust,
        &pc_identity,
        &credentials("victor", true),
        "PC de Victor",
    )
    .await
    .unwrap();

    let (pc, mut pc_events) = Live::open(pc_link.clone(), pc_identity.clone(), log());
    let snapshot = until_connected(&pc).await;
    assert_eq!(snapshot.me.as_deref(), Some(pc_link.device.as_str()));
    assert_eq!(snapshot.devices.len(), 1);
    assert_eq!(
        snapshot.server.as_ref().map(|s| s.registration),
        Some(Registration::Open)
    );

    // Un second appareil se rattache : le premier l'apprend sans rien
    // demander.
    let portable_identity = Arc::new(Identity::generate().unwrap());
    let portable_link = zyr_account::attach(
        &server.address(),
        trust,
        &portable_identity,
        &credentials("victor", false),
        "Portable",
    )
    .await
    .unwrap();
    let deadline = tokio::time::Instant::now() + PATIENCE;
    while pc.snapshot().devices.len() < 2 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "le portable n'est jamais apparu"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // Le PC accepte l'accès distant ; le portable arrive et le voit prêt.
    pc.set_access(Access::Ready);
    let (portable, mut portable_events) =
        Live::open(portable_link.clone(), portable_identity.clone(), log());
    let snapshot = until_connected(&portable).await;
    let seen = snapshot
        .devices
        .iter()
        .find(|d| d.id == pc_link.device)
        .unwrap();
    assert!(seen.online);
    assert_eq!(seen.access, Access::Ready);

    // Le rendez-vous : le portable va vers le PC.
    portable.say(FromDevice::SessionOpen {
        to: pc_link.device.clone(),
    });
    let Event::SessionStart(seen_by_portable) = expect(&mut portable_events).await else {
        panic!("le portable devait voir la session commencer");
    };
    assert_eq!(seen_by_portable.peer.device, pc_link.device);
    assert!(seen_by_portable.relay.is_none());
    let Event::SessionStart(seen_by_pc) = expect(&mut pc_events).await else {
        panic!("le PC devait voir la session commencer");
    };
    let session = seen_by_portable.session.clone();
    assert_eq!(seen_by_pc.session, session);
    assert_eq!(seen_by_pc.peer.device, portable_link.device);
    let read = Verifier::new(pc_link.signing_key)
        .ticket_for_host(&seen_by_pc.ticket, pc_identity.fingerprint(), now())
        .unwrap();
    assert_eq!(read.grant, Grant::Owner);

    let candidates = vec!["10.0.0.2:47000".parse().unwrap()];
    pc.say(FromDevice::SessionCandidates {
        session: session.clone(),
        candidates: candidates.clone(),
    });
    assert_eq!(
        expect(&mut portable_events).await,
        Event::SessionCandidates {
            session: session.clone(),
            candidates
        }
    );
    portable.say(FromDevice::SessionEnd {
        session: session.clone(),
    });
    assert_eq!(expect(&mut pc_events).await, Event::SessionEnd { session });

    // Révoqué depuis le PC, le portable l'apprend et doit oublier son lien.
    let rest = Rest::new(&server.address(), trust).unwrap();
    rest.revoke_device(&pc_link.token, &portable_link.device)
        .await
        .unwrap();
    assert_eq!(expect(&mut portable_events).await, Event::Revoked);
    let deadline = tokio::time::Instant::now() + PATIENCE;
    while pc.snapshot().devices.len() > 1 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "le portable est resté"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    drop(portable);
    drop(pc);
    server.stop().await;
}
