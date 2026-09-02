//! The live channel: what travels on the WebSocket, in both directions.
//!
//! One connection per attached device, held open by the service. The
//! server speaks first with a challenge, the device answers with its
//! proof and its version, and from then on the channel carries what is
//! alive: presence, changes to the account, and the rendezvous of a
//! session. Every message is a JSON object with a `type`.

use std::net::SocketAddr;

use serde::{Deserialize, Serialize};
use zyr_transport::Fingerprint;

use crate::code::Code;
use crate::rest::{Access, ContactInfo, DeviceInfo, ServerInfo, ShareInfo};
use crate::signing::Signed;

/// What a device says to the server.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum FromDevice {
    /// First word, with the proof of the challenge just received.
    Hello {
        protocol: u32,
        build: String,
        /// The challenge signed with the device's key, base64.
        signature: String,
    },
    /// Whether this device accepts remote access, each time it changes.
    State {
        access: Access,
    },
    /// Open a session towards that device of the account, or shared.
    SessionOpen {
        to: String,
    },
    /// Where this device may be reached, for that session, as they are
    /// found.
    SessionCandidates {
        session: String,
        candidates: Vec<SocketAddr>,
    },
    SessionEnd {
        session: String,
    },
}

/// What the server says to a device.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum FromServer {
    /// First word: sign this.
    Challenge {
        nonce: String,
    },
    /// The proof was taken: here is everything the device needs to know.
    Welcome {
        server: ServerInfo,
        /// This device's own identifier.
        me: String,
        devices: Vec<DeviceInfo>,
        contacts: Vec<ContactInfo>,
        shares: Vec<ShareInfo>,
        /// A fresh token, when the one presented is about to run out:
        /// the device keeps it in place of the old one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        token: Option<String>,
    },
    /// A device of the account, or one shared, came or went.
    Presence {
        device: String,
        online: bool,
        access: Access,
    },
    DeviceAdded {
        device: DeviceInfo,
    },
    DeviceRenamed {
        device: DeviceInfo,
    },
    /// The device named is no longer of the account. When it is this
    /// one, the channel closes right after.
    DeviceRevoked {
        device: String,
    },
    ContactRequested {
        contact: ContactInfo,
    },
    ContactAnswered {
        contact: ContactInfo,
    },
    ContactRemoved {
        contact: String,
    },
    ShareGiven {
        share: ShareInfo,
    },
    ShareRemoved {
        share: String,
    },
    /// A session is on: here is the other device, and the way to it.
    SessionStart {
        session: String,
        ticket: Signed,
        peer: Peer,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        relay: Option<Relay>,
    },
    SessionCandidates {
        session: String,
        candidates: Vec<SocketAddr>,
    },
    SessionEnd {
        session: String,
    },
    /// The session asked for could not be opened.
    SessionRefused {
        to: String,
        code: Code,
    },
    /// The channel is being closed for this reason.
    Bye {
        code: Code,
    },
}

/// The other device of a session.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Peer {
    pub device: String,
    #[serde(with = "crate::fingerprint")]
    pub fingerprint: Fingerprint,
    pub name: String,
    /// The username of the account it belongs to.
    pub account: String,
}

