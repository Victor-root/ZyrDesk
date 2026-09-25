//! The conversation: what the service and the player say, and what the
//! engine answers, from the first message to the last.
//!
//! The service speaks first, with the size of the datagrams the tunnel
//! carries. Meanwhile the pipeline aims the capture at the main screen
//! and tries the encoders; once it has, the service hears what this
//! computer can do. The player says hello with what it wants; the engine
//! welcomes it once both of those are known, and the pictures start.
//! From then on the viewer's changes reach the pipeline and the sound as
//! they come, and the session ends when the service says stop, when the
//! player says goodbye, or when the link closes. Whatever ends it,
//! everything the viewer held down is let go of first.

use std::sync::mpsc;
use std::thread::JoinHandle;

use tokio::runtime::Runtime;
use zyr_control::link::Link;
use zyr_media::codec::CodecSet;
use zyr_media::control::{ByeReason, NoticeKind, ToPlayer, Wanted};
use zyr_media::service::{Display, ToEngine as FromService, ToService};
use zyr_media::{MEDIA_VERSION, WireError};
use zyr_proto::log::Log;
use zyr_proto::net::UNHEARD_LIMIT;

use crate::clock::HostClock;
use crate::link::{self, Handlers, Outbox};
use crate::pipeline::{self, Report};
use crate::{Ending, Parts, input, sound};

/// What reaches the engine's thread.
pub(crate) enum Event {
    Link(Heard),
    Pipeline(Report),
}

/// What the link read.
pub(crate) enum Heard {
    Service(FromService),
    Player(Asked),
    /// A message of the player that could not be read.
    Unreadable(WireError),
    /// The link closed or failed, which its thread said in the log.
    Ended,
}

/// What the player asks of the engine itself; keys and pointer go to
/// the input thread, pings are answered by the link.
pub(crate) enum Asked {
    Hello { wanted: Wanted, decodable: CodecSet },
    Change { wanted: Wanted },
    Recover { stream: u16, frame: u32 },
    Bye,
}

pub(crate) fn run(runtime: Runtime, link: Link, parts: Parts, log: &Log) -> Ending {
    log.write("link connected");
    let clock = HostClock::new();
    let (events, received) = mpsc::channel();
    let Parts {
        ffmpeg,
        screen,
        injector,
        sound,
    } = parts;

    let threads = (|| {
        let (input, input_thread) = input::start(injector, UNHEARD_LIMIT, log.clone())?;
        let handlers = Handlers {
            events: events.clone(),
            input: input.clone(),
        };
        let (outbox, link_thread) = link::start(runtime, link, handlers, clock, log.clone())?;
        let (pipeline, pipeline_thread) = pipeline::start(
            screen,
            pipeline::Shared {
                ffmpeg: ffmpeg.clone(),
                outbox: outbox.clone(),
                events: events.clone(),
                input: input.clone(),
                clock,
                log: log.clone(),
            },
        )?;
        let (sound, sound_thread) = sound::start(
            sound,
            sound::Shared {
                ffmpeg,
                outbox: outbox.clone(),
                clock,
                log: log.clone(),
            },
        )?;
        Ok::<_, std::io::Error>(Threads {
            engine: Engine::new(outbox, pipeline, sound, input, log.clone()),
            link: link_thread,
            others: vec![input_thread, pipeline_thread, sound_thread],
        })
    })();
    // Only the threads hold a way to the engine now: once they are gone,
    // so is every message.
    drop(events);
    let threads = match threads {
        Ok(threads) => threads,
        Err(e) => {
            let why = format!("a thread of the engine could not start: {e}");
            log.write(&why);
            return Ending::Failed(why);
        }
    };
    let Threads {
        mut engine,
        link,
        others,
    } = threads;
    let ending = engine.run(&received);
    log.write(&format!("session over: {}", said(&ending)));
    engine.close(&ending);
    for thread in others {
        let _ = thread.join();
    }
    // Every other writer is gone: the link closes once what they left
    // behind is out.
    engine.outbox.end();
    drop(engine);
    let _ = link.join();
    ending
}

struct Threads {
    engine: Engine,
    link: JoinHandle<()>,
    others: Vec<JoinHandle<()>>,
}

