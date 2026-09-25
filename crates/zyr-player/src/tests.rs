//! A whole session, headless, against a scripted engine at the other end
//! of a real local link: the picture, the sound, the measures, the
//! input and the three ways a session ends.

use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use zyr_control::link::{Access, Channel, LinkListener};
use zyr_media::MEDIA_VERSION;
use zyr_media::codec::CodecSet;
use zyr_media::control::{ByeReason, ControlReader, NoticeKind, ToEngine, ToPlayer};
use zyr_media::service;

use crate::testing::{self, HEIGHT, WIDTH};
use crate::{
    Button, CodecChoice, Ending, Event, InputEvent, Measures, Player, PlayerError, Surface,
    Tallies, VideoCodec, Wanted,
};

/// Long enough for anything that should happen, on a busy machine.
const PATIENCE: Duration = Duration::from_secs(10);

/// The host's clock runs this far ahead of the player's.
const HOST_AHEAD_US: u64 = 5_000_000_000;

fn wanted() -> Wanted {
    Wanted {
        width: WIDTH as u16,
        height: HEIGHT as u16,
        fps: 60,
        bitrate_kbps: 2_000,
        codec: CodecChoice::Auto,
        draw_pointer: false,
        audio: true,
        steady: false,
    }
}

/// What the fake engine hears from the player.
#[derive(Debug)]
enum Heard {
    Said(ToEngine),
    Closed,
}

/// What the test makes the fake engine send.
enum Send {
    Control(ToPlayer),
    Frame(Channel, Vec<u8>),
    Close,
}

/// An engine that answers pings by itself and does whatever else it is
/// told.
struct Engine {
    name: String,
    heard: Receiver<Heard>,
    send: UnboundedSender<Send>,
    epoch: Instant,
    _thread: JoinHandle<()>,
}

impl Engine {
    fn start() -> Self {
        let (heard_out, heard) = channel();
        let (send, mut to_send) = unbounded_channel::<Send>();
        let (named, name) = channel();
        let epoch = Instant::now();
        let thread = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async move {
                let listener = LinkListener::create(Access::SystemOnly).unwrap();
                named.send(listener.name().to_string()).unwrap();
                let (mut reader, mut writer) = listener.accept().await.unwrap().split();
                let mut control = ControlReader::<ToEngine>::new();
                loop {
                    tokio::select! {
                        frame = reader.next() => {
                            let payload = match frame {
                                Ok(Some((Channel::Control, payload))) => payload,
                                Ok(Some(_)) => continue,
                                Ok(None) | Err(_) => {
                                    let _ = heard_out.send(Heard::Closed);
                                    return;
                                }
                            };
                            control.feed(&payload);
                            for message in control.by_ref() {
                                match message.unwrap() {
                                    ToEngine::Ping { sent_us } => {
                                        let pong = ToPlayer::Pong { sent_us, host_us: host_us(epoch) };
                                        let mut bytes = Vec::new();
                                        pong.write(&mut bytes);
                                        writer.send(Channel::Control, &bytes).await.unwrap();
                                    }
                                    other => {
                                        let _ = heard_out.send(Heard::Said(other));
                                    }
                                }
                            }
                        }
                        order = to_send.recv() => match order {
                            Some(Send::Control(message)) => {
                                let mut bytes = Vec::new();
                                message.write(&mut bytes);
                                writer.send(Channel::Control, &bytes).await.unwrap();
                            }
                            Some(Send::Frame(channel, payload)) => {
                                writer.send(channel, &payload).await.unwrap();
                            }
                            Some(Send::Close) | None => return,
                        },
                    }
                }
            });
        });
        Self {
            name: name.recv().unwrap(),
            heard,
            send,
            epoch,
            _thread: thread,
        }
    }

    fn say(&self, message: ToPlayer) {
        self.send.send(Send::Control(message)).unwrap();
    }

    fn frame(&self, channel: Channel, payload: Vec<u8>) {
        self.send.send(Send::Frame(channel, payload)).unwrap();
    }

    /// Sends one video frame, captured now on the host's clock.
    fn picture(&self, stream: u16, frame: u32, packet: &testing::Encoded) {
        let captured = host_us(self.epoch) as u32;
        for datagram in testing::datagrams(stream, frame, packet, captured) {
            self.frame(Channel::Video, datagram);
        }
    }

    /// The next message from the player, whatever it is.
    fn next(&self) -> Heard {
        self.heard
            .recv_timeout(PATIENCE)
            .expect("the player says something")
    }

    /// The next message from the player, which must be this one.
    fn expect(&self, expected: ToEngine) {
        match self.next() {
            Heard::Said(message) => assert_eq!(message, expected),
            Heard::Closed => panic!("the link closed, {expected:?} expected"),
        }
    }

    /// Nothing from the player for that long.
    fn silent_for(&self, span: Duration) {
        match self.heard.recv_timeout(span) {
            Err(RecvTimeoutError::Timeout) => {}
            other => panic!("the player said {other:?}"),
        }
    }

    /// Reads the hello and answers it: the stream starts.
    fn open(&self) -> CodecSet {
        let Heard::Said(ToEngine::Hello {
            version,
            wanted: asked,
            decodable,
        }) = self.next()
        else {
            panic!("the player starts with its hello");
        };
        assert_eq!((version, asked), (MEDIA_VERSION, wanted()));
        self.say(ToPlayer::Welcome {
            version: MEDIA_VERSION,
            encodable: CodecSet::empty().with(VideoCodec::H264),
            display_width: 1920,
            display_height: 1080,
        });
        self.say(ToPlayer::Streaming {
            stream: 1,
            codec: VideoCodec::H264,
            width: WIDTH as u16,
            height: HEIGHT as u16,
            fps: 60,
        });
        decodable
    }
}

