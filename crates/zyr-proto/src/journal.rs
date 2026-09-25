//! The journal of one computer, gathered onto one page.
//!
//! Everything the product writes down lives in four files nobody should
//! have to go looking for. This puts them behind one button, under the
//! build that produced them, so that reporting a fault is one copy and
//! one paste rather than an expedition through a disk.
//!
//! The build at the top is not decoration. Two halves of the product
//! compiled at different times is the fault nobody thinks to check for
//! and the one that wastes the most time; here it is simply written
//! down, for the window and for the service, every time.
//!
//! It is gathered here rather than in the window because the same page
//! is asked for twice: once by the person sitting at the machine, and
//! once by a computer that wants to read this one's journal from where
//! it is rather than walk over. One page, one shape, whoever asks.
//!
//! What each half knows is not the same, though, and that is the whole
//! of the arrangement below: this module writes what any program on the
//! machine can say by itself, and whoever gathers the page adds the
//! lines only it holds. The service knows this computer's fingerprint
//! and who it lets in; a window with no service running knows neither,
//! and still has a journal worth reading, which is exactly when one is
//! wanted most.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::Path;

use crate::paths;
use crate::sifting::Sifting;

/// The heading line naming what can be asked for on this page.
///
/// Written by whoever gathered the page, so it names what is really in
/// that computer's files rather than what this build happens to know
/// about, and read back by the box above the button to offer them. One
/// word, defined here, so that what writes it and what reads it cannot
/// drift apart.
pub const NAMES_HEADING: &str = "Étiquettes";

