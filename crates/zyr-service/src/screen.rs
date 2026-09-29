//! The service's dealings with the virtual screen.
//!
//! The screen itself is grown by `zyr-screen`, which knows drivers and
//! nothing about ZyrDesk, and so is this computer's desk, noted and given
//! back around a session. This file is the other half: when the screen
//! is woken for a session and put back to sleep after it, where its
//! papers live, and what that writes into the service's log. Putting it
//! on this computer and taking it off again is installing, and belongs to
//! the program that installs the service.
//!
//! Everything here is deliberately forgiving. A computer with no virtual
//! screen is a computer that still opens sessions, still reaches other
//! computers and still shows a picture; what it loses is the ability to
//! serve a screen bigger than its own properly. That is worth saying out
//! loud at every step and worth failing not one single thing over.

use std::fmt;
use std::path::Path;

use zyr_proto::log::Log;
use zyr_proto::paths;
use zyr_proto::session::{UNKNOWN_SCREEN, WantedScreen};

/// What this module's lines are filed under.
const TAG: &str = "screen";

/// What this computer knows of its own screens when a session asks.
///
/// Read from what the session on screen last wrote down, since a service
/// cannot see a screen. « Nothing says » and « none is on » are told
/// apart on purpose: the first is a session on screen that never got to
/// look, and only the second is a computer with nothing plugged in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Own {
    /// Nothing was written down.
    Unknown,
    /// Nothing is switched on.
    NoneOn,
    /// The main screen is on and shows that size, and `stuck` says it has
    /// once refused a desktop larger than itself.
    Main { size: (u32, u32), stuck: bool },
    /// The main screen is on, as far as Windows says, and gives the
    /// engine that films it no picture at all.
    ///
    /// Not read from anything written down: a monitor switched off is off
    /// for as long as it is, and the next session may find it on. It is
    /// what a session's engine tells the service after it began.
    Silent,
}

impl fmt::Display for Own {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Own::Unknown => f.write_str(
                "nothing says whether this computer has a screen of its own switched on",
            ),
            Own::NoneOn => f.write_str("none of this computer's own screens is switched on"),
            Own::Silent => {
                f.write_str("this computer's main screen is on but gives no picture at all")
            }
            Own::Main {
                size: (wide, high),
                stuck,
            } => write!(
                f,
                "this computer's main screen shows {wide}x{high}{}",
                if *stuck {
                    " and draws nothing larger than itself"
                } else {
                    ""
                }
            ),
        }
    }
}

/// How this computer's own screens stand, as the session on screen last
/// found them under `home`.
pub fn how_the_screens_stand(home: &Path) -> Own {
    match zyr_screen::desk::showing_now(home) {
        Some(size) => Own::Main {
            size,
            stuck: zyr_screen::desk::the_main_screen_is_stuck(home),
        },
        None if zyr_screen::desk::nothing_is_switched_on(home) => Own::NoneOn,
        None => Own::Unknown,
    }
}

/// What a session calls for from this computer's screens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Call {
    /// This computer's own screen serves the session as it is.
    OwnScreen,
    /// The screen this computer grows is woken at that size, and is the
    /// only thing there is to film: Windows puts the desktop on it
    /// unasked.
    GrownAlone(WantedScreen),
    /// Woken at that size, the desktop moved onto it: this computer's own
    /// screen draws nothing larger than itself.
    GrownInstead(WantedScreen),
}

/// The screen woken for a session that asks for no size: the common one.
fn the_common_screen() -> WantedScreen {
    WantedScreen {
        wide: UNKNOWN_SCREEN.0,
        high: UNKNOWN_SCREEN.1,
        scale: 0,
    }
}