fn host_us(epoch: Instant) -> u64 {
    HOST_AHEAD_US + epoch.elapsed().as_micros() as u64
}

/// A headless player on `engine`, and the events it tells.
fn player(engine: &Engine) -> (Player, Receiver<Event>) {
    let (told, events) = channel();
    let player = Player::start_with(
        testing::ffmpeg(),
        &engine.name,
        wanted(),
        Surface::Headless,
        testing::log(crate::TAG),
        Box::new(move |event| {
            let _ = Sender::send(&told, event);
        }),
    )
    .unwrap();
    (player, events)
}

fn next_event(events: &Receiver<Event>) -> Event {
    events
        .recv_timeout(PATIENCE)
        .expect("the player tells something")
}

/// Waits until the player's counters say so.
fn tallies_until(player: &Player, done: impl Fn(&Tallies) -> bool) -> Tallies {
    let until = Instant::now() + PATIENCE;
    loop {
        let tallies = player.tallies();
        if done(&tallies) {
            return tallies;
        }
        assert!(Instant::now() < until, "still {tallies:#?}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Waits until the player's measures say so.
fn measures_until(player: &Player, done: impl Fn(&Measures) -> bool) -> Measures {
    let until = Instant::now() + PATIENCE;
    loop {
        let measures = player.measures();
        if done(&measures) {
            return measures;
        }
        assert!(Instant::now() < until, "still {measures:#?}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_whole_session_plays_and_ends_when_asked() {
    let (frames, looks) = testing::h264(12, &[8]);
    let sound = testing::sound(30);
    let engine = Engine::start();
    let (player, events) = player(&engine);
    let decodable = engine.open();
    assert!(decodable.contains(VideoCodec::H264));
    assert_eq!(
        next_event(&events),
        Event::Streaming {
            codec: VideoCodec::H264,
            width: WIDTH,
            height: HEIGHT
        }
    );
    // What the engine can encode is kept from its welcome, which comes
    // before the stream: the window strikes out the others.
    assert_eq!(
        player.encodable(),
        Some(CodecSet::empty().with(VideoCodec::H264))
    );
    // And silence is read back as it was set.
    assert!(!player.muted());
    player.set_muted(true);
    assert!(player.muted());
    player.set_muted(false);
    engine.frame(
        Channel::Service,
        service::ToPlayer::Tunnel {
            rtt_us: 12_345,
            relayed: false,
        }
        .encode(),
    );

    // Five frames, and the first picture, once.
    for (n, frame) in frames.iter().enumerate().take(5) {
        engine.picture(1, n as u32, frame);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(next_event(&events), Event::FirstPicture);

    // Frame 5 never comes. Frame 6 shows it is lost: a key frame is asked
    // for, once, then again a quarter second later, and frames 6 and 7
    // wait for it.
    engine.picture(1, 6, &frames[6]);
    engine.expect(ToEngine::Recover {
        stream: 1,
        frame: 4,
    });
    let first_ask = Instant::now();
    engine.picture(1, 7, &frames[7]);
    engine.silent_for(Duration::from_millis(150));
    engine.expect(ToEngine::Recover {
        stream: 1,
        frame: 4,
    });
    let waited = first_ask.elapsed();
    assert!(
        waited >= Duration::from_millis(200) && waited < Duration::from_millis(600),
        "{waited:?}"
    );
    // The key frame comes, and the picture goes on from it.
    for (n, frame) in frames.iter().enumerate().skip(8) {
        engine.picture(1, n as u32, frame);
        std::thread::sleep(Duration::from_millis(10));
    }
    let tallies = tallies_until(&player, |t| t.pictures.decoded == 9);
    assert_eq!(tallies.pictures.skipped, 2);
    assert_eq!(tallies.pictures.recovers, 2);
    assert_eq!(tallies.assembly.frames_lost, 1);
    assert_eq!(
        tallies.pictures.shown + tallies.pictures.unshown,
        tallies.pictures.decoded
    );
    assert_eq!(tallies.pictures.checksum, Some(looks[11]));
    assert_eq!(player.picture_rect(), Some((0, 0, WIDTH, HEIGHT)));
    player.resize(640, 480);
    tallies_until(&player, |t| t.pictures.redrawn == 1);
    assert_eq!(player.picture_rect(), Some((0, 0, 640, 480)));

    let measures = measures_until(&player, |m| m.latency_ms.is_some() && m.fps.is_some());
    assert_eq!(measures.codec.as_deref(), Some("H.264"));
    assert_eq!(
        (measures.width, measures.height),
        (Some(WIDTH), Some(HEIGHT))
    );
    assert!(measures.fps.unwrap() > 0.0, "{measures:?}");
    assert!(measures.decode_ms.is_some(), "{measures:?}");
    assert!(measures.render_ms.is_some(), "{measures:?}");
    assert!(measures.bitrate_mbps.unwrap() > 0.0, "{measures:?}");
    assert_eq!(measures.host_ms, Some(2.0));
    assert!(measures.dropped_network_pct.unwrap() > 0.0, "{measures:?}");
    assert!(measures.since_frame_ms.is_some(), "{measures:?}");
    assert_eq!(measures.network_ms, Some(12.345));
    let latency = measures.latency_ms.unwrap();
    assert!((0.0..1_000.0).contains(&latency), "{latency}");

    // Sound, packet 12 missing: three packets ahead, then one every
    // 10 ms, as the host's sound card gives them.
    let start = Instant::now();
    for (sequence, datagram) in sound.into_iter().enumerate() {
        if sequence == 12 {
            continue;
        }
        let due = start + Duration::from_millis(10 * sequence.saturating_sub(3) as u64);
        std::thread::sleep(due.saturating_duration_since(Instant::now()));
        engine.frame(Channel::Audio, datagram);
    }
    let tallies = tallies_until(&player, |t| t.sound.decoded + t.sound.concealed >= 25);
    assert!(tallies.sound.concealed >= 1, "{tallies:#?}");
    assert!(tallies.sound.decoded >= 20, "{tallies:#?}");

    // Input, in order: a second press is not sent, a repeat is, and
    // losing the focus lets go of what is still down.
    let a = |down| InputEvent::Key {
        scancode: 0x1e,
        extended: false,
        down,
    };
    player.send(a(true));
    player.send(a(true));
    player.send_repeat(0x1e, false);
    player.send(InputEvent::PointerAt { x: 100, y: 200 });
    player.send(a(false));
    player.send(InputEvent::Button {
        button: Button::Left,
        down: true,
    });
    player.release_everything();
    for expected in [
        a(true),
        a(true),
        InputEvent::PointerAt { x: 100, y: 200 },
        a(false),
        InputEvent::Button {
            button: Button::Left,
            down: true,
        },
        InputEvent::Button {
            button: Button::Left,
            down: false,
        },
    ] {
        engine.expect(ToEngine::Input(expected));
    }
    let tallies = player.tallies();
    assert_eq!((tallies.link.sent, tallies.link.pressed_again), (6, 1));

    player.stop();
    engine.expect(ToEngine::Bye);
    assert_eq!(next_event(&events), Event::Ended(Ending::Asked));
    assert!(matches!(engine.next(), Heard::Closed));
    assert!(events.recv_timeout(Duration::from_millis(200)).is_err());
}

#[test]
fn a_player_whose_every_handle_is_dropped_says_goodbye_like_a_stop() {
    let engine = Engine::start();
    let (player, events) = player(&engine);
    engine.open();
    assert!(matches!(next_event(&events), Event::Streaming { .. }));
    let clone = player.clone();
    drop(player);
    engine.silent_for(Duration::from_millis(100));
    drop(clone);
    engine.expect(ToEngine::Bye);
    assert_eq!(next_event(&events), Event::Ended(Ending::Asked));
    assert!(matches!(engine.next(), Heard::Closed));
}

#[test]
fn a_link_that_closes_without_a_goodbye_is_lost() {
    let engine = Engine::start();
    let (_player, events) = player(&engine);
    engine.open();
    assert!(matches!(next_event(&events), Event::Streaming { .. }));
    engine.send.send(Send::Close).unwrap();
    assert_eq!(next_event(&events), Event::Ended(Ending::LinkLost));
}

#[test]
fn a_host_that_leaves_says_so_and_a_fatal_goodbye_gives_its_reason() {
    let engine = Engine::start();
    let (_player, events) = player(&engine);
    engine.open();
    assert!(matches!(next_event(&events), Event::Streaming { .. }));
    engine.say(ToPlayer::Bye {
        reason: ByeReason::ServiceStop,
    });
    assert_eq!(next_event(&events), Event::Ended(Ending::HostLeft));
    assert!(matches!(engine.next(), Heard::Closed));

    let engine = Engine::start();
    let (_player, events) = player(&engine);
    engine.open();
    assert!(matches!(next_event(&events), Event::Streaming { .. }));
    let reason = "Aucun encodeur ne sait produire cette image.".to_string();
    engine.say(ToPlayer::Notice {
        kind: NoticeKind::NoEncoder,
        text: reason.clone(),
    });
    engine.say(ToPlayer::Bye {
        reason: ByeReason::Fatal,
    });
    assert_eq!(next_event(&events), Event::Notice(reason.clone()));
    assert_eq!(
        next_event(&events),
        Event::Ended(Ending::EngineFailed(reason))
    );
}

#[test]
fn a_change_mid_session_reaches_the_engine() {
    let engine = Engine::start();
    let (player, _events) = player(&engine);
    engine.open();
    let faster = Wanted {
        fps: 120,
        bitrate_kbps: 30_000,
        ..wanted()
    };
    player.change(faster);
    engine.expect(ToEngine::Change { wanted: faster });
}

#[test]
fn a_player_cannot_start_without_its_link() {
    let refused = Player::start_with(
        testing::ffmpeg(),
        "/nowhere/zyrdesk-link",
        wanted(),
        Surface::Headless,
        testing::log(crate::TAG),
        Box::new(|_| {}),
    );
    assert!(matches!(refused, Err(PlayerError::Link(_))));
}

#[cfg(not(windows))]
#[test]
fn a_window_is_only_drawn_into_on_windows() {
    let engine = Engine::start();
    let refused = Player::start_with(
        testing::ffmpeg(),
        &engine.name,
        wanted(),
        Surface::Window { hwnd: 1 },
        testing::log(crate::TAG),
        Box::new(|_| {}),
    );
    assert!(matches!(refused, Err(PlayerError::Surface)));
}
