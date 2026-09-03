//! Congestion control fit for a real-time video stream.
//!
//! Ordinary congestion controllers cut their rate the moment they see a
//! loss: they assume the loss signals saturation and that slowing down
//! is the answer. For a file transfer, that is right. For interactive
//! video, it is ruinous: at 1% loss and 25 ms round trip, such a
//! controller converges to about 5 Mb/s, where a comfortable session
//! asks for forty. The stream is either strangled or its queue swells
//! into seconds of latency.
//!
//! This controller instead holds a window sized on what the session
//! genuinely needs to keep in flight: the rate times the round trip,
//! doubled for margin, plus enough room for a whole frame sent at once.
//!
//! Ignoring losses would be unreasonable for a stream able to saturate a
//! link. This one cannot: the rate is set by the encoder and never goes
//! past its target. The window is therefore not there to send more, only
//! to avoid blocking what the encoder already produces. Losses stay the
//! business of the video protocol's error correction, which exists for
//! exactly that.
//!
//! Wanted side effect: a wide window also defuses send pacing. Each
//! frame leaves as a burst of several dozen packets; a pacer would
//! spread them out, adding a steady jitter that the client's frame
//! pacing would then have to absorb.

use std::any::Any;
use std::sync::Arc;
use std::time::{Duration, Instant};

use quinn::congestion::{Controller, ControllerFactory};
use quinn_proto::RttEstimator;

/// Floor for the window, whatever the rate and the round trip.
const MINIMUM_WINDOW: u64 = 64 * 1024;

/// Longest silence from the far computer the picture should survive.
///
/// The window holds what has gone out and not yet been answered for, and
/// nothing goes out beyond it. A far computer busy with something else
/// for a moment stops answering for that moment, and everything the
/// encoder makes meanwhile has to fit in the window, then in the queue
/// behind it. Past both, the transport throws packets away, and what it
/// throws away is the oldest, which is the frame on its way out: that
/// frame arrives in pieces, cannot be rebuilt from them, and the picture
/// waits for a key frame that is cut short in its turn.
///
/// Sized on time rather than on a fixed number of bytes, because what
/// has to fit is a length of stream and not a length of anything else:
/// sixty-four kibibytes, which is what this was, is a tenth of a second
/// at five megabits and a hundred and fiftieth of one at eighty. And it
/// costs nothing while the path is healthy: what is in flight is on the
/// wire, not waiting anywhere, and the rate stays the encoder's own.
const LONGEST_STALL: Duration = Duration::from_millis(500);

/// Frames the send queue holds.
///
/// A key frame is bigger than the average frame the size is worked out
/// from, and whatever the encoder produces while that key frame is
/// going out has to fit behind it.
const QUEUED_FRAMES: u64 = 6;

/// Floor for the send queue, whatever the rate.
///
/// At a low rate the average frame is small and the key frame is not:
/// the ratio between them widens as the rate drops.
const MINIMUM_QUEUE: u64 = 256 * 1024;

/// Largest round trip the computation will take into account.
///
/// A wild reading, taken during a stall, would otherwise produce an
/// absurd window that takes a long time to come back down.
const MAXIMUM_RTT: Duration = Duration::from_millis(500);

/// Round trip assumed before the first measurement.
const INITIAL_RTT: Duration = Duration::from_millis(25);

/// Shape of the stream to carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaProfile {
    pub bits_per_second: u64,
    pub frames_per_second: u32,
}

impl Default for MediaProfile {
    fn default() -> Self {
        Self {
            bits_per_second: 20_000_000,
            frames_per_second: 60,
        }
    }
}

impl MediaProfile {
    /// Window this profile needs at the given round trip.
    ///
    /// Twice the bandwidth-delay product, plus one whole frame: the
    /// first term covers what is in flight, the second the burst of
    /// packets a frame amounts to. Never less than what the encoder
    /// makes while the far computer is not answering (`LONGEST_STALL`),
    /// which on a short road is far more than the other two.
    pub fn window(&self, rtt: Duration) -> u64 {
        let rtt = rtt.min(MAXIMUM_RTT);
        let in_flight = (self.bytes_per_second() as f64 * rtt.as_secs_f64()) as u64;
        in_flight
            .saturating_mul(2)
            .saturating_add(self.frame())
            .max(self.made_while_silent())
            .max(MINIMUM_WINDOW)
    }

