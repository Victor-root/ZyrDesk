//! Something that happened, told as a code and the values that go with it.
//!
//! What the engines, the service and the server have to tell the person
//! is never written as a sentence. A sentence is written once, in one
//! language, by whichever part of the product happened to notice, and it
//! can only ever be shown the way it was written. A fact is named
//! instead: a code that says what happened, and the values the telling
//! needs, such as the name of a computer or a number of seconds. The
//! words are chosen where the person reads them, in their language, by
//! `zyr-i18n`, whose texts are keyed on these very codes.
//!
//! A fact travels as one line, its code first and its values after it as
//! `name=value` fields, the shape everything else the product says to
//! itself already has. The service's answers carry it that way, and so do
//! the engine's notices; a journal writes it down as it is, and it reads
//! plainly there for whoever is looking for a fault.

use std::fmt;
use std::str::FromStr;

use crate::fields::{packed, unpacked};

/// The value a fact keeps the fact that caused it under.
pub const BECAUSE: &str = "because";

/// Something that happened, and what the telling of it needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fact {
    code: String,
    values: Vec<(String, String)>,
}

impl Fact {
    /// A fact, with nothing yet to go with it.
    ///
    /// The code is the key of the words that tell it, in every language:
    /// it is written out where the fact is made, which is what lets the
    /// translations be checked against every fact the product tells.
    pub fn new(code: &'static str) -> Self {
        debug_assert!(is_code(code), "« {code} » is not a code");
        Self {
            code: code.to_string(),
            values: Vec::new(),
        }
    }

    /// The same fact, with one more value to tell it with.
    pub fn with(mut self, name: &'static str, value: impl fmt::Display) -> Self {
        debug_assert!(is_name(name), "« {name} » is not a value's name");
        self.values.push((name.to_string(), value.to_string()));
        self
    }

    /// The same fact, told with the one that caused it.
    ///
    /// Kept under the value [`BECAUSE`] as that fact's own line, so that
    /// the words of this one can say the words of its cause, in whatever
    /// language they are read: « the server refused the session: this
    /// computer is not connected to it ».
    pub fn because(self, cause: &Fact) -> Self {
        self.with(BECAUSE, cause)
    }

    /// The fact this one was told with, when there is one.
    pub fn cause(&self) -> Option<Fact> {
        self.value(BECAUSE)?.parse().ok()
    }

    /// What happened.
    pub fn code(&self) -> &str {
        &self.code
    }

    /// One of the values, by name.
    pub fn value(&self, name: &str) -> Option<&str> {
        self.values
            .iter()
            .find(|(named, _)| named == name)
            .map(|(_, value)| value.as_str())
    }
}

/// Whether a word can be a code: lowercase words joined by dots, as the
/// keys of the texts are.
pub fn is_code(word: &str) -> bool {
    !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_'))
}

/// Whether a word can name a value: the words a text puts between braces.
pub fn is_name(word: &str) -> bool {
    !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

impl fmt::Display for Fact {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.code)?;
        for (name, value) in &self.values {
            write!(f, " {name}={}", packed(value))?;
        }
        Ok(())
    }
}

/// A line that does not tell a fact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotAFact(pub String);

impl fmt::Display for NotAFact {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "not a fact: {}", self.0)
    }
}

impl std::error::Error for NotAFact {}

impl FromStr for Fact {
    type Err = NotAFact;

    fn from_str(line: &str) -> Result<Self, Self::Err> {
        let refused = || NotAFact(line.to_string());
        let mut words = line.split_whitespace();
        let code = words
            .next()
            .filter(|code| is_code(code))
            .ok_or_else(refused)?;
        let values = words
            .map(|word| match word.split_once('=') {
                Some((name, value)) if is_name(name) => Ok((name.to_string(), unpacked(value))),
                _ => Err(refused()),
            })
            .collect::<Result<_, _>>()?;
        Ok(Self {
            code: code.to_string(),
            values,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fact_travels_as_one_line_and_comes_back_whole() {
        let fact = Fact::new("example.silent")
            .with("host", "PC de Victor")
            .with("waited", 12)
            .with("detail", "first line\nsecond line");
        let line = fact.to_string();
        assert!(!line.contains('\n'), "{line}");
        assert_eq!(
            line,
            r"example.silent host=PC\sde\sVictor waited=12 detail=first\sline\nsecond\sline"
        );
        assert_eq!(line.parse(), Ok(fact));
    }

    #[test]
    fn a_fact_says_its_values_by_name() {
        let fact = Fact::new("example.silent").with("host", "PC-17");
        assert_eq!(fact.code(), "example.silent");
        assert_eq!(fact.value("host"), Some("PC-17"));
        assert_eq!(fact.value("waited"), None);
        assert_eq!(Fact::new("example.done").to_string(), "example.done");
    }

    #[test]
    fn a_fact_carries_the_one_that_caused_it() {
        let cause = Fact::new("example.refused").with("code", "no right");
        let fact = Fact::new("example.meeting")
            .with("host", "PC-17")
            .because(&cause);
        let travelled: Fact = fact.to_string().parse().unwrap();
        assert_eq!(travelled.cause(), Some(cause));
        assert_eq!(Fact::new("example.meeting").cause(), None);
    }

    #[test]
    fn a_sentence_is_not_a_fact() {
        for line in [
            "",
            "L'écran filmé n'est plus là : la session montre maintenant Écran 2.",
            "example.silent host",
            "example.silent Host=PC",
            "host=PC",
        ] {
            assert_eq!(line.parse::<Fact>(), Err(NotAFact(line.to_string())));
        }
    }
}
