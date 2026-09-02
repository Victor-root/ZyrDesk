//! Why the server said no, in a word that does not change.
//!
//! The server answers a code and an English sentence; the code is the
//! contract, and it is the window that turns it into French for the
//! person. A new reason is a new code, never a reworded sentence.

use std::fmt;

use serde::{Deserialize, Serialize};

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
    /// What the window says to the person for this code.
    pub fn explanation(self) -> &'static str {
        match self {
            Code::InvalidCredentials => "nom d'utilisateur ou mot de passe incorrect",
            Code::RegistrationClosed => "ce serveur n'accepte pas de nouveau compte",
            Code::InvitationInvalid => "ce code d'invitation n'est pas valable",
            Code::UsernameTaken => "ce nom d'utilisateur est déjà pris",
            Code::WeakPassword => "le mot de passe doit faire douze caractères au moins",
            Code::InvalidUsername => {
                "un nom d'utilisateur fait de 3 à 32 caractères : lettres, chiffres, point, tiret \
                 ou souligné"
            }
            Code::Unauthorized => {
                "ce serveur ne reconnaît plus cet appareil : à rattacher de nouveau"
            }
            Code::DeviceRevoked => "cet appareil a été révoqué du compte",
            Code::DeviceUnknown => "cet appareil n'appartient pas au compte",
            Code::ProofInvalid => "la preuve de la clé de cet appareil n'a pas été acceptée",
            Code::ChallengeExpired => "le défi du serveur a expiré, il faut recommencer",
            Code::NotFound => "ce que la demande nomme n'existe pas",
            Code::ContactExists => "une demande existe déjà entre ces deux comptes",
            Code::NotAContact => "ce compte n'est pas un contact",
            Code::ContactSelf => "on ne peut pas être son propre contact",
            Code::ShareInvalid => "ce partage nomme un appareil ou un contact qui ne convient pas",
            Code::PeerOffline => "cet ordinateur n'est pas connecté au serveur",
            Code::PeerNotHosting => {
                "cet ordinateur est en ligne mais n'accepte pas l'accès distant"
            }
            Code::NoRight => "aucun droit sur cet ordinateur",
            Code::UpgradeNeeded => "le serveur et cet appareil ne parlent pas la même version",
            Code::RateLimited => "trop de tentatives, attendre un peu",
            Code::BadRequest => "le serveur n'a pas compris la demande",
            Code::Internal => "le serveur a rencontré une erreur, son journal dit laquelle",
        }
    }
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.explanation())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_code_travels_as_a_stable_word() {
        // Le mot est le contrat : la fenêtre le lit pour parler français,
        // et un serveur plus récent ne doit pas le reformuler.
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
