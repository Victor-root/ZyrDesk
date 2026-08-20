//! The journal, gathered onto one screen.
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
//! The window writes to it as well. During a session it stands behind
//! the picture, where anything it put on screen would be read by nobody.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::OnceLock;

use zyr_control::{Answer, Holdup, Request};
use zyr_proto::log::Log;
use zyr_proto::paths;

use crate::service;

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

/// The whole journal, ready to be copied out.
#[tauri::command]
pub async fn journal() -> String {
    let mut text = heading().await;
    for (file, what) in FILES {
        let path = paths::logs_dir().join(file);
        let _ = write!(text, "\n\n--- {what} ({file}) ---\n");
        text.push_str(&last_lines(&path));
    }
    text
}

/// Empties everything the product has written.
///
/// Asked for before a test, so that what comes out afterwards is that
/// test and nothing else: a journal carrying three weeks of unrelated
/// lines is a journal nobody reads to the end.
///
/// Emptied rather than deleted. The service and the engines hold these
/// files open while they run, and Windows does not let go of a file
/// somebody is writing to; emptying works all the same, the next line
/// appended landing at the start of a file that is now blank.
#[tauri::command]
pub fn clear_journal() -> Result<(), String> {
    let mut refused = Vec::new();
    for (file, what) in FILES {
        if let Err(e) = emptied(&paths::logs_dir().join(file)) {
            refused.push(format!("{what} ({file}) : {e}"));
        }
    }

    // Written after the emptying, so the journal opens on the moment it
    // was cleared rather than on nothing at all.
    note("journal vidé");

    if refused.is_empty() {
        return Ok(());
    }
    Err(format!(
        "une partie du journal n'a pas pu être vidée :\n  {}",
        refused.join("\n  ")
    ))
}

