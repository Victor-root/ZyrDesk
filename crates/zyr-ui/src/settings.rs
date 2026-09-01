//! The settings screen, and the three lines of the session menu.
//!
//! Nothing is kept here. What the person chooses goes straight to the
//! service, which writes it down and hands it back at the next question.
//! That is what makes a choice survive the window being closed, and what
//! lets the command line read the same file when it has to be looked at
//! by hand. It is also what makes a choice made from inside a session
//! still be there at the next one.
//!
//! What a session asks for lives in the session's own menu and not on
//! this screen. Size, rate and codec are the three numbers somebody
//! changes while looking at the picture they change, and walking back to
//! a settings screen to try one is walking away from the only thing that
//! says whether it worked. The lists themselves stay in `zyr-proto`: a
//! second copy written in JavaScript would drift from them.

// Ce qu'une session propose et ce qu'on y choisit ne se lit que dans le
// menu du bouton flottant, qui n'existe que sous Windows comme la session
// elle-même. Le reste de ce fichier, les réglages de l'accueil, sert
// partout.
#![cfg_attr(not(windows), allow(dead_code))]

use zyr_control::{Answer, Request};
use zyr_proto::session::{
    Asked, CODECS_OFFERED, Codec, DisplayMode, FarScreen, Preferred, RATES_OFFERED, SIZES_OFFERED,
    Screen,
};

use crate::service;

/// What the settings screen shows.
#[derive(PartialEq)]
pub struct Settings {
    pub codec: String,
    pub display: String,
    pub absolute_mouse: bool,
    pub stats_overlay: bool,
    /// Whether the far computer's speakers fall silent for the length of
    /// a session opened from here.
    pub mute_far_speakers: bool,
    /// What a session opened right now would ask for, so the screen can
    /// say it out loud rather than leave it to be guessed.
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
}

impl Settings {
    /// What the screen shows on a computer whose own screen is `screen`,
    /// `None` standing for one that could not be measured.
    fn shown(preferred: Preferred, screen: Option<Screen>) -> Self {
        let settings = preferred.settings(screen);
        Self {
            codec: preferred.codec.to_string(),
            display: preferred.display_mode.to_string(),
            absolute_mouse: preferred.absolute_mouse,
            stats_overlay: preferred.stats_overlay,
            mute_far_speakers: preferred.mute_far_speakers,
            width: settings.width,
            height: settings.height,
            fps: settings.fps,
            bitrate_kbps: settings.bitrate_kbps,
        }
    }
}

/// What the screen sends back. Only the choices: the rest follows.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Chosen {
    pub codec: Codec,
    pub display: DisplayMode,
    pub absolute_mouse: bool,
    pub stats_overlay: bool,
    pub mute_far_speakers: bool,
}

impl Chosen {
    /// What the screen would send back if nothing were touched.
    ///
    /// The screen changes one line at a time and hands the whole thing
    /// back, so it has to start from what is written down: built any
    /// other way, every click would carry four settings nobody chose.
    pub fn of(preferred: Preferred) -> Self {
        Self {
            codec: preferred.codec,
            display: preferred.display_mode,
            absolute_mouse: preferred.absolute_mouse,
            stats_overlay: preferred.stats_overlay,
            mute_far_speakers: preferred.mute_far_speakers,
        }
    }

    /// Lays these choices over the ones already written down.
    ///
    /// Over and not instead of: this screen no longer carries every
    /// setting, and the ones it does not carry belong to whoever set
    /// them, which is the session's own menu.
    fn laid_over(&self, preferred: Preferred) -> Preferred {
        Preferred {
            codec: self.codec,
            display_mode: self.display,
            absolute_mouse: self.absolute_mouse,
            stats_overlay: self.stats_overlay,
            mute_far_speakers: self.mute_far_speakers,
            ..preferred
        }
    }
}

pub async fn settings(app: tauri::AppHandle) -> Settings {
    Settings::shown(
        preferred().await,
        crate::picture::the_screen_of_this_computer(&app),
    )
}