/// The names a gathered page says can be asked for.
///
/// Empty for a page that carries no such line, which is a page gathered
/// by an older half of the product: the box then offers nothing and
/// everything still has to be typed, which is what it did before.
pub fn names_in(page: &str) -> Vec<String> {
    page.lines()
        .find_map(|line| line.strip_prefix(NAMES_HEADING)?.split_once(':'))
        .map(|(_, named)| {
            named
                .split(',')
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// How many lines are kept from each file.
///
/// The end of each, which is where a fault is. Enough to hold a session
/// that has just gone wrong, short enough to stay one readable paste.
const KEPT: usize = 120;

/// And how many when something is asked for.
///
/// A sift comes from someone who knows what they are looking for and has
/// already narrowed down what comes out: keeping more only lengthens the
/// page where that is exactly what was wanted. Without this, the
/// beginning of a start-up stays out of reach whatever is asked for, and
/// that is precisely what is being looked for when an opening drags.
const KEPT_WHEN_ASKED: usize = 500;

/// The files gathered, in the order they are read.
///
/// The host engine writes two: its own journal, and what its console
/// says, which is where a crash leaves its last words. The player writes
/// into the journal of the program it plays in.
const FILES: [(&str, &str); 4] = [
    ("service.log", "Le service"),
    ("engine.log", "Le moteur hôte"),
    ("engine-console.log", "La console du moteur hôte"),
    ("interface.log", "La fenêtre"),
];

/// The files emptied with the others and never gathered.
///
/// `reach.log` holds one measurement a second, which is exactly what
/// makes it worth keeping and exactly what would drown a paste meant to
/// be read in one go. `reach-distant.log` is the last such measurement
/// fetched from a far computer, and a clean slate must not leave it
/// sitting there from whatever test came before this one. Both are
/// emptied all the same: whoever clears the journal before a test wants
/// a clean slate, and would otherwise read stale minutes against a
/// session that has not even started yet.
const ALSO_EMPTIED: [(&str, &str); 2] = [
    ("reach.log", "Ce que cet ordinateur atteint"),
    (
        "reach-distant.log",
        "Ce qu'un ordinateur distant atteignait",
    ),
];

/// A journal being written.
///
/// Opened on what this computer is, filled with what the one gathering
/// it knows, and closed on the files themselves.
pub struct Journal(String);

impl Journal {
    /// Opens on what this computer is, whoever is asking.
    pub fn of_this_computer() -> Self {
        let mut text = String::new();
        let _ = writeln!(text, "{}", crate::version_line());
        let mut journal = Self(text);
        journal.says("Ordinateur", &crate::machine::name());
        journal.says("Adresses", &own_addresses());
        journal
    }

    /// One more line of the heading.
    pub fn says(&mut self, label: &str, value: &str) {
        let _ = writeln!(self.0, "{label:<17}: {value}");
    }

    /// Closes the heading, then gathers the files.
    pub fn gathered(self) -> String {
        self.sifted(&Sifting::everything())
    }

    /// The same, keeping only the lines that answer what was asked.
    ///
    /// The asking happens as the files are read and never on the page
    /// once it is made, and that is the whole of what makes it worth
    /// anything: only the end of each file reaches a page, and six lines
    /// about the clipboard are almost never among the last of a session.
    pub fn sifted(mut self, sift: &Sifting) -> String {
        self.says("Journaux", &paths::logs_dir().display().to_string());
        // Said in the heading, because a page of six lines that does not
        // say what it was sifted through reads as a product with nothing
        // to say rather than as an answer to a question.
        if !sift.takes_everything() {
            self.says("Tri", sift.said());
        }

        // The files are read before the heading is closed, so that it can
        // name what is really in them. Nothing else can: the box that
        // offers these names is on another computer half the time, and a
        // name offered that no line carries is a dead end offered.
        let mut named = BTreeSet::new();
        let mut bodies = String::new();
        for (file, what) in FILES {
            let _ = write!(bodies, "\n\n--- {what} ({file}) ---\n");
            bodies.push_str(&last_lines(
                &paths::logs_dir().join(file),
                file,
                sift,
                &mut named,
            ));
        }
        if !named.is_empty() {
            self.says(
                NAMES_HEADING,
                &named.into_iter().collect::<Vec<_>>().join(", "),
            );
        }

        let mut text = self.0;
        text.push_str(&bodies);
        text
    }
}

/// Empties everything this computer has written.
///
/// Asked for before a test, so that what comes out afterwards is that
/// test and nothing else: a journal carrying three weeks of unrelated
/// lines is a journal nobody reads to the end.
///
/// Emptied rather than deleted. The service and the engine hold these
/// files open while they run, and Windows does not let go of a file
/// somebody is writing to; emptying works all the same, the next line
/// appended landing at the start of a file that is now blank.
///
/// Answers what could not be emptied, said in words meant to be read.
pub fn emptied() -> Vec<String> {
    let mut refused = Vec::new();
    for (file, what) in FILES.iter().chain(ALSO_EMPTIED.iter()) {
        if let Err(e) = empty(&paths::logs_dir().join(file)) {
            refused.push(format!("{what} ({file}) : {e}"));
        }
    }
    refused
}

/// Empties one file. One that was never written is already empty.
fn empty(path: &Path) -> std::io::Result<()> {
    match std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(path)
    {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// Where this computer answers, card by card.
///
/// Two machines that never find each other are almost always two
/// machines on two different networks, and nothing else in a journal
/// says so. Written down here so the answer travels with the journal
/// instead of costing an evening and a command to go and fetch.
fn own_addresses() -> String {
    let answering = crate::machine::addresses();
    if answering.is_empty() {
        return "aucune".to_string();
    }
    answering
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// The end of a file, or a word saying why there is none.
///
/// Only the end is ever read from disk. A log can have grown for months,
/// and reading the whole of it to keep a hundred lines would hold the
/// program on a file nobody asked to see all of.
///
/// `within` is what the file is called, which stands in for a tag on the
/// lines that carry none: a console writes in no shape of ours, and this
/// is what lets it be asked for whole.
///
/// Every name met on the way is put in `named`, whether or not its line
/// survives the sifting. That is the whole point of collecting them
/// here: what can be asked for is what the files hold, not what is left
/// once the asking has been done.
fn last_lines(path: &Path, within: &str, sift: &Sifting, named: &mut BTreeSet<String>) -> String {
    use std::io::{Read, Seek, SeekFrom};

    // How much of the end is read, at most. Far more than the lines
    // kept can need, so the cap never shows in an ordinary journal.
    //
    // Wider when something is being asked for, and by a good deal: what
    // is asked for is rare by definition, and a hundred lines about the
    // clipboard are spread across a session's whole journal rather than
    // sitting at the end of it.
    const READ_AT_MOST: u64 = 256 * 1024;
    const READ_AT_MOST_WHEN_ASKED: u64 = 4 * 1024 * 1024;
    let read_at_most = if sift.takes_everything() {
        READ_AT_MOST
    } else {
        READ_AT_MOST_WHEN_ASKED
    };

    let read = std::fs::File::open(path).and_then(|mut file| {
        let written = file.metadata()?.len();
        let skipped = written.saturating_sub(read_at_most);
        file.seek(SeekFrom::Start(skipped))?;
        let mut end = Vec::new();
        file.read_to_end(&mut end)?;
        Ok((skipped, end))
    });
    let (skipped, end) = match read {
        Ok(read) => read,
        // Not written yet, most of the time: a computer that has never
        // hosted has no host engine log, and that is worth saying rather
        // than leaving an empty gap. Anything else is worth its reason:
        // an existing file that cannot be read is not « nothing ».
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return "(rien d'écrit pour l'instant)".to_string();
        }
        Err(e) => return format!("(illisible : {e})"),
    };

    // Read as it comes, accents or not: what a console wrote in another
    // encoding is shown with holes rather than refused whole.
    let text = String::from_utf8_lossy(&end);
    let lines: Vec<&str> = text.lines().collect();
    // The first line of a cut read is half a line: dropped with the rest
    // of the beginning.
    let whole = if skipped > 0 && !lines.is_empty() {
        &lines[1..]
    } else {
        &lines[..]
    };
    // Asked of every line read and not of the ones kept, which is the
    // point: what is being looked for is rare, and a file's last hundred
    // and twenty lines almost never hold it.
    //
    // A line with no name of its own answers to the file it is in, so
    // that is what goes in the list for it: a console writes a whole
    // file that way, which would otherwise be impossible to ask for from
    // a list of names.
    let stem = within.strip_suffix(".log").unwrap_or(within);
    let mut answered: Vec<&str> = Vec::new();
    // The names in this file itself, to know what to say if it gives
    // nothing back.
    let mut its_own = BTreeSet::new();
    for line in whole {
        let name = crate::sifting::about(line)
            .map_or(stem, |(_, tag)| tag)
            .to_lowercase();
        its_own.insert(name.clone());
        named.insert(name);
        if sift.keeps(line, within) {
            answered.push(line);
        }
    }
    if answered.is_empty() && !sift.takes_everything() {
        return nothing_here(sift, stem, &its_own);
    }
    let kept = if sift.takes_everything() {
        KEPT
    } else {
        KEPT_WHEN_ASKED
    };
    let from = answered.len().saturating_sub(kept);
    let mut kept = answered[from..].join("\n");
    if from > 0 || skipped > 0 {
        kept.insert_str(0, "(le début n'est pas montré)\n");
    }
    kept
}

/// What to say of a file that answered nothing, which is not always the
/// same news.
///
/// A file whose lines carry no name answers to its own, and to that
/// alone. Asking for "panicked" therefore leaves out the engine's
/// console entirely, including the lines that say it, and the page used
/// to say "rien ici ne répond au tri": that reads like a file that said
/// nothing, when it had said everything. An evening's hunt went into it.
/// The sentence now names the word to add.
fn nothing_here(sift: &Sifting, stem: &str, its_own: &BTreeSet<String>) -> String {
    let nameless = its_own.len() == 1 && its_own.contains(stem);
    if sift.asks_for_a_name() && nameless {
        return format!(
            "(rien ici ne répond au tri ; les lignes de ce fichier ne portent pas de nom et \
             répondent au sien : ajoutez « {stem} » au tri pour les lire)"
        );
    }
    "(rien ici ne répond au tri)".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The names met on the way are not what these tests are about.
    fn read(path: &Path, within: &str, sift: &Sifting) -> String {
        last_lines(path, within, sift, &mut BTreeSet::new())
    }

    /// Nothing asked for, so everything kept.
    fn everything() -> Sifting {
        Sifting::everything()
    }

    fn a_folder_of_its_own(what: &str) -> std::path::PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "zyrdesk-journal-{what}-{}",
            crate::random::alphanumeric_string(8)
        ));
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn a_file_that_does_not_exist_is_said_rather_than_left_blank() {
        let nowhere = Path::new("/nowhere/zyrdesk/none.log");
        assert!(read(nowhere, "none", &everything()).contains("rien d'écrit"));
    }

    #[test]
    fn only_the_end_of_a_long_file_is_kept_and_it_says_so() {
        let folder = a_folder_of_its_own("long");
        let path = folder.join("long.log");

        let written: Vec<String> = (0..KEPT + 40).map(|line| format!("ligne {line}")).collect();
        std::fs::write(&path, written.join("\n")).unwrap();

        let kept = read(&path, "essai", &everything());
        // The end, which is where the fault is, and never the
        // beginning.
        assert!(kept.ends_with(&format!("ligne {}", KEPT + 39)), "{kept}");
        assert!(!kept.contains("ligne 0\n"), "{kept}");
        // And what was left out is announced: a journal cut short in
        // silence reads like a complete one.
        assert!(kept.starts_with("(le début n'est pas montré)"), "{kept}");

        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_huge_file_costs_only_its_end() {
        let folder = a_folder_of_its_own("enorme");
        let path = folder.join("enorme.log");

        // Well beyond what the reading allows itself: if it read
        // everything, this test would show on the stopwatch and in the
        // memory.
        let mut written = String::new();
        for line in 0..40_000 {
            written.push_str(&format!("ligne {line} avec un peu de matière autour\n"));
        }
        std::fs::write(&path, &written).unwrap();

        let kept = read(&path, "essai", &everything());
        assert!(
            kept.ends_with("ligne 39999 avec un peu de matière autour"),
            "fin : {}",
            &kept[kept.len().saturating_sub(80)..]
        );
        assert!(
            kept.starts_with("(le début n'est pas montré)"),
            "{}",
            &kept[..60]
        );
        // Never half a line at the top after the cut.
        let second = kept.lines().nth(1).unwrap();
        assert!(second.starts_with("ligne "), "{second}");

        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_nameless_file_says_which_word_to_add_to_the_sift() {
        // The exact fault: asking for "panicked" leaves out the
        // engine's console entirely, because its lines carry no name
        // and answer to its own. The page used to say "rien ici ne
        // répond au tri", which reads like a file that said nothing
        // when it had said everything.
        let folder = a_folder_of_its_own("sans-nom");

        // A console: no line has a tag.
        let console = folder.join("engine-console.log");
        std::fs::write(
            &console,
            "thread 'zyr-host-pictures' panicked at src/pipeline.rs:512:9\n",
        )
        .unwrap();
        let shown = read(&console, "engine-console.log", &Sifting::of("panicked"));
        assert!(
            shown.contains("ajoutez « engine-console » au tri"),
            "{shown}"
        );

        // A file whose lines carry names says nothing of the kind:
        // adding its own name to it would change nothing.
        let service = folder.join("service.log");
        std::fs::write(&service, "2026-09-15 18:30:18 I [way] voie 1 ouverte\n").unwrap();
        let shown = read(&service, "service.log", &Sifting::of("panicked"));
        assert_eq!(shown, "(rien ici ne répond au tri)", "{shown}");

        // And the sift that does name the file gives it
        // back.
        let shown = read(
            &console,
            "engine-console.log",
            &Sifting::of("panicked engine-console"),
        );
        assert!(shown.contains("pipeline.rs"), "{shown}");

        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn the_sift_happens_on_reading_and_not_on_the_page() {
        // That is the whole difference: only the last hundred and
        // twenty lines of a file reach a page, and six lines about
        // the clipboard are almost never among the last hundred and
        // twenty of a session.
        let folder = a_folder_of_its_own("tri");
        let path = folder.join("service.log");

        let mut written = String::new();
        written.push_str("2026-09-11 18:55:03 I [clipboard] ce que tient cet ordinateur\n");
        for line in 0..KEPT + 40 {
            let _ = writeln!(written, "2026-09-11 18:55:04 I [ways] voie {line} ouverte");
        }
        std::fs::write(&path, &written).unwrap();

        // Without a sift, the line at the beginning is out
        // of reach.
        let unsifted = read(&path, "service", &everything());
        assert!(!unsifted.contains("ce que tient"), "{unsifted}");

        // With one, it is the only one left.
        let sifted = read(&path, "service", &Sifting::of("tag:clipboard"));
        assert_eq!(
            sifted,
            "2026-09-11 18:55:03 I [clipboard] ce que tient cet ordinateur"
        );

        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn what_can_be_asked_for_is_read_from_the_files_and_read_back_from_the_heading() {
        // The sift box cannot guess it: half the time the page comes
        // from another computer, and a name offered that no line carries
        // is a dead end offered.
        let folder = a_folder_of_its_own("noms");
        let path = folder.join("service.log");
        let mut written = String::new();
        let _ = writeln!(written, "2026-09-11 18:55:03 I [clipboard] ce qu'il tient");
        let _ = writeln!(written, "2026-09-11 18:55:04 I [way] voie 1 ouverte");
        std::fs::write(&path, &written).unwrap();

        let mut named = BTreeSet::new();
        last_lines(&path, "service.log", &Sifting::of("clipboard"), &mut named);
        // Collected even when the sift leaves them out: what can be
        // asked for is what the files carry, not what is left once the
        // asking is done.
        assert_eq!(
            named.iter().cloned().collect::<Vec<_>>(),
            ["clipboard", "way"]
        );

        // A line with no name answers to the name of its file, otherwise
        // a console could not be asked for from a list.
        let console = folder.join("engine-console.log");
        std::fs::write(&console, "thread 'main' panicked at src/lib.rs:1:1\n").unwrap();
        let mut named = BTreeSet::new();
        last_lines(
            &console,
            "engine-console.log",
            &Sifting::everything(),
            &mut named,
        );
        assert_eq!(
            named.iter().cloned().collect::<Vec<_>>(),
            ["engine-console"]
        );

        // And what the heading writes, the box reads back as it
        // is.
        let mut journal = Journal(String::new());
        journal.says(NAMES_HEADING, "clipboard, files, way");
        assert_eq!(names_in(&journal.0), ["clipboard", "files", "way"]);
        // A page from an older half of the product carries none: the box
        // then offers nothing and everything is typed, as before.
        assert!(names_in("Ordinateur       : PC-SAV").is_empty());

        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_sift_that_gives_nothing_says_so_rather_than_leaving_a_blank() {
        // A blank reads like an empty file, and the question becomes
        // "does it work?" instead of "there was nothing".
        let folder = a_folder_of_its_own("tri-vide");
        let path = folder.join("service.log");
        std::fs::write(&path, "2026-09-11 18:55:04 I [ways] voie 1 ouverte\n").unwrap();

        let sifted = read(&path, "service", &Sifting::of("tag:clipboard"));
        assert!(sifted.contains("rien ici ne répond au tri"), "{sifted}");

        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_sifted_page_says_which_sift_it_was_taken_under() {
        // Otherwise it reads as a product with nothing to say rather
        // than as the answer to a question.
        let text = Journal::of_this_computer().sifted(&Sifting::of("tag:clipboard"));
        assert!(text.contains("Tri "), "{}", &text[..400]);
        assert!(text.contains("tag:clipboard"), "{}", &text[..400]);
        // And a page that was not sifted does not carry the
        // line at all.
        assert!(!Journal::of_this_computer().gathered().contains("\nTri "));
    }

    #[test]
    fn an_unreadable_file_says_so_rather_than_nothing() {
        // A folder cannot be read like a file: that is the portable
        // way of getting a refusal that is not "missing".
        let folder = a_folder_of_its_own("illisible");
        let refused = read(&folder, "essai", &everything());
        assert!(refused.starts_with("(illisible"), "{refused}");
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn the_heading_lines_up() {
        // Counted in characters and not in bytes: "ô" takes up two, and
        // a column measured in bytes would think itself crooked where
        // it is perfectly straight.
        let mut journal = Journal(String::new());
        journal.says("Service", "en marche");
        journal.says("Accès distant", "activé");
        let columns: Vec<usize> = journal
            .0
            .lines()
            .map(|line| {
                line.chars()
                    .position(|c| c == ':')
                    .expect("un séparateur par ligne")
            })
            .collect();
        assert_eq!(columns[0], columns[1], "{}", journal.0);
    }

    #[test]
    fn a_journal_opens_on_the_build_and_the_computer() {
        // Two halves of the product compiled at different times is the
        // fault nobody thinks to check for: it is on the first line,
        // before anything else.
        let text = Journal::of_this_computer().gathered();
        let mut lines = text.lines();
        assert_eq!(lines.next().unwrap(), crate::version_line());
        assert!(lines.next().unwrap().starts_with("Ordinateur"), "{text}");
        // And the four files are there, named, even the ones this
        // computer has never written.
        for (file, what) in FILES {
            assert!(text.contains(&format!("--- {what} ({file}) ---")), "{text}");
        }
    }

    #[test]
    fn what_is_emptied_covers_more_than_what_is_gathered() {
        // The record of what the computer reaches holds one measurement
        // a second: it would drown the copy that is read in one go, so
        // it is not in it. But emptying the journal before a test must
        // empty it too, otherwise three weeks of records get read
        // against a five-minute session.
        let gathered: Vec<&str> = FILES.iter().map(|(file, _)| *file).collect();
        let also: Vec<&str> = ALSO_EMPTIED.iter().map(|(file, _)| *file).collect();
        assert!(also.contains(&"reach.log"));
        assert!(also.contains(&"reach-distant.log"));
        assert!(
            !gathered.contains(&"reach.log"),
            "le relevé noierait la copie qu'on relit"
        );
        for file in &also {
            assert!(!gathered.contains(file), "{file} serait vidé deux fois");
        }
    }

    #[test]
    fn a_file_that_was_never_written_is_already_empty() {
        // Emptying the journal of a computer that has never hosted must
        // not complain about the file the host engine never opened.
        let folder = a_folder_of_its_own("vidage");
        assert!(empty(&folder.join("jamais.log")).is_ok());

        let path = folder.join("plein.log");
        std::fs::write(&path, "trois semaines de lignes\n").unwrap();
        assert!(empty(&path).is_ok());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "");

        std::fs::remove_dir_all(&folder).unwrap();
    }
}
