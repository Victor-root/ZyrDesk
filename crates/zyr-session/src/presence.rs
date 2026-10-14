//! Where a computer of the home screen stands, and the road to it.
//!
//! What its card shows and whether a session can be asked of it, read
//! once here from what the service says rather than worked out by the
//! window: the window draws a dot and a button, and this says which.

use zyr_control::{Access, Peer};
use zyr_proto::fact::Fact;

/// Whether a session can be opened towards a computer right now.
#[derive(Debug, Clone, PartialEq)]
pub enum Presence {
    /// It answers, and takes sessions.
    Ready,
    /// It is there and says so, but takes no session right now, for the
    /// reason it gives.
    Closed { why: Fact },
    /// It is not there.
    Gone,
    /// Nothing says either way: a computer written down by hand, on a
    /// network that carries no announcement. It is tried when asked.
    Unknown,
}

impl Presence {
    /// Whether a session may be asked of it.
    pub fn can_be_reached(&self) -> bool {
        matches!(self, Presence::Ready | Presence::Unknown)
    }
}

/// The road a session towards a computer takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Road {
    /// This network, where it announces itself.
    Local,
    /// The account's server, which meets the two computers wherever they
    /// are.
    Internet,
    /// The address somebody wrote down by hand.
    Written,
}

/// Where that computer stands.
///
/// The account, when it knows the computer and the computer is online,
/// has the last word: it is the one that says why a machine announcing
/// itself on this network still takes no session. Announcing itself is
/// otherwise enough, since a computer only does while its service runs.
pub fn presence(peer: &Peer) -> Presence {
    match &peer.account {
        Some(account) if account.online && account.access != Access::Ready => Presence::Closed {
            why: account.access.fact(),
        },
        Some(account) if account.online => Presence::Ready,
        _ if peer.seen => Presence::Ready,
        Some(_) => Presence::Gone,
        None if peer.written => Presence::Unknown,
        None => Presence::Gone,
    }
}

/// The computers in the order they are shown: those a session can be
/// opened towards first, then those that are there but take none, then
/// those that are gone. Within each, the order they came in is kept, so
/// that a card never jumps for nothing.
pub fn reachable_first(peers: &mut [Peer]) {
    peers.sort_by_key(|peer| match presence(peer) {
        Presence::Ready => 0,
        Presence::Unknown => 1,
        Presence::Closed { .. } => 2,
        Presence::Gone => 3,
    });
}

/// The road a session towards that computer takes.
///
/// This network first: a computer that announces itself here is reached
/// here, account or not, since that is the shortest road there is.
pub fn road(peer: &Peer) -> Road {
    if peer.seen {
        Road::Local
    } else if peer.account.is_some() {
        Road::Internet
    } else {
        Road::Written
    }
}

#[cfg(test)]
mod tests {
    use zyr_control::Answer;

    use super::*;

    /// A computer as the service describes it, from the line it sends.
    fn said(fields: &str) -> Peer {
        let line = format!(
            "peer name=PC fingerprint={} address=192.168.1.20 port=47000 {fields}",
            "0829cc7ecb9e9ba53cd36e6f342268ddf3c8ef05a49d1d7944ac6332c89cf237"
        );
        match Answer::parse(&line) {
            Ok(Answer::Peer(peer)) => peer,
            other => panic!("« {line} » read as {other:?}"),
        }
    }

    #[test]
    fn a_computer_announcing_itself_here_is_ready_and_reached_here() {
        let peer = said("seen=yes written=no");
        assert_eq!(presence(&peer), Presence::Ready);
        assert_eq!(road(&peer), Road::Local);
        // Of the account as well: this network is still the shorter
        // road.
        let peer = said("seen=yes written=no device=d1 online=yes access=ready");
        assert_eq!(road(&peer), Road::Local);
    }

    #[test]
    fn the_account_says_why_a_computer_that_is_there_takes_no_session() {
        let peer = said("seen=yes written=no device=d1 online=yes access=off");
        assert_eq!(
            presence(&peer),
            Presence::Closed {
                why: Fact::new("access.off")
            }
        );
        assert!(!presence(&peer).can_be_reached());
    }

    #[test]
    fn a_computer_only_the_account_knows_is_reached_through_it() {
        let online = said("seen=no written=no device=d2 online=yes access=ready");
        assert_eq!(presence(&online), Presence::Ready);
        assert_eq!(road(&online), Road::Internet);
        let offline = said("seen=no written=no device=d2 online=no access=ready");
        assert_eq!(presence(&offline), Presence::Gone);
        assert!(!presence(&offline).can_be_reached());
    }

    #[test]
    fn the_computers_that_answer_come_first_and_keep_their_order() {
        let named = |name: &str, fields: &str| {
            let mut peer = said(fields);
            peer.name = name.to_string();
            peer
        };
        let mut peers = vec![
            named(
                "gone",
                "seen=no written=no device=d1 online=no access=ready",
            ),
            named(
                "closed",
                "seen=yes written=no device=d2 online=yes access=off",
            ),
            named("first", "seen=yes written=no"),
            named("written", "seen=no written=yes"),
            named(
                "second",
                "seen=no written=no device=d3 online=yes access=ready",
            ),
        ];
        reachable_first(&mut peers);
        let order: Vec<&str> = peers.iter().map(|peer| peer.name.as_str()).collect();
        assert_eq!(order, ["first", "second", "written", "closed", "gone"]);
    }

    #[test]
    fn a_computer_written_down_by_hand_is_tried_when_asked() {
        let peer = said("seen=no written=yes");
        assert_eq!(presence(&peer), Presence::Unknown);
        assert!(presence(&peer).can_be_reached());
        assert_eq!(road(&peer), Road::Written);
    }
}
