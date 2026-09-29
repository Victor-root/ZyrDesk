//! The engine of one incoming session, as the service knows it.
//!
//! The engine runs in the session that owns the screen and says what it
//! finds over its link: the encoders it could open, the screens it can
//! film, the screen it films, what it serves. That is kept here, for the
//! questions the far computer asks beside the picture, and so is which
//! screen the session is to be served from, which the engine is told.
//!
//! Nothing here knows Windows or the link itself: what arrives is handed
//! in, and what has to leave is handed back or put on a channel. That is
//! what lets all of it be tried anywhere.

use std::sync::Mutex;

use tokio::sync::mpsc;
use zyr_media::codec::VideoCodec;
use zyr_media::service::{Display, ToEngine, ToService};
use zyr_proto::session::FarScreen;
use zyr_transport::MediaProfile;

/// Which screen a session is to be served from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Film {
    /// This computer's main screen, whichever it is at the moment.
    Main,
    /// That screen, by the name the engine lists it under.
    This(String),
    /// The screen this computer grows for itself, once the engine can
    /// see it: it is named by what it says about itself, and it only
    /// says it once it is awake.
    Grown,
}

/// What the engine has said, and what it has been told.
#[derive(Debug)]
struct Heard {
    displays: Vec<Display>,
    film: Film,
    /// The screen the engine was last told to film, as it was told.
    told: Option<String>,
    /// The size of the screen the engine films, as it last said.
    filming: Option<(u32, u32)>,
}

/// What a message from the engine comes to, for whoever holds it.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Said {
    /// Lines for the journal.
    pub lines: Vec<String>,
    /// What the engine now serves, for the window the tunnel holds open.
    pub serving: Option<MediaProfile>,
    /// The screen the engine films gives it nothing: what to do about it
    /// is for the product to work out, since the engine arranges nothing.
    pub silent: bool,
}

/// The engine of one session.
///
/// Shared between the door, which reads what the engine said and tells
/// it which screen to film, and the task that hears the engine.
#[derive(Debug)]
pub struct Engine {
    heard: Mutex<Heard>,
    /// Where messages for the engine go, once it is connected.
    telling: Mutex<Option<mpsc::Sender<Vec<u8>>>>,
}

impl Default for Engine {
    fn default() -> Self {
        Self {
            heard: Mutex::new(Heard {
                displays: Vec::new(),
                film: Film::Main,
                told: None,
                filming: None,
            }),
            telling: Mutex::new(None),
        }
    }
}

impl Engine {
    /// The engine is connected: from now on it is told things there.
    pub fn connected(&self, telling: mpsc::Sender<Vec<u8>>) {
        *self.telling.lock().expect("session's engine") = Some(telling);
    }

    /// Tells the engine something, never waiting: what the engine is
    /// told is a handful of messages a session, and a queue full of them
    /// is an engine that has stopped reading.
    pub fn tell(&self, message: &ToEngine) -> Result<(), String> {
        let telling = self.telling.lock().expect("session's engine");
        let Some(telling) = telling.as_ref() else {
            return Err("no engine serves this session yet".to_string());
        };
        telling
            .try_send(message.encode())
            .map_err(|_| "this session's engine no longer answers".to_string())
    }

    /// Asks the engine to end the session: it says goodbye to the player
    /// before it goes, which a connection closed from under it never does.
    pub fn send_away(&self) -> Result<(), String> {
        self.tell(&ToEngine::Stop)
    }

    /// Decides which screen the session is served from, and tells the
    /// engine when that changes what it films.
    pub fn film(&self, wanted: Film) -> Result<(), String> {
        let mut heard = self.heard.lock().expect("session's engine");
        heard.film = wanted;
        self.film_if_it_changed(&mut heard)
    }

    /// Tells the engine the screen decided on, if it has not been told
    /// already.
    pub fn film_now(&self) -> Result<(), String> {
        let mut heard = self.heard.lock().expect("session's engine");
        self.film_if_it_changed(&mut heard)
    }

    /// Tells the engine to film the screen decided on when it has not
    /// been told that one, and that one can be named. Written down as
    /// told only once it was: a screen the engine never heard of is told
    /// again at the next chance.
    fn film_if_it_changed(&self, heard: &mut Heard) -> Result<(), String> {
        let Some(display) = heard.to_film() else {
            return Ok(());
        };
        if heard.told.as_ref() == Some(&display) {
            return Ok(());
        }
        self.tell(&ToEngine::Film {
            display: display.clone(),
        })?;
        heard.told = Some(display);
        Ok(())
    }

    /// Whether the session is served from the screen this computer grew,
    /// which leaves no other screen to choose between.
    pub fn films_the_grown_screen(&self) -> bool {
        self.heard.lock().expect("session's engine").film == Film::Grown
    }

