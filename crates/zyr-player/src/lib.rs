//! Client side of the ZyrDesk engine: the picture and the sound of the
//! computer being watched, and the keyboard and mouse sent back to it.
//!
//! A [`Player`] talks to the engine through the local link the service
//! holds open for it, and to nothing else. It keeps four threads of its
//! own: the link, which reads and writes the pipe on a small runtime;
//! the video, which assembles, decodes and draws on the same thread, so
//! that a picture never waits for another thread to pick it up; the
//! sound; and a timer that takes the measures, apart from the picture so
//! that they keep coming when the picture stops.
//!
//! Nothing a window calls ever waits on any of them: every method sends
//! a message or reads a copy. What happens comes back as [`Event`]s,
//! from one of the player's threads, for the window to take to its own.
//!
//! On Windows the graphics card decodes and draws, into the window it
//! is given. Headless, the processor decodes and nothing is drawn: what
//! the tests and the diagnostic command line use.

mod audio;
mod link;
mod present;
mod seldom;
mod stats;
mod tallies;
#[cfg(test)]
mod testing;
#[cfg(test)]
mod tests;
mod video;
#[cfg(windows)]
mod windows;

use std::fmt;
use std::io;
use std::panic::AssertUnwindSafe;
use std::sync::mpsc::{Receiver, sync_channel};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use bytes::Bytes;
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use zyr_codec::{CodecError, Ffmpeg};
use zyr_proto::log::Log;

pub use present::Rect;
pub use tallies::{LinkTallies, PictureTallies, SoundTallies, Tallies};
pub use zyr_media::codec::{CodecChoice, CodecSet, VideoCodec};
pub use zyr_media::control::Wanted;
pub use zyr_media::input::{Button, InputEvent};
pub use zyr_media::stats::Measures;

use audio::{Muted, Sound};
use link::{Inner, Order};
use present::{Headless, Presenter};
use stats::{Clock, Tally};
use video::{Said, Video, VideoInput};

/// The tag of the player's lines in the journal.
const TAG: &str = "player";

/// Video datagrams waiting for the video thread at most: two large key
/// frames' worth. Past that the thread is not keeping up, and holding
/// more would only make the picture later.
const VIDEO_WAITING: usize = 4096;

/// Sound datagrams waiting for the sound thread at most: half a second.
const SOUND_WAITING: usize = 50;

/// Where the pictures go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    /// A window of this process, drawn into by the graphics card.
    Window { hwnd: isize },
    /// Nowhere: decoded on the processor, counted, never drawn.
    Headless,
}

/// What the player tells the window, from one of its own threads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A stream starts, with pictures of this size.
    Streaming {
        codec: VideoCodec,
        width: u32,
        height: u32,
    },
    /// The first picture is on the surface: once per player.
    FirstPicture,
    /// A sentence in French, for the person.
    Notice(String),
    /// The session is over, and nothing more will be told. The window
    /// is no longer drawn into by then, and may go.
    Ended(Ending),
}

/// Why a session ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ending {
    /// [`Player::stop`] was called.
    Asked,
    /// The host ended it.
    HostLeft,
    /// The link to the service closed without a goodbye.
    LinkLost,
    /// Something here or on the host could not go on, said in French.
    EngineFailed(String),
}

/// Why a player could not start.
#[derive(Debug)]
pub enum PlayerError {
    /// FFmpeg is missing from `vendor/ffmpeg`, or is not the one
    /// expected.
    Ffmpeg(CodecError),
    /// The link to the service could not be reached.
    Link(io::Error),
    /// One of the player's threads could not be started.
    Thread(io::Error),
    /// Pictures can only be drawn into a window on Windows.
    Surface,
}

impl fmt::Display for PlayerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlayerError::Ffmpeg(e) => write!(f, "le lecteur ne trouve pas FFmpeg : {e}"),
            PlayerError::Link(e) => write!(f, "le lecteur ne joint pas le service : {e}"),
            PlayerError::Thread(e) => write!(f, "le lecteur ne peut pas démarrer : {e}"),
            PlayerError::Surface => {
                f.write_str("le lecteur ne dessine dans une fenêtre que sous Windows")
            }
        }
    }
}

impl std::error::Error for PlayerError {}

/// A session being played. Cheap to clone: every clone is the same
/// player, and the session stops once all of them are gone.
#[derive(Clone)]
pub struct Player {
    shared: Arc<Shared>,
}

struct Shared {
    orders: UnboundedSender<Order>,
    measures: Arc<Mutex<Measures>>,
    tallies: Arc<Mutex<Tallies>>,
    rect: Arc<Mutex<Option<Rect>>>,
    muted: Arc<Muted>,
    encodable: Arc<Mutex<Option<CodecSet>>>,
}

