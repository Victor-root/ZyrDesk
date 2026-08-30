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

use std::fmt::Write as _;
use std::path::Path;

use crate::paths;

/// How many lines are kept from each file.
///
/// The end of each, which is where a fault is. Enough to hold a session
/// that has just gone wrong, short enough to stay one readable paste.
const KEPT: usize = 120;

/// The files gathered, in the order they are read.
const FILES: [(&str, &str); 4] = [
    ("service.log", "Le service"),
    ("session.log", "Le moteur client"),
    ("engine-console.log", "Le moteur hôte"),
    ("interface.log", "La fenêtre"),
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

    /// Closes the heading on the engines in place, then gathers the
    /// files.
    pub fn gathered(mut self) -> String {
        let here = |present: bool| if present { "présent" } else { "absent" };
        self.says("Moteur hôte", here(paths::host_engine_exe().is_file()));
        self.says("Moteur client", here(paths::client_engine_exe().is_file()));
        let engines = engines_build();
        if !engines.is_empty() {
            self.says("Moteurs", &engines);
        }
        self.says("Journaux", &paths::logs_dir().display().to_string());

        let mut text = self.0;
        for (file, what) in FILES {
            let _ = write!(text, "\n\n--- {what} ({file}) ---\n");
            text.push_str(&last_lines(&paths::logs_dir().join(file)));
        }
        text
    }
}

