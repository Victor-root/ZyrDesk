//! The server's signature, and the envelope that carries it.
//!
//! The server has one signing key, Ed25519, made at its installation.
//! What it signs is what the two services believe of it: a ticket that
//! presents one device to another, a pass that lets a device into the
//! relay. A device learns the public half once, over a channel it has
//! already verified, and pins it: a server that changed key would be
//! refused with a sentence that says so.
//!
//! The envelope carries the signed bytes themselves, not a description
//! of them: whoever verifies checks exactly what was signed, then reads
//! it. Nothing has to agree on how a JSON object is spelt.

use std::fmt;
use std::str::FromStr;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The server's private signing key.
///
/// Not clonable and not printable: it exists once, in the server's
/// memory, and its file on disk is the server's identity.
pub struct ServerKey(SigningKey);

impl ServerKey {
    /// Draws a new key from the system generator.
    pub fn generate() -> Self {
        let secret: [u8; 32] = rand::random();
        Self(SigningKey::from_bytes(&secret))
    }

    /// The key as it is kept on disk.
    pub fn from_bytes(secret: &[u8; 32]) -> Self {
        Self(SigningKey::from_bytes(secret))
    }

    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_bytes()
    }

    /// The half that travels.
    pub fn public(&self) -> ServerPublicKey {
        ServerPublicKey(self.0.verifying_key())
    }

    /// Signs that body, as JSON, and hands back what carries it.
    pub fn seal<T: Serialize>(&self, body: &T) -> Result<Signed, serde_json::Error> {
        let bytes = serde_json::to_vec(body)?;
        let signature = self.0.sign(&bytes);
        Ok(Signed {
            body: BASE64.encode(&bytes),
            signature: BASE64.encode(signature.to_bytes()),
        })
    }
}

impl fmt::Debug for ServerKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ServerKey({})", self.public())
    }
}

/// The server's public key, as every device knows it.
///
/// Shown and carried in base64: thirty-two bytes, forty-four characters,
/// which is what `zyrdesk-server fingerprint` prints beside the TLS
/// fingerprint.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServerPublicKey(VerifyingKey);

impl ServerPublicKey {
    pub fn from_bytes(bytes: &[u8; 32]) -> Result<Self, NotAKey> {
        VerifyingKey::from_bytes(bytes)
            .map(Self)
            .map_err(|_| NotAKey)
    }

    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_bytes()
    }
}

/// Text that is not a server key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotAKey;

impl fmt::Display for NotAKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("une clé de serveur s'écrit en 44 caractères de base64")
    }
}

impl std::error::Error for NotAKey {}

impl fmt::Display for ServerPublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&BASE64.encode(self.0.to_bytes()))
    }
}

impl fmt::Debug for ServerPublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ServerPublicKey({self})")
    }
}

impl FromStr for ServerPublicKey {
    type Err = NotAKey;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let bytes = BASE64.decode(text.trim()).map_err(|_| NotAKey)?;
        let bytes: [u8; 32] = bytes.try_into().map_err(|_| NotAKey)?;
        Self::from_bytes(&bytes)
    }
}

impl Serialize for ServerPublicKey {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.to_string().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ServerPublicKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

/// Something the server signed, exactly as it signed it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Signed {
    /// The signed bytes, base64.
    pub body: String,
    /// Their Ed25519 signature, base64.
    pub signature: String,
}

impl Signed {
    /// The bytes inside, once the signature has vouched for them.
    pub fn verified(&self, key: &ServerPublicKey) -> Result<Vec<u8>, Forged> {
        let body = BASE64.decode(&self.body).map_err(|_| Forged::Unreadable)?;
        let signature = BASE64
            .decode(&self.signature)
            .map_err(|_| Forged::Unreadable)?;
        let signature = Signature::from_slice(&signature).map_err(|_| Forged::Unreadable)?;
        key.0
            .verify_strict(&body, &signature)
            .map_err(|_| Forged::Signature)?;
        Ok(body)
    }

