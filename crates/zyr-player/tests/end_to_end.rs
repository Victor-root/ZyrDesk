//! A whole session on one computer: the real host engine, the real
//! tunnel and the real player, headless.
//!
//! Everything runs as it does between two computers, except what only
//! Windows has: the host engine films a synthetic screen, plays keys and
//! pointer into a recorder and hears a tone, and the player decodes on
//! the processor and draws nowhere. Between them the tunnel is a real
//! QUIC connection on loopback, each engine is on a real local link, and
//! the services are played as the services play them: the session opened
//! by the far computer's question, the engine told its datagram budget
//! and its screen before the answer goes back, the player told once a
//! second how the tunnel stands.
//!
//! They need a build of FFmpeg for this system, named by
//! `ZYR_FFMPEG_DIR`, and run one at a time: pictures coming at a steady
//! rate are part of what is checked, and two sessions at once on a small
//! machine would only check the machine.

use std::fmt::Debug;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::mpsc::{self as std_mpsc, Receiver};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use tokio::runtime::Runtime;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;
use zyr_codec::{Ffmpeg, Frame, GpuVendor, Input, VideoEncoder};
use zyr_control::link::{Access, LinkListener};
use zyr_host::fake::{Recorded, RecordingInjector, SilentSound, SyntheticScreen, ToneSound};
use zyr_host::picture::{Mapping, Rect, Size, picture_size, placement};
use zyr_host::{
    Aimed, Captured, Drawing, Ending as EngineEnding, Feed, Injected, MakeScreen, Parts, Screen,
    ScreenError, Sound,
};
use zyr_media::service::{Display, ToEngine, ToPlayer, ToService};
use zyr_player::{
    Button, CodecChoice, Ending, Event, InputEvent, Measures, Player, Surface, Tallies, VideoCodec,
    Wanted,
};
use zyr_proto::clipboard::{Clip, Stamp};
use zyr_proto::log::Log;
use zyr_proto::session::{Pointer, WantedScreen};
use zyr_transport::{Bytes, Connection, Identity, MediaProfile, Path, TunnelEndpoint};
use zyr_tunnel::aside::{self, Given};
use zyr_tunnel::{Answers, Presence, Tunnel, service_channel};

/// Longest wait for anything that should happen.
const PATIENCE: Duration = Duration::from_secs(10);

/// From the player's start to its first picture, at most.
const FIRST_PICTURE_WITHIN: Duration = Duration::from_secs(5);

/// How often a count or a measure is looked at again while waited for.
const LOOK_EVERY: Duration = Duration::from_millis(5);

/// The pictures a second asked for.
const FPS: u16 = 60;

/// The picture asked for: small, so that the synthetic screen, drawn
/// pixel by pixel in an unoptimised build, keeps up with the rate.
const WIDTH: u32 = 256;
const HEIGHT: u32 = 144;

/// The size asked for instead, mid-session.
const OTHER_WIDTH: u32 = 192;
const OTHER_HEIGHT: u32 = 108;

/// Names the folder of a build of FFmpeg for this system.
const FFMPEG_VARIABLE: &str = "ZYR_FFMPEG_DIR";

/// What the viewer asks for.
fn wanted() -> Wanted {
    Wanted {
        width: WIDTH as u16,
        height: HEIGHT as u16,
        fps: FPS,
        bitrate_kbps: 2_000,
        codec: CodecChoice::Auto,
        draw_pointer: false,
        audio: true,
        steady: false,
    }
}

/// The synthetic screen, changing at the rate the viewer asks for.
fn synthetic_screen() -> MakeScreen {
    Box::new(|| Ok(Box::new(SyntheticScreen::new(u32::from(FPS))) as Box<dyn Screen>))
}

/// Whether `rate` pictures a second is the rate asked for, give or take
/// what a busy machine costs.
fn at_the_rate_asked(rate: f64) -> bool {
    let asked = f64::from(FPS);
    (asked * 0.9..=asked * 1.1).contains(&rate)
}

