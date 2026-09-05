//! Whether this computer could still reach the outside world.
//!
//! A session that goes quiet leaves the same trace three different ways:
//! the far computer stopped speaking, the road between the two stopped
//! carrying, or this computer's own connection went away for a moment.
//! The product says a great deal about the first two. About the third it
//! can say nothing at all, and not for want of counters: every
//! measurement the service takes travels the very link in question, so a
//! link that is gone takes the measurement with it.
//!
//! So a second measurement is taken beside the session, on a socket of
//! its own, towards a computer that has nothing to do with ZyrDesk. Once
//! a second, this asks the public resolver at `OUTSIDE` for a name, and
//! writes down how long the answer took or that none came. On the fifth
//! of September that measurement was made by hand, in two windows, and
//! it settled in one reading a question five sessions had left open: two
//! pings lost on one of the two computers, none on the other, at the
//! very second the session went quiet. It should not have to be made by
//! hand.
//!
//! Held only while a session is open, on both sides, since outside one
//! there is nothing to explain. What it costs is one datagram of some
//! thirty bytes a second, and a file of its own.
//!
//! A question and not an echo, because an echo needs a raw socket and
//! the right to open one; this needs neither, and answers the same
//! thing: whether anything at all still comes back from the Internet.

// Outside Windows nothing calls this module: the service does not exist
// there. Its logic has nothing platform-specific about it and stays
// compiled and tested everywhere.
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use tokio::net::UdpSocket;
use tokio::runtime::Handle;
use zyr_proto::log::Log;
use zyr_proto::paths;

/// Who is asked, and it is deliberately nobody of ours: a resolver every
/// network in the world reaches, run by somebody with no stake in this
/// product and no way to be down at the same moment as our own server.
const OUTSIDE: &str = "8.8.8.8:53";

/// How often the question is asked. The same rhythm as `ping`, so the
/// two can be read side by side.
const EVERY: Duration = Duration::from_secs(1);

/// How long an answer is waited for before it counts as none.
const PATIENCE: Duration = Duration::from_secs(1);

/// Sessions open right now, on either side.
static SESSIONS: AtomicUsize = AtomicUsize::new(0);

/// Whether the asking is already under way. Set once and never cleared:
/// one computer has one outside, and one task is enough to watch it for
/// as long as the service runs.
static ASKING: AtomicBool = AtomicBool::new(false);

/// Watches the outside world for as long as this is held.
///
/// Handed to whatever owns a session, so the watch ends exactly when the
/// session does, whichever way it ends.
pub struct Watching;

impl Drop for Watching {
    fn drop(&mut self) {
        SESSIONS.fetch_sub(1, Ordering::SeqCst);
    }
}

/// A session is opening here: from now until it ends, what this computer
/// can reach is written down.
pub fn watch(log: &Log) -> Watching {
    SESSIONS.fetch_add(1, Ordering::SeqCst);
    if !ASKING.swap(true, Ordering::SeqCst)
        && let Ok(runtime) = Handle::try_current()
    {
        runtime.spawn(keep_asking(log.clone()));
    }
    Watching
}

/// Asks, for as long as the service runs, and writes only while a
/// session is open.
async fn keep_asking(log: Log) {
    let Ok(trace) = Log::open(&paths::reach_log()) else {
        log.write("nothing can be written down about what this computer reaches");
        return;
    };
    let mut answering = true;
    loop {
        if SESSIONS.load(Ordering::Relaxed) == 0 {
            // Between sessions there is nothing to explain, and the next
            // one starts without inheriting what the last one saw.
            answering = true;
            tokio::time::sleep(EVERY).await;
            continue;
        }
        let asked = Instant::now();
        let answer = ask_once().await;
        match answer {
            Some(took) => {
                trace.write(&format!("{OUTSIDE} answered in {} ms", took.as_millis()));
                if !answering {
                    answering = true;
                    log.write(&format!(
                        "this computer reaches {OUTSIDE} again: what it could not reach before \
                         was the Internet itself, and not the far computer"
                    ));
                }
            }
            None => {
                trace.write(&format!(
                    "{OUTSIDE} said nothing in {} ms",
                    PATIENCE.as_millis()
                ));
                if answering {
                    answering = false;
                    log.write(&format!(
                        "this computer no longer reaches {OUTSIDE}: its own connection is gone, \
                         and a session going quiet now says nothing about the far computer"
                    ));
                }
            }
        }
        let spent = asked.elapsed();
        if spent < EVERY {
            tokio::time::sleep(EVERY - spent).await;
        }
    }
}