fn said(ending: &Ending) -> String {
    match ending {
        Ending::Stopped => "the service stopped it".to_string(),
        Ending::PlayerLeft => "the player said goodbye".to_string(),
        Ending::LinkLost => "the link closed".to_string(),
        Ending::Unreachable(why) => format!("the link could not be reached: {why}"),
        Ending::Failed(why) => format!("the engine could not go on: {why}"),
    }
}

struct Engine {
    outbox: Outbox,
    pipeline: mpsc::Sender<pipeline::Command>,
    sound: mpsc::Sender<sound::Command>,
    input: mpsc::Sender<input::Command>,
    log: Log,
    datagram_budget: Option<u16>,
    /// What the encoders can do, once tried.
    encodable: Option<CodecSet>,
    /// Why nothing can be filmed, if nothing can.
    no_screen: Option<String>,
    /// The screen filmed, whose size the welcome gives.
    display: Option<Display>,
    /// A hello waiting for the setup or the encoders.
    hello: Option<(Wanted, CodecSet)>,
    welcomed: bool,
    sound_on: bool,
}

impl Engine {
    fn new(
        outbox: Outbox,
        pipeline: mpsc::Sender<pipeline::Command>,
        sound: mpsc::Sender<sound::Command>,
        input: mpsc::Sender<input::Command>,
        log: Log,
    ) -> Self {
        Self {
            outbox,
            pipeline,
            sound,
            input,
            log,
            datagram_budget: None,
            encodable: None,
            no_screen: None,
            display: None,
            hello: None,
            welcomed: false,
            sound_on: false,
        }
    }

    fn run(&mut self, events: &mpsc::Receiver<Event>) -> Ending {
        loop {
            let Ok(event) = events.recv() else {
                return Ending::Failed("every thread of the engine is gone".to_string());
            };
            let ended = match event {
                Event::Link(Heard::Service(message)) => self.service(message),
                Event::Link(Heard::Player(asked)) => self.player(asked),
                Event::Link(Heard::Unreadable(e)) => self.unreadable(e),
                Event::Link(Heard::Ended) => Some(Ending::LinkLost),
                Event::Pipeline(report) => self.pipeline(report),
            };
            if let Some(ending) = ended {
                return ending;
            }
        }
    }

    fn service(&mut self, message: FromService) -> Option<Ending> {
        match message {
            FromService::Setup { datagram_budget } => {
                self.log.write(&format!(
                    "set up: datagrams of {datagram_budget} bytes at most"
                ));
                self.datagram_budget = Some(datagram_budget);
                self.welcome()
            }
            FromService::Film { display } => {
                self.log.write(&format!(
                    "asked to film {}",
                    if display.is_empty() {
                        "the main screen"
                    } else {
                        &display
                    }
                ));
                let _ = self.pipeline.send(pipeline::Command::Film { display });
                None
            }
            FromService::Stop => {
                self.outbox.player(&ToPlayer::Bye {
                    reason: ByeReason::ServiceStop,
                });
                Some(Ending::Stopped)
            }
        }
    }

    fn player(&mut self, asked: Asked) -> Option<Ending> {
        match asked {
            Asked::Hello { wanted, decodable } => {
                self.log
                    .write(&format!("player says hello: {}", described(&wanted)));
                if self.welcomed || self.hello.is_some() {
                    self.log.write("a second hello is ignored");
                    return None;
                }
                self.hello = Some((wanted, decodable));
                self.welcome()
            }
            Asked::Change { wanted } => {
                self.log
                    .write(&format!("player changes: {}", described(&wanted)));
                if let Some((waiting, _)) = &mut self.hello {
                    *waiting = wanted;
                } else if self.welcomed {
                    let _ = self.pipeline.send(pipeline::Command::Change { wanted });
                    self.sound(wanted.audio);
                }
                None
            }
            Asked::Recover { stream, frame } => {
                self.log.debug(|| {
                    format!("player lost stream {stream} after frame {frame}: key frame asked")
                });
                let _ = self.pipeline.send(pipeline::Command::Recover { stream });
                None
            }
            Asked::Bye => Some(Ending::PlayerLeft),
        }
    }