/// Works out what a session calls for, given what it asks and how this
/// computer's own screens stand.
///
/// A session that asks for a size and finds nothing known about the
/// screens is served from the grown one, as it always was. One that asks
/// for none, which is a session leaving this computer as it is, is served
/// from it only where it is known that nothing is switched on: with no
/// screen at all there is nobody to leave anything as it was for, and a
/// session with nothing to film is a session that never shows a picture.
/// A main screen that turns out to give no picture is as good as none.
pub fn what_the_session_calls_for(wanted: Option<WantedScreen>, own: Own) -> Call {
    match (wanted, own) {
        (Some(screen), Own::Unknown | Own::NoneOn) => Call::GrownAlone(screen),
        (None, Own::NoneOn) => Call::GrownAlone(the_common_screen()),
        // A screen that shows nothing serves no session, whatever size
        // that session asks for or leaves alone.
        (Some(screen), Own::Silent) => Call::GrownInstead(screen),
        (None, Own::Silent) => Call::GrownInstead(the_common_screen()),
        (Some(screen), Own::Main { size, stuck: true }) if size != (screen.wide, screen.high) => {
            Call::GrownInstead(screen)
        }
        _ => Call::OwnScreen,
    }
}

/// Wakes the virtual screen for a session that wants a picture that size.
///
/// The one moment it is awake. Between sessions it sleeps at the device,
/// which is the difference between a product that leaves a second screen
/// on somebody's desk for ever and one that borrows it while somebody is
/// actually looking.
///
/// The size is settled on the way, because waking is when the driver
/// reads the sizes written down for it and there is no second chance
/// until the next wake.
pub fn wake_for_a_session(size: (u32, u32)) -> Result<Vec<String>, String> {
    let (width, height) = size;
    let mode = zyr_screen::Mode::new(width, height, SESSION_RATE);
    zyr_screen::wake_up(zyr_screen::shipped(), &paths::virtual_screen_dir(), mode)
        .map(|done| done.steps)
        .map_err(|e| e.to_string())
}

/// Puts it back to sleep now that no session wants it.
///
/// `still_nobody` is asked again at the last moment, once the desktop has
/// stopped being rearranged, because that wait lasts about a second and a
/// session can open inside it. Asked only at the start, the screen was
/// taken away from a session that had just asked for it, and the wake
/// that followed found a device still being stopped.
pub fn sleep_after_a_session(still_nobody: &dyn Fn() -> bool) -> Result<Vec<String>, String> {
    zyr_screen::go_to_sleep(zyr_screen::shipped(), still_nobody)
        .map(|done| done.steps)
        .map_err(|e| e.to_string())
}

/// Puts it back to sleep, saying so, whoever asked.
///
/// Answers whether it is asleep now. Windows refuses to stop a display
/// device while something else is rearranging the desktop, which at the
/// end of a session is exactly what the desk going home is doing, so a
/// refusal here is ordinary and means try again in a moment, never give
/// up.
pub fn back_to_sleep(log: &Log, still_nobody: &dyn Fn() -> bool) -> bool {
    let log = &log.about(TAG);
    match sleep_after_a_session(still_nobody) {
        Ok(said) => {
            for line in said {
                log.write(&line);
            }
            // Asked of the device rather than inferred from nothing
            // having failed. Leaving it awake for a session that asked
            // for it in the meantime is a success, and answering « done »
            // to it would have the caller write down that the screen is
            // asleep when it is drawing.
            asleep()
        }
        Err(e) => {
            log.write(&format!(
                "the virtual screen would not go to sleep, trying again in a moment: {e}"
            ));
            false
        }
    }
}

/// Whether it is asleep right now, and `true` where there is none at all:
/// both mean there is no virtual screen to film.
pub fn asleep() -> bool {
    !zyr_screen::awake(zyr_screen::shipped()).is_ok_and(|awake| awake == Some(true))
}

/// Rate the virtual screen is offered at.
///
/// Sixty and not the rate the session asked for, and that is not a
/// shortcut. This screen is drawn by software into memory: nothing is
/// ever shown on it, so its rate is only the ceiling on how often the
/// engine can find something new to capture. Sixty covers every desktop,
/// and a session asking for more is served by the engine resending, which
/// is the setting that already exists for it.
const SESSION_RATE: u32 = 60;