/// FFmpeg, from `ZYR_FFMPEG_DIR` or else `vendor/ffmpeg`: a test that
/// needs it fails when it cannot be had, saying how to get it.
fn ffmpeg() -> Arc<Ffmpeg> {
    static LOADED: OnceLock<Arc<Ffmpeg>> = OnceLock::new();
    Arc::clone(LOADED.get_or_init(|| {
        let dir = std::env::var_os(FFMPEG_VARIABLE)
            .filter(|dir| !dir.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(zyr_proto::paths::ffmpeg_dir);
        Ffmpeg::load(&dir).unwrap_or_else(|e| {
            panic!(
                "these tests need FFmpeg and could not load it from {}: {e}\n\
                 Build it for this system with `packaging/ffmpeg/build.sh linux <out-dir>` and \
                 run the tests with {FFMPEG_VARIABLE}=<out-dir>/lib.",
                dir.display()
            )
        })
    }))
}

/// Held by each test for as long as it runs.
fn one_at_a_time() -> MutexGuard<'static, ()> {
    static ALONE: Mutex<()> = Mutex::new(());
    ALONE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The journal both engines and both tunnels write to: shown when a test
/// fails, and gone either way.
struct Journal {
    log: Log,
    path: PathBuf,
}

impl Journal {
    fn new(test: &str) -> Self {
        let path = std::env::temp_dir()
            .join(format!("zyr-player-end-to-end-{}", std::process::id()))
            .join(format!("{test}.log"));
        let log = Log::open(&path).expect("a journal for the test");
        Self { log, path }
    }

    /// The lines written so far that hold `words`.
    fn lines_with(&self, words: &str) -> Vec<String> {
        std::fs::read_to_string(&self.path)
            .expect("the test's journal")
            .lines()
            .filter(|line| line.contains(words))
            .map(str::to_string)
            .collect()
    }

    /// The streams the player opened a decoder for, in order.
    fn decoded_streams(&self) -> Vec<u16> {
        const OPENED: &str = "decoding stream ";
        self.lines_with(OPENED)
            .iter()
            .filter_map(|line| line.split(OPENED).nth(1)?.split(' ').next()?.parse().ok())
            .collect()
    }

    /// The key frames the host engine sent, and the requests for one it
    /// heard, as its pictures' summary gives them at the end of its
    /// session.
    fn key_frames_and_recovers(&self) -> (u64, u64) {
        let summary = self
            .lines_with("pictures: ")
            .pop()
            .expect("the host engine's summary");
        let count = |of: &str| {
            let before = &summary[..summary.find(of)?];
            before.rsplit(' ').next()?.parse().ok()
        };
        (
            count(" key frames").expect("key frames in the summary"),
            count(" recovers (").expect("requests in the summary"),
        )
    }
}

impl Drop for Journal {
    fn drop(&mut self) {
        if thread::panicking()
            && let Ok(written) = std::fs::read_to_string(&self.path)
        {
            eprintln!("what the session wrote:\n{written}");
        }
        let _ = std::fs::remove_file(&self.path);
        if let Some(folder) = self.path.parent() {
            // Only once the last test of the run is done with it.
            let _ = std::fs::remove_dir(folder);
        }
    }
}

/// What the watched computer answers beside the session: nothing, these
/// tests never ask.
struct NothingAsked;

fn not_asked_here<T>() -> Result<T, String> {
    Err("rien de tel n'est demandé dans cet essai".to_string())
}

impl Answers for NothingAsked {
    fn secure_attention(&self) -> Result<(), String> {
        not_asked_here()
    }

    fn hush_the_speakers(&self, _quiet: bool) -> Result<(), String> {
        not_asked_here()
    }

    fn lock_the_screen(&self) -> Result<(), String> {
        not_asked_here()
    }

    fn screen_for_a_session(
        &self,
        _wanted: Option<WantedScreen>,
    ) -> Result<Option<(u32, u32)>, String> {
        not_asked_here()
    }

    fn journal(&self, _sift: &str) -> Result<String, String> {
        not_asked_here()
    }

    fn reach_log(&self) -> Result<String, String> {
        not_asked_here()
    }

    fn empty_the_journal(&self) -> Result<(), String> {
        not_asked_here()
    }

    fn pointer(&self) -> Result<Pointer, String> {
        not_asked_here()
    }

    fn screens(&self) -> Result<String, String> {
        not_asked_here()
    }

    fn film_this_screen(&self, _id: Option<String>) -> Result<(), String> {
        not_asked_here()
    }

    fn clipboard(
        &self,
        _pushing: Option<Clip>,
        _seen: Option<Stamp>,
    ) -> Result<Option<Clip>, String> {
        not_asked_here()
    }

    fn pieces(
        &self,
        _asking: Option<aside::Wanted>,
        _giving: Option<Given>,
    ) -> Result<(Option<Given>, Option<aside::Wanted>), String> {
        not_asked_here()
    }
}

async fn within<T>(work: impl Future<Output = T>) -> T {
    tokio::time::timeout(PATIENCE, work)
        .await
        .expect("the tunnel let nothing through")
}

/// Both ends of a connection over loopback, the host's on `path`, and
/// the endpoints under them, which have to outlive it.
async fn connected(
    profile: MediaProfile,
    path: Path,
) -> (Connection, Connection, (TunnelEndpoint, TunnelEndpoint)) {
    let host_identity = Identity::generate().unwrap();
    let client_identity = Identity::generate().unwrap();
    let loopback = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 0);
    let host_endpoint = TunnelEndpoint::host_on_path(
        &host_identity,
        client_identity.fingerprint(),
        profile,
        loopback,
        path,
    )
    .unwrap();
    let meeting_point = host_endpoint.local_address().unwrap();
    let client_endpoint = TunnelEndpoint::client(
        &client_identity,
        host_identity.fingerprint(),
        profile,
        loopback,
    )
    .unwrap();
    let (host_side, client_side) = within(async {
        tokio::join!(
            host_endpoint.accept(),
            client_endpoint.connect(meeting_point)
        )
    })
    .await;
    (
        host_side.unwrap(),
        client_side.unwrap(),
        (host_endpoint, client_endpoint),
    )
}

