//! The whole engine, over a real link, against a player and a service
//! played by the test.
//!
//! The engine runs on its own thread with the stand-ins of
//! [`crate::fake`]; the test holds the other end of the link, where the
//! service and the player would be, speaks for both, and takes the
//! pictures back apart and through a decoder as the player would.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use tokio::time::timeout;
use zyr_codec::{DecodeOutput, DecodedFrame, Ffmpeg, OpusDecoder, VideoDecoder};
use zyr_control::link::{Access, Channel, LinkListener, LinkReader, LinkWriter};
use zyr_media::MEDIA_VERSION;
use zyr_media::audio::read_audio;
use zyr_media::codec::{CodecChoice, CodecSet, VideoCodec};
use zyr_media::control::{
    ByeReason, ControlReader, NoticeKind, ToEngine as ToEngineControl, ToPlayer, Wanted,
};
use zyr_media::input::{Button, InputEvent};
use zyr_media::service::{ToEngine as ToEngineService, ToService};
use zyr_media::video::{Assembled, Assembler, AssemblyLimits};
use zyr_proto::log::Log;

use crate::fake::{Recorded, RecordingInjector, SilentSound, SyntheticScreen, ToneSound};
use crate::parts::{Injected, Screen, Sound};
use crate::{Ending, Parts, run};

/// The log of one test: shown when the test fails, and gone either way.
pub(crate) struct TestLog {
    pub(crate) log: Log,
    path: PathBuf,
}

impl TestLog {
    pub(crate) fn new(test: &str) -> Self {
        let path = std::env::temp_dir()
            .join(format!("zyr-host-tests-{}", std::process::id()))
            .join(format!("{test}.log"));
        let log = Log::open(&path).expect("a log file for the test");
        Self { log, path }
    }
}

impl Drop for TestLog {
    fn drop(&mut self) {
        if thread::panicking()
            && let Ok(written) = std::fs::read_to_string(&self.path)
        {
            eprintln!("what the engine wrote:\n{written}");
        }
        let _ = std::fs::remove_file(&self.path);
        if let Some(folder) = self.path.parent() {
            // Only once the last test of the run is done with it.
            let _ = std::fs::remove_dir(folder);
        }
    }
}

/// Names the folder of a Linux build of FFmpeg for the tests.
const FFMPEG_VARIABLE: &str = "ZYR_FFMPEG_DIR";

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

/// Longest wait for anything from the engine.
const PATIENCE: Duration = Duration::from_secs(10);

const BUDGET: u16 = 1161;

fn wanted() -> Wanted {
    Wanted {
        width: 640,
        height: 360,
        fps: 30,
        bitrate_kbps: 3_000,
        codec: CodecChoice::Auto,
        draw_pointer: false,
        audio: false,
        steady: false,
    }
}

fn h264() -> CodecSet {
    CodecSet::empty().with(VideoCodec::H264)
}

/// A picture as the player got it.
#[derive(Debug, Clone)]
struct Picture {
    stream: u16,
    frame: u32,
    key: bool,
    repeat: bool,
    width: u32,
    height: u32,
    /// Luma in the top left corner.
    marker: u8,
    at: Instant,
}

/// Something the engine said, other than pictures.
#[derive(Debug, Clone)]
enum Said {
    Player(ToPlayer),
    Service(ToService),
}

/// The test's end of the link: the service and the player.
struct Far {
    engine: Option<thread::JoinHandle<Ending>>,
    reader: LinkReader,
    writer: LinkWriter,
    control: ControlReader<ToPlayer>,
    said: VecDeque<Said>,
    assembler: Assembler,
    decoder: Option<(u16, VideoDecoder)>,
    pictures: Vec<Picture>,
    lost: u64,
    sounds: Vec<(u16, Vec<u8>)>,
    recorded: Recorded,
    /// Last, so that it goes after the engine it logs.
    _log: TestLog,
}

impl Far {
    async fn start(test: &str, screen_rate: u32, sound: Box<dyn Sound>) -> Self {
        let listener = LinkListener::create(Access::SystemOnly).unwrap();
        let name = listener.name().to_owned();
        let (injector, recorded) = RecordingInjector::new();
        let parts = Parts {
            ffmpeg: ffmpeg(),
            screen: Box::new(move || {
                Ok(Box::new(SyntheticScreen::new(screen_rate)) as Box<dyn Screen>)
            }),
            injector: Box::new(move || Box::new(injector)),
            sound,
        };
        let log = TestLog::new(test);
        let engine_log = log.log.clone();
        let engine = thread::spawn(move || run(&name, parts, engine_log));
        let link = timeout(PATIENCE, listener.accept())
            .await
            .expect("the engine connects")
            .unwrap();
        let (reader, writer) = link.split();
        Self {
            engine: Some(engine),
            reader,
            writer,
            control: ControlReader::new(),
            said: VecDeque::new(),
            assembler: Assembler::new(AssemblyLimits::default()),
            decoder: None,
            pictures: Vec::new(),
            lost: 0,
            sounds: Vec::new(),
            recorded,
            _log: log,
        }
    }