    /// What the encoder makes while nothing is being answered.
    fn made_while_silent(&self) -> u64 {
        (self.bytes_per_second() as f64 * LONGEST_STALL.as_secs_f64()) as u64
    }

    /// Room the queue of datagrams waiting to go out needs.
    ///
    /// A frame leaves the encoder in one block, and the pump pushes it
    /// into that queue far faster than the transport puts it on the
    /// wire. A queue too short to hold a whole frame therefore loses
    /// part of every key frame, on the finest network there is, and that
    /// loss is not repairable: the video protocol's error correction
    /// covers a few packets of a frame, not a quarter of it. The picture
    /// then waits for the next key frame, which is cut short in its
    /// turn, and never comes up at all.
    ///
    /// What this costs is bounded by the same number of frames: on a
    /// path that genuinely cannot take the rate, the queue fills, the
    /// transport drops the oldest, and the delay through it never
    /// exceeds what those frames represent.
    pub fn send_queue(&self) -> usize {
        self.frame()
            .saturating_mul(QUEUED_FRAMES)
            .max(MINIMUM_QUEUE) as usize
    }

    fn bytes_per_second(&self) -> u64 {
        self.bits_per_second / 8
    }

    /// What one frame weighs on average at this rate.
    fn frame(&self) -> u64 {
        self.bytes_per_second() / self.frames_per_second.max(1) as u64
    }
}

/// Controller holding the window the stream needs.
#[derive(Debug, Clone)]
pub struct MediaController {
    profile: MediaProfile,
    rtt: Duration,
}

impl MediaController {
    pub fn new(profile: MediaProfile) -> Self {
        Self {
            profile,
            rtt: INITIAL_RTT,
        }
    }
}

impl Controller for MediaController {
    fn on_ack(
        &mut self,
        _now: Instant,
        _sent: Instant,
        _bytes: u64,
        _app_limited: bool,
        rtt: &RttEstimator,
    ) {
        self.rtt = rtt.get();
    }

    /// Losses do not shrink the window.
    ///
    /// Slowing down would not speed the video up: it is produced at a
    /// fixed rate, and its losses are repaired by the protocol's error
    /// correction. Cutting back would only add delay.
    fn on_congestion_event(
        &mut self,
        _now: Instant,
        _sent: Instant,
        _is_persistent_congestion: bool,
        _lost_bytes: u64,
    ) {
    }

    fn on_mtu_update(&mut self, _new_mtu: u16) {}

    fn window(&self) -> u64 {
        self.profile.window(self.rtt)
    }

    fn clone_box(&self) -> Box<dyn Controller> {
        Box::new(self.clone())
    }

