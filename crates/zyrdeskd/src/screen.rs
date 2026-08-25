//! The service's dealings with the virtual screen.
//!
//! The screen itself is grown by `zyr-screen`, which knows drivers and
//! nothing about ZyrDesk. This file is the other half: when the screen
//! is put in place, where its papers live, how the host engine is told
//! to capture it, and what all of that writes into the service's log.
//!
//! Everything here is deliberately forgiving. A computer with no virtual
//! screen is a computer that still opens sessions, still reaches other
//! computers and still shows a picture; what it loses is the ability to
//! serve a screen bigger than its own properly. That is worth saying out
//! loud at every step and worth failing not one single thing over.

use std::path::PathBuf;

use zyr_proto::log::Log;
use zyr_proto::paths;

/// Where the identifier the engine knows the virtual screen by is kept.
///
/// Learned from the engine, which is the only thing that computes it,
/// and written down because learning it costs the engine a restart.
const LEARNED: &str = "engine-screen.txt";

fn learned_path() -> PathBuf {
    paths::virtual_screen_dir().join(LEARNED)
}

/// Puts the virtual screen on this computer.
///
/// Called where the service is registered, which is the one moment
/// administrator rights are already in hand and the one moment nobody is
/// in a session.
#[cfg(windows)]
pub fn put_in_place(log: Option<&Log>) {
    let driver = zyr_screen::shipped();
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
    let _ = std::fs::remove_file(learned_path());
    write_down(log, said);
}

/// The identifier the host engine knows the virtual screen by, as
/// learned at some earlier start.
///
/// Absent means one of two things, and they are not worth telling apart
/// here: no virtual screen on this computer, or one that no engine has
/// listed yet. Either way the engine is started without being told which
/// screen to capture, and it captures the main one.
pub fn remembered() -> Option<String> {
    let said = std::fs::read_to_string(learned_path()).ok()?;
    let said = said.trim();
    (!said.is_empty()).then(|| said.to_string())
}

/// What reading the engine's list of screens changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Learned {
    /// The engine is aimed where it should be.
    NothingToChange,
    /// It is not, and the note has been corrected. The engine reads that
    /// note once, when it starts, so it has to start again.
    StartAgain,
}

/// Reads the engine's own list of screens and picks the virtual one out.
///
/// Two things can be wrong and both are put right here. The virtual
/// screen may be there under a name the engine was not started with,
/// which is what happens the first time this computer ever runs one; and
/// the engine may have been started aimed at a screen that is no longer
/// there, which is what happens when the driver goes. That second one
/// matters more than it looks: the engine is not merely told which
/// screen to capture but told to put every other screen out for the
/// length of a session, and it is worth being sure that screen exists.
pub fn learn_from(engine_log: &std::path::Path, started_with: Option<&str>, log: &Log) -> Learned {
    /// The engine lists its screens as it starts and answers on its own
    /// port a moment later, but the two are not the same moment and the
    /// log is written through a buffer. Read a few times rather than
    /// once, so a list that was still on its way is not taken for a
    /// computer with no screens.
    const TRIES: u32 = 5;
    const BETWEEN: std::time::Duration = std::time::Duration::from_millis(400);

    let driver = zyr_screen::shipped();
    let mut seen = Vec::new();
    let mut text = String::new();
    for attempt in 0..TRIES {
        if attempt > 0 {
            std::thread::sleep(BETWEEN);
        }
        let Ok(read) = std::fs::read_to_string(engine_log) else {
            continue;
        };
        seen = zyr_screen::engine::screens_in_the_log(&read);
        text = read;
        if !seen.is_empty() {
            break;
        }
    }
    if seen.is_empty() {
        log.write(&format!(
            "the engine listed no screen in {} after {} tries, so which screen it captures stays \
             unknown and it captures the main one",
            engine_log.display(),
            TRIES
        ));
        return Learned::NothingToChange;
    }
    log.write(&format!(
        "screens the engine sees: {}",
        seen.iter()
            .map(|screen| format!(
                "{} ({}{})",
                if screen.friendly_name.is_empty() {
                    "unnamed"
                } else {
                    &screen.friendly_name
                },
                screen.device_id,
                if screen.active { ", on" } else { ", off" }
            ))
            .collect::<Vec<_>>()
            .join(" ; ")
    ));

    let Some(ours) = zyr_screen::engine::the_virtual_screen(&text, driver) else {
        log.write(&format!(
            "no virtual screen among them: looked for one calling itself the way {} does",
            driver.name()
        ));
        let Some(gone) = started_with else {
            log.write(
                "the engine captures the main screen and does not touch it: a session asking \
                 for another size gets that screen blown up, and asking for another shape gets \
                 black bars burned into the picture. A screen belonging to whoever sits in \
                 front of it is not this product's to move",
            );
            return Learned::NothingToChange;
        };
        // The engine was started aimed at a screen that is not there,
        // which also means told to put every other screen out for a
        // screen that cannot come back. Forgotten and started over.
        if let Err(e) = std::fs::remove_file(learned_path()) {
            log.write(&format!(
                "the engine is aimed at a screen that is gone ({gone}) and the note saying so \
                 could not be removed: {e}"
            ));
            return Learned::NothingToChange;
        }
        log.write(&format!(
            "the engine was aimed at a virtual screen that is no longer there ({gone}), forgotten \
             and started over so it captures the main screen"
        ));
        return Learned::StartAgain;
    };

    if started_with == Some(ours.device_id.as_str()) {
        log.write(&format!(
            "the engine is capturing the virtual screen ({}), and puts every other screen out for \
             the length of a session",
            ours.device_id
        ));
        return Learned::NothingToChange;
    }
    if let Err(e) = write_learned(&ours.device_id) {
        log.write(&format!(
            "the virtual screen's name could not be written down: {e}"
        ));
        return Learned::NothingToChange;
    }
    log.write(&format!(
        "virtual screen found under a name the engine was not started with ({} instead of {}), \
         the engine starts over so it captures it",
        ours.device_id,
        started_with.unwrap_or("none")
    ));
    Learned::StartAgain
}

fn write_learned(device_id: &str) -> std::io::Result<()> {
    let path = learned_path();
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)?;
    }
    std::fs::write(path, device_id.as_bytes())
}

#[cfg(windows)]
fn write_down(log: Option<&Log>, said: Vec<String>) {
    let Some(log) = log else {
        return;
    };
    for line in said {
        log.write(&line);
    }
}
