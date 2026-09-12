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

/// The files gathered, in the order they are read.
const FILES: [(&str, &str); 4] = [
    ("service.log", "Le service"),
    ("session.log", "Le moteur client"),
    ("engine-console.log", "Le moteur hôte"),
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

    /// Closes the heading on the engines in place, then gathers the
    /// files.
    pub fn gathered(self) -> String {
        self.sifted(&Sifting::everything())
    }

    /// The same, keeping only the lines that answer what was asked.
    ///
    /// The asking happens as the files are read and never on the page
    /// once it is made, and that is the whole of what makes it worth
    /// anything: only the last hundred and twenty lines of each file
    /// reach a page, and six lines about the clipboard are almost never
    /// among the last hundred and twenty of a session.
    pub fn sifted(mut self, sift: &Sifting) -> String {
        let here = |present: bool| if present { "présent" } else { "absent" };
        self.says("Moteur hôte", here(paths::host_engine_exe().is_file()));
        self.says("Moteur client", here(paths::client_engine_exe().is_file()));
        let engines = engines_build();
        if !engines.is_empty() {
            self.says("Moteurs", &engines);
        }
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
/// Emptied rather than deleted. The service and the engines hold these
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
///
/// `within` is what the file is called, which stands in for a tag on the
/// lines that carry none: the engines write their own journals in their
/// own shape, and this is what lets one of them be asked for whole.
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
    // Asked of every line read and not of the ones kept, which is the
    // point: what is being looked for is rare, and a file's last hundred
    // and twenty lines almost never hold it.
    //
    // A line with no name of its own answers to the file it is in, so
    // that is what goes in the list for it: the engines write whole
    // journals that way, and theirs would otherwise be impossible to ask
    // for from a list of names.
    let stem = within.strip_suffix(".log").unwrap_or(within);
    let mut answered: Vec<&str> = Vec::new();
    for line in whole {
        named.insert(
            crate::sifting::about(line)
                .map_or(stem, |(_, tag)| tag)
                .to_lowercase(),
        );
        if sift.keeps(line, within) {
            answered.push(line);
        }
    }
    if answered.is_empty() && !sift.takes_everything() {
        return "(rien ici ne répond au tri)".to_string();
    }
    let from = answered.len().saturating_sub(KEPT);
    let mut kept = answered[from..].join("\n");
    if from > 0 || skipped > 0 {
        kept.insert_str(0, "(le début n'est pas montré)\n");
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Les noms rencontrés ne sont pas le sujet de ces essais-là.
    fn read(path: &Path, within: &str, sift: &Sifting) -> String {
        last_lines(path, within, sift, &mut BTreeSet::new())
    }

    /// Rien de demandé, donc tout gardé.
    fn tout() -> Sifting {
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
        assert!(read(nowhere, "none", &tout()).contains("rien d'écrit"));
    }

    #[test]
    fn only_the_end_of_a_long_file_is_kept_and_it_says_so() {
        let folder = a_folder_of_its_own("long");
        let path = folder.join("long.log");

        let written: Vec<String> = (0..KEPT + 40).map(|line| format!("ligne {line}")).collect();
        std::fs::write(&path, written.join("\n")).unwrap();

        let kept = read(&path, "essai", &tout());
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

        let kept = read(&path, "essai", &tout());
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
    fn le_tri_se_fait_a_la_lecture_et_non_sur_la_page() {
        // C'est toute la différence : seules les cent vingt dernières
        // lignes d'un fichier arrivent sur une page, et six lignes de
        // presse-papiers ne sont presque jamais parmi les cent vingt
        // dernières d'une session.
        let folder = a_folder_of_its_own("tri");
        let path = folder.join("service.log");

        let mut written = String::new();
        written.push_str("2026-09-11 18:55:03 I [clipboard] ce que tient cet ordinateur\n");
        for line in 0..KEPT + 40 {
            let _ = writeln!(written, "2026-09-11 18:55:04 I [ways] voie {line} ouverte");
        }
        std::fs::write(&path, &written).unwrap();

        // Sans tri, la ligne du début est hors de portée.
        let tout = read(&path, "service", &tout());
        assert!(!tout.contains("ce que tient"), "{tout}");

        // Avec, elle est la seule qui reste.
        let trie = read(&path, "service", &Sifting::of("tag:clipboard"));
        assert_eq!(
            trie,
            "2026-09-11 18:55:03 I [clipboard] ce que tient cet ordinateur"
        );

        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn ce_qu_on_peut_demander_se_lit_dans_les_fichiers_et_se_relit_en_tete() {
        // La boîte de tri ne peut pas le deviner : la moitié du temps la
        // page vient d'un autre ordinateur, et un nom proposé qu'aucune
        // ligne ne porte est une impasse proposée.
        let folder = a_folder_of_its_own("noms");
        let path = folder.join("service.log");
        let mut written = String::new();
        let _ = writeln!(written, "2026-09-11 18:55:03 I [clipboard] ce qu'il tient");
        let _ = writeln!(written, "2026-09-11 18:55:04 I [way] voie 1 ouverte");
        std::fs::write(&path, &written).unwrap();

        let mut named = BTreeSet::new();
        last_lines(&path, "service.log", &Sifting::of("clipboard"), &mut named);
        // Relevés même quand le tri les écarte : ce qu'on peut demander
        // est ce que les fichiers portent, pas ce qui reste une fois la
        // demande faite.
        assert_eq!(
            named.iter().cloned().collect::<Vec<_>>(),
            ["clipboard", "way"]
        );

        // Une ligne sans nom répond à celui de son fichier, faute de quoi
        // le journal d'un moteur ne se demanderait pas d'une liste.
        let engine = folder.join("session.log");
        std::fs::write(&engine, "00:00:03 - SDL Info (0): IDR demandée\n").unwrap();
        let mut named = BTreeSet::new();
        last_lines(&engine, "session.log", &Sifting::everything(), &mut named);
        assert_eq!(named.iter().cloned().collect::<Vec<_>>(), ["session"]);

        // Et ce que l'entête écrit, la boîte le relit tel quel.
        let mut journal = Journal(String::new());
        journal.says(NAMES_HEADING, "clipboard, files, way");
        assert_eq!(names_in(&journal.0), ["clipboard", "files", "way"]);
        // Une page d'une moitié plus ancienne du produit n'en porte pas :
        // la boîte ne propose alors rien et tout se tape, comme avant.
        assert!(names_in("Ordinateur       : PC-SAV").is_empty());

        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn un_tri_qui_ne_rend_rien_le_dit_plutot_que_de_laisser_un_blanc() {
        // Un blanc se lit comme un fichier vide, et la question devient
        // « est-ce que ça marche ? » au lieu de « il n'y avait rien ».
        let folder = a_folder_of_its_own("tri-vide");
        let path = folder.join("service.log");
        std::fs::write(&path, "2026-09-11 18:55:04 I [ways] voie 1 ouverte\n").unwrap();

        let trie = read(&path, "service", &Sifting::of("tag:clipboard"));
        assert!(trie.contains("rien ici ne répond au tri"), "{trie}");

        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn une_page_triee_dit_sous_quel_tri_elle_a_ete_prise() {
        // Sinon elle se lit comme un produit qui n'a rien à dire plutôt
        // que comme la réponse à une question.
        let text = Journal::of_this_computer().sifted(&Sifting::of("tag:clipboard"));
        assert!(text.contains("Tri "), "{}", &text[..400]);
        assert!(text.contains("tag:clipboard"), "{}", &text[..400]);
        // Et une page non triée ne porte pas la ligne du tout.
        assert!(!Journal::of_this_computer().gathered().contains("\nTri "));
    }

    #[test]
    fn an_unreadable_file_says_so_rather_than_nothing() {
        // Un dossier n'est pas lisible comme un fichier : c'est le
        // moyen portable d'obtenir un refus qui n'est pas « absent ».
        let folder = a_folder_of_its_own("illisible");
        let refused = read(&folder, "essai", &tout());
        assert!(refused.starts_with("(illisible"), "{refused}");
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
    fn what_is_emptied_covers_more_than_what_is_gathered() {
        // Le relevé de ce que l'ordinateur atteint tient une mesure par
        // seconde : il noierait la copie qu'on relit d'un trait, donc il
        // n'y est pas. Mais vider le journal avant un essai doit le
        // vider aussi, sans quoi on lit trois semaines de relevé en face
        // d'une séance de cinq minutes.
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
