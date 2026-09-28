//! What the person reads, in their language.
//!
//! Every text the product shows lives in `words/`, one file per language
//! and one line per text: `key = text`. English is the base and says
//! everything; every other language grafts onto it. Adding a language is
//! adding a file there and nothing else: the build finds it, and the
//! tests below check that it says everything English says, asking for
//! the same values.
//!
//! A text names what it needs between braces, `{host}`, and is handed it
//! by name. A fact the engines, the service or the server tell
//! ([`Fact`]) is said the same way: its code is the key, and its values
//! fill the text.
//!
//! Only the programs a person reads choose words, the window and the
//! command line. Everything below them tells facts, and the layer map of
//! `zyr-layers` checks that nothing else uses this brick.

use std::collections::HashMap;
use std::fmt;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicUsize, Ordering};

use zyr_proto::fact::{BECAUSE, Fact, is_name};

// Every language the product speaks, English first: its code, and the
// file of its texts. Listed by the build from the files of `words/`.
include!(concat!(env!("OUT_DIR"), "/languages.rs"));

/// The texts of every language, by key, in the order of the list.
static TEXTS: LazyLock<Vec<HashMap<&'static str, String>>> =
    LazyLock::new(|| LANGUAGES.iter().map(|(_, file)| texts_of(file)).collect());

/// The language spoken, by its place in the list: English until the
/// program says otherwise.
static SPEAKING: AtomicUsize = AtomicUsize::new(0);

/// A file's texts, by key, with their line breaks in place.
fn texts_of(file: &'static str) -> HashMap<&'static str, String> {
    zyr_proto::files::settings(file)
        .map(|(key, text)| (key, text.replace(r"\n", "\n")))
        .collect()
}

/// Speaks the first of these languages the product knows, and English
/// when it knows none of them.
///
/// They come in the person's order of preference, the way the system
/// gives them: `fr-FR`, `en-US`. A language is recognised by its whole
/// name first, then by its first part alone, so French from Canada is
/// spoken in French. Says which one it chose.
pub fn speak<S: AsRef<str>>(preferred: &[S]) -> &'static str {
    let known: Vec<&str> = languages().collect();
    let chosen = chosen(&known, preferred);
    SPEAKING.store(chosen, Ordering::Relaxed);
    known[chosen]
}

/// Which of the known languages to speak, by place.
fn chosen<S: AsRef<str>>(known: &[&str], preferred: &[S]) -> usize {
    preferred
        .iter()
        .find_map(|tag| {
            let tag = tag.as_ref().trim().to_ascii_lowercase().replace('_', "-");
            let first = tag.split('-').next().unwrap_or_default();
            known
                .iter()
                .position(|code| *code == tag)
                .or_else(|| known.iter().position(|code| *code == first))
        })
        .unwrap_or(0)
}

/// The code of the language spoken.
pub fn speaking() -> &'static str {
    LANGUAGES[spoken()].0
}

/// Every language the product speaks, English first.
pub fn languages() -> impl Iterator<Item = &'static str> {
    LANGUAGES.iter().map(|(code, _)| *code)
}

/// The text for a key, in the language spoken.
pub fn text(key: &str) -> String {
    text_with(key, &[])
}

/// The text for a key, in the language spoken, filled with these values.
pub fn text_with(key: &str, values: &[(&str, &dyn fmt::Display)]) -> String {
    match found(spoken(), key) {
        Some(text) => filled(text, |name| {
            values
                .iter()
                .find(|(named, _)| *named == name)
                .map(|(_, value)| value.to_string())
        }),
        None => key.to_string(),
    }
}

/// The words for a fact: the text its code is the key of, filled with
/// its values, and with the words for the fact that caused it where the
/// text says `{because}`.
///
/// A fact no text is written for, one a newer half of the product tells,
/// is shown the way it travelled rather than not at all: its code and its
/// values still say what happened, if not in the person's words.
pub fn fact(told: &Fact) -> String {
    fact_in(spoken(), told)
}

/// The words for a fact in that language, by its place in the list.
fn fact_in(language: usize, told: &Fact) -> String {
    match found(language, told.code()) {
        Some(text) => filled(text, |name| match told.cause() {
            Some(cause) if name == BECAUSE => Some(fact_in(language, &cause)),
            _ => told.value(name).map(str::to_string),
        }),
        None => told.to_string(),
    }
}

/// The language spoken, by its place in the list.
fn spoken() -> usize {
    SPEAKING.load(Ordering::Relaxed)
}

