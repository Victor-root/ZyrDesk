//! The cards of the computers of this network, kept to wake them.
//!
//! A computer that is switched off is woken by a packet carrying the
//! hardware address of its card, which it does not announce. This
//! computer learns it while the two can hear each other, from what
//! Windows keeps of its neighbours, and keeps it in a file for the day
//! that computer is off. Readable and correctable in a text editor: one
//! computer a line, its fingerprint then its card.

use std::collections::HashMap;
use std::io;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use zyr_lan::Found;
use zyr_proto::fact::Fact;
use zyr_proto::fingerprint::Fingerprint;
use zyr_proto::log::Log;

/// What this module's lines are filed under.
const TAG: &str = "wake";

/// How often the cards of the computers in view are looked at.
const LOOKED_AT: Duration = Duration::from_secs(30);

/// The cards known, by computer.
#[derive(Clone)]
pub struct Cards {
    kept: Arc<Mutex<HashMap<Fingerprint, [u8; 6]>>>,
    path: PathBuf,
    log: Log,
}

impl Cards {
    /// Reads the cards kept in that file. A missing file is none yet,
    /// and a line nobody can read is skipped.
    pub fn at(path: PathBuf, log: Log) -> Self {
        let log = log.about(TAG);
        let kept = match std::fs::read_to_string(&path) {
            Ok(text) => text.lines().filter_map(read_one).collect(),
            Err(e) => {
                if e.kind() != io::ErrorKind::NotFound {
                    log.write(&format!("cards to wake unreadable, starting afresh: {e}"));
                }
                HashMap::new()
            }
        };
        Cards {
            kept: Arc::new(Mutex::new(kept)),
            path,
            log,
        }
    }

    /// Whether that computer can be woken from here.
    pub fn knows(&self, peer: Fingerprint) -> bool {
        self.kept.lock().expect("cards to wake").contains_key(&peer)
    }

    /// Wakes that computer.
    pub fn wake(&self, peer: Fingerprint) -> Result<(), Fact> {
        let card = self
            .kept
            .lock()
            .expect("cards to wake")
            .get(&peer)
            .copied()
            .ok_or_else(|| Fact::new("wake.unknown"))?;
        zyr_lan::wake(card).map_err(|e| Fact::new("wake.refused").with("detail", e))?;
        self.log
            .write(&format!("{peer} woken by its card {}", spelled(card)));
        Ok(())
    }

    /// Learns the cards of the computers in view, for as long as the
    /// service runs.
    pub async fn keep_learning(self, found: Found) {
        loop {
            let seen: Vec<(Fingerprint, IpAddr)> = found
                .peers()
                .into_iter()
                .map(|peer| (peer.fingerprint, peer.address))
                .collect();
            let cards = self.clone();
            let _ = tokio::task::spawn_blocking(move || cards.learn(&seen)).await;
            tokio::time::sleep(LOOKED_AT).await;
        }
    }

    /// Writes down the card each of these computers answers with, and
    /// the file when one is new.
    fn learn(&self, seen: &[(Fingerprint, IpAddr)]) {
        let mut learnt = Vec::new();
        let written = {
            let mut kept = self.kept.lock().expect("cards to wake");
            for (peer, address) in seen {
                let IpAddr::V4(address) = address else {
                    continue;
                };
                let Some(card) = zyr_system::card_of(*address) else {
                    continue;
                };
                if kept.insert(*peer, card) != Some(card) {
                    learnt.push((*peer, card));
                }
            }
            (!learnt.is_empty()).then(|| written(&kept))
        };
        let Some(written) = written else {
            return;
        };
        for (peer, card) in learnt {
            self.log
                .write(&format!("{peer} answers with the card {}", spelled(card)));
        }
        if let Err(e) = zyr_proto::files::replace(&self.path, &written) {
            self.log.write(&format!("cards to wake not kept: {e}"));
        }
    }
}

/// A card as people write it: six pairs of hexadecimal digits.
fn spelled(card: [u8; 6]) -> String {
    card.iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}

/// One line of the file, or nothing when it says nothing usable.
fn read_one(line: &str) -> Option<(Fingerprint, [u8; 6])> {
    let line = line.trim();
    if line.starts_with('#') {
        return None;
    }
    let (peer, card) = line.split_once(char::is_whitespace)?;
    let bytes: Vec<u8> = card
        .trim()
        .split([':', '-'])
        .map(|pair| u8::from_str_radix(pair, 16).ok())
        .collect::<Option<_>>()?;
    Some((peer.parse().ok()?, bytes.try_into().ok()?))
}

/// The file, whole, in a settled order.
fn written(kept: &HashMap<Fingerprint, [u8; 6]>) -> String {
    let mut lines: Vec<String> = kept
        .iter()
        .map(|(peer, card)| format!("{peer} {}\n", spelled(*card)))
        .collect();
    lines.sort();
    let mut written = String::from(
        "# The cards of the computers of this network, to wake them when they are off.\n\
         # Written by the ZyrDesk service, can be corrected by hand.\n",
    );
    written.extend(lines);
    written
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fingerprint(seed: u8) -> Fingerprint {
        format!("{seed:02x}").repeat(32).parse().unwrap()
    }

    #[test]
    fn a_card_written_reads_back_the_same() {
        let card = [0x00, 0x1a, 0x2b, 0x3c, 0x4d, 0x5e];
        let kept = HashMap::from([(fingerprint(1), card)]);
        let read: HashMap<_, _> = written(&kept).lines().filter_map(read_one).collect();
        assert_eq!(read, kept);
    }

    #[test]
    fn a_card_is_read_however_its_pairs_are_joined() {
        let line = format!("{} 00-1A-2B-3C-4D-5E", fingerprint(2));
        assert_eq!(
            read_one(&line),
            Some((fingerprint(2), [0x00, 0x1a, 0x2b, 0x3c, 0x4d, 0x5e]))
        );
    }

    #[test]
    fn a_line_that_is_not_a_card_is_skipped() {
        assert_eq!(read_one("# a comment"), None);
        assert_eq!(read_one(""), None);
        assert_eq!(read_one(&format!("{} 00:1a:2b", fingerprint(3))), None);
        assert_eq!(read_one("not-a-fingerprint 00:1a:2b:3c:4d:5e"), None);
    }

    #[test]
    fn a_computer_whose_card_is_unknown_is_not_woken() {
        let folder = std::env::temp_dir().join(format!(
            "zyrdeskd-wake-{}",
            zyr_proto::random::alphanumeric_string(8)
        ));
        let log = Log::open(&folder.join("service.log")).unwrap();
        let cards = Cards::at(folder.join("cards-to-wake.conf"), log);
        assert!(!cards.knows(fingerprint(1)));
        assert_eq!(cards.wake(fingerprint(1)), Err(Fact::new("wake.unknown")));
        let _ = std::fs::remove_dir_all(folder);
    }
}
