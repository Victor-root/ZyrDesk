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
//! This controller instead holds a window nothing fills for as long as
//! the connection lives. Ignoring losses would be unreasonable for a
//! stream able to saturate a link. This one cannot: the rate is set by
//! the encoder and never goes past its target. The window is therefore
//! not there to send more, only never to hold back what the encoder
//! already produces. Losses stay the business of the video protocol's
//! error correction, which exists for exactly that.
//!
//! It used to hold half a second of the stream, on the reasoning that a
//! far computer busy elsewhere stops answering for about that long, and
//! what went out meanwhile had to fit. What that missed is what a full
//! window does once the road has been silent longer than that: nothing
//! but the transport's own probes leaves, and the transport spaces those
//! further and further apart, doubling each time. Seven seconds into a
//! silence the next probe is five seconds away, and the road coming back
//! changes nothing until that probe has gone out and been answered. On
//! the fourth of September the road came back seven seconds into a
//! silence and the computer watching stayed mute for three more, which
//! is exactly what the client engine of the time did not survive: it
//! gave up after ten. The window is now what the connection could
//! possibly have out before it dies of the same silence, and no hiccup
//! ever fills it.
//!
//! Wanted side effect: a wide window also defuses send pacing. Each
//! frame leaves as a burst of several dozen packets; a pacer would
//! spread them out, adding a steady jitter that the client's frame
//! pacing would then have to absorb.

use std::any::Any;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::Instant;

use quinn::congestion::{Controller, ControllerFactory};
use quinn_proto::RttEstimator;
use zyr_proto::session::RATES_OFFERED;

use crate::endpoint::MAXIMUM_IDLE;

/// Floor for the window, whatever the rate.
const MINIMUM_WINDOW: u64 = 64 * 1024;

/// What travels beside the picture and is not counted in its rate: the
/// repair the engine adds to every frame, the sound, the headers of each
/// packet, and the key frames that overshoot. Twice the rate covers all
/// of it with room to spare.
const BESIDE_THE_PICTURE: u64 = 2;

/// Frames the send queue holds.
///
/// A key frame is bigger than the average frame the size is worked out
/// from, and whatever the encoder produces while that key frame is
/// going out has to fit behind it.
const QUEUED_FRAMES: u64 = 6;

/// Shape of the stream to carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaProfile {
    pub bits_per_second: u64,
    pub frames_per_second: u32,
}

/// The fastest stream a session may ask for.
///
/// Read from the one place the rates offered are written down, so that
/// widening what the person may ask for widens what the tunnel is built
/// to carry, and the two can never drift apart.
///
/// This is what the send queue is sized on, and it is the one thing here
/// that cannot follow the session: the queue is settled when a
/// connection is made and never moves again, while the rate does. The
/// computer being watched opens its tunnel once, when the service
/// starts, long before anybody asks it for a picture; sizing that queue
/// on the rate of the moment sized it on a rate nobody had asked for,
/// and a queue too short to hold a whole frame chops every key frame it
/// carries. Too long only costs a megabyte and some staleness after a
/// stall the session would otherwise not have survived at all.
///
/// The queue of the computer being watched, and of it alone; and the
/// window of the computer watching, whose own stream is no stream at
/// all. `Sending` says why.
pub const FASTEST: MediaProfile = MediaProfile {
    bits_per_second: RATES_OFFERED[RATES_OFFERED.len() - 1] as u64 * 1_000,
    frames_per_second: 60,
};

/// Room the queue of datagrams waiting to go out needs on the computer
/// that is watching.
///
/// What leaves that one is the keyboard, the mouse and the engine's
/// control channel: a few dozen bytes at a time, and never a picture.
/// Sized on the stream it receives instead, its queue held a megabyte of
/// them, which is thousands of packets and tens of seconds of staleness.
///
/// And staleness there is not what it is on the other side. The channel
/// carrying the inputs is a reliable one: it numbers its packets and
/// sends again what is not answered for. A queue deeper than that
/// channel's patience therefore turns one lost input into two, then
/// four: the transport quietly drops the oldest, the engine hears
/// nothing back, sends it again, and the new copy queues behind the
/// copies before it. What was a road gone briefly bad becomes a machine
/// talking to itself. Short enough that the engine loses an input once
/// and moves on, wide enough to take the burst a hand on a mouse makes.
pub const WATCHING_QUEUE: usize = 32 * 1024;

