//! Proving a device's key without ever showing it.
//!
//! A token can be stolen; a private key stays where it is. So a device
//! signs, with the key of its certificate, a challenge the server hands
//! out: once when it is attached to an account, and again each time it
//! opens its live channel. What is signed names the server as well, so a
//! proof made for one server is worth nothing to another.

use std::fmt;

use crate::signing::ServerPublicKey;

/// What the proof is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    /// Attaching this device to an account.
    Link,
    /// Opening the live channel.
    Live,
}

impl fmt::Display for Purpose {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Purpose::Link => "link",
            Purpose::Live => "live",
        })
    }
}

/// The bytes a device signs in answer to that challenge.
pub fn challenge_message(server: &ServerPublicKey, nonce: &str, purpose: Purpose) -> Vec<u8> {
    format!("zyrdesk-proof/1\n{purpose}\n{server}\n{nonce}\n").into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signing::ServerKey;

    #[test]
    fn the_message_binds_the_server_the_nonce_and_the_purpose() {
        // Une preuve faite pour un serveur, un défi et un usage ne vaut
        // pour aucun autre : changer l'un des trois change les octets.
        let server = ServerKey::generate().public();
        let other = ServerKey::generate().public();
        let reference = challenge_message(&server, "n1", Purpose::Link);
        assert_ne!(reference, challenge_message(&other, "n1", Purpose::Link));
        assert_ne!(reference, challenge_message(&server, "n2", Purpose::Link));
        assert_ne!(reference, challenge_message(&server, "n1", Purpose::Live));
        assert_eq!(reference, challenge_message(&server, "n1", Purpose::Link));
        assert!(
            std::str::from_utf8(&reference)
                .unwrap()
                .starts_with("zyrdesk-proof/1\nlink\n")
        );
    }
}