    /// Whether the session is served from this computer's main screen,
    /// whichever it is: the one it gets unless somebody picked another.
    pub fn films_the_main_screen(&self) -> bool {
        self.heard.lock().expect("session's engine").film == Film::Main
    }

    /// The size of the screen the engine films, once it said.
    pub fn size_it_films(&self) -> Option<(u32, u32)> {
        self.heard.lock().expect("session's engine").filming
    }

    /// Serves the session from this computer's own screens again, if it
    /// was served from the one it grew.
    ///
    /// The engine is told to film the main screen even if that is what
    /// it was last told: while the desktop was on the grown screen, the
    /// main screen was that one, and a capture that never heard otherwise
    /// goes on duplicating a screen that is asleep.
    pub fn no_longer_the_grown_screen(&self) -> Result<(), String> {
        let mut heard = self.heard.lock().expect("session's engine");
        if heard.film != Film::Grown {
            return Ok(());
        }
        heard.film = Film::Main;
        heard.told = None;
        self.film_if_it_changed(&mut heard)
    }

    /// The screens a session may ask to be served from: every screen the
    /// engine can film but the one this computer grows for itself, which
    /// nobody sitting at this machine can see.
    pub fn screens(&self) -> Vec<FarScreen> {
        let driver = zyr_screen::shipped();
        self.heard
            .lock()
            .expect("session's engine")
            .displays
            .iter()
            .filter(|display| !driver.is_its_screen(&display.name))
            .map(|display| FarScreen {
                id: display.id.clone(),
                main: display.main,
                wide: display.width,
                high: display.height,
                name: display.name.clone(),
            })
            .collect()
    }

    /// Takes in what the engine said, and tells it the screen to film if
    /// what it said lets that be named now.
    pub fn heard(&self, message: ToService) -> Said {
        let mut heard = self.heard.lock().expect("session's engine");
        let mut said = heard.take_in(message);
        if let Err(e) = self.film_if_it_changed(&mut heard) {
            said.lines.push(format!(
                "the engine could not be told which screen to film: {e}"
            ));
        }
        said
    }
}

impl Heard {
    /// The screen the engine is to film, as it is to be told it, when it
    /// can be named.
    fn to_film(&self) -> Option<String> {
        match &self.film {
            Film::Main => Some(String::new()),
            Film::This(id) => Some(id.clone()),
            Film::Grown => {
                let driver = zyr_screen::shipped();
                self.displays
                    .iter()
                    .find(|display| driver.is_its_screen(&display.name))
                    .map(|display| display.id.clone())
            }
        }
    }

    fn take_in(&mut self, message: ToService) -> Said {
        match message {
            ToService::Ready {
                encodable,
                encoders,
                displays,
            } => {
                let codecs = encodable.iter().map(VideoCodec::name).collect::<Vec<_>>();
                let line = format!(
                    "the engine is ready: it encodes {} with {}, and can film {}",
                    if codecs.is_empty() {
                        "nothing".to_string()
                    } else {
                        codecs.join(", ")
                    },
                    if encoders.is_empty() {
                        "no encoder"
                    } else {
                        &encoders
                    },
                    listed(&displays)
                );
                self.displays = displays;
                Said {
                    lines: vec![line],
                    ..Said::default()
                }
            }
            ToService::Filming {
                display,
                width,
                height,
            } => {
                self.filming = Some((width, height));
                Said {
                    lines: vec![format!(
                        "the engine films {} at {width}x{height}",
                        if display.is_empty() {
                            "the main screen"
                        } else {
                            &display
                        }
                    )],
                    ..Said::default()
                }
            }
            ToService::Displays(displays) => {
                let line = format!("the screens changed: {}", listed(&displays));
                self.displays = displays;
                Said {
                    lines: vec![line],
                    ..Said::default()
                }
            }
            ToService::Trouble { fact } => Said {
                lines: vec![format!("the engine says: {fact}")],
                ..Said::default()
            },
            ToService::Serving { kbps, fps } => Said {
                lines: vec![format!(
                    "the engine serves {kbps} kbps at {fps} images a second, and the tunnel is \
                     held open for that"
                )],
                serving: Some(MediaProfile {
                    bits_per_second: u64::from(kbps) * 1_000,
                    frames_per_second: u32::from(fps),
                }),
                ..Said::default()
            },
            ToService::Silent { display } => Said {
                lines: vec![format!(
                    "the engine has been given no picture by {}",
                    if display.is_empty() {
                        "the main screen"
                    } else {
                        &display
                    }
                )],
                silent: true,
                ..Said::default()
            },
        }
    }
}

