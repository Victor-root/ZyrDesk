//! Which of the machine's screens the host engine is to capture.
//!
//! The engine names a screen by an identifier of its own making: a
//! digest of the screen's identity block and of the place it is plugged
//! into, which survives restarts and which nothing else on the machine
//! computes the same way. Working it out again here would mean copying
//! that recipe, and a copy that drifts by one byte gives an identifier
//! that names nothing, on which the engine silently falls back to the
//! main screen. The whole point of the virtual screen would be lost, and
//! lost quietly.
//!
//! So it is not worked out: it is read. The engine writes its whole list
//! of screens into its own log every time it starts, identifiers
//! included, and that list is what is read here. The engine stays the
//! one authority on what its screens are called.

use crate::driver::Driver;

/// What the engine says it announces the list with.
const THE_LINE: &str = "Currently available display devices:";

/// What the engine says when it gives up putting the screens back.
///
/// Its own words, matched on the part that carries the meaning rather
/// than on the whole sentence: the tail names the devices it saw and
/// changes with the machine.
const GAVE_UP: &str = "Failed to revert display device configuration";

/// One screen, as the engine sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Screen {
    /// What `output_name` in the engine's configuration expects.
    pub device_id: String,
    /// What the screen calls itself.
    pub friendly_name: String,
    /// What Windows numbers it as, when it is switched on. Only ever
    /// shown: it changes with the machine's plugging and is no good to
    /// name a screen by.
    pub display_name: String,
    /// Whether it is switched on at all. A screen that is off carries no
    /// size and no place, which is exactly the state a virtual screen
    /// sits in between sessions.
    pub active: bool,
    /// Whether it is the one the desktop starts at.
    ///
    /// Which is the screen a session is served from and the screen a
    /// session puts at a size, so it is the one the engine is aimed at.
    /// Read here rather than worked out, for the reason the whole of this
    /// file exists: the engine is the authority on what its screens are,
    /// and it is the only thing that can name one in a way its own
    /// configuration accepts.
    pub main: bool,
    /// How many pixels it is showing, when it is showing any.
    ///
    /// Read rather than asked of Windows, for the same reason as the rest
    /// of this: the engine is the one authority on what its screens are.
    /// It is written into the service's journal at every start of the
    /// engine, and that is what it is for. Whether the host's screen came
    /// home after a session is the question this product is asked most
    /// often, and without this it can only be answered by somebody
    /// standing in front of that computer.
    pub size: Option<(u32, u32)>,
}

/// Every screen the engine listed the last time it started.
///
/// The last list and not the first: a log is appended to across
/// restarts, and an identifier that changed at the last one is the one
/// that counts.
pub fn screens_in_the_log(log: &str) -> Vec<Screen> {
    let Some(json) = the_last_list(log) else {
        return Vec::new();
    };
    let Ok(serde_json::Value::Array(items)) = serde_json::from_str::<serde_json::Value>(json)
    else {
        return Vec::new();
    };
    items
        .into_iter()
        .filter_map(|item| {
            let text = |key: &str| {
                item.get(key)
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            };
            let info = item.get("info").filter(|info| !info.is_null());
            let side = |which: &str| {
                info?
                    .get("resolution")?
                    .get(which)
                    .and_then(serde_json::Value::as_u64)
                    .map(|side| side as u32)
            };
            let device_id = text("device_id");
            (!device_id.is_empty()).then(|| Screen {
                device_id,
                friendly_name: text("friendly_name"),
                display_name: text("display_name"),
                active: info.is_some(),
                main: info
                    .and_then(|info| info.get("primary"))
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
                size: side("width").zip(side("height")),
            })
        })
        .collect()
}

/// Whether the engine said, anywhere in there, that it could not put the
/// screens back the way it found them.
///
/// The engine changes the host's screens for the length of a session and
/// puts them back when it ends. Putting back is the half that can fail,
/// and it fails when something else has moved the screens in the
/// meantime: another remote desktop taking over, a monitor waking up, a
/// cable. The engine then keeps trying, but only ever again when a screen
/// is added or removed, which is precisely what that something else keeps
/// doing: the two take turns undoing each other for as long as both are
/// there, and the person at the host computer hears their machine click
/// through its monitors.
///
/// Nothing else in the product can tell that has happened. The engine is
/// the only thing that knows what the screens were before the session,
/// and this sentence is the only place it says it could not get back to
/// it.
pub fn could_not_put_the_screens_back(said: &str) -> bool {
    said.contains(GAVE_UP)
}

/// The virtual screen among them, if the engine saw it at all.
pub fn the_virtual_screen(log: &str, driver: &dyn Driver) -> Option<Screen> {
    screens_in_the_log(log)
        .into_iter()
        .find(|screen| driver.is_its_screen(&screen.friendly_name))
}

/// The main screen among them, under the name the engine takes orders by.
///
/// The one the engine is aimed at on any computer that has a screen of
/// its own. Left unnamed, the engine films whichever screen its graphics
/// card enumerates first, which is not the main one on every machine and
/// is not even the same one from one enumeration to the next: a host
/// whose main screen had just been resized for a session went on filming
/// the screen beside it for the rest of the evening.
pub fn the_main_screen(log: &str) -> Option<Screen> {
    screens_in_the_log(log)
        .into_iter()
        .find(|screen| screen.main)
}