/// The watched computer's side, as its service does it: the far
/// computer's question opens the session, the engine is brought up on
/// its link and told its first words, the answer goes back, then the
/// tunnel carries the link.
///
/// Hands back the tunnel and the service's word to the engine, kept for
/// as long as the session lasts.
async fn host_side(
    connection: Connection,
    parts: Parts,
    ended: std_mpsc::Sender<EngineEnding>,
    told: Arc<Mutex<Vec<ToService>>>,
    log: Log,
) -> (Tunnel, mpsc::Sender<Vec<u8>>) {
    let answering: Arc<dyn Answers> = Arc::new(NothingAsked);
    let opening = aside::until_a_session_opens(&connection, answering.clone(), None)
        .await
        .unwrap();
    // What the path always carries, less the byte naming the channel.
    let datagram_budget = connection
        .guaranteed_usable_datagram()
        .and_then(|usable| usable.checked_sub(1))
        .unwrap();
    let listener = LinkListener::create(Access::SystemOnly).unwrap();
    let name = listener.name().to_string();
    let engine_log = log.clone();
    thread::spawn(move || {
        let _ = ended.send(zyr_host::run(&name, parts, engine_log));
    });
    let link = within(listener.accept()).await.unwrap();
    let (side, service) = service_channel();
    for first_words in [
        ToEngine::Setup { datagram_budget },
        ToEngine::Film {
            display: String::new(),
        },
    ] {
        service.to_link.send(first_words.encode()).await.unwrap();
    }
    tokio::spawn(hear_the_engine(service.from_link, told));
    opening.opened().await.unwrap();
    let tunnel = Tunnel::host(connection, answering, link, side, Some(log));
    (tunnel, service.to_link)
}

/// Keeps what the engine tells its service.
async fn hear_the_engine(mut from_link: mpsc::Receiver<Bytes>, told: Arc<Mutex<Vec<ToService>>>) {
    while let Some(said) = from_link.recv().await {
        let said = ToService::decode(&said).expect("the engine speaks the service's words");
        told.lock().unwrap().push(said);
    }
}

/// The watching computer's side, as its way does it: a link for the
/// player, the tunnel waiting on it, and the player told once a second
/// how the tunnel stands. Hands back the tunnel and the link's name.
fn client_side(connection: Connection, log: Log) -> (Tunnel, String) {
    let listener = LinkListener::create(Access::SystemAndInteractive).unwrap();
    let name = listener.name().to_string();
    let (side, way) = service_channel();
    let tunnel = Tunnel::client(connection.clone(), listener, side, Some(log));
    tokio::spawn(tell_the_player(connection, tunnel.presence(), way.to_link));
    (tunnel, name)
}

async fn tell_the_player(connection: Connection, player: Presence, to_link: mpsc::Sender<Vec<u8>>) {
    let mut every = tokio::time::interval(Duration::from_secs(1));
    every.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        every.tick().await;
        if !player.here() {
            continue;
        }
        let told = ToPlayer::Tunnel {
            rtt_us: u32::try_from(connection.round_trip().as_micros()).unwrap_or(u32::MAX),
            relayed: false,
        };
        if let Err(TrySendError::Closed(_)) = to_link.try_send(told.encode()) {
            return;
        }
    }
}

/// Looks at `read` again and again until `done` says so, and fails with
/// what it saw last when that takes longer than [`PATIENCE`].
fn waited<S: Debug>(read: impl Fn() -> S, done: impl Fn(&S) -> bool) -> S {
    let deadline = Instant::now() + PATIENCE;
    loop {
        let seen = read();
        if done(&seen) {
            return seen;
        }
        assert!(Instant::now() < deadline, "still {seen:#?}");
        thread::sleep(LOOK_EVERY);
    }
}

