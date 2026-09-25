//! What a session says about itself, on both sides of it.
//!
//! A tunnel throws packets away in four places, and all of them are
//! silent. A packet too large for the path, thrown away rather than cut
//! in two; the send queue overflowing, where the transport sacrifices the
//! oldest; the queue towards the local link overflowing, where the tunnel
//! does the same for an engine or a player that is not reading fast
//! enough; and a word for the service that the service did not take in
//! time. None ends the session, and that is exactly why they have to be
//! said: a session that dies leaves a reason behind, a session that goes
//! quiet leaves nothing at all.
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

/// Readings in a row with nothing arriving before it is worth saying.
///
/// One is a hiccup and says nothing; three of them is several seconds
/// during which the far computer sent not one packet, which is not
/// something a live session does. Kept below the wait the engines
/// themselves give up after, so that the journal names the silence
/// before the session dies of it rather than after.
const QUIET_READINGS: u32 = 3;

/// What has already been told about one session.
///
/// Losses start and do not stop, so the moment is the news and the count
/// is not: each kind is said once. The round trip is said whenever it
/// really changed, never at every reading.
#[derive(Debug, Default)]
pub struct Said {
    too_large: bool,
    crowded: bool,
    crowded_here: bool,
    service_dropped: bool,
    round_trip: Duration,
    /// What had arrived at the previous reading, and how many readings
    /// in a row have added nothing to it.
    arrived: u64,
    quiet: u32,
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

        if reading.crowded > 0 && !self.crowded {
            self.crowded = true;
            said.push(format!(
                "{named}: the path is not taking packets as fast as the engine makes them, so the \
                 transport is throwing the oldest away, {} so far, round trip {} ms, {} bytes may \
                 be out unanswered at once",
                reading.crowded,
                path.round_trip.as_millis(),
                path.window
            ));
        }

        if reading.crowded_here > 0 && !self.crowded_here {
            self.crowded_here = true;
            said.push(format!(
                "{named}: what is at the other end of the local link is not reading as fast as \
                 the tunnel brings pictures in, so the oldest are thrown away here, {} so far",
                reading.crowded_here
            ));
        }

        if reading.service_dropped > 0 && !self.service_dropped {
            self.service_dropped = true;
            said.push(format!(
                "{named}: the service is not hearing what comes over the local link in time, {} \
                 of its messages thrown away so far",
                reading.service_dropped
            ));
        }