/// Empties one file. One that was never written is already empty.
fn emptied(path: &std::path::Path) -> std::io::Result<()> {
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

/// What this computer is, in a dozen lines.
async fn heading() -> String {
    let mut text = String::new();
    let _ = writeln!(text, "{}", zyr_proto::version_line());
    say(&mut text, "Ordinateur", &zyr_proto::machine::name());
    say(&mut text, "Adresses", &own_addresses());

    match service::ask(&Request::Standing).await {
        Ok(Answer::Standing(standing)) => {
            say(
                &mut text,
                "Service",
                &format!(
                    "{}, dialecte {}",
                    if standing.build.is_empty() {
                        "compilation inconnue".to_string()
                    } else {
                        standing.build.clone()
                    },
                    standing.protocol
                ),
            );
            say(&mut text, "Empreinte", &standing.fingerprint.to_string());
            say(&mut text, "Accès distant", &remote_access(&standing));
            say(
                &mut text,
                "Réseau local",
                if standing.trusting {
                    "ordinateurs de confiance"
                } else {
                    "aucune confiance accordée"
                },
            );
            say(&mut text, "Sessions ouvertes", &standing.ways.to_string());
        }
        Ok(other) => say(&mut text, "Service", &service::unexpected(other)),
        Err(reason) => say(&mut text, "Service", &reason.replace('\n', " ")),
    }

    // Qui est vu sur le réseau, nommément. C'est la première chose qu'on
    // se demande quand rien n'apparaît, et la liste à l'écran ne dit pas
    // si elle est vide faute de voisin ou faute de réponse.
    say(&mut text, "Ordinateurs vus", &neighbours().await);

    let engines = crate::folders::engines();
    say(&mut text, "Moteur hôte", present(engines.host_here));
    say(&mut text, "Moteur client", present(engines.client_here));
    if !engines.build.is_empty() {
        say(&mut text, "Moteurs", &engines.build);
    }
    say(
        &mut text,
        "Journaux",
        &paths::logs_dir().display().to_string(),
    );
    text
}

/// Where this computer answers, card by card.
///
/// Two machines that never find each other are almost always two
/// machines on two different networks, and nothing else in a journal
/// says so. Written down here so the answer travels with the journal
/// instead of costing an evening and a command to go and fetch.
fn own_addresses() -> String {
    let answering = zyr_proto::machine::addresses();
    if answering.is_empty() {
        return "aucune".to_string();
    }
    answering
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// The computers this one sees on the local network, by name.
async fn neighbours() -> String {
    let seen = crate::desk::peers().await;
    if seen.is_empty() {
        return "aucun".to_string();
    }
    seen.iter()
        .map(|peer| format!("{} ({})", peer.name, peer.address))
        .collect::<Vec<_>>()
        .join(", ")
}

/// One line of the heading, its label padded so the values line up.
fn say(text: &mut String, label: &str, value: &str) {
    let _ = writeln!(text, "{label:<17}: {value}");
}

fn present(here: bool) -> &'static str {
    if here { "présent" } else { "absent" }
}

/// What remote access amounts to right now, in the same words the home
/// screen uses.
fn remote_access(standing: &zyr_control::Standing) -> String {
    if !standing.wanted {
        return "désactivé".to_string();
    }
    if standing.hosting {
        return "activé, prêt à être contrôlé".to_string();
    }
    match standing.holdup {
        Holdup::Starting => "activé, démarrage en cours".to_string(),
        Holdup::EngineMissing => "activé, mais le moteur hôte est absent".to_string(),
        Holdup::EngineWontStand => "activé, mais le moteur hôte ne tient pas".to_string(),
    }
}

/// The end of a file, or a word saying why there is none.
///
/// Only the end is ever read from disk. A log can have grown for months,
/// and reading the whole of it to keep a hundred lines would hold the
/// window on a file nobody asked to see all of.
fn last_lines(path: &std::path::Path) -> String {
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
        // an existing file this window cannot read is not « nothing ».
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

/// Where the window writes its own trace.
///
/// Opened once and kept: a window during a session writes a line every
/// time a button is pressed, and reopening the file each time would be
/// waste.
fn own_log() -> Option<&'static Log> {
    static LOG: OnceLock<Option<Log>> = OnceLock::new();
    LOG.get_or_init(|| Log::open(&interface_log()).ok())
        .as_ref()
}

fn interface_log() -> PathBuf {
    paths::logs_dir().join("interface.log")
}

/// Writes down what the window just did.
///
/// Never fails and never says so: a trace that could stop the thing it
/// is watching would be worse than no trace.
pub fn note(what: &str) {
    if let Some(log) = own_log() {
        log.write(what);
    }
}

/// Says which build this window is, the moment it opens.
pub fn opened() {
    note(&format!("fenêtre ouverte, {}", zyr_proto::version_line()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_that_does_not_exist_is_said_rather_than_left_blank() {
        let nowhere = std::path::Path::new("/nowhere/zyrdesk/none.log");
        assert!(last_lines(nowhere).contains("rien d'écrit"));
    }

    #[test]
    fn only_the_end_of_a_long_file_is_kept_and_it_says_so() {
        let folder = std::env::temp_dir().join(format!(
            "zyrdesk-journal-{}",
            zyr_proto::random::alphanumeric_string(8)
        ));
        std::fs::create_dir_all(&folder).unwrap();
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
        let folder = std::env::temp_dir().join(format!(
            "zyrdesk-journal-{}",
            zyr_proto::random::alphanumeric_string(8)
        ));
        std::fs::create_dir_all(&folder).unwrap();
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
        let folder = std::env::temp_dir().join(format!(
            "zyrdesk-journal-{}",
            zyr_proto::random::alphanumeric_string(8)
        ));
        std::fs::create_dir_all(&folder).unwrap();
        let read = last_lines(&folder);
        assert!(read.starts_with("(illisible"), "{read}");
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn the_heading_lines_up() {
        // Compté en caractères et non en octets : « ô » en occupe deux,
        // et une colonne mesurée à l'octet se croirait de travers là où
        // elle est parfaitement droite.
        let mut text = String::new();
        say(&mut text, "Service", "en marche");
        say(&mut text, "Moteur hôte", "présent");
        let colonnes: Vec<usize> = text
            .lines()
            .map(|line| {
                line.chars()
                    .position(|c| c == ':')
                    .expect("un séparateur par ligne")
            })
            .collect();
        assert_eq!(colonnes[0], colonnes[1], "{text}");
    }
}