    async fn service(&mut self, message: ToEngineService) {
        self.writer
            .send(Channel::Service, &message.encode())
            .await
            .unwrap();
    }

    async fn player(&mut self, message: ToEngineControl) {
        let mut out = Vec::new();
        message.write(&mut out);
        self.writer.send(Channel::Control, &out).await.unwrap();
    }

    async fn input(&mut self, event: InputEvent) {
        self.player(ToEngineControl::Input(event)).await;
    }

    /// Sets up, says hello, and waits for the welcome and the stream.
    async fn open(&mut self, wanted: Wanted) -> (ToPlayer, u16) {
        self.service(ToEngineService::Setup {
            datagram_budget: BUDGET,
        })
        .await;
        self.player(ToEngineControl::Hello {
            version: MEDIA_VERSION,
            wanted,
            decodable: h264(),
        })
        .await;
        let welcome = self
            .until(|said| match said {
                Said::Player(welcome @ ToPlayer::Welcome { .. }) => Some(welcome.clone()),
                _ => None,
            })
            .await;
        let stream = self.streaming().await.0;
        (welcome, stream)
    }

    /// The next stream announced: its id and picture size.
    async fn streaming(&mut self) -> (u16, u16, u16) {
        self.until(|said| match said {
            Said::Player(ToPlayer::Streaming {
                stream,
                width,
                height,
                ..
            }) => Some((*stream, *width, *height)),
            _ => None,
        })
        .await
    }

    /// Reads the link until something the engine said is picked, taking
    /// in pictures and sound on the way.
    async fn until<T>(&mut self, mut pick: impl FnMut(&Said) -> Option<T>) -> T {
        let deadline = Instant::now() + PATIENCE;
        loop {
            while let Some(said) = self.said.pop_front() {
                if let Some(picked) = pick(&said) {
                    return picked;
                }
            }
            assert!(Instant::now() < deadline, "the engine never said it");
            self.read_once(deadline).await;
        }
    }

    /// Reads the link for `span`, taking in everything.
    async fn read_for(&mut self, span: Duration) {
        let deadline = Instant::now() + span;
        while Instant::now() < deadline {
            self.read_once(deadline).await;
        }
    }

    /// Reads until `enough` pictures of `stream` came.
    async fn pictures_of(&mut self, stream: u16, enough: usize) -> Vec<Picture> {
        let deadline = Instant::now() + PATIENCE;
        loop {
            let of: Vec<Picture> = self
                .pictures
                .iter()
                .filter(|picture| picture.stream == stream)
                .cloned()
                .collect();
            if of.len() >= enough {
                return of;
            }
            assert!(
                Instant::now() < deadline,
                "only {} pictures of stream {stream}",
                of.len()
            );
            self.read_once(deadline).await;
        }
    }

    async fn read_once(&mut self, deadline: Instant) {
        let left = deadline.saturating_duration_since(Instant::now());
        let Ok(read) = timeout(left, self.reader.next()).await else {
            return;
        };
        let (channel, frame) = read.unwrap().expect("the link is open");
        match channel {
            Channel::Control => {
                self.control.feed(&frame);
                for message in self.control.by_ref() {
                    self.said.push_back(Said::Player(message.unwrap()));
                }
            }
            Channel::Service => self
                .said
                .push_back(Said::Service(ToService::decode(&frame).unwrap())),
            Channel::Video => self.picture_packet(&frame),
            Channel::Audio => {
                let (header, opus) = read_audio(&frame).unwrap();
                self.sounds.push((header.sequence, opus.to_vec()));
            }
        }
    }

