//! Why the server said no, in a word that does not change.
//!
//! The server answers a code and an English sentence; the code is the
//! contract, and the sentence is for journals. What the person reads is
//! the fact the code stands for, which the window puts into their
//! language. A new reason is a new code, never a reworded sentence.

use std::fmt;

use serde::{Deserialize, Serialize};
use zyr_proto::fact::Fact;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Code {
    /// The name or the password is wrong. One code for both, so the
    /// answer does not say which names exist.
    InvalidCredentials,
    /// Nobody may create an account on this server.
    RegistrationClosed,
    /// An invitation code is required, or the one given is spent or
    /// unknown.
    InvitationInvalid,
    UsernameTaken,
    /// Too short, or otherwise not what the server accepts.
    WeakPassword,
    /// The name is not one the server accepts as a username.
    InvalidUsername,
    /// No token, or a token the server no longer honours.
    Unauthorized,
    /// The device this token belonged to was revoked.
    DeviceRevoked,
    /// The device named does not belong to this account.
    DeviceUnknown,
    /// The signed proof does not match the device's certificate.
    ProofInvalid,
    /// The challenge answered is unknown or has expired.
    ChallengeExpired,
    NotFound,
    /// A contact request already exists between the two, in one
    /// direction or the other, or they are contacts already.
    ContactExists,
    /// The account named is not a contact.
    NotAContact,
    /// One cannot be one's own contact.
    ContactSelf,
    /// The share names a device or a contact that does not fit.
    ShareInvalid,
    /// The device asked for is not connected to the server.
    PeerOffline,
    /// The device asked for is connected but does not accept remote
    /// access right now.
    PeerNotHosting,
    /// Neither the same account nor a valid share.
    NoRight,
    /// The other half speaks another version of the dialect.
    UpgradeNeeded,
    /// Too many attempts; wait.
    RateLimited,
    /// The body of the request could not be read.
    BadRequest,
    /// Something broke on the server, and its journal says what.
    Internal,
}

impl Code {
    /// What the code stands for, for the person to read in their
    /// language.
    pub fn fact(self) -> Fact {
        Fact::new(match self {
            Code::InvalidCredentials => "server.invalid_credentials",
            Code::RegistrationClosed => "server.registration_closed",
            Code::InvitationInvalid => "server.invitation_invalid",
            Code::UsernameTaken => "server.username_taken",
            Code::WeakPassword => "server.weak_password",
            Code::InvalidUsername => "server.invalid_username",
            Code::Unauthorized => "server.unauthorized",
            Code::DeviceRevoked => "server.device_revoked",
            Code::DeviceUnknown => "server.device_unknown",
            Code::ProofInvalid => "server.proof_invalid",
            Code::ChallengeExpired => "server.challenge_expired",
            Code::NotFound => "server.not_found",
            Code::ContactExists => "server.contact_exists",
            Code::NotAContact => "server.not_a_contact",
            Code::ContactSelf => "server.contact_self",
            Code::ShareInvalid => "server.share_invalid",
            Code::PeerOffline => "server.peer_offline",
            Code::PeerNotHosting => "server.peer_not_hosting",
            Code::NoRight => "server.no_right",
            Code::UpgradeNeeded => "server.upgrade_needed",
            Code::RateLimited => "server.rate_limited",
            Code::BadRequest => "server.bad_request",
            Code::Internal => "server.internal",
        })
    }
}

/// The courtesy sentence the server sends beside a code, and what a
/// journal writes of it.
impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Code::InvalidCredentials => "wrong username or password",
            Code::RegistrationClosed => "this server does not take new accounts",
            Code::InvitationInvalid => "an invitation code is required, and this one is not valid",
            Code::UsernameTaken => "this username is taken",
            Code::WeakPassword => "the password must be at least twelve characters",
            Code::InvalidUsername => {
                "a username is 3 to 32 letters, digits, dots, dashes or underscores"
            }
            Code::Unauthorized => "no valid token",
            Code::DeviceRevoked => "this device was revoked",
            Code::DeviceUnknown => "no such device on this account",
            Code::ProofInvalid => "the signature does not match the certificate",
            Code::ChallengeExpired => "unknown or expired challenge",
            Code::NotFound => "not found",
            Code::ContactExists => "a request already stands between these accounts",
            Code::NotAContact => "not a contact",
            Code::ContactSelf => "one cannot be one's own contact",
            Code::ShareInvalid => "the share names a device or a contact that does not fit",
            Code::PeerOffline => "that device is not connected",
            Code::PeerNotHosting => "that device does not accept remote access right now",
            Code::NoRight => "no right on that device",
            Code::UpgradeNeeded => "the server and the device speak different versions",
            Code::RateLimited => "too many attempts, wait",
            Code::BadRequest => "the request could not be read",
            Code::Internal => "the server failed, its journal says why",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_code_travels_as_a_stable_word() {
        // The word is the contract: the window reads it to tell the
        // person what happened, and a newer server must not reword it.
        assert_eq!(
            serde_json::to_string(&Code::InvalidCredentials).unwrap(),
            "\"invalid_credentials\""
        );
        assert_eq!(
            serde_json::from_str::<Code>("\"peer_not_hosting\"").unwrap(),
            Code::PeerNotHosting
        );
    }
}
