//! Asking a journal for the lines that are about one thing.
//!
//! A session writes four thousand lines. Six of them are about the
//! clipboard, and the difference between reading those six and reading
//! all four thousand is whether they can be asked for by name. Every
//! line the product writes carries a tag saying which part of it wrote
//! the line; this is the other half, the asking.
//!
//! # What can be written in the box
//!
//! A plain word is the name of a part of the product, and that is nine
//! askings out of ten: `clipboard` keeps the lines that part wrote.
//! Lines a console writes carry no name of their own, so the file they
//! are in stands in for one, and `engine-console` is the host engine's
//! console whole.
//!
//! Several of them keep **one or the other**, because a line carries one
//! name and never two: `clipboard files` is both subjects at once, which
//! is the whole reason for typing two. Everything else piles up as
//! « and », so `clipboard -level:debug` is that part of the product with
//! its hunting lines left out.
//!
//! The rest is there for when a plain word is not enough:
//!
//! - `tag:clipboard` is the long way of writing `clipboard`, and means
//!   exactly the same thing.
//! - `"deux mots"` or `message:refusé` looks inside the line rather than
//!   at its name. Quotes are what tells one from the other: quoted is
//!   text to find, bare is a name.
//! - a word carrying a colon, like `192.168.1.5:47000`, is an address, a
//!   time or one of this product's own spellings, never the name of a
//!   part of it, so it too is looked for inside the line.
//! - `level:debug` keeps only what was written for a hunt, and
//!   `-level:debug` throws all of it away. There are two levels and no
//!   more: what the product says of itself, and what only a hunt wants.
//!   The second is written by a build made for hunting and by no other,
//!   so in an ordinary build there is none of it to ask for.
//! - a `-` in front of anything throws away what it names instead of
//!   keeping it.
//!
//! Nothing matches by case, since nobody remembers the case of a name
//! they read once.
//!
//! An empty box is not a filter that keeps nothing: it is no filter at
//! all, and the whole journal comes out, which is what it has always
//! done.

use std::fmt;

/// How wide a written timestamp is, which is where a tag starts.
///
/// Read from the shape `log` writes rather than counted here twice: the
/// two have to agree, and a test below is what says they do.
const AFTER_THE_DATE: usize = "2026-09-11 18:55:03".len();

/// What one thing written in the box asks of a line.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Term {
    /// Which part of the line it is asked of.
    of: Part,
    /// What is looked for there, already lowered.
    wanted: String,
    /// Whether finding it throws the line away rather than keeping it.
    against: bool,
}

/// Which part of a line a term is asked of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    /// What the line is filed under, or the file it is in when it is
    /// filed under nothing.
    Tag,
    /// Which of the two voices it was written in.
    Level,
    /// The line itself, tag and date and all.
    Anything,
}

/// What was asked of a journal, ready to be asked of each line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sifting {
    terms: Vec<Term>,
    /// What was written, kept word for word so that a journal can say
    /// what it was sifted through. A page of six lines that does not
    /// say it was sifted reads as a product that did nothing.
    said: String,
}

impl Sifting {
    /// Nothing asked, so everything kept.
    pub fn everything() -> Self {
        Self::default()
    }

    /// Reads what was written in the box.
    ///
    /// Nothing here can be wrong: what is not one of the shapes above is
    /// read as the name of a part of the product, which is the useful
    /// reading of a mistyped one, since a name nothing carries simply
    /// keeps nothing. A box that refused what was typed would be a box
    /// that argues instead of answering.
    pub fn of(said: &str) -> Self {
        let mut terms = Vec::new();
        for piece in pieces(said) {
            let (against, piece) = match piece.strip_prefix('-') {
                Some(rest) => (true, rest.to_string()),
                None => (false, piece),
            };
            let (key, after) = match piece.split_once(':') {
                Some((key, rest)) => (key.to_lowercase(), Some(rest)),
                None => (String::new(), None),
            };
            let (of, wanted) = match (key.as_str(), after) {
                ("tag", Some(wanted)) => (Part::Tag, wanted),
                ("level", Some(wanted)) => (Part::Level, wanted),
                ("message", Some(wanted)) => (Part::Anything, wanted),
                // A colon inside a word is an address, a time or a
                // spelling this product writes everywhere, and never the
                // name of one of its parts: looked for inside the line
                // as it stands rather than read as a key nobody meant.
                (_, Some(_)) => (Part::Anything, piece.as_str()),
                // Quotes say « these words, somewhere in the line », and
                // that is what makes them worth typing at all now that a
                // bare word means something else.
                (_, None) if piece.starts_with('"') => (Part::Anything, piece.as_str()),
                // And a plain word names a part of the product, which is
                // nine askings out of ten and therefore costs nothing.
                (_, None) => (Part::Tag, piece.as_str()),
            };
            let wanted = unquoted(wanted).to_lowercase();
            if !wanted.is_empty() {
                terms.push(Term {
                    of,
                    wanted,
                    against,
                });
            }
        }
        Self {
            terms,
            said: said.trim().to_string(),
        }
    }