/// Changes what every session from now on looks like.
pub async fn choose(chosen: Chosen) -> Result<(), String> {
    write_down(chosen.laid_over(preferred().await)).await
}

/// The three lines of the session menu, as they stand.
///
/// Machine values and not words: what a size or a rate is called in
/// French is the window's business, and the window is where the rest of
/// what a person reads is written.
#[derive(PartialEq)]
pub struct SessionChoice {
    pub asked: String,
    pub bitrate_kbps: u32,
    pub codec: String,
    /// Whether the far computer is asked to resend a still screen at
    /// full rate. Its own setting, changed from here, which is the whole
    /// point: the person who can tell whether the picture feels smooth is
    /// the one watching it.
    pub steady: bool,
    /// What the size comes down to on this computer, which is the whole
    /// point of the word « screen » and cannot be worked out from it.
    pub width: u32,
    pub height: u32,
    /// Which of the far computer's screens the session is served from,
    /// empty naming its main one.
    ///
    /// Apart from the rest and written into no settings file: it names one
    /// screen of one particular computer, so it lasts as long as the
    /// session and no longer.
    pub screen: String,
    /// Whether what is chosen is not what the picture on screen shows.
    ///
    /// The one thing the window cannot work out for itself: a choice is
    /// written down the moment it is made, so what is chosen and what is
    /// being shown are the same numbers read from two different places.
    pub to_apply: bool,
}

impl SessionChoice {
    fn of(preferred: Preferred, screen: Option<Screen>) -> Self {
        let (width, height) = preferred.asked.size(screen.map(Screen::size));
        Self {
            asked: preferred.asked.to_string(),
            bitrate_kbps: preferred.bitrate_kbps,
            codec: preferred.codec.to_string(),
            steady: preferred.steady_far_rate,
            screen: crate::session::the_far_screen_named(),
            width,
            height,
            to_apply: crate::session::waiting_to_be_applied(&preferred),
        }
    }
}

/// One value a line of the session menu offers.
#[derive(PartialEq)]
pub struct Offered {
    /// What travels and is written down.
    pub value: String,
    /// What that size comes down to on this computer. Zero for a line
    /// that is not a size: « screen » is the only value whose meaning is
    /// not in the value itself.
    pub width: u32,
    pub height: u32,
}

/// One of the far computer's screens, as the menu offers it.
#[derive(PartialEq)]
pub struct OfferedScreen {
    /// What travels back when it is picked. That computer's own name for
    /// the screen, opaque here on purpose.
    pub id: String,
    /// What a person reads.
    pub name: String,
    /// Whether it is the one that computer starts its desktop on, which
    /// is the one a session is served from unless somebody says
    /// otherwise.
    pub main: bool,
    pub wide: u32,
    pub high: u32,
}

/// Everything the lines of the session menu offer, and where they stand
/// right now.
///
/// Handed over whole rather than a list at a time: the window builds the
/// lists once, when it opens, and a person clicking through them then
/// waits for nothing.
#[derive(PartialEq)]
pub struct SessionMenu {
    pub sizes: Vec<Offered>,
    pub rates: Vec<u32>,
    pub codecs: Vec<String>,
    /// The screens the far computer is showing on.
    ///
    /// Empty outside a session, on a far computer whose engine has not
    /// said, and on one that has a single screen worth offering: the line
    /// then does not show at all, which is right in all three cases.
    pub screens: Vec<OfferedScreen>,
    /// The ones the far computer's engine says it cannot make.
    ///
    /// Empty means it has not said, which is every menu opened outside a
    /// session and every far computer whose engine has not finished
    /// starting. Empty is never « it can make none »: a computer that
    /// could encode nothing could not be watched at all.
    pub beyond_it: Vec<String>,
    pub now: SessionChoice,
}

