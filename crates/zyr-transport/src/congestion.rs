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
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use quinn::congestion::{Controller, ControllerFactory};
use quinn_proto::RttEstimator;
use zyr_proto::session::RATES_OFFERED;

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
/// Of the computer being watched, and of it alone: `Sending` says why.
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

/// What one end of a tunnel sends, which is what its queue is sized on.
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

/// Controller holding the window the stream needs.
#[derive(Debug, Clone)]
pub struct MediaController {
    media: Media,
    rtt: Duration,
}

impl MediaController {
    pub fn new(media: impl Into<Media>) -> Self {
        Self {
            media: media.into(),
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
        self.media.now().window(self.rtt)
    }

    /// The copy shares what the original reads: the transport clones its
    /// controller where it pleases, and a copy holding a rate of its own
    /// would stop following the session the moment it was made.
    fn clone_box(&self) -> Box<dyn Controller> {
        Box::new(self.clone())
    }

    fn initial_window(&self) -> u64 {
        self.media.now().window(INITIAL_RTT)
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

impl ControllerFactory for Media {
    fn build(self: Arc<Self>, _now: Instant, _current_mtu: u16) -> Box<dyn Controller> {
        Box::new(MediaController::new(Media::clone(&self)))
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
        //
        // Vérifié sur la file réellement construite, celle du flux le
        // plus rapide, et pour chaque débit offert : c'est la seule
        // qu'une connexion aura jamais, quel que soit le débit demandé
        // après coup.
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
        // Le 4 septembre, le client a jeté 23805 des 38565 paquets qu'il
        // a confiés au tunnel, soit 62 %, alors qu'il n'envoyait que du
        // clavier-souris. Sa file était celle du flux le plus rapide,
        // taillée pour ne pas couper une image clé chez celui qui
        // encode : un mégaoctet de paquets de cinquante octets, ce sont
        // des milliers de paquets et des dizaines de secondes de retard.
        // Et le canal qui les porte est fiable : tout ce que la file
        // jetait en silence était renvoyé, et le renvoi tombait dans la
        // même file pleine.
        assert!(
            Sending::Inputs.queue() * 8 < Sending::Pictures.queue(),
            "les deux côtés ont presque la même file : {} contre {}",
            Sending::Inputs.queue(),
            Sending::Pictures.queue()
        );
        // Assez large tout de même pour la rafale que fait une main sur
        // une souris, sans quoi on aurait échangé une panne contre une
        // autre.
        assert!(Sending::Inputs.queue() >= 16 * 1024);
        // Et celle de l'ordinateur regardé ne bouge pas : elle tient six
        // images du flux le plus rapide, ce que le test au-dessus vérifie.
        assert_eq!(Sending::Pictures.queue(), FASTEST.send_queue());
    }

    #[test]
    fn the_window_follows_the_session_that_is_being_served() {
        // Le défaut que ceci répare : l'ordinateur regardé ouvre son
        // tunnel au démarrage du service, bien avant qu'une session lui
        // demande quoi que ce soit, et gardait donc la fenêtre du profil
        // nominal pendant qu'il servait à quatre-vingts mégabits.
        let media = Media::from(profile(20));
        let controller = MediaController::new(media.clone());
        let nominal = controller.window();

        media.serving(profile(80));
        assert!(
            controller.window() > nominal,
            "la fenêtre est restée à {nominal} octets pendant que la session passait à 80 Mb/s"
        );
        assert_eq!(controller.window(), profile(80).window(INITIAL_RTT));

        // Et la copie que le transport se fait de son contrôleur suit la
        // même session : sans cela, la fenêtre se figerait au premier
        // clonage.
        let copy = controller.clone_box();
        media.serving_at(profile(40).bits_per_second);
        assert_eq!(copy.window(), controller.window());

        // Plus personne à servir : elle revient à ce avec quoi la porte
        // a été construite, et n'hérite pas de la session précédente.
        media.serving_nobody();
        assert_eq!(controller.window(), nominal);
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
