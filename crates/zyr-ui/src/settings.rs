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

use serde::{Deserialize, Serialize};
use zyr_control::{Answer, Request};
use zyr_proto::session::{
    Asked, CODECS_OFFERED, Codec, DisplayMode, Preferred, RATES_OFFERED, SIZES_OFFERED,
};

use crate::service;

/// What the settings screen shows.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
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
    fn shown(preferred: Preferred, screen: Option<(u32, u32)>) -> Self {
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
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chosen {
    codec: String,
    display: String,
    absolute_mouse: bool,
    stats_overlay: bool,
    mute_far_speakers: bool,
}

impl Chosen {
    /// Lays these choices over the ones already written down.
    ///
    /// Over and not instead of: this screen no longer carries every
    /// setting, and the ones it does not carry belong to whoever set
    /// them, which is the session's own menu.
    fn laid_over(&self, preferred: Preferred) -> Result<Preferred, String> {
        Ok(Preferred {
            codec: self.codec.parse::<Codec>()?,
            display_mode: self.display.parse::<DisplayMode>()?,
            absolute_mouse: self.absolute_mouse,
            stats_overlay: self.stats_overlay,
            mute_far_speakers: self.mute_far_speakers,
            ..preferred
        })
    }
}

#[tauri::command]
pub async fn settings(app: tauri::AppHandle) -> Settings {
    Settings::shown(
        preferred().await,
        crate::picture::the_screen_of_this_computer(&app),
    )
}

/// Changes what every session from now on looks like.
#[tauri::command]
pub async fn choose(chosen: Chosen) -> Result<(), String> {
    let preferred = chosen.laid_over(preferred().await)?;
    write_down(preferred).await
}

/// The three lines of the session menu, as they stand.
///
/// Machine values and not words: what a size or a rate is called in
/// French is the window's business, and the window is where the rest of
/// what a person reads is written.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionChoice {
    pub asked: String,
    pub bitrate_kbps: u32,
    pub codec: String,
    /// What the size comes down to on this computer, which is the whole
    /// point of the word « screen » and cannot be worked out from it.
    pub width: u32,
    pub height: u32,
    /// Whether these three are not what the picture on screen is showing.
    ///
    /// The one thing the window cannot work out for itself: a choice is
    /// written down the moment it is made, so what is chosen and what is
    /// being shown are the same numbers read from two different places.
    pub to_apply: bool,
}

impl SessionChoice {
    fn of(preferred: Preferred, screen: Option<(u32, u32)>) -> Self {
        let (width, height) = preferred.asked.size(screen);
        Self {
            asked: preferred.asked.to_string(),
            bitrate_kbps: preferred.bitrate_kbps,
            codec: preferred.codec.to_string(),
            width,
            height,
            to_apply: crate::session::waiting_to_be_applied(&preferred),
        }
    }
}

#[tauri::command]
pub async fn session_choice(app: tauri::AppHandle) -> SessionChoice {
    SessionChoice::of(
        preferred().await,
        crate::picture::the_screen_of_this_computer(&app),
    )
}

/// One value a line of the session menu offers.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Offered {
    /// What travels and is written down.
    pub value: String,
    /// What that size comes down to on this computer. Zero for a line
    /// that is not a size: « screen » is the only value whose meaning is
    /// not in the value itself.
    pub width: u32,
    pub height: u32,
}

/// Everything the three lines of the session menu offer, and where they
/// stand right now.
///
/// Handed over whole rather than a list at a time: the window builds the
/// three lists once, when it opens, and a person clicking through them
/// then waits for nothing.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMenu {
    pub sizes: Vec<Offered>,
    pub rates: Vec<u32>,
    pub codecs: Vec<String>,
    pub now: SessionChoice,
}

#[tauri::command]
pub async fn session_menu(app: tauri::AppHandle) -> SessionMenu {
    let screen = crate::picture::the_screen_of_this_computer(&app);
    let preferred = preferred().await;
    SessionMenu {
        sizes: SIZES_OFFERED
            .iter()
            .map(|asked| {
                let (width, height) = asked.size(screen);
                Offered {
                    value: asked.to_string(),
                    width,
                    height,
                }
            })
            .collect(),
        rates: RATES_OFFERED.to_vec(),
        codecs: CODECS_OFFERED.iter().map(Codec::to_string).collect(),
        now: SessionChoice::of(preferred, screen),
    }
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
#[tauri::command]
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

/// Writes down how a session is shown, the person having just changed it
/// from inside one.
///
/// A choice made in the middle of a session is a choice all the same: the
/// next one opens the way the last one was left, without anybody having
/// to go back to the settings screen to say so twice.
pub async fn remember_display(mode: DisplayMode) {
    let mut preferred = preferred().await;
    if preferred.display_mode == mode {
        return;
    }
    preferred.display_mode = mode;
    match service::ask(&Request::Choose { preferred }).await {
        Ok(Answer::Done) => crate::journal::note(&format!("sessions will open {mode} from now on")),
        Ok(other) => crate::journal::note(&format!(
            "display mode not written down: {}",
            service::unexpected(other)
        )),
        Err(reason) => crate::journal::note(&format!("display mode not written down: {reason}")),
    }
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
    use super::*;

    fn chosen() -> Chosen {
        Chosen {
            codec: "HEVC".to_string(),
            display: "windowed".to_string(),
            absolute_mouse: false,
            stats_overlay: true,
            mute_far_speakers: true,
        }
    }

    #[test]
    fn what_the_screen_shows_and_sends_back_is_the_same_thing() {
        // Les deux formes se croisent à chaque changement : ce qui
        // s'affiche doit pouvoir être renvoyé tel quel.
        let shown = Settings::shown(chosen().laid_over(Preferred::default()).unwrap(), None);
        let returned = Chosen {
            codec: shown.codec,
            display: shown.display,
            absolute_mouse: shown.absolute_mouse,
            stats_overlay: shown.stats_overlay,
            mute_far_speakers: shown.mute_far_speakers,
        };
        assert_eq!(
            returned.laid_over(Preferred::default()).unwrap(),
            chosen().laid_over(Preferred::default()).unwrap()
        );
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
        let after = chosen().laid_over(set_from_the_menu).unwrap();
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

        let big = Settings::shown(Preferred::default(), Some((3840, 2160)));
        assert_eq!((big.width, big.height), (3840, 2160));
    }

    #[test]
    fn a_session_line_says_what_its_value_comes_down_to_here() {
        // « L'écran » ne se lit pas dans le mot : la ligne doit dire à
        // quoi il revient sur cet ordinateur-ci, sinon on ne sait pas si
        // on demande du 4K ou du 1080p.
        let screen = SessionChoice::of(Preferred::default(), Some((2560, 1440)));
        assert_eq!(screen.asked, "screen");
        assert_eq!((screen.width, screen.height), (2560, 1440));

        let fixed = SessionChoice::of(
            Preferred {
                asked: Asked::Fixed(1920, 1080),
                ..Preferred::default()
            },
            Some((3840, 2160)),
        );
        assert_eq!((fixed.width, fixed.height), (1920, 1080));
    }

    #[test]
    fn a_value_the_product_does_not_know_is_refused_and_named() {
        let nonsense = Chosen {
            codec: "ultra".to_string(),
            ..chosen()
        };
        let refusal = nonsense.laid_over(Preferred::default()).unwrap_err();
        assert!(refusal.contains("ultra"), "{refusal}");
    }
}