pub async fn session_menu(app: tauri::AppHandle) -> SessionMenu {
    let screen = crate::picture::the_screen_of_this_computer(&app);
    let preferred = preferred().await;
    SessionMenu {
        sizes: SIZES_OFFERED
            .iter()
            .map(|asked| {
                let (width, height) = asked.size(screen.map(Screen::size));
                Offered {
                    value: asked.to_string(),
                    width,
                    height,
                }
            })
            .collect(),
        rates: RATES_OFFERED.to_vec(),
        codecs: CODECS_OFFERED.iter().map(Codec::to_string).collect(),
        screens: the_far_computers_screens().await,
        beyond_it: beyond_the_far_computer().await,
        now: SessionChoice::of(preferred, screen),
    }
}

/// The screens the far computer of the session in progress is showing on.
///
/// Asked of that computer, because nothing here can know what is plugged
/// in over there. Nothing at all when there is no session, when the way is
/// gone, or when its engine has not said: an unanswered question leaves
/// the menu without that line rather than offering a list of one.
///
/// A single screen is no choice, so it is not offered either. Every
/// computer with one screen would otherwise carry a menu line that can
/// only be set to what it already is.
async fn the_far_computers_screens() -> Vec<OfferedScreen> {
    let Some(way) = crate::session::the_way_in_use().await else {
        return Vec::new();
    };
    let Ok(Answer::Screens(listed)) = service::ask(&Request::FarScreens { way }).await else {
        return Vec::new();
    };
    let screens = zyr_proto::session::far_screens_read(&listed);
    crate::session::remember_the_far_screens(&screens);
    if screens.len() < 2 {
        return Vec::new();
    }
    screens.iter().map(offered).collect()
}

fn offered(screen: &FarScreen) -> OfferedScreen {
    OfferedScreen {
        id: screen.id.clone(),
        name: screen.name.clone(),
        main: screen.main,
        wide: screen.wide,
        high: screen.high,
    }
}

/// The codecs the far computer of the session in progress cannot make.
///
/// Worked out from what it says it can, and not the other way round: a
/// computer names what it found, and anything it did not name is either
/// beyond it or a codec this product has never heard of. « Automatique »
/// is never beyond anybody, being the choice not to choose.
///
/// Nothing at all when there is no session, when the way is gone, or when
/// that computer's engine has not said: an unanswered question must leave
/// the menu exactly as it was rather than grey half of it out.
async fn beyond_the_far_computer() -> Vec<String> {
    let Some(way) = crate::session::the_way_in_use().await else {
        return Vec::new();
    };
    let Ok(Answer::Codecs(named)) = service::ask(&Request::FarCodecs { way }).await else {
        return Vec::new();
    };
    let can: Vec<Codec> = named
        .split_whitespace()
        .filter_map(|it| it.parse().ok())
        .collect();
    if can.is_empty() {
        return Vec::new();
    }
    CODECS_OFFERED
        .iter()
        .filter(|codec| **codec != Codec::Auto && !can.contains(codec))
        .map(Codec::to_string)
        .collect()
}