/// The text of the last list the log holds.
///
/// The list is written as one record whose first line carries the
/// engine's own timestamp and level, and whose remaining lines are the
/// list itself. Rather than count lines, the brackets are matched: a
/// screen whose name holds a bracket is a screen that would otherwise
/// cut the list in half.
fn the_last_list(log: &str) -> Option<&str> {
    let announced = log.rfind(THE_LINE)? + THE_LINE.len();
    let rest = &log[announced..];
    let opens = rest.find('[')?;
    let mut depth = 0usize;
    let mut in_text = false;
    let mut escaped = false;
    for (at, letter) in rest[opens..].char_indices() {
        if in_text {
            match letter {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => in_text = false,
                _ => {}
            }
            continue;
        }
        match letter {
            '"' => in_text = true,
            '[' | '{' => depth += 1,
            ']' | '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&rest[opens..opens + at + letter.len_utf8()]);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = r#"
[2026-08-20 09:12:03]: Info: Sunshine version: v2026.516.143833
[2026-08-20 09:12:03]: Info: Currently available display devices:
[
    {
        "device_id": "{64243705-4020-5895-b923-adc862c3457e}",
        "display_name": "",
        "friendly_name": "VDD by MTT",
        "info": null
    },
    {
        "device_id": "{daeac860-f4db-5208-b1f5-cf59444fb768}",
        "display_name": "\\\\.\\DISPLAY1",
        "friendly_name": "ROG PG279Q",
        "info": {
            "primary": true,
            "resolution": { "height": 1080, "width": 1920 }
        }
    }
]
[2026-08-20 09:12:04]: Info: Configuration UI available at [https://localhost:47990]
"#;

    #[test]
    fn every_screen_the_engine_listed_is_read_back() {
        let screens = screens_in_the_log(LOG);
        assert_eq!(screens.len(), 2);
        assert_eq!(screens[0].friendly_name, "VDD by MTT");
        assert!(!screens[0].active);
        assert_eq!(screens[1].display_name, r"\\.\DISPLAY1");
        assert!(screens[1].active);
    }

    #[test]
    fn a_screen_that_is_on_says_how_many_pixels_it_shows() {
        // C'est ce nombre-là qui part dans le journal du service à chaque
        // démarrage du moteur. Sans lui, « est-ce que l'écran de l'hôte
        // est bien revenu » ne se répond qu'en allant voir la machine.
        let screens = screens_in_the_log(LOG);
        assert_eq!(screens[1].size, Some((1920, 1080)));
        // Un écran éteint n'a pas de taille, et n'en invente pas une.
        assert_eq!(screens[0].size, None);
    }

    #[test]
    fn the_engine_giving_up_on_the_screens_is_recognised() {
        // La phrase exacte du moteur, telle qu'elle sort de son journal.
        // C'est le seul endroit du produit qui sache que l'écran de
        // l'hôte n'est pas revenu à ce qu'il était.
        let gave_up = "[2026-08-25 21:16:26]: Warning: Failed to revert display device \
                       configuration (will retry once devices are added or removed). Enabling all \
                       of the available devices:\n[\n]";
        assert!(could_not_put_the_screens_back(gave_up));
        // Une session ordinaire n'en parle jamais.
        assert!(!could_not_put_the_screens_back(LOG));
        assert!(!could_not_put_the_screens_back(""));
    }

    #[test]
    fn the_main_screen_is_picked_out_by_what_the_engine_says_of_it() {
        // C'est l'écran que le moteur doit filmer sur toute machine qui a
        // un écran à elle. Sans ce nom, il filme celui que la carte
        // graphique énumère en premier, qui n'est pas le même d'une
        // énumération à l'autre.
        let main = the_main_screen(LOG).unwrap();
        assert_eq!(main.display_name, r"\\.\DISPLAY1");
        // L'écran virtuel est éteint : il n'est le principal de personne.
        assert!(!screens_in_the_log(LOG)[0].main);
    }

    #[test]
    fn the_virtual_screen_is_picked_out_by_the_name_it_publishes() {
        let found = the_virtual_screen(LOG, &crate::mtt::MTT).unwrap();
        assert_eq!(found.device_id, "{64243705-4020-5895-b923-adc862c3457e}");
    }

    #[test]
    fn a_machine_without_the_virtual_screen_yields_nothing() {
        let without = LOG.replace("VDD by MTT", "Dell U2720Q");
        assert!(the_virtual_screen(&without, &crate::mtt::MTT).is_none());
        // Et ce n'est pas parce que rien n'a été lu.
        assert_eq!(screens_in_the_log(&without).len(), 2);
    }

    #[test]
    fn the_last_list_wins_over_the_ones_before_it() {
        // Le journal du moteur s'accumule d'un démarrage à l'autre : un
        // identifiant qui a changé au dernier est celui qui compte.
        let twice = format!("{LOG}{}", LOG.replace("64243705", "11111111"));
        let found = the_virtual_screen(&twice, &crate::mtt::MTT).unwrap();
        assert!(found.device_id.starts_with("{11111111"), "{found:?}");
    }

    #[test]
    fn a_bracket_inside_a_name_does_not_cut_the_list_short() {
        let awkward = LOG.replace("ROG PG279Q", "Screen [left]");
        let screens = screens_in_the_log(&awkward);
        assert_eq!(screens.len(), 2);
        assert_eq!(screens[1].friendly_name, "Screen [left]");
    }

    #[test]
    fn a_log_that_never_listed_anything_yields_nothing_rather_than_failing() {
        assert!(screens_in_the_log("").is_empty());
        assert!(screens_in_the_log("[2026-08-20 09:12:03]: Info: started").is_empty());
        // Annoncée mais coupée en plein milieu, par exemple un journal
        // lu pendant que le moteur écrivait dedans.
        let cut = "Currently available display devices:\n[\n  { \"device_id\": \"{a}\"";
        assert!(screens_in_the_log(cut).is_empty());
    }
}