/// A session, from the host engine through the tunnel to the player.
struct Session {
    player: Player,
    events: Receiver<Event>,
    /// When the player was started.
    started: Instant,
    /// What the host engine played of the keys and the pointer.
    played: Recorded,
    /// How the host engine's session ended, once it has.
    engine: Receiver<EngineEnding>,
    /// What the host engine told its service.
    told_the_service: Arc<Mutex<Vec<ToService>>>,
    host: Tunnel,
    client: Tunnel,
    _to_the_engine: mpsc::Sender<Vec<u8>>,
    _endpoints: (TunnelEndpoint, TunnelEndpoint),
    runtime: Runtime,
    /// Last, so that it goes after everything that writes to it.
    journal: Journal,
}

impl Session {
    /// Opens a session asking for `wanted`, the host's packets going out
    /// on `path`, and starts the player on it.
    fn start(
        test: &str,
        wanted: Wanted,
        path: Path,
        screen: MakeScreen,
        sound: Box<dyn Sound>,
    ) -> Self {
        let journal = Journal::new(test);
        // A worker for each computer's pumps.
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let (injector, played) = RecordingInjector::new();
        let parts = Parts {
            ffmpeg: ffmpeg(),
            screen,
            injector: Box::new(move || Box::new(injector)),
            sound,
        };
        let profile = MediaProfile {
            bits_per_second: u64::from(wanted.bitrate_kbps) * 1_000,
            frames_per_second: u32::from(wanted.fps),
        };
        let (ended, engine) = std_mpsc::channel();
        let told_the_service = Arc::new(Mutex::new(Vec::new()));
        let log = journal.log.clone();
        let told = told_the_service.clone();
        let (host, client, to_the_engine, endpoints, link) = runtime.block_on(async move {
            let (host_connection, client_connection, endpoints) = connected(profile, path).await;
            let hosting = host_side(host_connection, parts, ended, told, log.clone());
            let ((host, to_the_engine), opened) = within(async {
                tokio::join!(hosting, aside::ask_to_open(&client_connection, profile))
            })
            .await;
            opened.unwrap();
            let (client, link) = client_side(client_connection, log);
            (host, client, to_the_engine, endpoints, link)
        });

        let (tell, events) = std_mpsc::channel();
        let started = Instant::now();
        let player = Player::start_with(
            ffmpeg(),
            &link,
            wanted,
            Surface::Headless,
            journal.log.clone(),
            Box::new(move |event| {
                let _ = tell.send(event);
            }),
        )
        .unwrap();
        Self {
            player,
            events,
            started,
            played,
            engine,
            told_the_service,
            host,
            client,
            _to_the_engine: to_the_engine,
            _endpoints: endpoints,
            runtime,
            journal,
        }
    }

    fn next_event(&self) -> Event {
        self.events
            .recv_timeout(PATIENCE)
            .expect("the player tells something")
    }

    /// The stream is announced, then its first picture is shown.
    fn first_picture(&self, width: u32, height: u32) {
        assert_eq!(
            self.next_event(),
            Event::Streaming {
                codec: VideoCodec::H264,
                width,
                height
            }
        );
        assert_eq!(self.next_event(), Event::FirstPicture);
    }

    fn tallies_until(&self, done: impl Fn(&Tallies) -> bool) -> Tallies {
        waited(|| self.player.tallies(), done)
    }

    fn measures_until(&self, done: impl Fn(&Measures) -> bool) -> Measures {
        waited(|| self.player.measures(), done)
    }

    /// Pictures decoded a second, counted over `span` from now.
    fn rate_over(&self, span: Duration) -> f64 {
        let (from, at) = (self.player.tallies().pictures.decoded, Instant::now());
        thread::sleep(span);
        let decoded = self.player.tallies().pictures.decoded - from;
        decoded as f64 / at.elapsed().as_secs_f64()
    }

    /// The last rate the host engine told its service it serves, once
    /// `done` with it.
    fn serving_until(&self, done: impl Fn(u32) -> bool) -> u32 {
        let serving = || {
            self.told_the_service
                .lock()
                .unwrap()
                .iter()
                .rev()
                .find_map(|said| match said {
                    ToService::Serving { kbps, .. } => Some(*kbps),
                    _ => None,
                })
        };
        waited(serving, |kbps| kbps.is_some_and(&done)).expect("a rate, once done with it")
    }

    /// What the host engine played, once it played that many.
    fn played_until(&self, count: usize) -> Vec<Injected> {
        waited(|| self.played.taken(), |played| played.len() >= count)
    }

    /// The player stops: the host engine's session ends, and the tunnel
    /// on the host with it, the engine's link closed. How the host
    /// engine's session ended.
    fn stop(&mut self) -> EngineEnding {
        self.player.stop();
        assert_eq!(self.next_event(), Event::Ended(Ending::Asked));
        let ending = self
            .engine
            .recv_timeout(PATIENCE)
            .expect("the host engine's session ends");
        self.runtime.block_on(within(self.host.wait())).unwrap();
        ending
    }
}