/// A key's text in that language, or in English when that one lacks it.
fn found(language: usize, key: &str) -> Option<&'static str> {
    let texts = &*TEXTS;
    texts[language]
        .get(key)
        .or_else(|| texts[0].get(key))
        .map(String::as_str)
}

/// A text with each `{name}` it holds replaced by the value of that name.
///
/// A name nobody gave a value for stays as it is written: a text showing
/// `{host}` says exactly what is missing, where a blank would say
/// nothing.
fn filled(text: &str, value_of: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after
            .find('}')
            .map(|close| (&after[..close], &after[close + 1..]))
        {
            Some((name, then)) if is_name(name) => {
                match value_of(name) {
                    Some(value) => out.push_str(&value),
                    None => {
                        out.push('{');
                        out.push_str(name);
                        out.push('}');
                    }
                }
                rest = then;
            }
            _ => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The text for a key, in the language spoken, filled with the values
/// named after it: `say!("menu.quit")`, `say!("session.lost", host = name)`.
///
/// The key is written out, never computed, so that the tests of this
/// brick can check every key the code asks for against the texts.
#[macro_export]
macro_rules! say {
    ($key:literal $(,)?) => {
        $crate::text($key)
    };
    ($key:literal, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::text_with(
            $key,
            &[$((stringify!($name), &$value as &dyn ::std::fmt::Display)),+],
        )
    };
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

    use zyr_proto::fact::is_code;

    use super::*;

    #[test]
    fn a_text_is_filled_with_the_values_it_names() {
        let values = |name: &str| match name {
            "host" => Some("PC-17".to_string()),
            "waited" => Some("12".to_string()),
            _ => None,
        };
        assert_eq!(
            filled("{host} did not answer in {waited} s.", values),
            "PC-17 did not answer in 12 s."
        );
        // A value nobody gave stays named, and braces that name nothing
        // stay braces.
        assert_eq!(filled("{host}:{port}", values), "PC-17:{port}");
        assert_eq!(filled("{ } {Host} {", values), "{ } {Host} {");
    }

    #[test]
    fn the_first_language_known_is_spoken_and_english_otherwise() {
        let known = ["en", "fr", "pt-br"];
        assert_eq!(chosen(&known, &["de-DE", "fr-CA"]), 1);
        assert_eq!(chosen(&known, &["pt-BR", "fr"]), 2);
        assert_eq!(chosen(&known, &["pt_BR"]), 2);
        assert_eq!(chosen(&known, &["en-GB", "fr-FR"]), 0);
        assert_eq!(chosen(&known, &["xx"]), 0);
        assert_eq!(chosen::<&str>(&known, &[]), 0);
    }

    #[test]
    fn a_fact_is_said_with_the_words_of_its_cause() {
        let refused = Fact::new("account.meeting_refused")
            .with("name", "PC-17")
            .because(&Fact::new("server.no_right"));
        assert_eq!(
            fact_in(0, &refused),
            "the server refused the session towards PC-17: no right on this computer"
        );
        let french = languages().position(|code| code == "fr").unwrap();
        assert_eq!(
            fact_in(french, &refused),
            "le serveur a refusé la session vers PC-17 : aucun droit sur cet ordinateur"
        );
    }

    #[test]
    fn a_fact_nobody_wrote_words_for_is_shown_as_it_travelled() {
        let newer: Fact = "newer.fact host=PC-17".parse().unwrap();
        assert_eq!(fact_in(0, &newer), "newer.fact host=PC-17");
    }

    #[test]
    fn english_is_the_base() {
        assert_eq!(languages().next(), Some("en"));
        assert_eq!(speaking(), "en");
    }

    /// A file's texts, read strictly: the reader the product uses passes
    /// over a line it cannot read, and these tests do not.
    fn lines_of(file: &str) -> Vec<(&str, &str)> {
        file.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(|line| {
                let (key, text) = line
                    .split_once('=')
                    .unwrap_or_else(|| panic!("a line that is not `key = text`: {line}"));
                (key.trim(), text.trim())
            })
            .collect()
    }

    fn english() -> Vec<(&'static str, &'static str)> {
        lines_of(LANGUAGES[0].1)
    }

    /// The values a text asks for.
    fn asked_for(text: &str) -> BTreeSet<&str> {
        text.split('{')
            .skip(1)
            .filter_map(|piece| piece.split_once('}'))
            .map(|(name, _)| name)
            .filter(|name| is_name(name))
            .collect()
    }

    #[test]
    fn every_file_says_each_text_once_under_a_key() {
        for (code, file) in LANGUAGES {
            let mut seen = BTreeSet::new();
            for (key, text) in lines_of(file) {
                assert!(is_code(key), "{code}: « {key} » is not a key");
                assert!(!text.is_empty(), "{code}: {key} says nothing");
                assert!(seen.insert(key), "{code}: {key} is written twice");
            }
        }
    }

    #[test]
    fn every_language_says_everything_english_says_and_nothing_more() {
        let english: BTreeSet<&str> = english().into_iter().map(|(key, _)| key).collect();
        for (code, file) in &LANGUAGES[1..] {
            let said: BTreeSet<&str> = lines_of(file).into_iter().map(|(key, _)| key).collect();
            let missing: Vec<_> = english.difference(&said).collect();
            assert!(missing.is_empty(), "{code} does not say {missing:?}");
            let more: Vec<_> = said.difference(&english).collect();
            assert!(
                more.is_empty(),
                "{code} says {more:?}, which English does not"
            );
        }
    }

    #[test]
    fn every_translation_asks_for_the_values_english_asks_for() {
        let english: HashMap<&str, &str> = english().into_iter().collect();
        for (code, file) in &LANGUAGES[1..] {
            for (key, text) in lines_of(file) {
                if let Some(base) = english.get(key) {
                    assert_eq!(
                        asked_for(text),
                        asked_for(base),
                        "{code}: {key} does not ask for the values English asks for"
                    );
                }
            }
        }
    }

    /// Every file of Rust in the workspace but this brick's own, whose
    /// examples name keys that exist nowhere else.
    fn sources() -> Vec<(PathBuf, String)> {
        let here = Path::new(env!("CARGO_MANIFEST_DIR"));
        let root = here.join("../..");
        let mut found = Vec::new();
        for folder in ["crates", "server"] {
            walk(&root.join(folder), here, &mut found);
        }
        assert!(!found.is_empty(), "the workspace's code was not found");
        found
    }

    fn walk(folder: &Path, left_out: &Path, found: &mut Vec<(PathBuf, String)>) {
        let entries =
            std::fs::read_dir(folder).unwrap_or_else(|e| panic!("{}: {e}", folder.display()));
        for entry in entries {
            let path = entry.expect("a folder lists its entries").path();
            if path.is_dir() {
                let built = path.file_name().is_some_and(|name| name == "target");
                let ours = path.canonicalize().ok() == left_out.canonicalize().ok();
                if !built && !ours {
                    walk(&path, left_out, found);
                }
            } else if path.extension().is_some_and(|ending| ending == "rs") {
                let source = std::fs::read_to_string(&path)
                    .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                found.push((path, source));
            }
        }
    }

    /// The keys written out as the first thing after `opening`:
    /// `say!(` finds `menu.quit` in `say!("menu.quit")`.
    fn written_after<'a>(source: &'a str, opening: &str) -> Vec<&'a str> {
        source
            .match_indices(opening)
            .filter_map(|(at, _)| {
                source[at + opening.len()..]
                    .trim_start()
                    .strip_prefix('"')?
                    .split_once('"')
                    .map(|(key, _)| key)
            })
            .collect()
    }

    #[test]
    fn every_text_the_code_asks_for_is_written_in_english() {
        let english: BTreeSet<&str> = english().into_iter().map(|(key, _)| key).collect();
        for (path, source) in sources() {
            // The module a fact is defined in makes some of its own to
            // test the type, and they tell nothing to anybody.
            if path.ends_with("zyr-proto/src/fact.rs") {
                continue;
            }
            let asked = written_after(&source, "say!(")
                .into_iter()
                .chain(written_after(&source, "Fact::new("));
            for key in asked {
                assert!(
                    english.contains(key),
                    "{}: « {key} » has no text in words/en.txt",
                    path.display()
                );
            }
        }
    }

    #[test]
    fn every_english_text_is_asked_for_somewhere() {
        let sources: Vec<String> = sources().into_iter().map(|(_, source)| source).collect();
        for (key, _) in english() {
            let written = format!("\"{key}\"");
            assert!(
                sources.iter().any(|source| source.contains(&written)),
                "words/en.txt says {key}, which nothing asks for"
            );
        }
    }

    #[test]
    fn the_reader_finds_a_key_however_the_call_is_laid_out() {
        let source = "say!(\"menu.quit\"); Fact::new(\n    \"reach.silent\",\n) say!(key)";
        assert_eq!(written_after(source, "say!("), ["menu.quit"]);
        assert_eq!(written_after(source, "Fact::new("), ["reach.silent"]);
    }
}