/// What one end of a tunnel sends, which is what its queue and its window
/// are sized on.
///
/// The two ends of a session are not alike and never were: one sends a
/// picture and the other sends a hand. One queue for both was a queue
/// right for whichever end it was written for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sending {
    /// The picture. The computer being watched, and the relay branch
    /// that carries what it sends.
    Pictures,
    /// The keyboard and the mouse. The computer watching.
    Inputs,
}

impl Sending {
    /// Room its queue of datagrams needs.
    pub fn queue(self) -> usize {
        match self {
            Sending::Pictures => FASTEST.send_queue(),
            Sending::Inputs => WATCHING_QUEUE,
        }
    }
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
    /// Window this profile needs: twice what the stream makes for as long
    /// as the connection can go unanswered.
    ///
    /// What is in flight is what has gone out and not been answered for,
    /// and nothing goes out beyond the window. The far computer may stop
    /// answering for the whole of the transport's idle limit before the
    /// connection dies of it, everything sent meanwhile is in flight, and
    /// all of it has to fit: past the window the transport sends nothing
    /// but its probes, spaced out for seconds, and a silence the
    /// connection would have survived becomes one the engines do not. It
    /// costs nothing while the road is healthy: what is in flight is on
    /// the wire, not waiting anywhere, and the rate stays the encoder's
    /// own.
    pub fn window(&self) -> u64 {
        self.bytes_per_second()
            .saturating_mul(MAXIMUM_IDLE.as_secs())
            .saturating_mul(BESIDE_THE_PICTURE)
            .max(MINIMUM_WINDOW)
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
    /// Asked of `FASTEST` and of nothing else, since a queue is settled
    /// when a connection is made and the rate is not. What it costs a
    /// slower session is a tenth of a second of the fastest stream held
    /// behind a window that is already half a second of its own: on a
    /// path that genuinely cannot take the rate, the queue fills, the
    /// transport drops the oldest, and neither of the two grows without
    /// end.
    pub fn send_queue(&self) -> usize {
        self.frame().saturating_mul(QUEUED_FRAMES) as usize
    }

    fn bytes_per_second(&self) -> u64 {
        self.bits_per_second / 8
    }

    /// What one frame weighs on average at this rate.
    fn frame(&self) -> u64 {
        self.bytes_per_second() / self.frames_per_second.max(1) as u64
    }
}

/// The shape of the stream to carry, as it stands right now.
///
/// A window is worked out from a rate, and the rate is not settled when
/// a connection is made. The computer being watched opens its tunnel
/// once, when the service starts, and only learns what it is serving
/// when a session says so; the person then moves the rate under it while
/// the session runs. A profile frozen at the connection was therefore
/// the nominal one on the whole of the watched side, whatever the
/// session actually asked for: at eighty megabits it held an eighth of a
/// second of stream where it was meant to hold half of one, and every
/// hiccup longer than that cost the picture a key frame.
///
/// Cloned by the handful and shared: the door hands one to its tunnel
/// and one to every branch of relay carrying its sessions, and keeps one
/// to tell them all at once what this computer is being asked for.
#[derive(Debug, Clone)]
pub struct Media(Arc<Live>);

#[derive(Debug)]
struct Live {
    bits_per_second: AtomicU64,
    frames_per_second: AtomicU32,
    /// What it was built with, and what it goes back to once nothing is
    /// being served through it any more.
    built: MediaProfile,
}

impl From<MediaProfile> for Media {
    fn from(profile: MediaProfile) -> Self {
        Self(Arc::new(Live {
            bits_per_second: AtomicU64::new(profile.bits_per_second),
            frames_per_second: AtomicU32::new(profile.frames_per_second),
            built: profile,
        }))
    }
}

impl Default for Media {
    fn default() -> Self {
        MediaProfile::default().into()
    }
}

impl Media {
    /// What the tunnel is being asked to carry right now.
    pub fn now(&self) -> MediaProfile {
        MediaProfile {
            bits_per_second: self.0.bits_per_second.load(Ordering::Relaxed),
            frames_per_second: self.0.frames_per_second.load(Ordering::Relaxed),
        }
    }

    /// A session is opening, and this is what it asked to be served.
    pub fn serving(&self, profile: MediaProfile) {
        self.0
            .bits_per_second
            .store(profile.bits_per_second, Ordering::Relaxed);
        self.0
            .frames_per_second
            .store(profile.frames_per_second, Ordering::Relaxed);
    }

