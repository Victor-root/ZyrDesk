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

// Outside Windows nothing calls this module: the service does not exist
// there. Its logic has nothing platform-specific about it and stays
// compiled and tested everywhere.
#![cfg_attr(not(windows), allow(dead_code))]

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
}

/// What a message from the engine comes to, for whoever holds it.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Said {
    /// Lines for the journal.
    pub lines: Vec<String>,
    /// What the engine now serves, for the window the tunnel holds open.
    pub serving: Option<MediaProfile>,
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
            }),
            telling: Mutex::new(None),
        }
    }
}

impl Engine {
    /// The engine is connected: from now on it is told things there.
    pub fn connected(&self, telling: mpsc::Sender<Vec<u8>>) {
        *self.telling.lock().expect("moteur de la session") = Some(telling);
    }

    /// Tells the engine something, never waiting: what the engine is
    /// told is a handful of messages a session, and a queue full of them
    /// is an engine that has stopped reading.
    pub fn tell(&self, message: &ToEngine) -> Result<(), String> {
        let telling = self.telling.lock().expect("moteur de la session");
        let Some(telling) = telling.as_ref() else {
            return Err("aucun moteur ne sert encore cette session".to_string());
        };
        telling
            .try_send(message.encode())
            .map_err(|_| "le moteur de cette session ne répond plus".to_string())
    }

    /// Decides which screen the session is served from, and tells the
    /// engine when that changes what it films.
    pub fn film(&self, wanted: Film) -> Result<(), String> {
        let mut heard = self.heard.lock().expect("moteur de la session");
        heard.film = wanted;
        self.film_if_it_changed(&mut heard)
    }

    /// Tells the engine the screen decided on, if it has not been told
    /// already.
    pub fn film_now(&self) -> Result<(), String> {
        let mut heard = self.heard.lock().expect("moteur de la session");
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
        self.heard.lock().expect("moteur de la session").film == Film::Grown
    }

    /// Serves the session from this computer's own screens again, if it
    /// was served from the one it grew.
    pub fn no_longer_the_grown_screen(&self) -> Result<(), String> {
        if !self.films_the_grown_screen() {
            return Ok(());
        }
        self.film(Film::Main)
    }

    /// The screens a session may ask to be served from: every screen the
    /// engine can film but the one this computer grows for itself, which
    /// nobody sitting at this machine can see.
    pub fn screens(&self) -> Vec<FarScreen> {
        let driver = zyr_screen::shipped();
        self.heard
            .lock()
            .expect("moteur de la session")
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
        let mut heard = self.heard.lock().expect("moteur de la session");
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
                    serving: None,
                }
            }
            ToService::Filming {
                display,
                width,
                height,
            } => Said {
                lines: vec![format!(
                    "the engine films {} at {width}x{height}",
                    if display.is_empty() {
                        "the main screen"
                    } else {
                        &display
                    }
                )],
                serving: None,
            },
            ToService::Displays(displays) => {
                let line = format!("the screens changed: {}", listed(&displays));
                self.displays = displays;
                Said {
                    lines: vec![line],
                    serving: None,
                }
            }
            ToService::Trouble { text } => Said {
                lines: vec![format!("the engine says: {text}")],
                serving: None,
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
        .join(" ; ")
}

#[cfg(test)]
mod tests {
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
            text: "La capture de l'écran a échoué".to_string(),
        });
        assert!(said.lines[0].contains("La capture"), "{:?}", said.lines);
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
