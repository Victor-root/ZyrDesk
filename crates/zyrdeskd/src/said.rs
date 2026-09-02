//! What a session says about itself, on both sides of it.
//!
//! A tunnel throws packets away in two places, and both are silent. A
//! packet too large for the path, thrown away rather than cut in two;
//! and the send queue overflowing, where the transport sacrifices the
//! oldest. Neither ends the session, and that is exactly why they have
//! to be said: a session that dies leaves a reason behind, a session
//! that goes quiet leaves nothing at all.
//!
//! The length of the road is the other half, and it answers a question
//! nothing else could: a session whose road doubles in length mid way is
//! still a session, and the only trace of it was a number in a window
//! nobody had open.
//!
//! Here, and not beside the ways alone, because the computer being
//! watched throws packets away too. It is the one sending the picture,
//! so it is the one whose losses are seen at the other end, and its
//! journal said nothing of them: a fault visible only from the machine
//! that does not cause it is a fault chased on the wrong machine.

// Outside Windows nothing calls this module: the service does not exist
// there. Its logic has nothing platform-specific about it and stays
// compiled and tested everywhere.
#![cfg_attr(not(windows), allow(dead_code))]

use std::time::Duration;

use zyr_transport::Carrying;
use zyr_tunnel::Reading;

/// Round trip below which a change is not worth a line.
///
/// A wait this short is not felt by anybody, and on a cable the round
/// trip wanders between a third of a millisecond and one, which doubles
/// and halves constantly while meaning nothing at all.
const NOTICEABLE: Duration = Duration::from_millis(5);

/// What has already been told about one session.
///
/// Losses start and do not stop, so the moment is the news and the count
/// is not: each kind is said once. The round trip is said whenever it
/// really changed, never at every reading.
#[derive(Debug, Default)]
pub struct Said {
    too_large: bool,
    dropped_from_the_queue: bool,
    round_trip: Duration,
}

impl Said {
    /// Nothing said yet about a session whose road starts that long.
    pub fn from(round_trip: Duration) -> Self {
        Self {
            round_trip,
            ..Self::default()
        }
    }

    /// What is worth writing down about that session now, `named` being
    /// how the journal calls it.
    ///
    /// Nothing at all when nothing changed, which is the ordinary case a
    /// few times a minute for as long as the session lasts.
    pub fn what_changed(&mut self, named: &str, reading: &Reading, path: &Carrying) -> Vec<String> {
        let mut said = Vec::new();

        if reading.too_large > 0 && !self.too_large {
            self.too_large = true;
            said.push(format!(
                "{named}: the path no longer carries packets the size the engine was told to \
                 send, so the picture is stopping. {} dropped, {} bytes of room left, {} \
                 narrowings seen",
                reading.too_large, path.usable_datagram, path.narrowings
            ));
        }

        let queued = reading.to_tunnel.saturating_sub(path.sent);
        if queued > 0 && !self.dropped_from_the_queue {
            self.dropped_from_the_queue = true;
            said.push(format!(
                "{named}: the path is not taking packets as fast as the engine makes them, \
                 {queued} sacrificed so far, round trip {} ms",
                path.round_trip.as_millis()
            ));
        }

        if worth_saying(self.round_trip, path.round_trip) {
            let before = self.round_trip;
            self.round_trip = path.round_trip;
            said.push(format!(
                "{named}: the road is now {} ms, it was {} ms",
                path.round_trip.as_millis(),
                before.as_millis()
            ));
        }

        said
    }
}

/// Whether a change in round trip is worth a line in the journal.
///
/// Doubling or halving, and only once the wait is long enough to be
/// felt. Anything smaller is the ordinary breathing of a network, and a
/// journal that reports breathing reports nothing.
fn worth_saying(before: Duration, now: Duration) -> bool {
    now.max(before) >= NOTICEABLE && (now >= before * 2 || before >= now * 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(sent: u64, round_trip: Duration) -> Carrying {
        Carrying {
            usable_datagram: 1162,
            narrowings: 0,
            sent,
            lost: 0,
            round_trip,
        }
    }

    fn reading(to_tunnel: u64, too_large: u64) -> Reading {
        Reading {
            to_tunnel,
            too_large,
            ..Reading::default()
        }
    }

    #[test]
    fn a_loss_is_said_once_and_not_at_every_reading() {
        let ms = Duration::from_millis;
        let mut said = Said::from(ms(4));

        // Rien à dire tant que rien n'est jeté.
        assert!(
            said.what_changed("way 1", &reading(100, 0), &path(100, ms(4)))
                .is_empty()
        );

        // Ce qui compte est le moment, pas le nombre : la perte est dite
        // une fois, et le compte qui grimpe ne se redit pas.
        let lines = said.what_changed("way 1", &reading(529, 0), &path(100, ms(4)));
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("429 sacrificed"), "{}", lines[0]);
        assert!(
            said.what_changed("way 1", &reading(900, 0), &path(100, ms(4)))
                .is_empty()
        );

        // L'autre espèce de perte se dit à son tour.
        let lines = said.what_changed("way 1", &reading(900, 3), &path(100, ms(4)));
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("no longer carries"), "{}", lines[0]);
    }

    #[test]
    fn only_a_road_that_really_changed_length_is_worth_a_line() {
        let ms = Duration::from_millis;
        let mut said = Said::from(ms(11));

        // Le cas qui a valu cette ligne : la session double de longueur
        // en cours de route parce que le chemin passe soudain ailleurs.
        let lines = said.what_changed("way 1", &reading(0, 0), &path(0, ms(24)));
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("now 24 ms, it was 11 ms"), "{}", lines[0]);

        // Un réseau qui respire n'est pas une nouvelle.
        assert!(
            said.what_changed("way 1", &reading(0, 0), &path(0, ms(28)))
                .is_empty()
        );

        // Et sur un câble, un tiers de milliseconde qui en devient une
        // double sans que personne ne sente quoi que ce soit.
        let mut said = Said::from(Duration::from_micros(300));
        assert!(
            said.what_changed("way 1", &reading(0, 0), &path(0, ms(1)))
                .is_empty()
        );
    }
}
