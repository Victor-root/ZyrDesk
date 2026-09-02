//! What is asked and answered over HTTPS.
//!
//! One type per body, named after what it carries, and the paths beside
//! them so that neither side spells one differently. Everything is JSON;
//! dates are seconds since the epoch; identifiers are opaque strings the
//! server chose.

use serde::{Deserialize, Serialize};
use zyr_transport::Fingerprint;

use crate::code::Code;
use crate::signing::ServerPublicKey;

/// Where each thing is asked.
pub mod paths {
    /// What this server is, for anyone: `GET`.
    pub const SERVER: &str = "/v1/server";
    /// Creating an account: `POST`.
    pub const ACCOUNTS: &str = "/v1/accounts";
    /// Trading a password for an account token: `POST`.
    pub const LOGIN: &str = "/v1/login";
    /// A challenge for a device to sign: `POST`.
    pub const CHALLENGE: &str = "/v1/devices/challenge";
    /// Attaching this device (`POST`), listing them (`GET`); one device
    /// under `/v1/devices/{id}` to rename (`PATCH`) or revoke (`DELETE`).
    pub const DEVICES: &str = "/v1/devices";
    /// Contacts: list (`GET`), ask (`POST`); one under
    /// `/v1/contacts/{id}` to accept (`POST .../accept`), decline
    /// (`POST .../decline`) or remove (`DELETE`).
    pub const CONTACTS: &str = "/v1/contacts";
    /// Shares: list (`GET`), give (`POST`); one under `/v1/shares/{id}`
    /// to take back (`DELETE`).
    pub const SHARES: &str = "/v1/shares";
    /// The live channel, `GET` upgraded to a WebSocket.
    pub const LIVE: &str = "/v1/live";
}

/// Who may create an account.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Registration {
    Open,
    Invitation,
    Closed,
}

/// What a server says of itself to anyone who asks.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ServerInfo {
    /// What the application shows for it.
    pub name: String,
    pub version: String,
    pub protocol: u32,
    pub registration: Registration,
    /// Whether it has a relay to offer.
    pub relay: bool,
    /// The key its tickets are signed with, learned here and pinned.
    pub signing_key: ServerPublicKey,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Register {
    pub username: String,
    pub password: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invitation: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Login {
    pub username: String,
    pub password: String,
}

/// An account token, for the gestures of the account.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct LoginAnswer {
    pub username: String,
    pub token: String,
    pub expires: u64,
}

/// What a device signs to prove its key.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Challenge {
    pub nonce: String,
    pub expires: u64,
}

/// Attaching this device to the account of the token presented.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Link {
    /// The device's certificate, DER, base64.
    pub certificate: String,
    /// The challenge signed with the certificate's key, base64.
    pub nonce: String,
    pub signature: String,
    pub name: String,
}

/// The device as attached, and the token it will present from now on.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct LinkAnswer {
    pub device: DeviceInfo,
    pub token: String,
}

/// Whether a device accepts remote access right now, and if not, why.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    /// Remote access is switched off there.
    Off,
    /// Its engine stands: a session may be opened towards it.
    Ready,
    /// Remote access is wanted and the engine is on its way.
    Starting,
    /// Remote access is wanted and the engine is missing.
    EngineMissing,
    /// Remote access is wanted and the engine keeps falling over.
    EngineWontStand,
}

/// One device of an account, as the server sees it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    #[serde(with = "crate::fingerprint")]
    pub fingerprint: Fingerprint,
    /// Connected to the server right now.
    pub online: bool,
    pub access: Access,
    /// When it was last connected, if it is not now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_seen: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Rename {
    pub name: String,
}

/// Where a contact stands.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContactStatus {
    /// Asked, not yet answered.
    Pending,
    Accepted,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ContactInfo {
    pub id: String,
    pub username: String,
    pub status: ContactStatus,
    /// Whether this account asked, or was asked.
    pub asked_by_me: bool,
    /// Whether any device of that account is connected.
    pub online: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ContactRequest {
    pub username: String,
}

/// What a share lets a contact do on the machine it names.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    Connect,
    Keyboard,
    Mouse,
    Audio,
}

impl Permission {
    /// All of them, which is what a share carries until the finer ones
    /// are enforced.
    pub const ALL: [Permission; 4] = [
        Permission::Connect,
        Permission::Keyboard,
        Permission::Mouse,
        Permission::Audio,
    ];
}

/// One machine shared with one contact.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ShareInfo {
    pub id: String,
    /// The machine shared.
    pub device: DeviceInfo,
    /// Who owns it.
    pub owner: String,
    /// Who it is shared with.
    pub with: String,
    pub permissions: Vec<Permission>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires: Option<u64>,
    pub created: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ShareRequest {
    /// The device to share, by its identifier.
    pub device: String,
    /// The contact, by username.
    pub with: String,
    pub permissions: Vec<Permission>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires: Option<u64>,
}

/// Why the server said no.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub error: Code,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use zyr_transport::Identity;

    #[test]
    fn a_device_reads_back_and_leaves_out_what_it_has_not() {
        let device = DeviceInfo {
            id: "d1".into(),
            name: "PC de Victor".into(),
            fingerprint: Identity::generate().unwrap().fingerprint(),
            online: true,
            access: Access::Ready,
            last_seen: None,
        };
        let text = serde_json::to_string(&device).unwrap();
        assert!(text.contains("\"access\":\"ready\""), "{text}");
        assert!(!text.contains("last_seen"), "{text}");
        assert_eq!(serde_json::from_str::<DeviceInfo>(&text).unwrap(), device);
    }

    #[test]
    fn an_older_answer_without_the_optional_fields_still_reads() {
        // Un serveur qui ne dit ni e-mail ni invitation, ni expiration :
        // la demande se lit quand même.
        let register: Register =
            serde_json::from_str(r#"{"username":"victor","password":"douze caractères"}"#).unwrap();
        assert_eq!(register.email, None);
        assert_eq!(register.invitation, None);
        let share: ShareRequest = serde_json::from_str(
            r#"{"device":"d1","with":"ami","permissions":["connect","keyboard"]}"#,
        )
        .unwrap();
        assert_eq!(
            share.permissions,
            [Permission::Connect, Permission::Keyboard]
        );
        assert_eq!(share.expires, None);
    }

    #[test]
    fn the_registration_policy_is_a_word() {
        assert_eq!(
            serde_json::to_string(&Registration::Invitation).unwrap(),
            "\"invitation\""
        );
        let error: Error = serde_json::from_str(
            r#"{"error":"registration_closed","message":"nobody may register"}"#,
        )
        .unwrap();
        assert_eq!(error.error, Code::RegistrationClosed);
    }
}
