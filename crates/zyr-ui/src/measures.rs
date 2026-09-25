//! What the session is costing right now, read from what its player
//! writes.
//!
//! A player writes one line a second and nothing else; this reads it
//! when somebody is looking, which is while the floating menu is open.
//! Nothing polls in the background: numbers nobody is reading are worth
//! neither the file nor the thread.
//!
//! Every number is optional and stays optional all the way to the bar. A
//! window with no decoded frame in it has no decoding time, not a decoding
//! time of nought, and a bar that draws nought where there is no reading
//! tells the person something untrue. What is missing is left blank.

// The bar that shows them is the floating button's, which only exists
// under Windows, like the session it measures. The reading itself stays
// compiled and tested everywhere.
#![cfg_attr(not(windows), allow(dead_code))]

/// One reading, in the words the page shows.
#[derive(Default)]
pub struct Measures {
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
    /// How long the picture has been standing still, in milliseconds.
    ///
    /// The one number of the line that is not an average over the second
    /// that has just passed, and the only one that can say a session has
    /// stopped moving at the moment it stops: the rest is a window, and a
    /// window is always a second late.
    pub since_frame_ms: Option<u64>,
}

/// The last reading, or an empty one when there is no session, no engine
/// that reports, or no second gone by yet.
///
/// Empty rather than an error: a bar opening half a second into a session
/// has nothing to show and that is not a fault, it is a bar that fills in
/// a moment. An error here would put a red line in front of somebody for
/// something that rights itself.
pub fn session_measures() -> Measures {
    std::fs::read_to_string(readings())
        .ok()
        .map(|said| read(&said))
        .unwrap_or_default()
}

/// Where the readings are taken from: a file a player writes once a
/// second, replaced whole each time.
///
/// Nothing of this product writes it since its own engine replaced the
/// one that did, and this window plays no session of its own: a reading
/// is always empty until the window reads its own player's instead.
pub fn readings() -> std::path::PathBuf {
    zyr_proto::paths::data_dir().join("session-stats.txt")
}

/// Reads that line, taking what it knows and ignoring the rest.
///
/// Ignoring the rest on purpose: the engine is free to say more than this
/// window shows, and a word this half has never heard of must not cost the
/// whole reading.
fn read(said: &str) -> Measures {
    let mut measures = Measures::default();
    for pair in said.split_whitespace() {
        let Some((name, value)) = pair.split_once('=') else {
            continue;
        };
        if value.is_empty() {
            continue;
        }
        match name {
            "decode_ms" => measures.decode_ms = value.parse().ok(),
            "render_ms" => measures.render_ms = value.parse().ok(),
            "host_ms" => measures.host_ms = value.parse().ok(),
            "network_ms" => measures.network_ms = value.parse().ok(),
            "network_variance_ms" => measures.network_variance_ms = value.parse().ok(),
            "bitrate_mbps" => measures.bitrate_mbps = value.parse().ok(),
            "fps" => measures.fps = value.parse().ok(),
            "codec" => measures.codec = Some(value.to_string()),
            "width" => measures.width = value.parse().ok(),
            "height" => measures.height = value.parse().ok(),
            "dropped_network_pct" => measures.dropped_network_pct = value.parse().ok(),
            "dropped_jitter_pct" => measures.dropped_jitter_pct = value.parse().ok(),
            "since_frame_ms" => measures.since_frame_ms = value.parse().ok(),
            _ => {}
        }
    }
    measures
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_whole_line_turns_into_numbers() {
        let measures = read(
            "codec=HEVC width=1920 height=1080 fps=59.8 decode_ms=0.32 render_ms=0.29 \
             host_ms=2.30 network_ms=1 network_variance_ms=0 bitrate_mbps=0.93 \
             dropped_network_pct=0.00 dropped_jitter_pct=0.14",
        );
        assert_eq!(measures.codec.as_deref(), Some("HEVC"));
        assert_eq!(measures.width, Some(1920));
        assert_eq!(measures.decode_ms, Some(0.32));
        assert_eq!(measures.host_ms, Some(2.30));
        assert_eq!(measures.bitrate_mbps, Some(0.93));
        assert_eq!(measures.dropped_jitter_pct, Some(0.14));
    }

    #[test]
    fn the_one_number_that_is_not_a_window_is_read_too() {
        // This one arrives five times a second and the others once: it is
        // the one that lights the link badge, and reading it as an
        // unknown word would leave that badge off over a frozen picture.
        let measures = read("codec=HEVC since_frame_ms=1840 fps=0.0");
        assert_eq!(measures.since_frame_ms, Some(1840));
    }

    #[test]
    fn a_reading_that_could_not_be_taken_stays_empty() {
        // The engine writes the name with no value rather than zero: a
        // window with no decoded frame does not have a decoding time of
        // nought, it has no decoding time.
        let measures = read("codec=HEVC decode_ms= network_ms= bitrate_mbps=1.20");
        assert_eq!(measures.decode_ms, None);
        assert_eq!(measures.network_ms, None);
        assert_eq!(measures.bitrate_mbps, Some(1.20));
    }

    #[test]
    fn a_word_this_window_has_never_heard_of_costs_nothing() {
        let measures = read("decode_ms=0.40 quelque_chose_de_neuf=12 fps=60");
        assert_eq!(measures.decode_ms, Some(0.40));
        assert_eq!(measures.fps, Some(60.0));
    }

    #[test]
    fn a_line_that_is_not_one_reads_as_nothing_rather_than_as_a_fault() {
        let measures = read("n'importe quoi");
        assert!(measures.codec.is_none());
        assert!(measures.decode_ms.is_none());
    }
}
