//! The settings screen.
//!
//! Nothing is kept here either. What the person chooses goes straight to
//! the service, which writes it down and hands it back at the next
//! question. That is what makes a choice survive the window being
//! closed, and what lets the command line read the same file when it
//! has to be looked at by hand.
//!
//! The window is told what a quality actually asks for rather than
//! working it out: the table lives in one place, in `zyr-proto`, and a
//! second copy in JavaScript would drift from it.

use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};
use zyr_control::{Answer, Request};
use zyr_proto::paths;
use zyr_proto::session::{Codec, DisplayMode, Preferred, Quality};

use crate::service;

/// What the settings screen shows.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub quality: String,
    pub codec: String,
    pub display: String,
    pub absolute_mouse: bool,
    pub stats_overlay: bool,
    /// What the chosen quality comes down to, so the screen can say it
    /// out loud instead of hiding behind a word.
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
}

impl From<Preferred> for Settings {
    fn from(preferred: Preferred) -> Self {
        let settings = preferred.settings();
        Self {
            quality: preferred.quality.to_string(),
            codec: preferred.codec.to_string(),
            display: preferred.display_mode.to_string(),
            absolute_mouse: preferred.absolute_mouse,
            stats_overlay: preferred.stats_overlay,
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
    quality: String,
    codec: String,
    display: String,
    absolute_mouse: bool,
    stats_overlay: bool,
}

impl Chosen {
    fn understood(&self) -> Result<Preferred, String> {
        Ok(Preferred {
            quality: self.quality.parse::<Quality>()?,
            codec: self.codec.parse::<Codec>()?,
            display_mode: self.display.parse::<DisplayMode>()?,
            absolute_mouse: self.absolute_mouse,
            stats_overlay: self.stats_overlay,
        })
    }
}

#[tauri::command]
pub async fn settings() -> Settings {
    preferred().await.into()
}

/// Changes what every session from now on looks like.
#[tauri::command]
pub async fn choose(chosen: Chosen) -> Result<(), String> {
    let preferred = chosen.understood()?;
    match service::ask(&Request::Choose { preferred }).await? {
        Answer::Done => Ok(()),
        other => Err(service::unexpected(other)),
    }
}

/// What the service has been told a session should look like.
///
/// The ordinary settings when it cannot be asked: a session is about to
/// fail on the service being absent, and that is the trouble worth
/// showing, not a settings one.
pub async fn preferred() -> Preferred {
    match service::ask(&Request::Settings).await {
        Ok(Answer::Settings(preferred)) => preferred,
        _ => Preferred::default(),
    }
}

/// Where the product writes what it has to say.
#[tauri::command]
pub fn logs_folder() -> String {
    paths::logs_dir().display().to_string()
}

/// Opens that folder, so a problem can be looked at without anyone
/// having to be told where to click.
#[tauri::command]
pub fn open_logs() -> Result<(), String> {
    let folder = paths::logs_dir();
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    shown(&folder).map_err(|e| format!("le dossier n'a pas pu être ouvert : {e}"))
}

#[cfg(windows)]
fn shown(folder: &Path) -> std::io::Result<()> {
    // The file explorer answers with a code of its own whatever happens,
    // so only the launch is worth checking.
    Command::new("explorer").arg(folder).spawn().map(|_| ())
}

#[cfg(not(windows))]
fn shown(folder: &Path) -> std::io::Result<()> {
    Command::new("xdg-open").arg(folder).spawn().map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chosen() -> Chosen {
        Chosen {
            quality: "detailed".to_string(),
            codec: "HEVC".to_string(),
            display: "windowed".to_string(),
            absolute_mouse: false,
            stats_overlay: true,
        }
    }

    #[test]
    fn what_the_screen_shows_and_sends_back_is_the_same_thing() {
        // Les deux formes se croisent à chaque changement : ce qui
        // s'affiche doit pouvoir être renvoyé tel quel.
        let shown = Settings::from(chosen().understood().unwrap());
        let returned = Chosen {
            quality: shown.quality,
            codec: shown.codec,
            display: shown.display,
            absolute_mouse: shown.absolute_mouse,
            stats_overlay: shown.stats_overlay,
        };
        assert_eq!(
            returned.understood().unwrap(),
            chosen().understood().unwrap()
        );
    }

    #[test]
    fn the_screen_says_what_the_quality_comes_down_to() {
        // Sans ça, la fenêtre porterait sa propre table de qualités, qui
        // s'écarterait de celle du produit au premier changement.
        let shown = Settings::from(Preferred::default());
        assert_eq!((shown.width, shown.height), (1920, 1080));
        assert_eq!(shown.fps, 60);
        assert_eq!(shown.bitrate_kbps, 20_000);
    }

    #[test]
    fn a_value_the_product_does_not_know_is_refused_and_named() {
        let nonsense = Chosen {
            quality: "ultra".to_string(),
            ..chosen()
        };
        let refusal = nonsense.understood().unwrap_err();
        assert!(refusal.contains("ultra"), "{refusal}");
    }
}
