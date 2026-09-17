//! The journal, opened from the window.
//!
//! What a journal is made of lives in `zyr_proto::journal`, and who
//! gathers one is the service: half of what the page says is what the
//! service holds, and a window reading the four files by itself would
//! have a page missing exactly the lines nobody can work out alone.
//!
//! Two journals can be opened from here. This computer's, which is one
//! question to the service; and the one belonging to a computer on the
//! home screen, which the service goes and fetches through the tunnel.
//! The second is the whole point: a fault is diagnosed on both journals
//! at once or on neither, and walking to the other machine to copy its
//! own is the errand a remote desktop exists to spare.
//!
//! The window writes to the journal as well. During a session it stands
//! behind the picture, where anything it put on screen would be read by
//! nobody.

// Tout ce qui est ici est demandé par l'accueil, que ce programme dessine
// lui-même, et ce qui dessine n'existe que sous Windows comme les
// fenêtres qu'il habille. Ailleurs, rien ne pose ces questions : le
// fichier reste compilé et vérifié, il n'est simplement appelé par
// personne.
#![cfg_attr(not(windows), allow(dead_code))]

use std::path::PathBuf;
use std::sync::OnceLock;

use zyr_control::{Answer, Request};
use zyr_proto::journal::Journal;
use zyr_proto::log::Log;
use zyr_proto::paths;
use zyr_proto::sifting::Sifting;

use crate::service;

/// This computer's journal, ready to be copied out.
///
/// `sift` is what was written in the box above the button, and it is
/// asked of the service rather than applied to the page that comes back:
/// only the end of each file reaches a page, and lines about one thing
/// are almost never among the last of a session's four thousand.
pub async fn journal(sift: &str) -> String {
    let asking = Request::Journal {
        sift: sift.to_string(),
    };
    match service::ask(&asking).await {
        Ok(Answer::Journal(text)) => text,
        Ok(other) => gathered_here(&service::unexpected(other), sift),
        // A service that is not answering is exactly when a journal is
        // wanted, so the files are gathered here instead. What is lost
        // is what only the service knew, and its silence is written in
        // its place rather than left as a gap.
        Err(reason) => gathered_here(&reason, sift),
    }
}

/// Another computer's journal, fetched from it.
///
/// It can take a moment: reaching that computer means opening a tunnel
/// to it, and one that is asleep or gone answers nothing at all. The
/// refusal that comes back then is the same one a session would have
/// been refused with, which is what makes it worth reading.
pub async fn far_journal(
    host: String,
    fingerprint: String,
    sift: String,
) -> Result<String, String> {
    let peer = fingerprint
        .trim()
        .parse()
        .map_err(|_| "cette empreinte n'a pas la forme attendue".to_string())?;
    note(&format!("journal demandé à {peer}"));
    match service::ask(&Request::FarJournal { host, peer, sift }).await {
        Ok(Answer::Journal(text)) => Ok(text),
        Ok(other) => Err(service::unexpected(other)),
        Err(reason) => {
            note(&format!("journal de {peer} non obtenu : {reason}"));
            Err(reason)
        }
    }
}

/// What this window can gather on its own, the service being silent.
fn gathered_here(reason: &str, sift: &str) -> String {
    let mut journal = Journal::of_this_computer();
    journal.says("Service", &reason.replace('\n', " "));
    journal.sifted(&Sifting::of(sift))
}

/// Empties everything the product has written down here.
///
/// Asked for before a test, so that what comes out afterwards is that
/// test and nothing else: a journal carrying three weeks of unrelated
/// lines is a journal nobody reads to the end.
///
/// Done here rather than through the service, which the far one has to
/// go through: this one has to work when the service does not answer,
/// and that is when a fresh page is wanted most.
pub fn clear_journal() -> Result<(), String> {
    let refused = zyr_proto::journal::emptied();

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

/// Empties another computer's journal.
///
/// The other half of reading one, and the reason both halves are here:
/// a fault is found by emptying both journals, doing the thing that goes
/// wrong, and reading both. Emptying only the one within arm's reach
/// leaves the walk to the other machine exactly where it was.
pub async fn clear_far_journal(host: String, fingerprint: String) -> Result<(), String> {
    let peer = fingerprint
        .trim()
        .parse()
        .map_err(|_| "cette empreinte n'a pas la forme attendue".to_string())?;
    match service::ask(&Request::ClearFarJournal { host, peer }).await {
        Ok(Answer::Done) => {
            note(&format!("journal de {peer} vidé"));
            Ok(())
        }
        Ok(other) => Err(service::unexpected(other)),
        Err(reason) => {
            note(&format!("journal de {peer} non vidé : {reason}"));
            Err(reason)
        }
    }
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

/// Writes down what the window just did, under that tag.
///
/// Every part of the window declares one of its own where it writes,
/// rather than choosing one at each line: what a tag is worth is that it
/// is the same one every time, and one picked afresh a hundred and forty
/// times is a hundred and forty chances to spell it differently.
///
/// Never fails and never says so: a trace that could stop the thing it
/// is watching would be worse than no trace.
pub fn note_about(tag: &'static str, what: &str) {
    if let Some(log) = own_log() {
        log.about(tag).write(what);
    }
}

/// The same, for what belongs to the window itself rather than to one
/// part of it.
pub fn note(what: &str) {
    note_about("interface", what);
}

/// Says which build this window is, the moment it opens.
pub fn opened() {
    note(&format!("fenêtre ouverte, {}", zyr_proto::version_line()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_silent_service_is_named_in_its_own_line_rather_than_left_out() {
        // C'est justement quand le service ne répond pas qu'on ouvre le
        // journal : la page doit venir quand même, et dire ce qui
        // manque plutôt que laisser un blanc.
        let text = gathered_here(
            "le service ZyrDesk ne tourne pas.\n  Lancez « zyrdeskd status ».",
            "",
        );
        assert!(text.contains("Service"), "{text}");
        assert!(text.contains("ne tourne pas"), "{text}");
        // Sur une ligne : le journal aligne ses étiquettes, et une
        // raison repliée casserait la colonne.
        let service = text
            .lines()
            .find(|line| line.starts_with("Service"))
            .expect("une ligne de service");
        assert!(service.contains("zyrdeskd status"), "{service}");
    }
}