impl Player {
    /// Starts playing the session behind the link named `link_name`.
    ///
    /// Loads FFmpeg from `vendor/ffmpeg` and reaches the link, both on
    /// this computer and quick; everything else happens on the player's
    /// threads, and its outcome arrives through `events`.
    pub fn start(
        link_name: &str,
        wanted: Wanted,
        surface: Surface,
        log: Log,
        events: Box<dyn Fn(Event) + Send + Sync>,
    ) -> Result<Player, PlayerError> {
        let ff = Ffmpeg::load(&zyr_proto::paths::ffmpeg_dir()).map_err(PlayerError::Ffmpeg)?;
        Self::start_with(ff, link_name, wanted, surface, log, events)
    }

    /// [`Player::start`] with FFmpeg already loaded, from wherever the
    /// caller found it: the end-to-end tests load a build made for the
    /// system they run on, where `vendor/ffmpeg` holds Windows' alone.
    pub fn start_with(
        ff: Arc<Ffmpeg>,
        link_name: &str,
        wanted: Wanted,
        surface: Surface,
        log: Log,
        events: Box<dyn Fn(Event) + Send + Sync>,
    ) -> Result<Player, PlayerError> {
        if matches!(surface, Surface::Window { .. }) && !cfg!(windows) {
            return Err(PlayerError::Surface);
        }
        ff.log_into(&log);
        let events = Arc::new(Events::new(events));
        let clock = Clock::new();
        let tally = Arc::new(Mutex::new(Tally::new(clock)));
        let tallies = Arc::new(Mutex::new(Tallies::default()));
        let measures = Arc::new(Mutex::new(Measures::default()));
        let rect = Arc::new(Mutex::new(None));
        let size = Arc::new(Mutex::new(None));
        let muted = Arc::new(Muted::default());
        let encodable = Arc::new(Mutex::new(None));
        let (orders, orders_in) = unbounded_channel();
        let (inner, inner_in) = unbounded_channel();
        let (video, video_in) = sync_channel(VIDEO_WAITING);
        let (audio, audio_in) = sync_channel(SOUND_WAITING);
        let (stats_stop, stats_in) = std::sync::mpsc::channel();
        let (connected, connected_in) = sync_channel(1);

        let player_log = log.about(TAG);
        let parts = link::Parts {
            name: link_name.to_string(),
            wanted,
            orders: orders_in,
            inner: inner_in,
            video,
            audio,
            size: Arc::clone(&size),
            clock,
            tally: Arc::clone(&tally),
            tallies: Arc::clone(&tallies),
            encodable: Arc::clone(&encodable),
            events: Arc::clone(&events),
            stats_stop,
            log: player_log.clone(),
        };
        let on_panic = panic_ends(&events, &player_log);
        spawn("link", on_panic, move || link::run(parts, connected))?;
        match connected_in.recv() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(PlayerError::Link(e)),
            Err(_) => {
                return Err(PlayerError::Link(io::Error::other(
                    "le lien local s'est arrêté avant de répondre",
                )));
            }
        }