/// Nothing of the picture was lost, passed over or refused:
/// frames come out of the assembler in strict order, so every frame
/// whole and decoded, and none late, is the whole stream decoded in
/// order.
fn assert_whole(tallies: &Tallies) {
    let assembly = &tallies.assembly;
    let pictures = &tallies.pictures;
    assert_eq!(
        (
            assembly.frames_lost,
            assembly.late,
            assembly.duplicates,
            assembly.malformed,
            assembly.overflow,
        ),
        (0, 0, 0, 0, 0),
        "{tallies:#?}"
    );
    assert_eq!(
        (
            pictures.skipped,
            pictures.behind,
            pictures.broken,
            pictures.recovers,
            tallies.link.video_crowded,
        ),
        (0, 0, 0, 0, 0),
        "{tallies:#?}"
    );
    assert_eq!(pictures.decoded, assembly.frames_complete, "{tallies:#?}");
}

/// Where the host engine puts the pointer for a point of a picture of
/// `size` over the synthetic main screen.
fn pointer_on_the_main_screen(size: Size, x: u16, y: u16) -> Injected {
    let (_, screen) = SyntheticScreen::screens()[0].clone();
    let picture = picture_size(screen.size(), size);
    let mapping = Mapping {
        picture,
        placement: placement(screen.size(), picture),
        screen,
        desktop: SyntheticScreen::desktop(),
    };
    let (x, y) = mapping.absolute(x, y);
    Injected::PointerTo { x, y }
}

#[test]
fn a_session_goes_from_the_host_engine_through_the_tunnel_to_the_player_and_back() {
    let _alone = one_at_a_time();
    let mut session = Session::start(
        "whole",
        wanted(),
        Path::Direct,
        synthetic_screen(),
        Box::new(ToneSound),
    );

    // The stream, then its first picture, within seconds of the start.
    session.first_picture(WIDTH, HEIGHT);
    let first = session.started.elapsed();
    assert!(
        first < FIRST_PICTURE_WITHIN,
        "first picture after {first:?}"
    );

    // Fifty pictures and more, at the rate asked, the whole stream of
    // them decoded in order: nothing lost on a clean loopback.
    let shown = Instant::now();
    session.tallies_until(|t| t.pictures.decoded >= 50);
    let took = shown.elapsed();
    assert!(took < Duration::from_secs(3), "50 pictures took {took:?}");
    let rate = session.rate_over(Duration::from_secs(2));
    assert!(at_the_rate_asked(rate), "{rate:.1} pictures a second");
    assert_whole(&session.player.tallies());
    let (host, client) = (session.host.reading(), session.client.reading());
    assert_eq!((host.too_large, host.crowded), (0, 0), "{host:?}");
    assert_eq!(client.crowded_here, 0, "{client:?}");

    // Measured as they came: the rate, each step, and how long a
    // picture took from the host's screen to the player.
    let measures = session.measures_until(|m| {
        m.latency_ms.is_some() && m.network_ms.is_some() && m.fps.is_some_and(at_the_rate_asked)
    });
    assert_eq!(measures.codec.as_deref(), Some("H.264"));
    assert_eq!(
        (measures.width, measures.height),
        (Some(WIDTH), Some(HEIGHT))
    );
    let small = |ms: Option<f64>| ms.is_some_and(|ms| ms > 0.0 && ms < 50.0);
    assert!(small(measures.host_ms), "{measures:#?}");
    assert!(small(measures.decode_ms), "{measures:#?}");
    assert!(small(measures.latency_ms), "{measures:#?}");
    assert!(
        measures.network_ms.is_some_and(|ms| ms < 50.0),
        "{measures:#?}"
    );
    assert_eq!(measures.dropped_network_pct, Some(0.0), "{measures:#?}");

    // The sound, decoded as it comes.
    let tallies = session.tallies_until(|t| t.sound.decoded >= 50);
    assert_eq!(
        (tallies.sound.malformed, tallies.sound.broken),
        (0, 0),
        "{tallies:#?}"
    );

    // Keys, buttons, wheel and pointer, played on the host in order.
    let key = |scancode, extended, down| InputEvent::Key {
        scancode,
        extended,
        down,
    };
    let key_played = |scancode, extended, down| Injected::Key {
        scancode,
        extended,
        down,
    };
    let sent = [
        key(0x1e, false, true),
        InputEvent::PointerAt { x: 32768, y: 16384 },
        InputEvent::Button {
            button: Button::Left,
            down: true,
        },
        InputEvent::Wheel {
            vertical: -120,
            horizontal: 0,
        },
        InputEvent::PointerBy { dx: 12, dy: -7 },
        key(0x1d, true, true),
        key(0x1e, false, false),
        key(0x2a, false, true),
    ];
    let expected = [
        key_played(0x1e, false, true),
        pointer_on_the_main_screen(Size::new(WIDTH, HEIGHT), 32768, 16384),
        Injected::Button {
            button: Button::Left,
            down: true,
        },
        Injected::Wheel {
            vertical: -120,
            horizontal: 0,
        },
        Injected::PointerBy { dx: 12, dy: -7 },
        key_played(0x1d, true, true),
        key_played(0x1e, false, false),
        key_played(0x2a, false, true),
    ];
    for event in sent {
        session.player.send(event);
    }
    assert_eq!(session.played_until(expected.len()), expected);

    // Another rate: taken in place, the same stream going on.
    let before = session.serving_until(|_| true);
    session.player.change(Wanted {
        bitrate_kbps: 4_000,
        ..wanted()
    });
    session.serving_until(|kbps| kbps > before);
    let decoded = session.player.tallies().pictures.decoded;
    session.tallies_until(|t| t.pictures.decoded >= decoded + 30);
    assert!(
        session.events.try_recv().is_err(),
        "a new rate is no new stream"
    );
    assert_eq!(session.journal.decoded_streams().len(), 1);

    // Another size: a new stream, a new decoder, pictures of that size.
    session.player.change(Wanted {
        width: OTHER_WIDTH as u16,
        height: OTHER_HEIGHT as u16,
        bitrate_kbps: 4_000,
        ..wanted()
    });
    assert_eq!(
        session.next_event(),
        Event::Streaming {
            codec: VideoCodec::H264,
            width: OTHER_WIDTH,
            height: OTHER_HEIGHT
        }
    );
    let decoded = session.player.tallies().pictures.decoded;
    session.tallies_until(|t| t.pictures.decoded >= decoded + 30);
    waited(
        || session.player.picture_rect(),
        |rect| *rect == Some((0, 0, OTHER_WIDTH, OTHER_HEIGHT)),
    );
    let streams = session.journal.decoded_streams();
    assert_eq!(streams.len(), 2, "{streams:?}");
    assert_eq!(streams[1], streams[0].wrapping_add(1), "{streams:?}");
    let measures = session.measures_until(|m| m.width == Some(OTHER_WIDTH));
    assert_eq!(measures.height, Some(OTHER_HEIGHT));
    let tallies = session.player.tallies();
    assert_eq!(
        (tallies.assembly.frames_lost, tallies.pictures.skipped),
        (0, 0),
        "{tallies:#?}"
    );

    // The player stops: the host engine's session ends as the player
    // left it, and everything still held down is let go of.
    assert_eq!(session.stop(), EngineEnding::PlayerLeft);
    let taken = session.played.taken();
    assert_eq!(taken[..expected.len()], expected);
    let let_go = &taken[expected.len()..];
    let ups = [
        key_played(0x1d, true, false),
        key_played(0x2a, false, false),
        Injected::Button {
            button: Button::Left,
            down: false,
        },
    ];
    assert_eq!(let_go.len(), ups.len(), "{let_go:?}");
    assert!(ups.iter().all(|up| let_go.contains(up)), "{let_go:?}");
}

