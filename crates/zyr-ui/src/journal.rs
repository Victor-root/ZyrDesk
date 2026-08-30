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

use std::path::PathBuf;
use std::sync::OnceLock;

use zyr_control::{Answer, Request};
use zyr_proto::journal::Journal;
use zyr_proto::log::Log;
use zyr_proto::paths;

use crate::service;

/// This computer's journal, ready to be copied out.
#[tauri::command]
pub async fn journal() -> String {
    match service::ask(&Request::Journal).await {
        Ok(Answer::Journal(text)) => text,
        Ok(other) => gathered_here(&service::unexpected(other)),
        // A service that is not answering is exactly when a journal is
        // wanted, so the files are gathered here instead. What is lost
        // is what only the service knew, and its silence is written in
        // its place rather than left as a gap.
        Err(reason) => gathered_here(&reason),
    }
}

/// Another computer's journal, fetched from it.
///
/// It can take a moment: reaching that computer means opening a tunnel
/// to it, and one that is asleep or gone answers nothing at all. The
/// refusal that comes back then is the same one a session would have
/// been refused with, which is what makes it worth reading.
#[tauri::command]
pub async fn far_journal(host: String, fingerprint: String) -> Result<String, String> {
    let peer = fingerprint
        .trim()
        .parse()
        .map_err(|_| "cette empreinte n'a pas la forme attendue".to_string())?;
    note(&format!("journal demandé à {peer}"));
    match service::ask(&Request::FarJournal { host, peer }).await {
        Ok(Answer::Journal(text)) => Ok(text),
        Ok(other) => Err(service::unexpected(other)),
        Err(reason) => {
            note(&format!("journal de {peer} non obtenu : {reason}"));
            Err(reason)
        }
    }
}

/// What this window can gather on its own, the service being silent.
fn gathered_here(reason: &str) -> String {
    let mut journal = Journal::of_this_computer();
    journal.says("Service", &reason.replace('\n', " "));
    journal.gathered()
}

/// Empties everything the product has written down here.
///
/// Asked for before a test, so that what comes out afterwards is that
/// test and nothing else: a journal carrying three weeks of unrelated
/// lines is a journal nobody reads to the end. Only this computer's, and
/// that is deliberate: emptying a machine somebody else is looking at
/// would throw away the very lines they are reading.
#[tauri::command]
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
    fn a_silent_service_is_named_in_its_own_line_rather_than_left_out() {
        // C'est justement quand le service ne répond pas qu'on ouvre le
        // journal : la page doit venir quand même, et dire ce qui
        // manque plutôt que laisser un blanc.
        let text =
            gathered_here("le service ZyrDesk ne tourne pas.\n  Lancez « zyrdeskd status ».");
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