/// The relay assigned to a session, and the pass into it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Relay {
    /// Host and port, as the device is to reach it.
    pub address: String,
    /// The fingerprint of the certificate the relay presents.
    #[serde(with = "crate::fingerprint")]
    pub fingerprint: Fingerprint,
    pub pass: Signed,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rest::{ContactStatus, Registration};
    use crate::signing::ServerKey;
    use zyr_transport::Identity;

    fn device(id: &str) -> DeviceInfo {
        DeviceInfo {
            id: id.into(),
            name: format!("PC {id}"),
            fingerprint: Identity::generate().unwrap().fingerprint(),
            online: false,
            access: Access::Off,
            last_seen: Some(42),
        }
    }

    #[test]
    fn every_message_reads_back_as_itself() {
        let key = ServerKey::generate();
        let fingerprint = Identity::generate().unwrap().fingerprint();
        let signed = key.seal(&"quelque chose").unwrap();
        let from_device = [
            FromDevice::Hello {
                protocol: crate::PROTOCOL,
                build: "abc (2026-09-02)".into(),
                signature: "c2ln".into(),
            },
            FromDevice::State {
                access: Access::EngineMissing,
            },
            FromDevice::SessionOpen { to: "d2".into() },
            FromDevice::SessionCandidates {
                session: "s1".into(),
                candidates: vec![
                    "192.168.1.4:47000".parse().unwrap(),
                    "[fd00::1]:47000".parse().unwrap(),
                ],
            },
            FromDevice::SessionEnd {
                session: "s1".into(),
            },
        ];
        for message in from_device {
            let text = serde_json::to_string(&message).unwrap();
            assert!(text.starts_with("{\"type\":\""), "{text}");
            assert_eq!(serde_json::from_str::<FromDevice>(&text).unwrap(), message);
        }

        let from_server = [
            FromServer::Challenge { nonce: "n".into() },
            FromServer::Welcome {
                server: ServerInfo {
                    name: "Maison".into(),
                    version: "0.1.0".into(),
                    protocol: crate::PROTOCOL,
                    registration: Registration::Invitation,
                    relay: true,
                    udp_port: Some(443),
                    signing_key: key.public(),
                },
                me: "d1".into(),
                devices: vec![device("d1"), device("d2")],
                contacts: vec![ContactInfo {
                    id: "c1".into(),
                    username: "ami".into(),
                    status: ContactStatus::Pending,
                    asked_by_me: true,
                    online: false,
                }],
                shares: vec![ShareInfo {
                    id: "p1".into(),
                    device: device("d9"),
                    owner: "ami".into(),
                    with: "victor".into(),
                    permissions: crate::rest::Permission::ALL.to_vec(),
                    expires: None,
                    created: 1,
                }],
                token: Some("jeton".into()),
            },
            FromServer::Presence {
                device: "d2".into(),
                online: true,
                access: Access::Ready,
            },
            FromServer::DeviceAdded {
                device: device("d3"),
            },
            FromServer::DeviceRenamed {
                device: device("d3"),
            },
            FromServer::DeviceRevoked {
                device: "d3".into(),
            },
            FromServer::ContactRequested {
                contact: ContactInfo {
                    id: "c2".into(),
                    username: "autre".into(),
                    status: ContactStatus::Pending,
                    asked_by_me: false,
                    online: true,
                },
            },
            FromServer::ContactAnswered {
                contact: ContactInfo {
                    id: "c2".into(),
                    username: "autre".into(),
                    status: ContactStatus::Accepted,
                    asked_by_me: false,
                    online: true,
                },
            },
            FromServer::ContactRemoved {
                contact: "c2".into(),
            },
            FromServer::ShareGiven {
                share: ShareInfo {
                    id: "p2".into(),
                    device: device("d1"),
                    owner: "victor".into(),
                    with: "autre".into(),
                    permissions: vec![crate::rest::Permission::Connect],
                    expires: Some(9_000),
                    created: 2,
                },
            },
            FromServer::ShareRemoved { share: "p2".into() },
            FromServer::SessionStart {
                session: "s1".into(),
                ticket: signed.clone(),
                peer: Peer {
                    device: "d2".into(),
                    fingerprint,
                    name: "PC d2".into(),
                    account: "victor".into(),
                },
                relay: Some(Relay {
                    address: "zyr.exemple.fr:443".into(),
                    fingerprint,
                    pass: signed,
                }),
            },
            FromServer::SessionCandidates {
                session: "s1".into(),
                candidates: vec!["82.64.12.7:47000".parse().unwrap()],
            },
            FromServer::SessionEnd {
                session: "s1".into(),
            },
            FromServer::SessionRefused {
                to: "d2".into(),
                code: Code::PeerOffline,
            },
            FromServer::Bye {
                code: Code::DeviceRevoked,
            },
        ];
        for message in from_server {
            let text = serde_json::to_string(&message).unwrap();
            assert!(text.starts_with("{\"type\":\""), "{text}");
            assert_eq!(serde_json::from_str::<FromServer>(&text).unwrap(), message);
        }
    }

    #[test]
    fn a_session_without_relay_leaves_the_field_out() {
        let key = ServerKey::generate();
        let start = FromServer::SessionStart {
            session: "s1".into(),
            ticket: key.seal(&"t").unwrap(),
            peer: Peer {
                device: "d2".into(),
                fingerprint: Identity::generate().unwrap().fingerprint(),
                name: "PC".into(),
                account: "victor".into(),
            },
            relay: None,
        };
        let text = serde_json::to_string(&start).unwrap();
        assert!(!text.contains("relay"), "{text}");
        assert_eq!(serde_json::from_str::<FromServer>(&text).unwrap(), start);
    }

    #[test]
    fn a_message_of_an_unknown_type_is_refused_by_name() {
        let refusal = serde_json::from_str::<FromServer>(r#"{"type":"teleport","to":"mars"}"#)
            .unwrap_err()
            .to_string();
        assert!(refusal.contains("teleport"), "{refusal}");
    }
}