/// One question, and how long the answer took. Nothing when none came.
///
/// A socket of its own each time, and never one kept open: a computer
/// that changes network keeps a socket bound to the address it no longer
/// has, and would report a silence that is only its own staleness.
async fn ask_once() -> Option<Duration> {
    let socket = UdpSocket::bind("0.0.0.0:0").await.ok()?;
    socket.connect(OUTSIDE).await.ok()?;
    // Numbered from the port the system just gave, so two questions in
    // flight are never confused, and an old answer is never taken for a
    // new one.
    let asked = socket.local_addr().ok()?.port();
    socket.send(&question(asked)).await.ok()?;
    let started = Instant::now();
    let mut answer = [0u8; 512];
    loop {
        let read = tokio::time::timeout(
            PATIENCE.saturating_sub(started.elapsed()),
            socket.recv(&mut answer),
        )
        .await
        .ok()?
        .ok()?;
        if answers(&answer[..read], asked) {
            return Some(started.elapsed());
        }
    }
}

/// The question asked, and always the same one: where a name that has
/// existed as long as the Internet has can be found.
///
/// Written out by hand rather than through a library: it is thirty-six
/// bytes that never change, and the whole point of this file is to lean
/// on nothing the session leans on.
fn question(numbered: u16) -> Vec<u8> {
    let mut asked = Vec::with_capacity(36);
    asked.extend_from_slice(&numbered.to_be_bytes());
    // Ask, and please look it up for me.
    asked.extend_from_slice(&[0x01, 0x00]);
    // One question, and no answers offered.
    asked.extend_from_slice(&[0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
    for label in ["a", "root-servers", "net"] {
        asked.push(label.len() as u8);
        asked.extend_from_slice(label.as_bytes());
    }
    asked.push(0);
    // An address, on the Internet.
    asked.extend_from_slice(&[0x00, 0x01, 0x00, 0x01]);
    asked
}

/// Whether that datagram is the answer to that question.
fn answers(said: &[u8], numbered: u16) -> bool {
    said.len() >= 12 && said[..2] == numbered.to_be_bytes() && said[2] & 0x80 != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_question_is_the_one_a_resolver_expects() {
        let asked = question(0x1234);
        assert_eq!(&asked[..2], &[0x12, 0x34], "le numéro de la question");
        assert_eq!(&asked[2..4], &[0x01, 0x00], "la récursion est demandée");
        assert_eq!(&asked[4..6], &[0x00, 0x01], "une seule question");
        // Le nom, étiquette par étiquette, puis « une adresse, sur
        // Internet ».
        assert_eq!(
            &asked[12..],
            b"\x01a\x0croot-servers\x03net\x00\x00\x01\x00\x01"
        );
    }

    #[test]
    fn only_the_answer_to_our_own_question_counts() {
        // Une réponse porte le numéro demandé et le drapeau qui dit que
        // c'en est une. Sans ce dernier, notre propre question nous
        // reviendrait et compterait pour une réponse.
        let mut said = question(0x1234);
        assert!(!answers(&said, 0x1234), "la question n'est pas sa réponse");
        said[2] |= 0x80;
        assert!(answers(&said, 0x1234));
        assert!(!answers(&said, 0x1235), "une autre question");
        assert!(!answers(&said[..8], 0x1234), "trop court pour être lu");
    }
}