        let started = Self::start_threads(Threads {
            ff,
            surface,
            events: Arc::clone(&events),
            inner,
            video_in,
            audio_in,
            stats_in,
            size,
            tally,
            tallies: Arc::clone(&tallies),
            measures: Arc::clone(&measures),
            rect: Arc::clone(&rect),
            muted: Arc::clone(&muted),
            log,
        });
        if let Err(e) = started {
            // The caller hears of this failure from here, never from an
            // event; the link, its orders gone, says goodbye to the
            // engine.
            events.silence();
            return Err(e);
        }
        Ok(Player {
            shared: Arc::new(Shared {
                orders,
                measures,
                tallies,
                rect,
                muted,
                encodable,
            }),
        })
    }

    fn start_threads(threads: Threads) -> Result<(), PlayerError> {
        let Threads {
            ff,
            surface,
            events,
            inner,
            video_in,
            audio_in,
            stats_in,
            size,
            tally,
            tallies,
            measures,
            rect,
            muted,
            log,
        } = threads;

        let picture_log = log.about(video::TAG);
        let video_failed = inner.clone();
        let video_panic = move |what: String| {
            let _ = video_failed.send(Inner::Failed(format!(
                "L'affichage s'est arrêté sur une erreur interne : {what}"
            )));
        };
        let video_ff = Arc::clone(&ff);
        let video_tally = Arc::clone(&tally);
        let video_tallies = Arc::clone(&tallies);
        let video_events = Arc::clone(&events);
        spawn("video", video_panic, move || {
            let shared = VideoShared {
                ff: video_ff,
                input: video_in,
                size,
                tally: video_tally,
                tallies: video_tallies,
                rect,
                inner,
                events: video_events,
                log: picture_log,
            };
            match surface {
                Surface::Headless => play(Headless::new(), shared),
                Surface::Window { hwnd } => on_window(hwnd, shared),
            }
        })?;

        let sound_log = log.about(audio::TAG);
        let sound_events = Arc::clone(&events);
        let sound_panic = {
            let log = sound_log.clone();
            move |what: String| {
                log.write(&format!("the sound thread stopped on a bug: {what}"));
                sound_events.say(Event::Notice(format!(
                    "Le son s'est arrêté sur une erreur interne : {what}"
                )));
            }
        };
        let sound_ff = Arc::clone(&ff);
        let sound_tallies = Arc::clone(&tallies);
        spawn("sound", sound_panic, move || {
            hear(
                &sound_ff,
                surface,
                audio_in,
                &muted,
                sound_tallies,
                sound_log,
            )
        })?;

        let measures_log = log.about(stats::TAG);
        let stats_panic = {
            let log = measures_log.clone();
            move |what: String| log.write(&format!("the measures stopped on a bug: {what}"))
        };
        spawn("measures", stats_panic, move || {
            stats::run(&tally, &measures, &tallies, &stats_in, &measures_log)
        })?;
        Ok(())
    }

    /// Asks for another picture, rate or codec, mid-session.
    pub fn change(&self, wanted: Wanted) {
        self.order(Order::Change(wanted));
    }

    /// Sends a key, a pointer move, a button or the wheel, in order. A
    /// key or button already down is not pressed again.
    pub fn send(&self, event: InputEvent) {
        self.order(Order::Input(event));
    }

    /// The keyboard repeats a key held down: the host repeats it too.
    pub fn send_repeat(&self, scancode: u8, extended: bool) {
        self.order(Order::Repeat { scancode, extended });
    }

    /// Releases every key and button still down, for when the window
    /// loses the keyboard.
    pub fn release_everything(&self) {
        self.order(Order::ReleaseEverything);
    }

    /// The surface is now this large, in physical pixels.
    pub fn resize(&self, width: u32, height: u32) {
        self.order(Order::Resize { width, height });
    }

    pub fn set_muted(&self, muted: bool) {
        self.shared.muted.set(muted);
    }

    /// Whether the sound is silenced here, as last set.
    pub fn muted(&self) -> bool {
        self.shared.muted.get()
    }

    /// The codecs the engine said it can encode, once it has said.
    pub fn encodable(&self) -> Option<CodecSet> {
        *lock(&self.shared.encodable)
    }

    /// The measures as they stood at the last tick of the timer.
    pub fn measures(&self) -> Measures {
        lock(&self.shared.measures).clone()
    }

    /// The counters so far.
    pub fn tallies(&self) -> Tallies {
        lock(&self.shared.tallies).clone()
    }

    /// Where the picture is drawn in the surface: left, top, width and
    /// height, in the surface's pixels.
    pub fn picture_rect(&self) -> Option<(i32, i32, u32, u32)> {
        *lock(&self.shared.rect)
    }

    /// Says goodbye to the engine and ends the session, which
    /// [`Event::Ended`] with [`Ending::Asked`] confirms.
    pub fn stop(&self) {
        self.order(Order::Stop);
    }

    fn order(&self, order: Order) {
        // Refused only once the session is over, when there is nobody
        // left to tell.
        let _ = self.shared.orders.send(order);
    }
}

/// Delivers events, and nothing after the end.
pub(crate) struct Events {
    deliver: Box<dyn Fn(Event) + Send + Sync>,
    /// Whether the end was told; held while delivering, so that nothing
    /// can be told after it by another thread.
    over: Mutex<bool>,
}

impl Events {
    fn new(deliver: Box<dyn Fn(Event) + Send + Sync>) -> Self {
        Self {
            deliver,
            over: Mutex::new(false),
        }
    }

    pub(crate) fn say(&self, event: Event) {
        let over = lock(&self.over);
        if !*over {
            (self.deliver)(event);
        }
    }

    /// Tells the end, the first time only.
    pub(crate) fn end(&self, ending: Ending) {
        let mut over = lock(&self.over);
        if !*over {
            *over = true;
            (self.deliver)(Event::Ended(ending));
        }
    }

    /// Nothing more is told, not even the end.
    fn silence(&self) {
        *lock(&self.over) = true;
    }
}

/// A lock, whether or not a thread panicked while holding it: what it
/// guards are counters and copies, whole at every step.
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Starts a thread of the player. A panic in it is caught at its root
/// and handed to `on_panic`, rather than tearing the window down.
fn spawn(
    name: &'static str,
    on_panic: impl FnOnce(String) + Send + 'static,
    body: impl FnOnce() + Send + 'static,
) -> Result<(), PlayerError> {
    std::thread::Builder::new()
        .name(format!("zyr-player-{name}"))
        .spawn(move || {
            if let Err(panic) = std::panic::catch_unwind(AssertUnwindSafe(body)) {
                let what = panic
                    .downcast_ref::<&str>()
                    .map(|text| (*text).to_string())
                    .or_else(|| panic.downcast_ref::<String>().cloned())
                    .unwrap_or_default();
                on_panic(format!("{name}: {what}"));
            }
        })
        .map(drop)
        .map_err(PlayerError::Thread)
}