/// Empties everything this computer has written.
///
/// Asked for before a test, so that what comes out afterwards is that
/// test and nothing else: a journal carrying three weeks of unrelated
/// lines is a journal nobody reads to the end.
///
/// Emptied rather than deleted. The service and the engines hold these
/// files open while they run, and Windows does not let go of a file
/// somebody is writing to; emptying works all the same, the next line
/// appended landing at the start of a file that is now blank.
///
/// Answers what could not be emptied, said in words meant to be read.
pub fn emptied() -> Vec<String> {
    let mut refused = Vec::new();
    for (file, what) in FILES {
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

/// Which build produced the engines sitting on this machine.
///
/// Written by the script that fetches them. Without it, an engine that
/// is present says nothing about whether it is the one this code
/// expects, and the two drift apart in silence: the engines are the one
/// half of the product that a `git pull` does not carry.
fn engines_build() -> String {
    match std::fs::read_to_string(paths::engines_dir().join("build.txt")) {
        Ok(text) => build_from(&text),
        // No file at all: engines put there by hand, which stays
        // perfectly valid and simply says nothing about where they came
        // from.
        Err(_) => String::new(),
    }
}

/// What that file says, kept apart from the disk so that what the script
/// writes and what is read here can be checked against each other.
fn build_from(text: &str) -> String {
    let said = |key: &str| {
        text.lines()
            .filter(|line| !line.trim_start().starts_with('#'))
            .filter_map(|line| line.split_once('='))
            .find(|(name, _)| name.trim() == key)
            .map(|(_, value)| value.trim().to_string())
    };
    match (said("run"), said("date")) {
        (Some(run), Some(date)) => format!("compilation {run} du {date}"),
        (Some(run), None) => format!("compilation {run}"),
        _ => String::new(),
    }
}

/// The end of a file, or a word saying why there is none.
///
/// Only the end is ever read from disk. A log can have grown for months,
/// and reading the whole of it to keep a hundred lines would hold the
/// program on a file nobody asked to see all of.
fn last_lines(path: &Path) -> String {
    use std::io::{Read, Seek, SeekFrom};

    // How much of the end is read, at most. Far more than the lines
    // kept can need, so the cap never shows in an ordinary journal.
    const READ_AT_MOST: u64 = 256 * 1024;

    let read = std::fs::File::open(path).and_then(|mut file| {
        let written = file.metadata()?.len();
        let skipped = written.saturating_sub(READ_AT_MOST);
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

    // Read as it comes, accents or not: a log the engine wrote in
    // another encoding is shown with holes rather than refused whole.
    let text = String::from_utf8_lossy(&end);
    let lines: Vec<&str> = text.lines().collect();
    // The first line of a cut read is half a line: dropped with the rest
    // of the beginning.
    let whole = if skipped > 0 && !lines.is_empty() {
        &lines[1..]
    } else {
        &lines[..]
    };
    let from = whole.len().saturating_sub(KEPT);
    let mut kept = whole[from..].join("\n");
    if from > 0 || skipped > 0 {
        kept.insert_str(0, "(le début n'est pas montré)\n");
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(last_lines(nowhere).contains("rien d'écrit"));
    }

    #[test]
    fn only_the_end_of_a_long_file_is_kept_and_it_says_so() {
        let folder = a_folder_of_its_own("long");
        let path = folder.join("long.log");

        let written: Vec<String> = (0..KEPT + 40).map(|line| format!("ligne {line}")).collect();
        std::fs::write(&path, written.join("\n")).unwrap();

        let kept = last_lines(&path);
        // La fin, qui est là où se trouve la panne, et jamais le début.
        assert!(kept.ends_with(&format!("ligne {}", KEPT + 39)), "{kept}");
        assert!(!kept.contains("ligne 0\n"), "{kept}");
        // Et ce qui a été laissé de côté est annoncé : un journal amputé
        // en silence se lit comme un journal complet.
        assert!(kept.starts_with("(le début n'est pas montré)"), "{kept}");

        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_huge_file_costs_only_its_end() {
        let folder = a_folder_of_its_own("enorme");
        let path = folder.join("enorme.log");

        // Bien au-delà de ce que la lecture s'autorise : si elle lisait
        // tout, ce test se verrait au chronomètre et à la mémoire.
        let mut written = String::new();
        for line in 0..40_000 {
            written.push_str(&format!("ligne {line} avec un peu de matière autour\n"));
        }
        std::fs::write(&path, &written).unwrap();

        let kept = last_lines(&path);
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
        // Jamais de demi-ligne en tête après la coupe.
        let second = kept.lines().nth(1).unwrap();
        assert!(second.starts_with("ligne "), "{second}");

        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn an_unreadable_file_says_so_rather_than_nothing() {
        // Un dossier n'est pas lisible comme un fichier : c'est le
        // moyen portable d'obtenir un refus qui n'est pas « absent ».
        let folder = a_folder_of_its_own("illisible");
        let read = last_lines(&folder);
        assert!(read.starts_with("(illisible"), "{read}");
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn the_heading_lines_up() {
        // Compté en caractères et non en octets : « ô » en occupe deux,
        // et une colonne mesurée à l'octet se croirait de travers là où
        // elle est parfaitement droite.
        let mut journal = Journal(String::new());
        journal.says("Service", "en marche");
        journal.says("Moteur hôte", "présent");
        let colonnes: Vec<usize> = journal
            .0
            .lines()
            .map(|line| {
                line.chars()
                    .position(|c| c == ':')
                    .expect("un séparateur par ligne")
            })
            .collect();
        assert_eq!(colonnes[0], colonnes[1], "{}", journal.0);
    }

    #[test]
    fn a_journal_opens_on_the_build_and_the_computer() {
        // Les deux moitiés du produit compilées à des moments
        // différents, c'est la panne que personne ne pense à vérifier :
        // elle est en première ligne, avant toute autre chose.
        let text = Journal::of_this_computer().gathered();
        let mut lines = text.lines();
        assert_eq!(lines.next().unwrap(), crate::version_line());
        assert!(lines.next().unwrap().starts_with("Ordinateur"), "{text}");
        // Et les quatre fichiers y sont, nommés, même ceux que cet
        // ordinateur n'a jamais écrits.
        for (file, what) in FILES {
            assert!(text.contains(&format!("--- {what} ({file}) ---")), "{text}");
        }
    }

    #[test]
    fn the_engines_build_is_read_from_what_the_script_writes() {
        // Mot pour mot ce que packaging/engines/fetch-engines.ps1 écrit :
        // les deux doivent parler de la même chose, faute de quoi le
        // journal dirait « moteurs présents » sans jamais dire lesquels.
        let written = "# Moteurs ZyrDesk : d'où viennent ceux qui sont en place.\n\
             # Écrit par packaging/engines/fetch-engines.ps1, à ne pas corriger à la main.\n\
             run = 17392044\n\
             commit = a9f7db93c1\n\
             branche = develop\n\
             date = 2026-08-18T20:31:00Z\n";
        assert_eq!(
            build_from(written),
            "compilation 17392044 du 2026-08-18T20:31:00Z"
        );
    }

    #[test]
    fn engines_put_there_by_hand_say_nothing_rather_than_lie() {
        // Déposer les moteurs soi-même reste parfaitement valable : il
        // n'y a alors rien à dire de leur provenance, et surtout rien à
        // inventer.
        assert!(build_from("").is_empty());
        assert!(build_from("n'importe quoi").is_empty());
        assert!(build_from("# run = 1\n").is_empty());
    }

    #[test]
    fn a_file_that_was_never_written_is_already_empty() {
        // Vider le journal d'un ordinateur qui n'a jamais hébergé ne
        // doit pas se plaindre du fichier que le moteur hôte n'a jamais
        // ouvert.
        let folder = a_folder_of_its_own("vidage");
        assert!(empty(&folder.join("jamais.log")).is_ok());

        let path = folder.join("plein.log");
        std::fs::write(&path, "trois semaines de lignes\n").unwrap();
        assert!(empty(&path).is_ok());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "");

        std::fs::remove_dir_all(&folder).unwrap();
    }
}
