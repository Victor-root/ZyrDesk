//! Values carried in `name=value` fields, on one line.
//!
//! The product says most things to itself one line at a time: a word,
//! then `name=value` fields separated by spaces. The service and its
//! window talk that way, and a fact travels that way wherever it goes. A
//! value holding a space would be read as the start of the next field,
//! so it travels packed.

/// Packs a value so it survives inside a `name=value` field.
///
/// Spaces are what separate one field from the next, so a computer
/// called « PC de Victor » would otherwise be read as three fields and
/// lose everything after the first word.
pub fn packed(text: &str) -> String {
    text.replace('\\', r"\\")
        .replace(' ', r"\s")
        .replace('\n', r"\n")
}

/// Bytes written as text that holds no space, to travel in a field:
/// base64, which costs a third of their weight.
pub fn bytes_packed(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// The bytes written by `bytes_packed`, or nothing when the text is not
/// that.
pub fn bytes_unpacked(text: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(text).ok()
}

/// Gives a packed value its spaces back.
pub fn unpacked(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pieces = text.chars();
    while let Some(c) = pieces.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match pieces.next() {
            Some('s') => out.push(' '),
            Some('n') => out.push('\n'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_cross_a_field_whole_and_text_that_is_not_bytes_is_refused() {
        let bytes = [0u8, 255, 32, 10, 92, 7];
        let packed = bytes_packed(&bytes);
        assert!(!packed.contains(' '));
        assert_eq!(bytes_unpacked(&packed).as_deref(), Some(&bytes[..]));
        assert_eq!(bytes_unpacked("not bytes at all"), None);
    }

    #[test]
    fn a_packed_value_comes_back_whole() {
        for value in [
            "PC de Victor",
            r"C:\Users\victor",
            "two\nlines",
            r"a \s that was written",
            "",
        ] {
            let packed = packed(value);
            assert!(!packed.contains([' ', '\n']), "{packed}");
            assert_eq!(unpacked(&packed), value);
        }
    }

    #[test]
    fn a_backslash_nobody_packed_is_kept() {
        assert_eq!(unpacked(r"a\qb"), r"a\qb");
        assert_eq!(unpacked("end\\"), "end\\");
    }
}
