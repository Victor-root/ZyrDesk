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

    pub(crate) fn i16(&mut self) -> Result<i16, WireError> {
        self.array().map(i16::from_le_bytes)
    }

    pub(crate) fn u32(&mut self) -> Result<u32, WireError> {
        self.array().map(u32::from_le_bytes)
    }

    pub(crate) fn u64(&mut self) -> Result<u64, WireError> {
        self.array().map(u64::from_le_bytes)
    }

    /// A yes or no, written 0 or 1 and nothing else.
    pub(crate) fn flag(&mut self, field: &'static str) -> Result<bool, WireError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(WireError::Invalid(field)),
        }
    }

    /// A string behind its u16 length, which has to be UTF-8.
    pub(crate) fn text(&mut self, field: &'static str) -> Result<String, WireError> {
        let len = usize::from(self.u16()?);
        let (bytes, rest) = self
            .rest
            .split_at_checked(len)
            .ok_or(WireError::Truncated)?;
        self.rest = rest;
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|_| WireError::Invalid(field))
    }

    /// Everything not read yet, for a last field that runs to the end.
    pub(crate) fn rest(self) -> &'a [u8] {
        self.rest
    }

    /// Checks that nothing follows the last field.
    pub(crate) fn finish(self) -> Result<(), WireError> {
        if self.rest.is_empty() {
            Ok(())
        } else {
            Err(WireError::TooLong)
        }
    }
}

/// Writes a string behind its u16 length.
///
/// A string that would not fit is cut at the last whole character that
/// does: every string the engine sends is a name or a sentence, and the
/// start of one is worth more than a message that cannot be written.
pub(crate) fn put_text(out: &mut Vec<u8>, text: &str) {
    let text = clipped(text, usize::from(u16::MAX));
    out.extend_from_slice(&(text.len() as u16).to_le_bytes());
    out.extend_from_slice(text.as_bytes());
}

/// The longest start of `text` that fits in `most` bytes without
/// splitting a character.
pub(crate) fn clipped(text: &str, most: usize) -> &str {
    &text[..text.floor_char_boundary(most)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_are_read_in_order_and_little_endian() {
        let bytes = [
            7, 0x34, 0x12, 0xfe, 0xff, 1, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 1,
        ];
        let mut reader = Reader::new(&bytes);
        assert_eq!(reader.u8().unwrap(), 7);
        assert_eq!(reader.u16().unwrap(), 0x1234);
        assert_eq!(reader.i16().unwrap(), -2);
        assert_eq!(reader.u32().unwrap(), 1);
        assert_eq!(reader.u64().unwrap(), 2);
        assert!(reader.flag("flag").unwrap());
        reader.finish().unwrap();
    }

    #[test]
    fn a_missing_field_is_truncated_and_a_spare_byte_too_long() {
        assert_eq!(Reader::new(&[1]).u16(), Err(WireError::Truncated));
        assert_eq!(Reader::new(&[1, 2]).finish(), Err(WireError::TooLong));
        assert_eq!(Reader::new(&[2]).flag("on"), Err(WireError::Invalid("on")));
    }

    #[test]
    fn a_text_makes_the_round_trip_and_bad_utf8_is_refused() {
        let mut out = Vec::new();
        put_text(&mut out, "Écran principal");
        let mut reader = Reader::new(&out);
        assert_eq!(reader.text("name").unwrap(), "Écran principal");
        reader.finish().unwrap();

        let bad = [2, 0, 0xc3, 0x28];
        assert_eq!(
            Reader::new(&bad).text("name"),
            Err(WireError::Invalid("name"))
        );
        assert_eq!(
            Reader::new(&[5, 0, b'a']).text("name"),
            Err(WireError::Truncated)
        );
    }

    #[test]
    fn a_text_too_long_is_cut_on_a_character_boundary() {
        let long = "é".repeat(40_000);
        let mut out = Vec::new();
        put_text(&mut out, &long);
        let read = Reader::new(&out).text("name").unwrap();
        assert!(read.len() <= usize::from(u16::MAX));
        assert!(long.starts_with(&read));
        assert_eq!(clipped("abc", 10), "abc");
        assert_eq!(clipped("aé", 2), "a");
    }
}
