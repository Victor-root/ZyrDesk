//! Reading and writing the engine's binary formats.
//!
//! Every multi-byte integer is little-endian. Reading never trusts a
//! length: a field is taken only if its bytes are there, and bytes left
//! over after the last field are refused rather than ignored, so that a
//! message can only ever mean one thing.

use std::fmt;

/// Bytes that do not form what they claim to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireError {
    /// Shorter than its fields announce.
    Truncated,
    /// Longer than its fields, or than a limit allows.
    TooLong,
    /// Written in a version of the format this build does not speak.
    Version(u16),
    /// A kind of message this build does not know.
    Kind(u8),
    /// A field holding a value it may not take, named.
    Invalid(&'static str),
    /// A control stream whose framing is lost: nothing after it can be
    /// read, and the stream has to be closed.
    Framing,
}

impl fmt::Display for WireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WireError::Truncated => write!(f, "message tronqué"),
            WireError::TooLong => write!(f, "message trop long"),
            WireError::Version(version) => write!(f, "version de format inconnue : {version}"),
            WireError::Kind(kind) => write!(f, "type de message inconnu : {kind}"),
            WireError::Invalid(field) => write!(f, "valeur impossible pour « {field} »"),
            WireError::Framing => write!(f, "flux de contrôle désynchronisé"),
        }
    }
}

impl std::error::Error for WireError {}

/// Takes the fields of a message off its front, one after the other.
pub(crate) struct Reader<'a> {
    rest: &'a [u8],
}

impl<'a> Reader<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self { rest: bytes }
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], WireError> {
        let (head, rest) = self
            .rest
            .split_first_chunk::<N>()
            .ok_or(WireError::Truncated)?;
        self.rest = rest;
        Ok(*head)
    }

    pub(crate) fn u8(&mut self) -> Result<u8, WireError> {
        let [byte] = self.array()?;
        Ok(byte)
    }

    pub(crate) fn u16(&mut self) -> Result<u16, WireError> {
        self.array().map(u16::from_le_bytes)
    }

    pub(crate) fn u32(&mut self) -> Result<u32, WireError> {
        self.array().map(u32::from_le_bytes)
    }

    /// Everything not read yet, for a last field that runs to the end.
    pub(crate) fn rest(self) -> &'a [u8] {
        self.rest
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_are_read_in_order_and_little_endian() {
        let bytes = [7, 0x34, 0x12, 1, 0, 0, 0, 9];
        let mut reader = Reader::new(&bytes);
        assert_eq!(reader.u8().unwrap(), 7);
        assert_eq!(reader.u16().unwrap(), 0x1234);
        assert_eq!(reader.u32().unwrap(), 1);
        assert_eq!(reader.rest(), &[9]);
    }

    #[test]
    fn a_missing_field_is_truncated() {
        assert_eq!(Reader::new(&[1]).u16(), Err(WireError::Truncated));
        assert_eq!(Reader::new(&[1, 2, 3]).u32(), Err(WireError::Truncated));
    }
}
