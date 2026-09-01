//! What the session is costing right now, read from the engine.
//!
//! The engine writes one line a second and nothing else; this reads it
//! when somebody is looking, which is while the floating menu is open.
//! Nothing polls in the background: numbers nobody is reading are worth
//! neither the file nor the thread.
//!
//! Every number is optional and stays optional all the way to the bar. A
//! window with no decoded frame in it has no decoding time, not a decoding
//! time of nought, and a bar that draws nought where there is no reading
//! tells the person something untrue. What is missing is left blank.

// La barre qui les montre est celle du bouton flottant, qui n'existe que
// sous Windows comme la session qu'elle mesure. La lecture, elle, reste
// compilée et éprouvée partout.
#![cfg_attr(not(windows), allow(dead_code))]

use serde::Serialize;

/// One reading, in the words the page shows.
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Mesures {
    /// What a frame costs this computer to decode, in milliseconds.
    pub decode_ms: Option<f64>,
    /// And to draw, the wait for the screen's own refresh included.
    pub render_ms: Option<f64>,
    /// What the far computer spent on it before sending it.
    pub host_ms: Option<f64>,
    /// The round trip between the two, and how much it wanders.
    pub network_ms: Option<f64>,
    pub network_variance_ms: Option<f64>,
    /// What the wire really carried, which is not what was asked for.
    pub bitrate_mbps: Option<f64>,
    pub fps: Option<f64>,
    /// What the pictures are coded as, and how big they are.
    pub codec: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// What never arrived, and what arrived too late to be shown.
    pub dropped_network_pct: Option<f64>,
    pub dropped_jitter_pct: Option<f64>,
}

/// The last reading, or an empty one when there is no session, no engine
/// that reports, or no second gone by yet.
///
/// Empty rather than an error: a bar opening half a second into a session
/// has nothing to show and that is not a fault, it is a bar that fills in
/// a moment. An error here would put a red line in front of somebody for
/// something that rights itself.
#[tauri::command]
pub fn session_measures() -> Mesures {
    std::fs::read_to_string(zyr_proto::paths::session_stats())
        .ok()
        .map(|said| read(&said))
        .unwrap_or_default()
}

/// Reads that line, taking what it knows and ignoring the rest.
///
/// Ignoring the rest on purpose: the engine is free to say more than this
/// window shows, and a word this half has never heard of must not cost the
/// whole reading.
fn read(said: &str) -> Mesures {
    let mut mesures = Mesures::default();
    for pair in said.split_whitespace() {
        let Some((name, value)) = pair.split_once('=') else {
            continue;
        };
        if value.is_empty() {
            continue;
        }
        match name {
            "decode_ms" => mesures.decode_ms = value.parse().ok(),
            "render_ms" => mesures.render_ms = value.parse().ok(),
            "host_ms" => mesures.host_ms = value.parse().ok(),
            "network_ms" => mesures.network_ms = value.parse().ok(),
            "network_variance_ms" => mesures.network_variance_ms = value.parse().ok(),
            "bitrate_mbps" => mesures.bitrate_mbps = value.parse().ok(),
            "fps" => mesures.fps = value.parse().ok(),
            "codec" => mesures.codec = Some(value.to_string()),
            "width" => mesures.width = value.parse().ok(),
            "height" => mesures.height = value.parse().ok(),
            "dropped_network_pct" => mesures.dropped_network_pct = value.parse().ok(),
            "dropped_jitter_pct" => mesures.dropped_jitter_pct = value.parse().ok(),
            _ => {}
        }
    }
    mesures
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_whole_line_turns_into_numbers() {
        let mesures = read(
            "codec=HEVC width=1920 height=1080 fps=59.8 decode_ms=0.32 render_ms=0.29 \
             host_ms=2.30 network_ms=1 network_variance_ms=0 bitrate_mbps=0.93 \
             dropped_network_pct=0.00 dropped_jitter_pct=0.14",
        );
        assert_eq!(mesures.codec.as_deref(), Some("HEVC"));
        assert_eq!(mesures.width, Some(1920));
        assert_eq!(mesures.decode_ms, Some(0.32));
        assert_eq!(mesures.host_ms, Some(2.30));
        assert_eq!(mesures.bitrate_mbps, Some(0.93));
        assert_eq!(mesures.dropped_jitter_pct, Some(0.14));
    }

    #[test]
    fn a_reading_that_could_not_be_taken_stays_empty() {
        // Le moteur écrit le nom sans valeur plutôt que zéro : une fenêtre
        // sans image décodée n'a pas un temps de décodage nul, elle n'a
        // pas de temps de décodage.
        let mesures = read("codec=HEVC decode_ms= network_ms= bitrate_mbps=1.20");
        assert_eq!(mesures.decode_ms, None);
        assert_eq!(mesures.network_ms, None);
        assert_eq!(mesures.bitrate_mbps, Some(1.20));
    }

    #[test]
    fn a_word_this_window_has_never_heard_of_costs_nothing() {
        let mesures = read("decode_ms=0.40 quelque_chose_de_neuf=12 fps=60");
        assert_eq!(mesures.decode_ms, Some(0.40));
        assert_eq!(mesures.fps, Some(60.0));
    }

    #[test]
    fn a_line_that_is_not_one_reads_as_nothing_rather_than_as_a_fault() {
        let mesures = read("n'importe quoi");
        assert!(mesures.codec.is_none());
        assert!(mesures.decode_ms.is_none());
    }
}
