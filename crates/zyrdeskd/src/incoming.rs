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

// Outside Windows nothing calls this module: the service does not exist
// there. Its logic has nothing platform-specific about it and stays
// compiled and tested everywhere.
#![cfg_attr(not(windows), allow(dead_code))]

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use zyr_control::Watching;
use zyr_transport::{Connection, Fingerprint};

/// One computer connected to this one right now.
struct Entry {
    peer: Fingerprint,
    address: SocketAddr,
    since: Instant,
    /// Kept only to be closed: the one thing an entry is for that a
    /// count could never be.
    connection: Connection,
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
            .expect("ordinateurs connectés")
            .retain(|entry| entry.connection.stable_id() != self.id);
    }
}

impl Incoming {
    /// Writes a computer down as connected, and hands back what removes
    /// it again once the session that connected it is over.
    pub fn arrived(&self, peer: Fingerprint, address: SocketAddr, connection: Connection) -> Held {
        let id = connection.stable_id();
        self.entries
            .lock()
            .expect("ordinateurs connectés")
            .push(Entry {
                peer,
                address,
                since: Instant::now(),
                connection,
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
            .expect("ordinateurs connectés")
            .iter()
            .map(|entry| Watching {
                peer: entry.peer,
                address: entry.address.to_string(),
                since: now.duration_since(entry.since),
            })
            .collect()
    }

    /// Closes every connection from that computer, and says whether
    /// there was one to close.
    ///
    /// Closing is all this does: the entry itself is taken off the list
    /// by the guard [`Held`] returned when it arrived, the moment its
    /// session actually ends, which this triggers but does not wait for.
    pub fn kick(&self, peer: Fingerprint) -> bool {
        let entries = self.entries.lock().expect("ordinateurs connectés");
        let mut found = false;
        for entry in entries.iter().filter(|entry| entry.peer == peer) {
            entry.connection.close();
            found = true;
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