    /// Whether nothing was asked, so that nothing has to be read line by
    /// line.
    pub fn takes_everything(&self) -> bool {
        self.terms.is_empty()
    }

    /// Whether that line answers what was asked of it.
    ///
    /// `within` is the file the line is in, and it counts as a name of
    /// its own beside the one the line carries. Two reasons, and both
    /// matter: a console writes in no shape of ours and carries no name
    /// at all, so the file is the only one it has; and asking for one
    /// whole file is a thing somebody wants often enough that it should
    /// not need a second word for it.
    ///
    /// The names asked for are weighed together as « one or the other »,
    /// and everything else as « and ». A line carries one name: asking
    /// for two and keeping what answers both would keep nothing at all,
    /// which is the opposite of what somebody typing two of them wants.
    pub fn keeps(&self, line: &str, within: &str) -> bool {
        let lowered = line.to_lowercase();
        let (level, tag) = about(line).unwrap_or((SOMEBODY_ELSE, ""));
        let tag = tag.to_lowercase();
        let within = within.to_lowercase();
        let found = |term: &Term| match term.of {
            Part::Tag => tag.contains(&term.wanted) || within.contains(&term.wanted),
            Part::Level => level.starts_with(&term.wanted),
            Part::Anything => lowered.contains(&term.wanted),
        };

        let a_name_asked_for = |term: &&Term| term.of == Part::Tag && !term.against;
        let mut asked = self.terms.iter().filter(a_name_asked_for).peekable();
        if asked.peek().is_some() && !asked.any(&found) {
            return false;
        }
        self.terms
            .iter()
            .filter(|term| !a_name_asked_for(term))
            .all(|term| found(term) != term.against)
    }

    /// What was asked, as it was written.
    pub fn said(&self) -> &str {
        &self.said
    }

    /// Whether a name was asked for at all.
    ///
    /// Read by whoever has to explain an empty answer: a file that holds
    /// nothing because a name was asked for and its lines carry none is a
    /// very different thing from a file that holds nothing at all, and
    /// the two said the same sentence.
    pub fn asks_for_a_name(&self) -> bool {
        self.terms
            .iter()
            .any(|term| term.of == Part::Tag && !term.against)
    }
}

impl fmt::Display for Sifting {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.said)
    }
}

/// What a line says of itself: the voice it was written in and what it
/// is filed under.
///
/// Read at the one place they can be rather than hunted for: both sit
/// straight after the date, in that order, so a letter or a bracket
/// anywhere in the message is part of the message and nothing else.
///
/// Nothing at all for a line a console wrote, which carries neither.
pub(crate) fn about(line: &str) -> Option<(&'static str, &str)> {
    let rest = line.get(AFTER_THE_DATE..)?.strip_prefix(' ')?;
    let (voice, rest) = rest.split_at_checked(1)?;
    let rest = rest.strip_prefix(" [")?;
    let (tag, _) = rest.split_once(']')?;
    if tag.is_empty() || tag.contains(' ') {
        return None;
    }
    Some((named(voice), tag))
}

/// What a voice is called, in the word somebody would type.
///
/// Lines a console wrote are neither, and are called so: asking for one
/// level or the other must not quietly hand over a third kind.
const SOMEBODY_ELSE: &str = "engine";

fn named(voice: &str) -> &'static str {
    // Read from the letters the journal writes rather than spelled out
    // again here: two lists of the same letters drift apart the first
    // time one of them is touched, and nothing would say so.
    match voice.chars().next() {
        Some(crate::log::HUNTS) => "debug",
        Some(crate::log::SAYS) => "info",
        _ => SOMEBODY_ELSE,
    }
}

