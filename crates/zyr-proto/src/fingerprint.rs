//! What a computer is recognised by: the fingerprint of its certificate.
//!
//! The tunnel checks it, and far more than the tunnel names it: the
//! service, the server, the local network, the window and the command
//! line all say which computer they mean by its fingerprint. The value
//! lives here, in the base, so that naming a computer never pulls in the
//! encrypted connection that checks it.

use sha2::{Digest, Sha256};

/// Fingerprint of a certificate, the only identity the tunnel cares for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fingerprint([u8; 32]);

impl Fingerprint {
    /// The fingerprint of a certificate, or of a public key, from its
    /// bytes as it is encoded: their SHA-256.
    pub fn of(encoded: &[u8]) -> Self {
        Self(Sha256::digest(encoded).into())
    }

    /// The fingerprint as its bytes, for whoever needs a stable number
    /// derived from it rather than its spelling.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// A fingerprint read back from its bytes, as a datagram carries it.
impl From<[u8; 32]> for Fingerprint {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

/// Text that is not a fingerprint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidFingerprint;

impl std::fmt::Display for InvalidFingerprint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "une empreinte s'écrit avec 64 caractères hexadécimaux")
    }
}

impl std::error::Error for InvalidFingerprint {}

/// Reads a fingerprint back, exactly as it is displayed.
impl std::str::FromStr for Fingerprint {
    type Err = InvalidFingerprint;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let text = text.trim();
        if text.len() != 64 {
            return Err(InvalidFingerprint);
        }
        let mut bytes = [0u8; 32];
        let (pairs, _) = text.as_bytes().as_chunks::<2>();
        for (slot, pair) in bytes.iter_mut().zip(pairs) {
            let pair = std::str::from_utf8(pair).map_err(|_| InvalidFingerprint)?;
            *slot = u8::from_str_radix(pair, 16).map_err(|_| InvalidFingerprint)?;
        }
        Ok(Self(bytes))
    }
}

/// Shown in hexadecimal, the way a fingerprint is shown everywhere else.
impl std::fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fingerprint_is_stable_and_readable() {
        let fingerprint = Fingerprint::of(b"a certificate");
        assert_eq!(fingerprint, Fingerprint::of(b"a certificate"));
        assert_ne!(fingerprint, Fingerprint::of(b"another certificate"));
        let text = fingerprint.to_string();
        assert_eq!(text.len(), 64);
        assert!(text.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn a_displayed_fingerprint_reads_back() {
        let fingerprint = Fingerprint::of(b"a certificate");
        assert_eq!(
            fingerprint.to_string().parse::<Fingerprint>().unwrap(),
            fingerprint
        );
        // Copied out of a terminal, it often drags whitespace along.
        assert_eq!(
            format!("  {fingerprint}\n").parse::<Fingerprint>().unwrap(),
            fingerprint
        );
    }

    #[test]
    fn text_that_is_not_a_fingerprint_is_refused() {
        for text in ["", "abc", &"z".repeat(64), &"ab".repeat(31)] {
            assert!(text.parse::<Fingerprint>().is_err(), "{text}");
        }
    }
}