    /// Reads what is inside, once the signature has vouched for it.
    pub fn open<T: DeserializeOwned>(&self, key: &ServerPublicKey) -> Result<T, Forged> {
        serde_json::from_slice(&self.verified(key)?).map_err(|e| Forged::Body(e.to_string()))
    }

    /// Written out whole, for the one place a signed thing travels
    /// outside this dialect: the pass a device presents to a relay, on
    /// a stream that carries bytes and nothing else.
    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    /// Read back from those bytes, or nothing when they are not one.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        serde_json::from_slice(bytes).ok()
    }
}

/// Why a signed thing was not believed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Forged {
    /// Not even base64, or not a signature at all.
    Unreadable,
    /// The signature is not the server's over these bytes.
    Signature,
    /// Signed by the server, but not what was expected inside.
    Body(String),
}

impl fmt::Display for Forged {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Forged::Unreadable => f.write_str("illisible"),
            Forged::Signature => f.write_str("la signature n'est pas celle du serveur"),
            Forged::Body(e) => write!(
                f,
                "signé par le serveur, mais pas de la forme attendue : {e}"
            ),
        }
    }
}

impl std::error::Error for Forged {}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Word {
        said: String,
    }

    #[test]
    fn what_the_server_sealed_opens_with_its_public_key() {
        let key = ServerKey::generate();
        let sealed = key
            .seal(&Word {
                said: "bonjour".into(),
            })
            .unwrap();
        let back: Word = sealed.open(&key.public()).unwrap();
        assert_eq!(back.said, "bonjour");
    }

    #[test]
    fn another_key_does_not_open_it() {
        let key = ServerKey::generate();
        let other = ServerKey::generate();
        let sealed = key
            .seal(&Word {
                said: "bonjour".into(),
            })
            .unwrap();
        assert_eq!(
            sealed.open::<Word>(&other.public()).unwrap_err(),
            Forged::Signature
        );
    }

    #[test]
    fn a_body_touched_in_transit_is_refused() {
        // Le corps voyage tel quel : un octet changé, et la signature ne
        // le couvre plus.
        let key = ServerKey::generate();
        let sealed = key
            .seal(&Word {
                said: "bonjour".into(),
            })
            .unwrap();
        let touched = Signed {
            body: BASE64.encode(br#"{"said":"bonsoir"}"#),
            signature: sealed.signature.clone(),
        };
        assert_eq!(
            touched.open::<Word>(&key.public()).unwrap_err(),
            Forged::Signature
        );
        let garbage = Signed {
            body: "pas du base64 !".into(),
            signature: sealed.signature,
        };
        assert_eq!(
            garbage.open::<Word>(&key.public()).unwrap_err(),
            Forged::Unreadable
        );
    }

    #[test]
    fn a_body_of_another_shape_is_refused_after_the_signature() {
        #[derive(Serialize)]
        struct Other {
            number: u32,
        }
        let key = ServerKey::generate();
        let sealed = key.seal(&Other { number: 4 }).unwrap();
        assert!(matches!(
            sealed.open::<Word>(&key.public()).unwrap_err(),
            Forged::Body(_)
        ));
    }

    #[test]
    fn the_public_key_reads_back_from_its_spelling() {
        let key = ServerKey::generate().public();
        let text = key.to_string();
        assert_eq!(text.len(), 44, "{text}");
        assert_eq!(text.parse::<ServerPublicKey>().unwrap(), key);
        assert_eq!(
            format!(" {text}\n").parse::<ServerPublicKey>().unwrap(),
            key
        );
        assert_eq!(
            serde_json::from_str::<ServerPublicKey>(&serde_json::to_string(&key).unwrap()).unwrap(),
            key
        );
        for wrong in ["", "abc", &"A".repeat(44)] {
            assert!(wrong.parse::<ServerPublicKey>().is_err(), "{wrong}");
        }
    }

    #[test]
    fn the_secret_key_survives_the_disk() {
        let key = ServerKey::generate();
        let again = ServerKey::from_bytes(&key.to_bytes());
        assert_eq!(again.public(), key.public());
        let sealed = key
            .seal(&Word {
                said: "encore".into(),
            })
            .unwrap();
        assert!(sealed.open::<Word>(&again.public()).is_ok());
    }
}