    fn initial_window(&self) -> u64 {
        self.profile.window(INITIAL_RTT)
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

impl ControllerFactory for MediaProfile {
    fn build(self: Arc<Self>, _now: Instant, _current_mtu: u16) -> Box<dyn Controller> {
        Box::new(MediaController::new(*self))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Packet size used when comparing windows.
    const PACKET_SIZE: u16 = 1350;

    fn profile(mbps: u64) -> MediaProfile {
        MediaProfile {
            bits_per_second: mbps * 1_000_000,
            frames_per_second: 60,
        }
    }

    #[test]
    fn the_window_covers_what_is_in_flight() {
        // At 40 Mb/s and 25 ms there are 125 000 bytes in flight at any
        // moment: a shorter window would block the encoder.
        let window = profile(40).window(Duration::from_millis(25));
        assert!(
            window >= 250_000,
            "{window} bytes, less than twice the flight"
        );
    }

    #[test]
    fn the_window_absorbs_a_whole_frame() {
        // Even at a negligible round trip, a frame leaves in one go.
        let profile = profile(40);
        let frame = profile.bits_per_second / 8 / 60;
        assert!(profile.window(Duration::ZERO) >= frame);
    }

    #[test]
    fn the_window_follows_the_rate_and_the_round_trip() {
        // La route ne compte qu'une fois qu'elle coûte plus que le
        // plancher, qui est déjà une demi-seconde de flux : sur un
        // réseau local, deux routes de longueurs différentes donnent la
        // même fenêtre, et c'est voulu.
        let short = profile(40).window(Duration::from_millis(250));
        let long = profile(40).window(Duration::from_millis(500));
        assert!(long > short);

        let slow = profile(10).window(Duration::from_millis(25));
        let fast = profile(80).window(Duration::from_millis(25));
        assert!(fast > slow);
    }

    #[test]
    fn the_window_holds_what_the_encoder_makes_while_nothing_answers() {
        // Le relevé qui a valu cette règle : l'ordinateur regardé a jeté
        // huit cent trente-cinq paquets vidéo d'un coup, sur un réseau
        // local à sept millisecondes, pendant que celui qui regardait
        // était occupé ailleurs. Une fenêtre qui ne tient qu'un
        // trentième de seconde de flux s'épuise au premier hoquet, et
        // tout ce que l'encodeur fait pendant ce temps finit à la
        // poubelle.
        for mbps in [5, 20, 40, 80, 150] {
            let profile = profile(mbps);
            let half_a_second = profile.bits_per_second / 8 / 2;
            for rtt in [Duration::from_micros(300), Duration::from_millis(25)] {
                assert!(
                    profile.window(rtt) >= half_a_second,
                    "{} octets de fenêtre à {mbps} Mb/s sur {rtt:?}, pour {half_a_second} \
                     octets de flux en une demi-seconde",
                    profile.window(rtt)
                );
            }
        }
    }

    #[test]
    fn the_send_queue_holds_whole_frames() {
        // La propriété qui compte, et qui manquait : une file plus
        // courte qu'une image perd des paquets de chaque image clé sur
        // le meilleur des réseaux, la correction d'erreurs ne rattrape
        // pas un quart d'image, et le lecteur attend une image clé qui
        // est coupée à son tour. L'image ne s'établit jamais.
        for mbps in [5, 20, 40, 80, 150] {
            let profile = profile(mbps);
            let frame = profile.bits_per_second / 8 / 60;
            assert!(
                profile.send_queue() as u64 >= frame * 2,
                "{} octets de file pour des images de {frame} à {mbps} Mb/s",
                profile.send_queue()
            );
        }
    }

    #[test]
    fn a_wild_round_trip_does_not_blow_the_window_up() {
        let profile = profile(40);
        let capped = profile.window(MAXIMUM_RTT);
        assert_eq!(profile.window(Duration::from_secs(30)), capped);
    }

    #[test]
    fn a_degenerate_profile_stays_usable() {
        let profile = MediaProfile {
            bits_per_second: 0,
            frames_per_second: 0,
        };
        assert_eq!(profile.window(Duration::from_millis(25)), MINIMUM_WINDOW);
    }

    #[test]
    fn losses_do_not_shrink_the_window() {
        let mut controller = MediaController::new(profile(40));
        let before = controller.window();
        let now = Instant::now();
        for _ in 0..100 {
            controller.on_congestion_event(now, now, true, 100_000);
        }
        assert_eq!(controller.window(), before);
    }

    #[test]
    fn the_initial_window_is_already_usable() {
        let controller = MediaController::new(profile(40));
        assert_eq!(controller.initial_window(), controller.window());
        assert!(controller.initial_window() >= MINIMUM_WINDOW);
    }

    #[test]
    fn an_ordinary_controller_collapses_where_ours_holds() {
        // This is the whole reason this module exists, and the property
        // the decision to tunnel everything rests on. It is checked here
        // against the transport's own default controller.
        let profile = profile(40);
        let rtt = Duration::from_millis(25);
        let needed = profile.bits_per_second / 8 * 25 / 1000;

        let now = Instant::now();
        let mut ordinary =
            Arc::new(quinn::congestion::CubicConfig::default()).build(now, PACKET_SIZE);
        let mut ours = MediaController::new(profile);

        // Thirty-odd losses, roughly what 1% loss produces over a few
        // seconds of video.
        for _ in 0..30 {
            ordinary.on_congestion_event(now, now, false, PACKET_SIZE as u64);
            ours.on_congestion_event(now, now, false, PACKET_SIZE as u64);
        }

        assert!(
            ordinary.window() < needed,
            "the ordinary controller holds {} bytes, it should not have",
            ordinary.window()
        );
        assert!(
            ours.window() >= needed,
            "{} bytes only, {needed} are needed to hold 40 Mb/s at 25 ms",
            ours.window()
        );
        assert_eq!(ours.window(), profile.window(rtt));
    }
}