    fn picture_packet(&mut self, datagram: &[u8]) {
        let now = Instant::now();
        self.assembler.push(datagram, now).unwrap();
        while let Some(assembled) = self.assembler.poll(now) {
            let frame = match assembled {
                Assembled::Frame(frame) => frame,
                Assembled::Lost { .. } => {
                    self.lost += 1;
                    continue;
                }
            };
            if self
                .decoder
                .as_ref()
                .is_none_or(|(stream, _)| *stream != frame.stream)
            {
                let decoder = VideoDecoder::open(&ffmpeg(), frame.codec, DecodeOutput::Cpu);
                self.decoder = Some((frame.stream, decoder.unwrap()));
            }
            let Some((_, decoder)) = &mut self.decoder else {
                continue;
            };
            let Some(DecodedFrame::Cpu(decoded)) = decoder.decode(&frame.data).unwrap() else {
                panic!(
                    "frame {} of stream {} gave no picture",
                    frame.frame, frame.stream
                );
            };
            let luma = &decoded.planes()[0];
            self.pictures.push(Picture {
                stream: frame.stream,
                frame: frame.frame,
                key: frame.key,
                repeat: frame.repeat,
                width: decoded.width(),
                height: decoded.height(),
                marker: luma.data[8 * luma.stride + 8],
                at: now,
            });
        }
    }