    fn unreadable(&mut self, e: WireError) -> Option<Ending> {
        match e {
            WireError::Version(version) => {
                let text = format!(
                    "Cet ordinateur ne parle pas la même version du moteur que l'ordinateur d'en \
                     face ({version} ici, {MEDIA_VERSION} en face) : mettez ZyrDesk à jour des \
                     deux côtés."
                );
                Some(self.fail(NoticeKind::NoEncoder, text))
            }
            WireError::Framing => {
                self.log
                    .write("the player's messages can no longer be told apart");
                Some(self.fail(
                    NoticeKind::EncoderTrouble,
                    "Les messages du lecteur sont devenus illisibles.".to_string(),
                ))
            }
            other => {
                self.log
                    .write(&format!("a message of the player was left out: {other}"));
                None
            }
        }
    }

    fn pipeline(&mut self, report: Report) -> Option<Ending> {
        match report {
            Report::NoScreen(why) => {
                self.outbox
                    .service(&ToService::Trouble { text: why.clone() });
                self.outbox.service(&ToService::Ready {
                    encodable: CodecSet::empty(),
                    encoders: String::new(),
                    displays: Vec::new(),
                });
                self.no_screen = Some(why);
                self.welcome()
            }
            Report::Aimed(display) => {
                self.display = Some(display);
                None
            }
            Report::Probed {
                encodable,
                encoders,
                displays,
            } => {
                self.outbox.service(&ToService::Ready {
                    encodable,
                    encoders,
                    displays,
                });
                self.encodable = Some(encodable);
                self.welcome()
            }
            Report::Fatal { kind, text } => Some(self.fail(kind, text)),
        }
    }

    /// Welcomes the player and starts the pictures, once there is a
    /// hello, a setup, and an answer about the screen and the encoders.
    fn welcome(&mut self) -> Option<Ending> {
        let datagram_budget = self.datagram_budget?;
        self.hello?;
        if let Some(why) = self.no_screen.clone() {
            return Some(self.fail(NoticeKind::CaptureTrouble, why));
        }
        let encodable = self.encodable?;
        let (wanted, decodable) = self.hello.take()?;
        let (width, height) = self
            .display
            .as_ref()
            .map_or((0, 0), |display| (display.width, display.height));
        self.outbox.player(&ToPlayer::Welcome {
            version: MEDIA_VERSION,
            encodable,
            display_width: u16::try_from(width).unwrap_or(u16::MAX),
            display_height: u16::try_from(height).unwrap_or(u16::MAX),
        });
        self.welcomed = true;
        self.log.write("player welcomed");
        let _ = self.pipeline.send(pipeline::Command::Start {
            wanted,
            decodable,
            datagram_budget,
        });
        self.sound(wanted.audio);
        None
    }

    /// Carries the sound, or stops, as the viewer wants.
    fn sound(&mut self, on: bool) {
        if on != self.sound_on {
            self.sound_on = on;
            let _ = self.sound.send(if on {
                sound::Command::Listen
            } else {
                sound::Command::Hush
            });
        }
    }

    /// Tells the player why the session cannot go on, and the service,
    /// for its journal.
    fn fail(&mut self, kind: NoticeKind, text: String) -> Ending {
        self.outbox.player(&ToPlayer::Notice {
            kind,
            text: text.clone(),
        });
        self.outbox.player(&ToPlayer::Bye {
            reason: ByeReason::Fatal,
        });
        self.outbox
            .service(&ToService::Trouble { text: text.clone() });
        Ending::Failed(text)
    }

    /// Stops every thread, letting go of what the viewer holds first.
    fn close(&mut self, ending: &Ending) {
        let why = match ending {
            Ending::Stopped => "the service stopped the session",
            Ending::PlayerLeft => "the player said goodbye",
            Ending::LinkLost | Ending::Unreachable(_) => "the link is gone",
            Ending::Failed(_) => "the session failed",
        };
        let _ = self.input.send(input::Command::Quit(why));
        let _ = self.pipeline.send(pipeline::Command::Quit);
        let _ = self.sound.send(sound::Command::Quit);
    }
}

/// What the viewer wants, in a line of the log.
fn described(wanted: &Wanted) -> String {
    format!(
        "{}x{} at {} fps, {} kb/s, codec {:?}, pointer {}, sound {}, {}",
        wanted.width,
        wanted.height,
        wanted.fps,
        wanted.bitrate_kbps,
        wanted.codec,
        if wanted.draw_pointer {
            "drawn"
        } else {
            "not drawn"
        },
        if wanted.audio { "on" } else { "off" },
        if wanted.steady { "steady" } else { "on change" },
    )
}