/// Sets one line of the session menu to one of the values it offers,
/// writes the result down, and hands back where the three lines stand.
///
/// Written down and nothing more: what a session asks for is settled when
/// its engine is started, and it is told once. So the picture on screen
/// goes on showing what it was opened with, and the menu offers to open
/// it again as soon as the two differ, which is what `apply_session`
/// does. Opening it again at every click would stop and start the session
/// on each one, which is not what a menu line should do unasked, and is
/// the whole reason several changes can be made before applying them.
///
/// A value the product does not offer is refused rather than written
/// down. These come from a list the product handed over itself, so a
/// value from anywhere else is a window and a service that no longer
/// agree, and quietly keeping it would hide that.
pub async fn choose_session(
    app: tauri::AppHandle,
    which: String,
    value: String,
) -> Result<SessionChoice, String> {
    let mut preferred = preferred().await;
    match which.as_str() {
        "asked" => {
            let asked = value.parse::<Asked>()?;
            if !SIZES_OFFERED.contains(&asked) {
                return Err(format!("taille non proposée : {value}"));
            }
            preferred.asked = asked;
        }
        "bitrate" => {
            let rate = value
                .parse::<u32>()
                .map_err(|_| format!("débit illisible : {value}"))?;
            if !RATES_OFFERED.contains(&rate) {
                return Err(format!("débit non proposé : {value}"));
            }
            preferred.bitrate_kbps = rate;
        }
        "codec" => {
            let codec = value.parse::<Codec>()?;
            if !CODECS_OFFERED.contains(&codec) {
                return Err(format!("codec non proposé : {value}"));
            }
            preferred.codec = codec;
        }
        // Written down nowhere, so it never reaches the service: which of
        // the far computer's screens is being watched names one screen of
        // one particular machine, and it lasts exactly as long as the
        // session does.
        "screen" => {
            let Some(picked) = crate::session::the_far_screens()
                .into_iter()
                .find(|screen| screen.id == value)
            else {
                return Err(format!("écran non proposé : {value}"));
            };
            // Its main screen is what a session asks for when it asks for
            // nothing, so picking it by hand is asking for nothing: said
            // any other way, choosing the screen the session is already on
            // would offer to open the picture again for no change at all.
            crate::session::ask_for_the_far_screen((!picked.main).then_some(picked.id));
            return Ok(SessionChoice::of(
                preferred,
                crate::picture::the_screen_of_this_computer(&app),
            ));
        }
        // Two words and not a list: it is a switch, and the two sides are
        // named in the window like the ones beside them.
        "steady" => match value.as_str() {
            "on" => preferred.steady_far_rate = true,
            "off" => preferred.steady_far_rate = false,
            other => return Err(format!("cadence non proposée : {other}")),
        },
        other => return Err(format!("réglage inconnu : {other}")),
    }
    write_down(preferred).await?;
    Ok(SessionChoice::of(
        preferred,
        crate::picture::the_screen_of_this_computer(&app),
    ))
}

/// Hands a whole set of preferences to the service, which is the one
/// thing that writes them down.
async fn write_down(preferred: Preferred) -> Result<(), String> {
    match service::ask(&Request::Choose { preferred }).await? {
        Answer::Done => Ok(()),
        other => Err(service::unexpected(other)),
    }
}

/// Writes down one choice the person has just changed from inside a
/// session.
///
/// A choice made in the middle of a session is a choice all the same: the
/// next one opens the way the last one was left, without anybody having
/// to go back to the settings screen to say so twice.
///
/// `change` says whether it changed anything, so a switch put back where
/// it already was costs no round trip. Nothing here fails a session: what
/// the person asked for has already happened, and only its remembering is
/// at stake.
async fn remember(named: &str, said: String, change: impl FnOnce(&mut Preferred) -> bool) {
    let mut preferred = preferred().await;
    if !change(&mut preferred) {
        return;
    }
    match service::ask(&Request::Choose { preferred }).await {
        Ok(Answer::Done) => crate::journal::note(&said),
        Ok(other) => crate::journal::note(&format!(
            "{named} not written down: {}",
            service::unexpected(other)
        )),
        Err(reason) => crate::journal::note(&format!("{named} not written down: {reason}")),
    }
}

/// Writes down how a session is shown.
pub async fn remember_display(mode: DisplayMode) {
    remember(
        "display mode",
        format!("sessions will open {mode} from now on"),
        |preferred| {
            let moved = preferred.display_mode != mode;
            preferred.display_mode = mode;
            moved
        },
    )
    .await;
}

/// Writes down which side of the switch the system's keys are on.
pub async fn remember_system_keys(theirs: bool) {
    remember(
        "where the system's keys go",
        format!(
            "the system's keys will go to {} from now on",
            if theirs {
                "the session"
            } else {
                "this computer"
            }
        ),
        |preferred| {
            let moved = preferred.system_keys != theirs;
            preferred.system_keys = theirs;
            moved
        },
    )
    .await;
}

/// What the service has been told a session should look like.
///
/// The ordinary settings when it cannot be asked: a session is about to
/// fail on the service being absent, and that is the trouble worth
/// showing, not a settings one.
pub async fn preferred() -> Preferred {
    what_was_chosen().await.unwrap_or_default()
}

