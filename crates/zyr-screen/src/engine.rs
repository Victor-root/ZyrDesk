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
            let device_id = text("device_id");
            (!device_id.is_empty()).then(|| Screen {
                device_id,
                friendly_name: text("friendly_name"),
                display_name: text("display_name"),
                active: item.get("info").is_some_and(|info| !info.is_null()),
            })
        })
        .collect()
}

/// The virtual screen among them, if the engine saw it at all.
pub fn the_virtual_screen(log: &str, driver: &dyn Driver) -> Option<Screen> {
    screens_in_the_log(log)
        .into_iter()
        .find(|screen| driver.is_its_screen(&screen.friendly_name))
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