        // A session that has stopped arriving, which nothing else says.
        // The tunnel holds, the connection holds, the road is as short
        // as ever, and not one packet comes: from this end that is a
        // session in perfect health showing nothing at all, and it is
        // the shape every failure at the far end takes here. Counted
        // only once something has arrived: the first seconds of a
        // session are silent by construction and are not a fault.
        // Pictures on the side watching, the player's words on the side
        // being watched: whatever the far end sends reaches the link.
        let arrived = reading.to_link + reading.control_to_link;
        if arrived > 0 {
            if arrived == self.arrived {
                self.quiet += 1;
                if self.quiet == QUIET_READINGS {
                    said.push(format!(
                        "{named}: nothing has come from the far computer for several seconds, \
                         though the way is open and {arrived} packets came before that"
                    ));
                }
            } else {
                if self.quiet >= QUIET_READINGS {
                    said.push(format!("{named}: the far computer is sending again"));
                }
                self.quiet = 0;
                self.arrived = arrived;
            }
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

/// Everything one session carried, for the line written when it ends.
///
/// A session that ends takes its counters with it, and they are the only
/// place where a network that lost packets, a tunnel that threw them
/// away and an engine that never received them read differently. Said
/// once, at the end, whether it ended well or badly.
pub fn carried(reading: &Reading, path: &Carrying) -> String {
    format!(
        "{} packets into the tunnel, {} of them onto the wire, {} thrown away for want of room, \
         {} too large; {} onto the local link, {} thrown away there, {} with nobody to take them, \
         {} unreadable; {} pieces of the control stream out and {} in; {} streams refused, {} \
         service messages lost; {} bytes of room in a packet, {} narrowings, {} lost on the path, \
         {} bytes out unanswered at once, round trip {} ms",
        reading.to_tunnel,
        path.sent,
        reading.crowded,
        reading.too_large,
        reading.to_link,
        reading.crowded_here,
        reading.no_recipient,
        reading.unreadable,
        reading.control_to_tunnel,
        reading.control_to_link,
        reading.refused,
        reading.service_dropped,
        path.usable_datagram,
        path.narrowings,
        path.lost,
        path.window,
        path.round_trip.as_millis()
    )
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

    fn path(round_trip: Duration) -> Carrying {
        Carrying {
            usable_datagram: 1162,
            narrowings: 0,
            sent: 0,
            lost: 0,
            window: 256 * 1024,
            round_trip,
        }
    }

    fn reading(crowded: u64, too_large: u64) -> Reading {
        Reading {
            crowded,
            too_large,
            ..Reading::default()
        }
    }

    fn arriving(to_link: u64) -> Reading {
        Reading {
            to_link,
            ..Reading::default()
        }
    }

    #[test]
    fn a_session_that_stops_arriving_is_said_once_and_says_when_it_comes_back() {
        let ms = Duration::from_millis;
        let mut said = Said::from(ms(4));

        // The silence at the start is not one: nothing has left the far
        // side yet, and a session that is opening is not reported.
        for _ in 0..5 {
            assert!(
                said.what_changed("way 1", &arriving(0), &path(ms(4)))
                    .is_empty()
            );
        }

        // Then it arrives, and it stops.
        assert!(
            said.what_changed("way 1", &arriving(1_000), &path(ms(4)))
                .is_empty()
        );
        let mut lines = Vec::new();
        for _ in 0..5 {
            lines.extend(said.what_changed("way 1", &arriving(1_000), &path(ms(4))));
        }
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].contains("nothing has come"), "{}", lines[0]);

        // And the return is said, otherwise the journal would suggest
        // the session died where it only coughed.
        let lines = said.what_changed("way 1", &arriving(1_200), &path(ms(4)));
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].contains("sending again"), "{}", lines[0]);
    }

    #[test]
    fn a_loss_is_said_once_and_not_at_every_reading() {
        let ms = Duration::from_millis;
        let mut said = Said::from(ms(4));

        // Nothing to say while nothing is thrown away.
        assert!(
            said.what_changed("way 1", &reading(0, 0), &path(ms(4)))
                .is_empty()
        );

        // What matters is the moment, not the number: the loss is said
        // once, and the rising count is not said again.
        let lines = said.what_changed("way 1", &reading(429, 0), &path(ms(4)));
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("429 so far"), "{}", lines[0]);
        assert!(
            said.what_changed("way 1", &reading(900, 0), &path(ms(4)))
                .is_empty()
        );

        // The other kind of loss is said in its turn.
        let lines = said.what_changed("way 1", &reading(900, 3), &path(ms(4)));
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("no longer carries"), "{}", lines[0]);

        // And so are the two this side makes itself, once each.
        let here = Reading {
            crowded: 900,
            too_large: 3,
            crowded_here: 12,
            service_dropped: 1,
            ..Reading::default()
        };
        let lines = said.what_changed("way 1", &here, &path(ms(4)));
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines[0].contains("12 so far"), "{}", lines[0]);
        assert!(lines[1].contains("1 of its messages"), "{}", lines[1]);
        assert!(said.what_changed("way 1", &here, &path(ms(4))).is_empty());
    }

    #[test]
    fn the_player_s_words_count_as_the_far_computer_speaking() {
        // On the computer being watched, nothing but the player's words
        // come from the far end: a session whose player pings every half
        // second is not a silent one.
        let ms = Duration::from_millis;
        let mut said = Said::from(ms(4));
        let mut words = Reading::default();
        for heard in 1..6 {
            words.control_to_link = heard;
            assert!(said.what_changed("way 1", &words, &path(ms(4))).is_empty());
        }
    }

    #[test]
    fn only_a_road_that_really_changed_length_is_worth_a_line() {
        let ms = Duration::from_millis;
        let mut said = Said::from(ms(11));

        // The case this line exists for: the session doubles in length
        // along the way because the path suddenly goes somewhere else.
        let lines = said.what_changed("way 1", &reading(0, 0), &path(ms(24)));
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("now 24 ms, it was 11 ms"), "{}", lines[0]);

        // A network breathing is not news.
        assert!(
            said.what_changed("way 1", &reading(0, 0), &path(ms(28)))
                .is_empty()
        );

        // And on a cable, a third of a millisecond that becomes a
        // whole one has doubled without anybody feeling anything at
        // all.
        let mut said = Said::from(Duration::from_micros(300));
        assert!(
            said.what_changed("way 1", &reading(0, 0), &path(ms(1)))
                .is_empty()
        );
    }
}