/// What was written, cut into things, quotes holding their spaces.
fn pieces(said: &str) -> Vec<String> {
    let mut pieces = Vec::new();
    let mut piece = String::new();
    let mut quoted = false;
    for letter in said.chars() {
        match letter {
            '"' => {
                quoted = !quoted;
                piece.push(letter);
            }
            letter if letter.is_whitespace() && !quoted => {
                if !piece.is_empty() {
                    pieces.push(std::mem::take(&mut piece));
                }
            }
            letter => piece.push(letter),
        }
    }
    if !piece.is_empty() {
        pieces.push(piece);
    }
    pieces
}

/// The same, without the quotes that were holding its spaces together.
fn unquoted(said: &str) -> &str {
    said.strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(said)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A line the way the journal writes them, in the ordinary
    /// voice.
    fn a_line(tag: &str, message: &str) -> String {
        format!("2026-09-11 18:55:03 I [{tag}] {message}")
    }

    /// The same, in the one only a hunt wants.
    fn a_debug_line(tag: &str, message: &str) -> String {
        format!("2026-09-11 18:55:03 D [{tag}] {message}")
    }

    #[test]
    fn an_empty_box_sifts_nothing() {
        // This is what keeps the "Copier" button copying everything for
        // as long as nothing has been asked of it.
        let sift = Sifting::of("   ");
        assert!(sift.takes_everything());
        assert!(sift.keeps(&a_line("clipboard", "peu importe"), "service"));
    }

    #[test]
    fn a_tag_asked_for_leaves_out_everything_else() {
        // The bare word and the long form say the same thing: the
        // second is what was typed for weeks, and nothing that was
        // learnt must stop working.
        for said in ["clipboard", "tag:clipboard"] {
            let sift = Sifting::of(said);
            assert!(!sift.takes_everything(), "{said}");
            assert!(
                sift.keeps(
                    &a_line("clipboard", "ce que tient cet ordinateur"),
                    "service"
                ),
                "{said}"
            );
            assert!(
                !sift.keeps(&a_line("ways", "voie 1 ouverte"), "service"),
                "{said}"
            );
        }
    }

    #[test]
    fn several_tags_keep_either_one() {
        // A line carries only one tag: requiring them all would never
        // keep anything, which is the opposite of what someone typing
        // two of them wants.
        let sift = Sifting::of("clipboard files");
        assert!(sift.keeps(
            &a_line("clipboard", "ce que tient cet ordinateur"),
            "service"
        ));
        assert!(sift.keeps(&a_line("files", "1 fichier de 4,7 Go arrive"), "service"));
        assert!(!sift.keeps(&a_line("way", "voie 1 ouverte"), "service"));
    }

    #[test]
    fn a_tag_and_an_exclusion_add_up() {
        // Tags among themselves make "one or the other", everything
        // else makes "and": that is what makes "the subject, without
        // the known noise" possible in two words.
        let sift = Sifting::of("clipboard files -level:debug");
        assert!(sift.keeps(&a_line("files", "1 fichier arrive"), "service"));
        assert!(!sift.keeps(&a_debug_line("files", "morceau 12 demandé"), "service"));
        assert!(!sift.keeps(&a_line("way", "voie 1 ouverte"), "service"));
    }

    #[test]
    fn case_does_not_matter() {
        // Nobody remembers the case of a tag they read once.
        let sift = Sifting::of("TAG:ClipBoard");
        assert!(sift.keeps(&a_line("clipboard", "quoi que ce soit"), "service"));
    }

    #[test]
    fn the_file_name_counts_as_a_tag() {
        // A console writes its own way and carries no tag: without
        // this, it could not be asked for at all. And asking for a whole
        // file is a common enough thing not to deserve a second word.
        let sift = Sifting::of("tag:engine-console");
        let console = "thread 'zyr-host-pictures' panicked at src/pipeline.rs:512:9";
        assert!(sift.keeps(console, "engine-console.log"));
        assert!(!sift.keeps(console, "service.log"));
        // And a tagged line stays in its file: the two names go
        // together rather than one in place of the other.
        let ours = a_line("clipboard", "ce que tient cet ordinateur");
        assert!(sift.keeps(&ours, "engine-console.log"));
        assert!(Sifting::of("tag:clipboard").keeps(&ours, "service.log"));
    }

    #[test]
    fn a_word_in_the_line_is_asked_for_in_quotes_or_by_its_name() {
        // The two forms of the text search, now that a bare word names a
        // part of the product.
        for sift in [
            Sifting::of("\"DataObject\""),
            Sifting::of("message:DataObject"),
        ] {
            assert!(sift.keeps(&a_line("clipboard", "il tient DataObject"), "service"));
            assert!(!sift.keeps(&a_line("clipboard", "il tient du texte"), "service"));
        }
    }

    #[test]
    fn a_tag_and_an_excluded_word_add_up() {
        // This is what makes a pair like this one useful: the name for
        // the subject, the minus for the known noise.
        let sift = Sifting::of("clipboard -\"DataObject\"");
        assert!(sift.keeps(&a_line("clipboard", "15997 octets de texte"), "service"));
        assert!(!sift.keeps(&a_line("clipboard", "il tient DataObject"), "service"));
        assert!(!sift.keeps(&a_line("ways", "15997 octets de texte"), "service"));
    }

    #[test]
    fn what_is_in_quotes_is_a_single_thing() {
        let sift = Sifting::of("\"deux mots\"");
        assert!(sift.keeps(&a_line("ways", "voici deux mots ici"), "service"));
        assert!(!sift.keeps(&a_line("ways", "deux, puis mots"), "service"));
    }

    #[test]
    fn a_colon_inside_a_word_stays_in_the_word() {
        // This product writes addresses and times everywhere: reading
        // them as an unknown key would never give anything back.
        let sift = Sifting::of("192.168.1.5:57577");
        assert!(sift.keeps(&a_line("ways", "card 192.168.1.5:57577 sondée"), "service"));
    }

    #[test]
    fn the_tag_is_read_where_the_journal_writes_it() {
        // And nowhere else: a bracket in the message is a bracket in
        // the message.
        let written = a_line("clipboard", "[pas une étiquette] la suite");
        assert_eq!(about(&written), Some(("info", "clipboard")));
        assert_eq!(about("[hevc @ 0x55d4c1a2e340] rien de décodé"), None);
    }

    #[test]
    fn a_voice_is_asked_for_by_its_name_or_its_letter() {
        // What is only there for a hunt drowns everything else: it
        // must be possible to keep only that, or everything but that.
        let debug = a_debug_line("way", "pas un paquet depuis 1098 ms");
        let info = a_line("way", "voie 1 ouverte vers PC-SAV");

        assert!(Sifting::of("level:debug").keeps(&debug, "service.log"));
        assert!(!Sifting::of("level:debug").keeps(&info, "service.log"));
        assert!(Sifting::of("-level:debug").keeps(&info, "service.log"));
        // The letter is enough, and case does not count.
        assert!(Sifting::of("level:D").keeps(&debug, "service.log"));

        // And what a console writes is neither one nor the other:
        // asking for a voice must not quietly hand back a third kind
        // of line.
        let console = "thread 'zyr-host-pictures' panicked at src/pipeline.rs:512:9";
        assert!(!Sifting::of("level:debug").keeps(console, "engine-console.log"));
        assert!(!Sifting::of("level:info").keeps(console, "engine-console.log"));
        assert!(Sifting::of("level:engine").keeps(console, "engine-console.log"));
    }

    #[test]
    fn what_was_written_reads_back_as_it_was() {
        // The page will say which sift it was taken through: a page of
        // six lines that does not say so reads as a product that is
        // mute.
        let sift = Sifting::of("  tag:clipboard -DataObject  ");
        assert_eq!(sift.said(), "tag:clipboard -DataObject");
        assert_eq!(sift.to_string(), "tag:clipboard -DataObject");
    }

    #[test]
    fn the_tag_is_found_where_the_journal_really_wrote_it() {
        // The two halves hold each other up: if the journal changes the
        // shape of its date, the tag stops being where it is looked
        // for, and nothing sifts any more. Written for real and read
        // back, rather than copied out from memory here.
        let folder = std::env::temp_dir().join(format!(
            "zyrdesk-tamis-{}",
            crate::random::alphanumeric_string(8)
        ));
        let path = folder.join("service.log");
        let log = crate::log::Log::open(&path).unwrap();
        log.about("clipboard").write("ce que tient cet ordinateur");

        let written = std::fs::read_to_string(&path).unwrap();
        let line = written.lines().next().unwrap();
        assert_eq!(about(line), Some(("info", "clipboard")), "{line}");
        assert!(Sifting::of("tag:clipboard").keeps(line, "service"));

        std::fs::remove_dir_all(&folder).ok();
    }
}