    /// The engine's own end, once it ended.
    fn ended(&mut self) -> Ending {
        self.engine.take().unwrap().join().unwrap()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_session_streams_pictures_a_player_decodes_and_follows_every_change() {
    let mut far = Far::start("streams", 60, Box::new(SilentSound)).await;
    far.service(ToEngineService::Setup {
        datagram_budget: BUDGET,
    })
    .await;
    let (encodable, encoders, displays) = far
        .until(|said| match said {
            Said::Service(ToService::Ready {
                encodable,
                encoders,
                displays,
            }) => Some((*encodable, encoders.clone(), displays.clone())),
            _ => None,
        })
        .await;
    assert_eq!(encodable, h264());
    assert_eq!(encoders, "libx264");
    assert_eq!(displays.len(), 2);

    far.player(ToEngineControl::Hello {
        version: MEDIA_VERSION,
        wanted: wanted(),
        decodable: h264(),
    })
    .await;
    let welcome = far
        .until(|said| match said {
            Said::Player(welcome @ ToPlayer::Welcome { .. }) => Some(welcome.clone()),
            _ => None,
        })
        .await;
    assert_eq!(
        welcome,
        ToPlayer::Welcome {
            version: MEDIA_VERSION,
            encodable: h264(),
            display_width: 1280,
            display_height: 720,
        }
    );
    let (first, width, height) = far.streaming().await;
    assert_eq!((width, height), (640, 360));
    let serving = far
        .until(|said| match said {
            Said::Service(ToService::Serving { kbps, fps }) => Some((*kbps, *fps)),
            _ => None,
        })
        .await;
    assert_eq!(serving, (2_500, 30));

    // Pictures in order, the first a key frame, each one a new image.
    let pictures = far.pictures_of(first, 15).await;
    assert!(pictures[0].key);
    assert!(pictures[1..].iter().all(|picture| !picture.key));
    for (n, picture) in pictures.iter().enumerate() {
        assert_eq!(picture.frame, n as u32);
        assert_eq!((picture.width, picture.height), (640, 360));
        assert!(!picture.repeat);
    }
    let markers: Vec<u8> = pictures.iter().map(|picture| picture.marker).collect();
    assert!(
        markers.windows(2).all(|pair| pair[0] != pair[1]),
        "every picture a new image: {markers:?}"
    );
    // A 60 Hz screen at 30 pictures a second: never faster than 30 * 8/7.
    let span = pictures.last().unwrap().at - pictures[1].at;
    let rate = (pictures.len() - 2) as f64 / span.as_secs_f64();
    assert!(rate < 36.0, "{rate:.1} pictures a second");

    // A lost frame is answered with a key frame at once.
    let last = pictures.last().unwrap().frame;
    far.player(ToEngineControl::Recover {
        stream: first,
        frame: last,
    })
    .await;
    let asked = Instant::now();
    let key = loop {
        far.read_once(asked + PATIENCE).await;
        if let Some(key) = far
            .pictures
            .iter()
            .find(|picture| picture.stream == first && picture.frame > last && picture.key)
        {
            break key.clone();
        }
    };
    assert!(
        key.at - asked < Duration::from_millis(150),
        "{:?}",
        key.at - asked
    );

    // A recover for a stream that is not the current one is left alone.
    far.player(ToEngineControl::Recover {
        stream: first.wrapping_sub(1),
        frame: 0,
    })
    .await;
    far.read_for(Duration::from_millis(300)).await;
    assert!(
        !far.pictures
            .iter()
            .any(|picture| picture.frame > key.frame && picture.key),
        "no key frame for an older stream"
    );

    // Only the rate changes: in place, the stream goes on.
    far.player(ToEngineControl::Change {
        wanted: Wanted {
            bitrate_kbps: 6_000,
            ..wanted()
        },
    })
    .await;
    let serving = far
        .until(|said| match said {
            Said::Service(ToService::Serving { kbps, fps }) => Some((*kbps, *fps)),
            Said::Player(ToPlayer::Streaming { .. }) => panic!("a new stream for a new rate"),
            _ => None,
        })
        .await;
    assert_eq!(serving, (5_000, 30));
    let before = far.pictures.len();
    far.read_for(Duration::from_millis(200)).await;
    assert!(
        far.pictures[before..]
            .iter()
            .all(|picture| picture.stream == first)
    );
    assert!(
        far.said
            .iter()
            .all(|said| !matches!(said, Said::Player(ToPlayer::Streaming { .. })))
    );

    // A new size: a new stream, a new decoder.
    far.player(ToEngineControl::Change {
        wanted: Wanted {
            width: 320,
            height: 180,
            ..wanted()
        },
    })
    .await;
    let (second, width, height) = far.streaming().await;
    assert_eq!(second, first.wrapping_add(1));
    assert_eq!((width, height), (320, 180));
    let pictures = far.pictures_of(second, 5).await;
    assert!(pictures[0].key);
    assert!(
        pictures
            .iter()
            .all(|picture| (picture.width, picture.height) == (320, 180))
    );

    // Another screen, of another shape: filmed, and a picture of its shape.
    far.service(ToEngineService::Film {
        display: r"FAKE\SIDE".to_string(),
    })
    .await;
    let filming = far
        .until(|said| match said {
            Said::Service(filming @ ToService::Filming { .. }) => Some(filming.clone()),
            _ => None,
        })
        .await;
    assert_eq!(
        filming,
        ToService::Filming {
            display: r"FAKE\SIDE".to_string(),
            width: 640,
            height: 480,
        }
    );
    let (third, width, height) = far.streaming().await;
    assert_eq!(third, second.wrapping_add(1));
    assert_eq!((width, height), (240, 180));
    far.pictures_of(third, 3).await;
    assert_eq!(far.lost, 0);

    // The service stops the session: the player hears it.
    far.service(ToEngineService::Stop).await;
    let reason = far
        .until(|said| match said {
            Said::Player(ToPlayer::Bye { reason }) => Some(*reason),
            _ => None,
        })
        .await;
    assert_eq!(reason, ByeReason::ServiceStop);
    assert_eq!(far.ended(), Ending::Stopped);
}

#[tokio::test(flavor = "multi_thread")]
async fn keys_and_pointer_are_played_in_order_and_let_go_of_at_goodbye() {
    let mut far = Far::start("input", 60, Box::new(SilentSound)).await;
    far.open(wanted()).await;
    let events = [
        InputEvent::Key {
            scancode: 0x1e,
            extended: false,
            down: true,
        },
        InputEvent::PointerAt { x: 0, y: 0 },
        InputEvent::Button {
            button: Button::Left,
            down: true,
        },
        InputEvent::PointerBy { dx: -5, dy: 7 },
        InputEvent::Wheel {
            vertical: 120,
            horizontal: -120,
        },
        InputEvent::Key {
            scancode: 0x1d,
            extended: true,
            down: true,
        },
        InputEvent::Key {
            scancode: 0x1e,
            extended: false,
            down: false,
        },
    ];
    for event in events {
        far.input(event).await;
    }
    far.player(ToEngineControl::Bye).await;
    assert_eq!(far.ended(), Ending::PlayerLeft);
    let played = far.recorded.taken();
    let (x, y) = match played[1] {
        Injected::PointerTo { x, y } => (x, y),
        other => panic!("{other:?}"),
    };
    // The top left corner of the main screen, the left two thirds of the
    // desktop.
    assert!((0..40).contains(&x) && (0..100).contains(&y), "{x}, {y}");
    assert_eq!(
        played,
        vec![
            Injected::Key {
                scancode: 0x1e,
                extended: false,
                down: true
            },
            Injected::PointerTo { x, y },
            Injected::Button {
                button: Button::Left,
                down: true
            },
            Injected::PointerBy { dx: -5, dy: 7 },
            Injected::Wheel {
                vertical: 120,
                horizontal: -120
            },
            Injected::Key {
                scancode: 0x1d,
                extended: true,
                down: true
            },
            Injected::Key {
                scancode: 0x1e,
                extended: false,
                down: false
            },
            // Let go of when the player left.
            Injected::Key {
                scancode: 0x1d,
                extended: true,
                down: false
            },
            Injected::Button {
                button: Button::Left,
                down: false
            },
        ]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_lost_link_lets_go_of_what_is_held() {
    let mut far = Far::start("link-lost", 60, Box::new(SilentSound)).await;
    far.open(wanted()).await;
    far.input(InputEvent::Key {
        scancode: 0x38,
        extended: false,
        down: true,
    })
    .await;
    far.read_for(Duration::from_millis(100)).await;
    let Far {
        reader,
        writer,
        engine,
        recorded,
        _log,
        ..
    } = far;
    drop((reader, writer));
    let ending = tokio::task::spawn_blocking(move || engine.unwrap().join().unwrap())
        .await
        .unwrap();
    assert_eq!(ending, Ending::LinkLost);
    assert_eq!(
        recorded.taken(),
        vec![
            Injected::Key {
                scancode: 0x38,
                extended: false,
                down: true
            },
            Injected::Key {
                scancode: 0x38,
                extended: false,
                down: false
            },
        ]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn sound_goes_out_as_numbered_opus_packets() {
    let mut far = Far::start("sound", 60, Box::new(ToneSound)).await;
    far.open(Wanted {
        audio: true,
        ..wanted()
    })
    .await;
    let deadline = Instant::now() + PATIENCE;
    while far.sounds.len() < 30 {
        assert!(
            Instant::now() < deadline,
            "{} sound packets",
            far.sounds.len()
        );
        far.read_once(deadline).await;
    }
    let mut decoder = OpusDecoder::open(&ffmpeg()).unwrap();
    for pair in far.sounds.windows(2) {
        assert_eq!(pair[1].0, pair[0].0.wrapping_add(1));
    }
    for (_, packet) in &far.sounds {
        assert_eq!(decoder.decode(packet).unwrap().len(), 960);
    }

    // No sound asked, none sent.
    far.player(ToEngineControl::Change { wanted: wanted() })
        .await;
    far.read_for(Duration::from_millis(200)).await;
    let before = far.sounds.len();
    far.read_for(Duration::from_millis(300)).await;
    assert_eq!(far.sounds.len(), before);
    far.player(ToEngineControl::Bye).await;
    assert_eq!(far.ended(), Ending::PlayerLeft);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_still_screen_is_sent_again_only_when_steady() {
    let mut far = Far::start("still", 0, Box::new(SilentSound)).await;
    let (_, first) = far.open(wanted()).await;
    far.read_for(Duration::from_millis(500)).await;
    // The key frame that opens the stream, and nothing after.
    let pictures = far.pictures_of(first, 1).await;
    assert_eq!(pictures.len(), 1);
    assert!(pictures[0].key && pictures[0].repeat);

    far.player(ToEngineControl::Change {
        wanted: Wanted {
            steady: true,
            ..wanted()
        },
    })
    .await;
    far.read_for(Duration::from_millis(200)).await;
    let before = far.pictures.len();
    far.read_for(Duration::from_secs(1)).await;
    let repeats = &far.pictures[before..];
    assert!(
        (26..=34).contains(&repeats.len()),
        "{} repeats in a second at 30 fps",
        repeats.len()
    );
    assert!(repeats.iter().all(|picture| picture.repeat && !picture.key));
    far.player(ToEngineControl::Bye).await;
    assert_eq!(far.ended(), Ending::PlayerLeft);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_player_of_another_version_is_told_and_let_go() {
    let mut far = Far::start("version", 60, Box::new(SilentSound)).await;
    far.service(ToEngineService::Setup {
        datagram_budget: BUDGET,
    })
    .await;
    far.player(ToEngineControl::Hello {
        version: MEDIA_VERSION + 1,
        wanted: wanted(),
        decodable: h264(),
    })
    .await;
    let kind = far
        .until(|said| match said {
            Said::Player(ToPlayer::Notice { kind, .. }) => Some(*kind),
            _ => None,
        })
        .await;
    assert_eq!(kind, NoticeKind::NoEncoder);
    let reason = far
        .until(|said| match said {
            Said::Player(ToPlayer::Bye { reason }) => Some(*reason),
            _ => None,
        })
        .await;
    assert_eq!(reason, ByeReason::Fatal);
    // The service journals why.
    let trouble = far
        .until(|said| match said {
            Said::Service(ToService::Trouble { text }) => Some(text.clone()),
            _ => None,
        })
        .await;
    assert!(trouble.contains("version"), "{trouble}");
    assert!(matches!(far.ended(), Ending::Failed(_)));
}
