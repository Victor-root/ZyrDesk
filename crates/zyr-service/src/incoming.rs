//! Who is connected to this computer right now.
//!
//! The door counts the sessions it takes in, but a count forgets who is
//! behind it the moment it is drawn: an interface asking « is this the
//! computer connected to me » or « disconnect that one » needs an
//! identity, not a number. The fingerprint of whoever connects is read
//! at the door already, proven there by the same certificate that got
//! them in; this is only where it is kept, for exactly as long as the
//! connection itself lasts, and handed nowhere else.
//!
//! The shape mirrors [`crate::ways`], the same bookkeeping for the
//! opposite direction: a shared, cloneable register that the part of the
//! service taking connections in writes to, and the part answering the
//! interface only ever reads or asks to close.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use zyr_control::Watching;
use zyr_proto::fingerprint::Fingerprint;
use zyr_transport::Connection;

use crate::engine::Engine;

/// How long a computer sent away has to say goodbye before its
/// connection is closed from under it.
const GOODBYE_WITHIN: Duration = Duration::from_secs(3);

/// One computer connected to this one right now.
struct Entry {
    peer: Fingerprint,
    address: SocketAddr,
    since: Instant,
    /// Kept only to be closed: the one thing an entry is for that a
    /// count could never be.
    connection: Connection,
    /// The session's engine, which ends the session with a goodbye.
    engine: Arc<Engine>,
}

/// Who is connected to this computer, shared between the door that
/// takes sessions in and whoever answers the interface's questions.
///
/// Cloning shares the same list. The door is the only side that ever
/// writes to it; the interface only reads it, or asks for one entry to
/// be closed.
#[derive(Clone, Default)]
pub struct Incoming {
    entries: Arc<Mutex<Vec<Entry>>>,
}

/// Keeps one entry in the list for exactly as long as the session it
/// names lasts.
///
/// A guard and not two lines around the session's body: a session that
/// ends by anything other than a clean return must not leave a computer
/// listed as connected forever.
pub struct Held {
    incoming: Incoming,
    id: usize,
}

impl Drop for Held {
    fn drop(&mut self) {
        self.incoming
            .entries
            .lock()
            .expect("connected computers")
            .retain(|entry| entry.connection.stable_id() != self.id);
    }
}

impl Incoming {
    /// Writes a computer down as connected, and hands back what removes
    /// it again once the session that connected it is over.
    pub fn arrived(
        &self,
        peer: Fingerprint,
        address: SocketAddr,
        connection: Connection,
        engine: Arc<Engine>,
    ) -> Held {
        let id = connection.stable_id();
        self.entries
            .lock()
            .expect("connected computers")
            .push(Entry {
                peer,
                address,
                since: Instant::now(),
                connection,
                engine,
            });
        Held {
            incoming: self.clone(),
            id,
        }
    }

    /// Every computer connected to this one right now.
    pub fn watching(&self) -> Vec<Watching> {
        let now = Instant::now();
        self.entries
            .lock()
            .expect("connected computers")
            .iter()
            .map(|entry| Watching {
                peer: entry.peer,
                address: entry.address.to_string(),
                since: now.duration_since(entry.since),
            })
            .collect()
    }

    /// Sends every session from that computer away, and says whether
    /// there was one.
    ///
    /// The engine is asked to end the session so that the computer sees
    /// the host say goodbye: a connection closed with no word reads there
    /// as a link lost, and a link lost is brought back. The connection is
    /// closed all the same if the engine cannot be asked, or has not
    /// finished within [`GOODBYE_WITHIN`].
    ///
    /// The entry itself is taken off the list by the guard [`Held`]
    /// returned when it arrived, the moment its session actually ends,
    /// which this triggers but does not wait for.
    pub fn kick(&self, peer: Fingerprint) -> bool {
        let entries = self.entries.lock().expect("connected computers");
        let mut found = false;
        for entry in entries.iter().filter(|entry| entry.peer == peer) {
            found = true;
            if entry.engine.send_away().is_ok() {
                let connection = entry.connection.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(GOODBYE_WITHIN).await;
                    connection.close();
                });
            } else {
                entry.connection.close();
            }
        }
        found
    }
}

// `arrived()`/`kick()` both take a real `zyr_transport::Connection`,
// which only exists once two endpoints have actually found each other
// over a socket: nothing in this crate stands one up without doing
// exactly that, and building the machinery to do it just for this file
// would be its own small project. The empty-register case below needs
// none of it; the rest is exercised by the door itself, running.
#[cfg(test)]
mod tests {
    use super::*;

    fn fingerprint(seed: u8) -> Fingerprint {
        format!("{seed:02x}").repeat(32).parse().unwrap()
    }

    #[test]
    fn a_computer_nobody_ever_connected_is_not_on_the_list() {
        let incoming = Incoming::default();
        assert!(incoming.watching().is_empty());
        assert!(!incoming.kick(fingerprint(1)));
    }
}