/// The screens, as the journal says them.
fn listed(displays: &[Display]) -> String {
    if displays.is_empty() {
        return "no screen".to_string();
    }
    displays
        .iter()
        .map(|display| {
            format!(
                "{} ({}, {}x{}{})",
                display.name,
                display.id,
                display.width,
                display.height,
                if display.main { ", the main one" } else { "" }
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests {
    use zyr_proto::fact::Fact;

    use super::*;
    use zyr_media::codec::CodecSet;

    const MAIN: &str = r"MONITOR\GSM5B7F\{4d36e96e-e325-11ce-bfc1-08002be10318}\0003";
    const SIDE: &str = r"\\.\DISPLAY2";
    const GROWN: &str = r"MONITOR\MTT1337\{4d36e96e-e325-11ce-bfc1-08002be10318}\0007";

    fn display(id: &str, main: bool, name: &str) -> Display {
        Display {
            id: id.to_string(),
            main,
            width: 1920,
            height: 1080,
            name: name.to_string(),
        }
    }

    fn two_screens() -> Vec<Display> {
        vec![
            display(MAIN, true, "ROG PG279Q"),
            display(SIDE, false, "Dell U2412M"),
        ]
    }

    /// The engine of a session, and what it is told.
    fn connected() -> (Engine, mpsc::Receiver<Vec<u8>>) {
        let engine = Engine::default();
        let (telling, told) = mpsc::channel(16);
        engine.connected(telling);
        (engine, told)
    }

    #[test]
    fn a_session_sent_away_is_ended_through_its_engine() {
        let (engine, mut told) = connected();
        engine.send_away().unwrap();
        assert_eq!(
            ToEngine::decode(&told.try_recv().unwrap()).unwrap(),
            ToEngine::Stop
        );
    }

    #[test]
    fn a_session_whose_engine_is_not_there_cannot_be_sent_away_through_it() {
        assert!(Engine::default().send_away().is_err());
    }

    fn filmed(told: &mut mpsc::Receiver<Vec<u8>>) -> Vec<String> {
        let mut filmed = Vec::new();
        while let Ok(frame) = told.try_recv() {
            match ToEngine::decode(&frame).unwrap() {
                ToEngine::Film { display } => filmed.push(display),
                other => panic!("unexpected {other:?}"),
            }
        }
        filmed
    }

    #[test]
    fn a_session_starts_on_the_main_screen_and_says_so_once() {
        let (engine, mut told) = connected();
        engine.film_now().unwrap();
        engine.film_now().unwrap();
        assert_eq!(filmed(&mut told), vec![String::new()]);
    }

    #[test]
    fn another_screen_is_asked_where_the_engine_stands() {
        let (engine, mut told) = connected();
        engine.film_now().unwrap();
        engine.heard(ToService::Ready {
            encodable: CodecSet::empty().with(VideoCodec::Hevc),
            encoders: "hevc_nvenc".to_string(),
            displays: two_screens(),
        });
        engine.film(Film::This(SIDE.to_string())).unwrap();
        // Asked twice, told once: nothing moves when nothing changes.
        engine.film(Film::This(SIDE.to_string())).unwrap();
        engine.film(Film::Main).unwrap();
        assert_eq!(
            filmed(&mut told),
            vec![String::new(), SIDE.to_string(), String::new()]
        );
    }

    #[test]
    fn the_grown_screen_is_filmed_as_soon_as_the_engine_can_see_it() {
        let (engine, mut told) = connected();
        engine.film_now().unwrap();
        engine.heard(ToService::Ready {
            encodable: CodecSet::empty().with(VideoCodec::H264),
            encoders: "libx264".to_string(),
            displays: two_screens(),
        });
        // Woken, and not yet among the screens the engine sees: nothing
        // to name it by, so nothing is said.
        engine.film(Film::Grown).unwrap();
        assert_eq!(filmed(&mut told), vec![String::new()]);
        assert!(engine.films_the_grown_screen());

        // The engine sees it: it is told at once.
        let mut with_it = two_screens();
        with_it.push(display(GROWN, false, "VDD by MTT"));
        engine.heard(ToService::Displays(with_it));
        assert_eq!(filmed(&mut told), vec![GROWN.to_string()]);

        // And back to this computer's own screens.
        engine.no_longer_the_grown_screen().unwrap();
        assert!(!engine.films_the_grown_screen());
        assert_eq!(filmed(&mut told), vec![String::new()]);
    }

    #[test]
    fn an_engine_told_the_main_screen_all_along_is_told_again_when_the_grown_one_goes() {
        let (engine, mut told) = connected();
        engine.film_now().unwrap();
        engine.heard(ToService::Ready {
            encodable: CodecSet::empty().with(VideoCodec::H264),
            encoders: "libx264".to_string(),
            displays: two_screens(),
        });
        // The grown screen never showed among the engine's screens, so
        // the engine was never told its name: it has been filming "the
        // main screen", which was the grown one for as long as the
        // desktop was on it.
        engine.film(Film::Grown).unwrap();
        assert_eq!(filmed(&mut told), vec![String::new()]);

        // The desktop is back on this computer's own screens: "the main
        // screen" is another one, and the engine looks for it again.
        engine.no_longer_the_grown_screen().unwrap();
        assert_eq!(filmed(&mut told), vec![String::new()]);
        assert!(engine.films_the_main_screen());

        // A session that was never served from the grown screen has
        // nothing to be told again.
        engine.no_longer_the_grown_screen().unwrap();
        assert!(filmed(&mut told).is_empty());
    }

    #[test]
    fn the_grown_screen_is_never_offered_to_choose_from() {
        let (engine, _told) = connected();
        let mut screens = two_screens();
        screens.push(display(GROWN, false, "VDD by MTT"));
        engine.heard(ToService::Displays(screens));
        let offered = engine.screens();
        assert_eq!(offered.len(), 2);
        assert_eq!(offered[0].id, MAIN);
        assert!(offered[0].main);
        assert_eq!(offered[1].id, SIDE);
        // And they travel as the far end reads them.
        let written = zyr_proto::session::far_screens_written(&offered);
        assert_eq!(zyr_proto::session::far_screens_read(&written), offered);
    }

    #[test]
    fn what_the_engine_serves_sizes_the_tunnel() {
        let (engine, _told) = connected();
        engine.film_now().unwrap();
        let said = engine.heard(ToService::Serving {
            kbps: 62_872,
            fps: 144,
        });
        assert_eq!(
            said.serving,
            Some(MediaProfile {
                bits_per_second: 62_872_000,
                frames_per_second: 144,
            })
        );
        assert_eq!(said.lines.len(), 1);
        assert!(said.lines[0].contains("62872 kbps"), "{:?}", said.lines);
    }

    #[test]
    fn an_engine_given_no_picture_says_so_and_the_size_it_films_is_kept() {
        let (engine, _told) = connected();
        engine.film_now().unwrap();
        assert!(engine.films_the_main_screen());
        assert_eq!(engine.size_it_films(), None);

        engine.heard(ToService::Filming {
            display: String::new(),
            width: 2560,
            height: 1440,
        });
        assert_eq!(engine.size_it_films(), Some((2560, 1440)));

        let said = engine.heard(ToService::Silent {
            display: MAIN.to_string(),
        });
        assert!(said.silent);
        assert!(said.lines[0].contains("no picture"), "{:?}", said.lines);
        // Nothing else the engine says is that.
        let said = engine.heard(ToService::Serving { kbps: 1, fps: 1 });
        assert!(!said.silent);
    }

    #[test]
    fn only_a_session_served_from_the_main_screen_is_served_from_the_main_screen() {
        let (engine, _told) = connected();
        assert!(engine.films_the_main_screen());
        // Somebody picked another screen: what it gives is theirs to see.
        engine.film(Film::This(SIDE.to_string())).unwrap();
        assert!(!engine.films_the_main_screen());
        engine.film(Film::Grown).unwrap();
        assert!(!engine.films_the_main_screen());
        engine.film(Film::Main).unwrap();
        assert!(engine.films_the_main_screen());
    }

    #[test]
    fn an_engine_not_connected_yet_cannot_be_told_anything() {
        let engine = Engine::default();
        assert!(engine.film(Film::Main).is_err());
        assert!(engine.tell(&ToEngine::Stop).is_err());

        // And what it was not told is told once it can be.
        let (telling, mut told) = mpsc::channel(4);
        engine.connected(telling);
        engine.film_now().unwrap();
        assert_eq!(filmed(&mut told), vec![String::new()]);
    }

    #[test]
    fn what_the_engine_says_is_written_down_in_words() {
        let (engine, _told) = connected();
        engine.film_now().unwrap();
        let said = engine.heard(ToService::Trouble {
            fact: Fact::new("engine.capture_failed").with("detail", "the device was lost"),
        });
        assert!(
            said.lines[0].contains("engine.capture_failed"),
            "{:?}",
            said.lines
        );
        let said = engine.heard(ToService::Filming {
            display: String::new(),
            width: 2560,
            height: 1440,
        });
        assert!(said.lines[0].contains("2560x1440"), "{:?}", said.lines);
        let said = engine.heard(ToService::Ready {
            encodable: CodecSet::empty(),
            encoders: String::new(),
            displays: Vec::new(),
        });
        assert!(
            said.lines[0].contains("encodes nothing"),
            "{:?}",
            said.lines
        );
    }
}