    /// The rate changed under a session already running.
    ///
    /// Its cadence does not: what the person moves in the middle of a
    /// session is the rate, and the picture goes on being made at the
    /// rhythm of the screen it lands on.
    pub fn serving_at(&self, bits_per_second: u64) {
        self.0
            .bits_per_second
            .store(bits_per_second, Ordering::Relaxed);
    }

    /// Nothing is being served through it any more.
    pub fn serving_nobody(&self) {
        self.serving(self.0.built);
    }
}

/// Controller holding the window this end needs.
#[derive(Debug, Clone)]
pub struct MediaController {
    media: Media,
    sending: Sending,
}

impl MediaController {
    pub fn new(media: impl Into<Media>, sending: Sending) -> Self {
        Self {
            media: media.into(),
            sending,
        }
    }
}

impl Controller for MediaController {
    /// Nothing is read from an acknowledgement: the window is sized on
    /// the stream and on the idle limit, and the length of the road is
    /// no part of it.
    fn on_ack(
        &mut self,
        _now: Instant,
        _sent: Instant,
        _bytes: u64,
        _app_limited: bool,
        _rtt: &RttEstimator,
    ) {
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

    /// The session's stream for the end that sends the picture, and the
    /// fastest stream there is for the end that sends a hand.
    ///
    /// What that end sends never comes near it, and that is the point:
    /// its window is not there to hold anything back, not even the
    /// engine's control channel sending everything again at every turn
    /// of its clock while the road is silent, which is what it does.
    fn window(&self) -> u64 {
        match self.sending {
            Sending::Pictures => self.media.now().window(),
            Sending::Inputs => FASTEST.window(),
        }
    }

    /// The copy shares what the original reads: the transport clones its
    /// controller where it pleases, and a copy holding a rate of its own
    /// would stop following the session the moment it was made.
    fn clone_box(&self) -> Box<dyn Controller> {
        Box::new(self.clone())
    }

    fn initial_window(&self) -> u64 {
        self.window()
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

impl ControllerFactory for MediaController {
    fn build(self: Arc<Self>, _now: Instant, _current_mtu: u16) -> Box<dyn Controller> {
        Box::new((*self).clone())
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
    fn the_window_follows_the_rate() {
        let slow = profile(10).window();
        let fast = profile(80).window();
        assert!(fast > slow);
    }

    #[test]
    fn the_window_holds_everything_the_stream_makes_while_the_connection_lives() {
        // The record that earned this rule: on 4 September, the road came
        // back seven seconds after going silent, and the computer
        // watching stayed mute for three more seconds, its window being
        // full and the transport sending nothing but its probes, spaced
        // several seconds apart. The client engine of the time gave up
        // after ten seconds of silence: the session died of an outage
        // that the tunnel, for its part, had got through. The window therefore
        // holds everything the stream makes during the longest silence a
        // connection survives, the transport's idle limit, with room to
        // spare for what travels beside the picture.
        for mbps in [5, 20, 40, 80, 150] {
            let profile = profile(mbps);
            let over_the_silence = profile.bits_per_second / 8 * MAXIMUM_IDLE.as_secs();
            assert!(
                profile.window() >= over_the_silence * 2,
                "{} octets de fenêtre à {mbps} Mb/s, pour {over_the_silence} octets de flux \
                 pendant la limite d'inactivité",
                profile.window()
            );
        }
    }

    #[test]
    fn the_side_that_sends_no_picture_holds_the_fastest_streams_window() {
        // What this computer sends never comes near this window, and
        // that is the point: while the road is silent, the engine's
        // control channel sends again everything not acknowledged at
        // every turn of its clock, and a window sized on the bitrate
        // asked for filled up with those resends in a few seconds.
        let media = Media::from(profile(5));
        let watching = MediaController::new(media.clone(), Sending::Inputs);
        assert_eq!(watching.window(), FASTEST.window());
        assert!(watching.window() > profile(5).window());

        // And the session's bitrate changes nothing here: it is not
        // its stream.
        media.serving(profile(80));
        assert_eq!(watching.window(), FASTEST.window());
    }

    #[test]
    fn the_send_queue_holds_whole_frames() {
        // The property that matters, and that was missing: a queue
        // shorter than a frame loses packets from every key frame on
        // the best of networks, error correction does not make up for
        // a quarter of a frame, and the player waits for a key frame
        // that is cut short in its turn. The picture never comes up.
        //
        // Checked on the queue actually built, the fastest stream's,
        // and for every bitrate offered: it is the only one a
        // connection will ever have, whatever bitrate is asked for
        // afterwards.
        for kbps in RATES_OFFERED {
            let frame = u64::from(kbps) * 1_000 / 8 / 60;
            assert!(
                FASTEST.send_queue() as u64 >= frame * 2,
                "{} octets de file pour des images de {frame} à {kbps} kb/s",
                FASTEST.send_queue()
            );
        }
    }

    #[test]
    fn the_side_that_sends_no_picture_gets_a_queue_its_own_size() {
        // On 4 September, the client threw away 23805 of the 38565
        // packets it handed to the tunnel, that is 62%, when all it was
        // sending was keyboard and mouse. Its queue was the fastest
        // stream's, sized not to cut a key frame on the side that
        // encodes: a megabyte of fifty-byte packets is thousands of
        // packets and tens of seconds of delay. And the channel carrying
        // them is reliable: everything the queue silently threw away was
        // sent again, and the resend fell into the same full queue.
        assert!(
            Sending::Inputs.queue() * 8 < Sending::Pictures.queue(),
            "les deux côtés ont presque la même file : {} contre {}",
            Sending::Inputs.queue(),
            Sending::Pictures.queue()
        );
        // Wide enough all the same for the burst a hand on a mouse
        // makes, or one failure would have been traded for another.
        assert!(Sending::Inputs.queue() >= 16 * 1024);
        // And the queue of the computer being watched does not move: it
        // holds six frames of the fastest stream, which the test above
        // checks.
        assert_eq!(Sending::Pictures.queue(), FASTEST.send_queue());
    }

    #[test]
    fn the_window_follows_the_session_that_is_being_served() {
        // The fault this repairs: the computer being watched opens its
        // tunnel when the service starts, long before a session asks it
        // for anything, and so kept the nominal profile's window while
        // it was serving at eighty megabits.
        let media = Media::from(profile(20));
        let controller = MediaController::new(media.clone(), Sending::Pictures);
        let nominal = controller.window();

        media.serving(profile(80));
        assert!(
            controller.window() > nominal,
            "la fenêtre est restée à {nominal} octets pendant que la session passait à 80 Mb/s"
        );
        assert_eq!(controller.window(), profile(80).window());

        // And the copy the transport makes of its controller follows the
        // same session: without that, the window would freeze at the
        // first clone.
        let copy = controller.clone_box();
        media.serving_at(profile(40).bits_per_second);
        assert_eq!(copy.window(), controller.window());

        // Nobody left to serve: it goes back to what the door was built
        // with, and does not inherit from the previous session.
        media.serving_nobody();
        assert_eq!(controller.window(), nominal);
    }

    #[test]
    fn a_degenerate_profile_stays_usable() {
        let profile = MediaProfile {
            bits_per_second: 0,
            frames_per_second: 0,
        };
        assert_eq!(profile.window(), MINIMUM_WINDOW);
    }

    #[test]
    fn losses_do_not_shrink_the_window() {
        let mut controller = MediaController::new(profile(40), Sending::Pictures);
        let before = controller.window();
        let now = Instant::now();
        for _ in 0..100 {
            controller.on_congestion_event(now, now, true, 100_000);
        }
        assert_eq!(controller.window(), before);
    }

    #[test]
    fn the_initial_window_is_already_usable() {
        let controller = MediaController::new(profile(40), Sending::Pictures);
        assert_eq!(controller.initial_window(), controller.window());
        assert!(controller.initial_window() >= MINIMUM_WINDOW);
    }

    #[test]
    fn an_ordinary_controller_collapses_where_ours_holds() {
        // This is the whole reason this module exists, and the property
        // the decision to tunnel everything rests on. It is checked here
        // against the transport's own default controller.
        let profile = profile(40);
        let needed = profile.bits_per_second / 8 * 25 / 1000;

        let now = Instant::now();
        let mut ordinary =
            Arc::new(quinn::congestion::CubicConfig::default()).build(now, PACKET_SIZE);
        let mut ours = MediaController::new(profile, Sending::Pictures);

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
        assert_eq!(ours.window(), profile.window());
    }
}