/// Lost on the way from the host, one packet in twenty.
const LOSS_PER_THOUSAND: u16 = 50;

/// How long a lossy session is watched at least.
const LOSSY_FOR: Duration = Duration::from_secs(4);

/// Frames lost beyond what the parity repairs, at least, before what
/// became of them means anything.
const HOLES: u64 = 3;

/// Longest the picture may stand still in a lossy session: a key frame
/// asked for comes within the floor between two of them, and a lost one
/// is asked for again a quarter second later.
const LONGEST_STILL: Duration = Duration::from_secs(1);

/// Frames passed over at most for each key frame asked for: those that
/// come while it waits for the host's floor between two key frames.
const PASSED_OVER_PER_ASK: u64 = 12;

#[test]
fn pictures_keep_coming_through_a_lossy_tunnel_and_each_hole_closes_on_a_key_frame() {
    let _alone = one_at_a_time();
    let mut session = Session::start(
        "lossy",
        Wanted {
            audio: false,
            ..wanted()
        },
        Path::Degraded {
            loss_per_thousand: LOSS_PER_THOUSAND,
        },
        synthetic_screen(),
        Box::new(SilentSound),
    );
    session.first_picture(WIDTH, HEIGHT);

    // Watched until the parity could not repair everything, a few
    // times: the picture never stands still for long all the same.
    let watched = Instant::now();
    let from = session.player.tallies().pictures.decoded;
    let mut decoded = from;
    let mut moved = watched;
    let tallies = loop {
        let tallies = session.player.tallies();
        if tallies.pictures.decoded > decoded {
            decoded = tallies.pictures.decoded;
            moved = Instant::now();
        }
        let still = moved.elapsed();
        assert!(still < LONGEST_STILL, "still for {still:?}: {tallies:#?}");
        if watched.elapsed() >= LOSSY_FOR && tallies.assembly.frames_lost >= HOLES {
            break tallies;
        }
        assert!(
            watched.elapsed() < PATIENCE,
            "too few holes to judge: {tallies:#?}"
        );
        thread::sleep(LOOK_EVERY);
    };
    let rate = (tallies.pictures.decoded - from) as f64 / watched.elapsed().as_secs_f64();
    assert!(
        rate > f64::from(FPS) * 0.75,
        "{rate:.1} pictures a second: {tallies:#?}"
    );

    // The parity repaired frames. Each frame it could not bring back cost
    // one request for a key frame at most, answered before it had to be
    // made again, and only the frames that came while it was on its way
    // were passed over.
    let assembly = &tallies.assembly;
    let pictures = &tallies.pictures;
    assert!(assembly.frames_recovered_by_fec > 0, "{tallies:#?}");
    assert!(pictures.recovers >= 1, "{tallies:#?}");
    assert!(
        pictures.recovers <= assembly.frames_lost + pictures.behind,
        "{tallies:#?}"
    );
    assert!(
        pictures.skipped <= pictures.recovers * PASSED_OVER_PER_ASK,
        "{tallies:#?}"
    );
    assert_eq!(pictures.broken, 0, "{tallies:#?}");

    // The host heard every request, and answered with key frames: one
    // opening the stream, the others closing its holes.
    let asked = session.player.tallies().pictures.recovers;
    assert_eq!(session.stop(), EngineEnding::PlayerLeft);
    let (key_frames, recovers) = session.journal.key_frames_and_recovers();
    assert_eq!(recovers, asked);
    assert!(
        (2..=recovers + 1).contains(&key_frames),
        "{key_frames} key frames for {recovers} requests"
    );
}