/// The same, saying so when the service could not be asked.
///
/// For the one caller that has something better than the defaults to
/// fall back on: reopening a picture reads this to know what to reopen
/// it with, and a service that did not answer at that instant would have
/// it reopened with settings nobody chose. The person asked for one
/// thing to change and would watch three others change with it.
pub async fn what_was_chosen() -> Option<Preferred> {
    match service::ask(&Request::Settings).await {
        Ok(Answer::Settings(preferred)) => Some(preferred),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use zyr_proto::session::LIFE_SIZE;

    use super::*;

    fn chosen() -> Chosen {
        Chosen {
            codec: Codec::Hevc,
            display: DisplayMode::Windowed,
            absolute_mouse: false,
            stats_overlay: true,
            mute_far_speakers: true,
        }
    }

    /// Un écran ordinaire de cette taille, pour les essais qui ne parlent
    /// que de taille.
    fn a_screen(wide: u32, high: u32) -> Screen {
        Screen {
            wide,
            high,
            refresh: 60,
            scale: LIFE_SIZE,
        }
    }

    #[test]
    fn what_the_screen_shows_and_sends_back_is_the_same_thing() {
        // Les deux formes se croisent à chaque changement : ce qui
        // s'affiche doit pouvoir être renvoyé tel quel.
        let written = chosen().laid_over(Preferred::default());
        assert_eq!(Chosen::of(written), chosen());
    }

    #[test]
    fn the_settings_screen_leaves_the_session_menu_alone() {
        // L'écran des réglages ne porte plus ni la taille, ni le débit :
        // les renvoyer tels quels ne doit rien effacer de ce que le menu
        // de la session a réglé.
        let set_from_the_menu = Preferred {
            asked: Asked::Fixed(2560, 1440),
            bitrate_kbps: 15_000,
            ..Preferred::default()
        };
        let after = chosen().laid_over(set_from_the_menu);
        assert_eq!(after.asked, Asked::Fixed(2560, 1440));
        assert_eq!(after.bitrate_kbps, 15_000);
        assert_eq!(after.codec, Codec::Hevc);
    }

    #[test]
    fn the_screen_says_what_a_session_would_ask_for_here() {
        // Sans ça, la fenêtre porterait sa propre table, qui s'écarterait
        // de celle du produit au premier changement. Et la table ne
        // suffit pas : « l'écran » ne vaut pas la même chose partout.
        let shown = Settings::shown(Preferred::default(), None);
        assert_eq!((shown.width, shown.height), (1920, 1080));
        assert_eq!(shown.fps, 60);

        let big = Settings::shown(
            Preferred::default(),
            Some(Screen {
                wide: 3840,
                high: 2160,
                refresh: 144,
                scale: LIFE_SIZE,
            }),
        );
        assert_eq!((big.width, big.height), (3840, 2160));
        // La cadence de l'écran mesuré et non celle du défaut : c'est ce
        // que la ligne sous « Qualité » doit annoncer.
        assert_eq!(big.fps, 144);
    }

    #[test]
    fn a_session_line_says_what_its_value_comes_down_to_here() {
        // « L'écran » ne se lit pas dans le mot : la ligne doit dire à
        // quoi il revient sur cet ordinateur-ci, sinon on ne sait pas si
        // on demande du 4K ou du 1080p.
        let screen = SessionChoice::of(Preferred::default(), Some(a_screen(2560, 1440)));
        assert_eq!(screen.asked, "client");
        assert_eq!((screen.width, screen.height), (2560, 1440));

        // Et l'écran d'en face n'est pas connu ici : en attendant qu'il
        // le dise, la ligne montre ce qu'une session demanderait, qui est
        // l'écran de cet ordinateur-ci.
        let far = SessionChoice::of(
            Preferred {
                asked: Asked::Host,
                ..Preferred::default()
            },
            Some(a_screen(2560, 1440)),
        );
        assert_eq!(far.asked, "host");
        assert_eq!((far.width, far.height), (2560, 1440));

        let fixed = SessionChoice::of(
            Preferred {
                asked: Asked::Fixed(1920, 1080),
                ..Preferred::default()
            },
            Some(a_screen(3840, 2160)),
        );
        assert_eq!((fixed.width, fixed.height), (1920, 1080));
    }
}