/// What a panic of the link thread does: the session ends there.
fn panic_ends(events: &Arc<Events>, log: &Log) -> impl FnOnce(String) + Send + 'static {
    let events = Arc::clone(events);
    let log = log.clone();
    move |what: String| {
        log.write(&format!("the link thread stopped on a bug: {what}"));
        events.end(Ending::EngineFailed(format!(
            "Le lecteur s'est arrêté sur une erreur interne : {what}"
        )));
    }
}

/// What the threads besides the link are started with.
struct Threads {
    ff: Arc<Ffmpeg>,
    surface: Surface,
    events: Arc<Events>,
    inner: UnboundedSender<Inner>,
    video_in: Receiver<VideoInput>,
    audio_in: Receiver<Bytes>,
    stats_in: Receiver<()>,
    size: Arc<Mutex<Option<(u32, u32)>>>,
    tally: Arc<Mutex<Tally>>,
    tallies: Arc<Mutex<Tallies>>,
    measures: Arc<Mutex<Measures>>,
    rect: Arc<Mutex<Option<Rect>>>,
    muted: Arc<Muted>,
    log: Log,
}

/// What the video thread works with, whatever it draws on.
struct VideoShared {
    ff: Arc<Ffmpeg>,
    input: Receiver<VideoInput>,
    size: Arc<Mutex<Option<(u32, u32)>>>,
    tally: Arc<Mutex<Tally>>,
    tallies: Arc<Mutex<Tallies>>,
    rect: Arc<Mutex<Option<Rect>>>,
    inner: UnboundedSender<Inner>,
    events: Arc<Events>,
    log: Log,
}

/// The video thread with its presenter made: says what it decodes, then
/// plays until the link lets go.
fn play<P: Presenter>(presenter: P, shared: VideoShared) {
    let VideoShared {
        ff,
        input,
        size,
        tally,
        tallies,
        rect,
        inner,
        events,
        log,
    } = shared;
    let decodable = presenter.decodable(&ff);
    if decodable.is_empty() {
        log.write("nothing to decode with: no codec of the engine is decodable here");
        events.say(Event::Notice(
            "Cet ordinateur ne sait décoder aucune des images du moteur.".to_string(),
        ));
    }
    if inner.send(Inner::Hello(decodable)).is_err() {
        return;
    }
    let video = Video::new(ff, presenter, tally, tallies, rect, log);
    video::run(video, &input, &size, |said| match said {
        Said::Recover(recover) => inner.send(Inner::Recover(recover)).is_ok(),
        Said::FirstPicture => {
            events.say(Event::FirstPicture);
            true
        }
        Said::Notice(text) => {
            events.say(Event::Notice(text));
            true
        }
        Said::Failed(reason) => {
            let _ = inner.send(Inner::Failed(reason));
            false
        }
    });
}

/// The sound thread: headless, or on the sound card.
fn hear(
    ff: &Arc<Ffmpeg>,
    surface: Surface,
    input: Receiver<Bytes>,
    muted: &Muted,
    tallies: Arc<Mutex<Tallies>>,
    log: Log,
) {
    let sound = match Sound::open(ff, tallies, log.clone()) {
        Ok(sound) => sound,
        Err(e) => {
            log.write(&format!("no sound: the Opus decoder does not open: {e}"));
            return;
        }
    };
    match surface {
        Surface::Headless => audio::run_headless(sound, &input),
        Surface::Window { .. } => on_sound_card(sound, &input, muted, &log),
    }
}

#[cfg(windows)]
fn on_sound_card(sound: Sound, input: &Receiver<Bytes>, muted: &Muted, log: &Log) {
    windows::audio::run(sound, input, muted, log);
}

/// Never reached: a window is refused off Windows before any thread
/// starts.
#[cfg(not(windows))]
fn on_sound_card(_: Sound, _: &Receiver<Bytes>, _: &Muted, _: &Log) {}

/// The video thread drawing into a window.
#[cfg(windows)]
fn on_window(hwnd: isize, shared: VideoShared) {
    match windows::Screen::open(hwnd, &shared.log) {
        Ok(screen) => play(screen, shared),
        Err(reason) => {
            let _ = shared.inner.send(Inner::Failed(format!(
                "L'image ne peut pas s'afficher sur cet ordinateur : {reason}"
            )));
        }
    }
}

/// Never reached: a window is refused off Windows before any thread
/// starts.
#[cfg(not(windows))]
fn on_window(_: isize, _: VideoShared) {}