/// The benchmark's picture.
const BENCH_WIDTH: u32 = 1920;
const BENCH_HEIGHT: u32 = 1080;

/// A desktop as a video plays on it: fine detail everywhere, as text
/// has, still but for a band a quarter of the screen high that changes
/// whole with every image, 60 times a second. Drawn straight into the
/// encoder's own planes as fast as a copy goes, the image filling the
/// picture of the same size; and when each image it drew was captured,
/// in the order it drew them.
struct PlayingScreen {
    display: Display,
    next_image: Instant,
    /// The number of the latest image, and when it was captured.
    latest: Option<(usize, Instant)>,
    /// Rows of detail are cut out of this.
    detail: Vec<u8>,
    drawn: Arc<Mutex<Vec<Option<Instant>>>>,
}

impl PlayingScreen {
    fn new(drawn: Arc<Mutex<Vec<Option<Instant>>>>) -> Self {
        // Luma in limited range that no encoder makes small, from a
        // xorshift: no two rows alike.
        let mut state = 0x9E37_79B9_7F4A_7C15_u64;
        let detail = (0..BENCH_WIDTH as usize * 3)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                16 + (state % 220) as u8
            })
            .collect();
        Self {
            display: Display {
                id: r"BENCH\MAIN".to_string(),
                main: true,
                width: BENCH_WIDTH,
                height: BENCH_HEIGHT,
                name: "Écran de mesure".to_string(),
            },
            next_image: Instant::now(),
            latest: None,
            detail,
            drawn,
        }
    }
}

impl Screen for PlayingScreen {
    fn displays(&mut self) -> Vec<Display> {
        vec![self.display.clone()]
    }

    fn aim(&mut self, _display: &str) -> Result<Aimed, ScreenError> {
        let whole = Rect::new(0, 0, BENCH_WIDTH, BENCH_HEIGHT);
        Ok(Aimed {
            display: self.display.clone(),
            area: whole,
            desktop: whole,
            device: 1,
        })
    }

    fn encoder_input(&self) -> Input {
        Input::Cpu
    }

    fn vendor(&self) -> GpuVendor {
        GpuVendor::Other
    }

    fn wait(&mut self, until: Instant) -> Result<Captured, ScreenError> {
        if self.next_image > until {
            thread::sleep(until.saturating_duration_since(Instant::now()));
            return Ok(Captured::Nothing);
        }
        let at = self.next_image;
        thread::sleep(at.saturating_duration_since(Instant::now()));
        self.next_image = at + Duration::from_secs(1) / u32::from(FPS);
        let number = self.latest.map_or(0, |(number, _)| number + 1);
        self.latest = Some((number, at));
        Ok(Captured::Image { at })
    }