#[cfg(test)]
mod tests {
    use super::*;

    fn asks(wide: u32, high: u32) -> WantedScreen {
        WantedScreen {
            wide,
            high,
            scale: 125,
        }
    }

    fn a_folder(what: &str) -> std::path::PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "zyr-service-screen-{}-{what}",
            zyr_proto::random::alphanumeric_string(8)
        ));
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn a_computer_with_nothing_switched_on_is_filmed_on_the_screen_it_grew() {
        assert_eq!(
            what_the_session_calls_for(Some(asks(2560, 1440)), Own::NoneOn),
            Call::GrownAlone(asks(2560, 1440))
        );
    }

    #[test]
    fn so_is_a_session_that_asks_for_no_size_at_all() {
        // Leaving a computer as it is means nothing where there is no
        // screen to leave, and the common size is what it is woken at.
        assert_eq!(
            what_the_session_calls_for(None, Own::NoneOn),
            Call::GrownAlone(WantedScreen {
                wide: 1920,
                high: 1080,
                scale: 0,
            })
        );
    }

    #[test]
    fn nothing_known_grows_a_screen_only_for_a_session_that_asked_for_a_size() {
        assert_eq!(
            what_the_session_calls_for(Some(asks(1920, 1200)), Own::Unknown),
            Call::GrownAlone(asks(1920, 1200))
        );
        // A session leaving the computer as it is never rearranges one
        // that may have a screen.
        assert_eq!(
            what_the_session_calls_for(None, Own::Unknown),
            Call::OwnScreen
        );
    }

    #[test]
    fn a_screen_that_is_on_serves_a_session_that_leaves_it_as_it_is() {
        for stuck in [false, true] {
            let own = Own::Main {
                size: (1920, 1080),
                stuck,
            };
            assert_eq!(what_the_session_calls_for(None, own), Call::OwnScreen);
        }
    }

    #[test]
    fn a_screen_that_draws_nothing_larger_than_itself_gives_way_to_the_grown_one() {
        let stuck = Own::Main {
            size: (1920, 1080),
            stuck: true,
        };
        assert_eq!(
            what_the_session_calls_for(Some(asks(2560, 1440)), stuck),
            Call::GrownInstead(asks(2560, 1440))
        );
        // The size it does draw is served by it.
        assert_eq!(
            what_the_session_calls_for(Some(asks(1920, 1080)), stuck),
            Call::OwnScreen
        );
        // And one that never refused is asked to take the size.
        let free = Own::Main {
            size: (1920, 1080),
            stuck: false,
        };
        assert_eq!(
            what_the_session_calls_for(Some(asks(2560, 1440)), free),
            Call::OwnScreen
        );
    }

    #[test]
    fn a_main_screen_that_gives_no_picture_gives_way_to_the_grown_one() {
        assert_eq!(
            what_the_session_calls_for(Some(asks(2560, 1440)), Own::Silent),
            Call::GrownInstead(asks(2560, 1440))
        );
        // Also for a session that asked for no size: the screen it films
        // is woken at the common one, unless the caller knows better.
        assert_eq!(
            what_the_session_calls_for(None, Own::Silent),
            Call::GrownInstead(WantedScreen {
                wide: 1920,
                high: 1080,
                scale: 0,
            })
        );
    }

    #[test]
    fn no_note_is_not_a_computer_with_no_screen() {
        assert_eq!(how_the_screens_stand(&a_folder("unwritten")), Own::Unknown);
    }

    /// Here, where no screen exists at all, a session on screen that
    /// looks finds none switched on: the state of a computer with
    /// everything unplugged.
    #[cfg(not(windows))]
    #[test]
    fn a_session_on_screen_that_finds_none_switched_on_says_so() {
        let home = a_folder("looked");
        zyr_screen::desk::hold_the_desk_for(&home, None);
        assert_eq!(how_the_screens_stand(&home), Own::NoneOn);
    }
}
