//! The service's dealings with the virtual screen.
//!
//! The screen itself is grown by `zyr-screen`, which knows drivers and
//! nothing about ZyrDesk, and so is this computer's desk, noted and given
//! back around a session. This file is the other half: when the screen
//! is put in place, where its papers live, when it is woken and put back
//! to sleep, and what all of that writes into the service's log.
//!
//! Everything here is deliberately forgiving. A computer with no virtual
//! screen is a computer that still opens sessions, still reaches other
//! computers and still shows a picture; what it loses is the ability to
//! serve a screen bigger than its own properly. That is worth saying out
//! loud at every step and worth failing not one single thing over.

use zyr_proto::log::Log;
use zyr_proto::paths;

/// What this module's lines are filed under.
const TAG: &str = "screen";

/// Puts the virtual screen on this computer, if it is not on it already.
///
/// Asked for where the service is registered **and at every start of the
/// service**. Registration alone was not enough and never could be: a
/// computer whose service was registered before this existed would go on
/// without a virtual screen for ever, and nothing would ever try again or
/// even say so. That is exactly what happened, and the firewall rules
/// beside it had already learned the same lesson: they are laid at every
/// start for that very reason.
///
/// Both moments qualify. Laying a driver down needs administrator rights,
/// which the service has, and needs nobody to be watching a session,
/// which is true of a service whose door is not open yet.
///
/// Whether it is already there is asked first, and the whole of the
/// laying down hangs on that answer. Laying a driver onto a device that
/// already carries it makes Windows install it again, which takes the
/// screen away and hands it back; done at every start, that would be a
/// computer clicking through its monitors every time it is switched on.
#[cfg(windows)]
pub fn put_in_place(log: Option<&Log>) {
    let driver = zyr_screen::shipped();
    match zyr_screen::present(driver) {
        Ok(true) => {
            // Left as it is, awake or asleep. A service killed in the
            // middle of a session leaves the screen awake, and it does
            // have to go back; the supervisor does it as the door opens,
            // after the desk, which is the order everything here puts
            // them back in.
            write_down(log, vec!["virtual screen already in place".to_string()]);
            return;
        }
        Ok(false) => {}
        // Not laid down on a maybe. The answer to this question is what
        // keeps the laying down from happening twice, and without it the
        // safe thing is to leave the screen as it is and say why.
        Err(e) => {
            write_down(
                log,
                vec![format!(
                    "cannot tell whether the virtual screen is in place, leaving it alone: {e}"
                )],
            );
            return;
        }
    }
    let package = paths::virtual_screen_driver_dir();
    let home = paths::virtual_screen_dir();
    let said = match zyr_screen::install(driver, &package, &home) {
        Ok(done) => {
            let mut said = done.steps;
            said.push(if done.changed {
                "virtual screen ready: this computer can now be asked for a picture larger than \
                 its own screen"
                    .to_string()
            } else {
                "virtual screen was already in place".to_string()
            });
            said
        }
        Err(e) => vec![
            format!("virtual screen not installed: {e}"),
            "this computer will only serve pictures its own screen can draw; a session asked for \
             a larger one gets that screen blown up, which costs rate and gives no detail"
                .to_string(),
        ],
    };
    write_down(log, said);
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

/// Takes it back off, along with everything that pointed at it.
#[cfg(windows)]
pub fn take_away(log: Option<&Log>) {
    let driver = zyr_screen::shipped();
    let package = paths::virtual_screen_driver_dir();
    let home = paths::virtual_screen_dir();
    let said = match zyr_screen::uninstall(driver, &package, &home) {
        Ok(done) => done.steps,
        Err(e) => vec![format!("virtual screen not fully removed: {e}")],
    };
    write_down(log, said);
}

#[cfg(windows)]
fn write_down(log: Option<&Log>, said: Vec<String>) {
    let Some(log) = log else {
        return;
    };
    let log = log.about(TAG);
    for line in said {
        log.write(&line);
    }
}