    fn draw(
        &mut self,
        encoder: &VideoEncoder,
        _feed: Feed,
        _drawing: &Drawing,
    ) -> Result<Frame, ScreenError> {
        let mut frame = encoder
            .frame_for_cpu()
            .map_err(|e| ScreenError(e.to_string()))?;
        let (width, height) = (frame.width() as usize, frame.height() as usize);
        let planes = frame.planes();
        let playing = height / 3..height / 3 + height / 4;
        let number = self.latest.map_or(0, |(number, _)| number);
        for (row, line) in planes.luma.chunks_mut(planes.luma_stride).enumerate() {
            let turn = if playing.contains(&row) { number } else { 0 };
            let from = (row * 131 + turn * 977) % (self.detail.len() - width);
            line[..width].copy_from_slice(&self.detail[from..from + width]);
        }
        planes.chroma.fill(128);
        self.drawn
            .lock()
            .unwrap()
            .push(self.latest.map(|(_, at)| at));
        Ok(Frame::Cpu(frame))
    }
}

/// Pictures left out of the measure at the start, while the stream
/// settles.
const SETTLING: usize = 60;

/// How long the benchmark measures.
const MEASURED_FOR: Duration = Duration::from_secs(10);

/// How often the benchmark looks whether a picture was decoded.
const NOTICE_EVERY: Duration = Duration::from_micros(200);

/// Pictures measured at least, for the slowest of a hundred to mean
/// anything.
const ENOUGH: usize = 100;

/// From the capture of each image on the host to its picture decoded by
/// the player, through the tunnel, at 1080p60 and 20 Mb/s with libx264:
/// the percentiles, printed.
///
/// A measure, not a check, and a long one: left out of ordinary runs.
/// Run it optimised, as the product runs:
///
/// ```text
/// ZYR_FFMPEG_DIR=<ffmpeg>/lib cargo test --release -p zyr-player \
///     --test end_to_end -- --ignored --nocapture latency
/// ```
#[test]
#[ignore = "a measure, not a check: its comment says how to run it"]
fn capture_to_decoded_latency_at_1080p60_and_20_mbps() {
    let _alone = one_at_a_time();
    let drawn = Arc::new(Mutex::new(Vec::new()));
    let screen: MakeScreen = {
        let drawn = drawn.clone();
        Box::new(move || Ok(Box::new(PlayingScreen::new(drawn)) as Box<dyn Screen>))
    };
    let wanted = Wanted {
        width: BENCH_WIDTH as u16,
        height: BENCH_HEIGHT as u16,
        fps: FPS,
        bitrate_kbps: 20_000,
        codec: CodecChoice::H264,
        draw_pointer: false,
        audio: false,
        steady: false,
    };
    let mut session = Session::start(
        "latency",
        wanted,
        Path::Direct,
        screen,
        Box::new(SilentSound),
    );
    session.first_picture(BENCH_WIDTH, BENCH_HEIGHT);

    // When each picture was decoded, as the player's count of them
    // moves: it counts a picture once decoded and handed to its surface.
    // The count before a picture is its frame in the stream.
    let mut decoded_at = vec![None; session.player.tallies().pictures.decoded as usize];
    let measuring = Instant::now();
    while measuring.elapsed() < MEASURED_FOR {
        let decoded = session.player.tallies().pictures.decoded as usize;
        decoded_at.resize(decoded.max(decoded_at.len()), Some(Instant::now()));
        thread::sleep(NOTICE_EVERY);
    }
    let tallies = session.player.tallies();
    let measures = session.player.measures();
    // Frame n of the stream is the n-th image drawn only when none went
    // missing on the way.
    assert_whole(&tallies);
    let drawn = drawn.lock().unwrap().clone();
    let mut latencies: Vec<Duration> = decoded_at
        .iter()
        .zip(&drawn)
        .skip(SETTLING)
        .filter_map(|(decoded, captured)| Some(decoded.as_ref()?.duration_since((*captured)?)))
        .collect();
    assert!(
        latencies.len() >= ENOUGH,
        "{} pictures measured",
        latencies.len()
    );
    latencies.sort();
    let share = |part: f64| latencies[((latencies.len() - 1) as f64 * part).round() as usize];
    let ms = |duration: Duration| duration.as_secs_f64() * 1_000.0;
    println!(
        "capture to decoded, {} pictures at {BENCH_WIDTH}x{BENCH_HEIGHT}, {FPS} fps, 20 Mb/s, \
         libx264: p50 {:.2} ms, p90 {:.2} ms, p99 {:.2} ms, max {:.2} ms",
        latencies.len(),
        ms(share(0.5)),
        ms(share(0.9)),
        ms(share(0.99)),
        ms(share(1.0)),
    );
    println!("the player's own measures: {measures:#?}");
    assert_eq!(session.stop(), EngineEnding::PlayerLeft);
}
