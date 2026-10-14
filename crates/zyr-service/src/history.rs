//! The sessions that took place on this computer.
//!
//! Those it opened towards others and those others opened towards it,
//! each written down when it ends: which computer, where it was, when it
//! began and how long it lasted. Nothing of what crossed it: what a
//! session carried belongs to the two people who held it.
//!
//! A session that drops and comes back by itself is one session to the
//! person holding it, so a session that begins with the same computer,
//! in the same direction, soon after the last one ended carries it on
//! rather than standing apart.
//!
//! Kept in a file, one session to a line, the latest first, readable and
//! correctable in a text editor. The oldest go once there are too many.

use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use zyr_control::Past;
use zyr_proto::fingerprint::Fingerprint;
use zyr_proto::log::Log;

/// What this module's lines are filed under.
const TAG: &str = "history";

/// How many sessions are kept.
const KEPT: usize = 200;

/// How soon after a session ended one with the same computer carries it
/// on: what a session that dropped takes to come back by itself, with
/// room to spare.
const CARRIED_ON_WITHIN: Duration = Duration::from_secs(60);

/// The sessions that took place, shared by whoever sees one end and by
/// whoever answers the window.
#[derive(Clone)]
pub struct History {
    kept: Arc<Mutex<Vec<Past>>>,
    path: PathBuf,
    log: Log,
}

impl History {
    /// Reads the sessions kept in that file. A missing file is none yet,
    /// and a line nobody can read is skipped: one bad line must not cost
    /// the rest.
    pub fn at(path: PathBuf, log: Log) -> Self {
        let log = log.about(TAG);
        let kept = match std::fs::read_to_string(&path) {
            Ok(text) => text
                .lines()
                .filter(|line| !line.trim_start().starts_with('#'))
                .filter_map(|line| line.parse().ok())
                .collect(),
            Err(e) => {
                if e.kind() != io::ErrorKind::NotFound {
                    log.write(&format!("past sessions unreadable, starting afresh: {e}"));
                }
                Vec::new()
            }
        };
        History {
            kept: Arc::new(Mutex::new(kept)),
            path,
            log,
        }
    }

    /// Writes down a session that has just ended, which began `lasted`
    /// ago.
    pub fn ended(&self, outgoing: bool, peer: Fingerprint, address: String, lasted: Duration) {
        let began = SystemTime::now()
            .checked_sub(lasted)
            .and_then(|began| began.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |since| since.as_secs());
        let written = {
            let mut kept = self.kept.lock().expect("past sessions");
            add(
                &mut kept,
                Past {
                    outgoing,
                    peer,
                    address,
                    began,
                    lasted,
                },
            );
            written(&kept)
        };
        if let Err(e) = zyr_proto::files::replace(&self.path, &written) {
            self.log.write(&format!("past sessions not kept: {e}"));
        }
    }

    /// The sessions kept, the latest first.
    pub fn recent(&self) -> Vec<Past> {
        self.kept.lock().expect("past sessions").clone()
    }
}

/// Adds a session that has just ended to those kept, the latest first:
/// on the last one with the same computer in the same direction when it
/// carries it on, as a new one otherwise.
fn add(kept: &mut Vec<Past>, past: Past) {
    let carried = kept
        .iter_mut()
        .find(|before| before.outgoing == past.outgoing && before.peer == past.peer)
        .filter(|before| {
            let ended = before.began + before.lasted.as_secs();
            past.began >= before.began && past.began <= ended + CARRIED_ON_WITHIN.as_secs()
        });
    match carried {
        Some(before) => {
            let until = past.began + past.lasted.as_secs() - before.began;
            before.lasted = before.lasted.max(Duration::from_secs(until));
            before.address = past.address;
        }
        None => kept.insert(0, past),
    }
    kept.sort_by_key(|past| std::cmp::Reverse(past.began));
    kept.truncate(KEPT);
}

/// The file, whole.
fn written(kept: &[Past]) -> String {
    let mut written = String::from(
        "# The sessions that took place on this computer, the latest first.\n\
         # Written by the ZyrDesk service.\n",
    );
    for past in kept {
        written.push_str(&past.to_string());
        written.push('\n');
    }
    written
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fingerprint(seed: u8) -> Fingerprint {
        format!("{seed:02x}").repeat(32).parse().unwrap()
    }

    fn past(seed: u8, began: u64, lasted: u64) -> Past {
        Past {
            outgoing: true,
            peer: fingerprint(seed),
            address: "192.168.1.20".to_string(),
            began,
            lasted: Duration::from_secs(lasted),
        }
    }

    #[test]
    fn a_session_that_comes_back_soon_carries_the_last_one_on() {
        let mut kept = Vec::new();
        add(&mut kept, past(1, 1_000, 600));
        add(&mut kept, past(1, 1_630, 300));
        assert_eq!(kept, [past(1, 1_000, 930)]);
    }

    #[test]
    fn a_session_long_after_the_last_one_stands_apart_and_comes_first() {
        let mut kept = Vec::new();
        add(&mut kept, past(1, 1_000, 600));
        add(&mut kept, past(1, 5_000, 60));
        assert_eq!(kept, [past(1, 5_000, 60), past(1, 1_000, 600)]);
    }

    #[test]
    fn another_computer_or_the_other_direction_is_another_session() {
        let mut kept = Vec::new();
        add(&mut kept, past(1, 1_000, 600));
        add(&mut kept, past(2, 1_610, 60));
        let incoming = Past {
            outgoing: false,
            ..past(1, 1_620, 60)
        };
        add(&mut kept, incoming.clone());
        assert_eq!(kept, [incoming, past(2, 1_610, 60), past(1, 1_000, 600)]);
    }

    #[test]
    fn only_so_many_are_kept() {
        let mut kept = Vec::new();
        for rank in 0..KEPT as u64 + 5 {
            add(&mut kept, past(1, rank * 10_000, 60));
        }
        assert_eq!(kept.len(), KEPT);
        assert_eq!(kept[0].began, (KEPT as u64 + 4) * 10_000);
    }

    #[test]
    fn what_is_written_reads_back_the_same() {
        let folder = std::env::temp_dir().join(format!(
            "zyrdeskd-history-{}",
            zyr_proto::random::alphanumeric_string(8)
        ));
        let log = Log::open(&folder.join("service.log")).unwrap();
        let path = folder.join("session-history.txt");
        let history = History::at(path.clone(), log.clone());
        history.ended(
            true,
            fingerprint(1),
            "living room computer".to_string(),
            Duration::from_secs(42),
        );
        history.ended(
            false,
            fingerprint(2),
            "192.168.1.30:51000".to_string(),
            Duration::ZERO,
        );
        let read_back = History::at(path, log);
        assert_eq!(read_back.recent(), history.recent());
        assert_eq!(read_back.recent().len(), 2);
        let _ = std::fs::remove_dir_all(folder);
    }
}
