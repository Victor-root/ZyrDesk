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

use std::path::{Path, PathBuf};

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
/// which is true of a service that has not started its engine yet.
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
                "{} ({}, {})",
                if screen.friendly_name.is_empty() {
                    "unnamed"
                } else {
                    &screen.friendly_name
                },
                screen.device_id,
                showing(screen)
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
                "the engine captures the main screen, so a session asking for more than that \
                 screen can draw gets it blown up",
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

/// Follows the engine's log from where it stood, looking for the one
/// thing it says about the host's screens that nobody else can see.
///
/// The engine resizes the screens of the computer being watched for the
/// length of a session and puts them back when it ends. Putting them back
/// is what the person sitting at that computer cares about, and it is
/// also the half that can fail. When it does, the engine says so once and
/// then goes quiet, retrying on its own terms for as long as it lives.
/// Reading its log is the only way to hear it.
///
/// Only what the engine has written since the last look is read: the log
/// grows all session long, and reading it whole every few seconds would
/// cost the machine something for nothing. The offset stops at the last
/// complete line, so a sentence caught half-written is read whole at the
/// next look rather than split in two and recognised as neither.
pub struct Watching {
    log: PathBuf,
    read_up_to: u64,
}

impl Watching {
    /// Starts watching from the end of what is already written.
    ///
    /// From the end and not from the top: the log carries every earlier
    /// run of the engine, and a complaint from one of those is a screen
    /// somebody has long since put back by hand. A log that is not there
    /// yet has no past to skip, and starts at nought.
    pub fn from_here(log: &Path) -> Self {
        Self {
            log: log.to_path_buf(),
            read_up_to: std::fs::metadata(log).map(|it| it.len()).unwrap_or(0),
        }
    }

    /// Whether the engine has given up on the screens since the last look.
    pub fn gave_up_on_the_screens(&mut self) -> bool {
        use std::io::{Read, Seek, SeekFrom};

        let Ok(mut file) = std::fs::File::open(&self.log) else {
            return false;
        };
        // The journal writer cuts this file back from its top once it has
        // grown past reason. What was written before the cut is gone, and
        // carrying on from an offset into it would read the middle of a
        // line for ever. It cannot happen while the engine holds the file,
        // which is the whole of this watch, and the day it does the watch
        // takes its place again at the new end rather than losing itself.
        let length = file.metadata().map(|it| it.len()).unwrap_or(0);
        if length < self.read_up_to {
            self.read_up_to = length;
            return false;
        }
        if file.seek(SeekFrom::Start(self.read_up_to)).is_err() {
            return false;
        }
        let mut written = Vec::new();
        if file.read_to_end(&mut written).is_err() {
            return false;
        }
        let Some(complete) = written.iter().rposition(|byte| *byte == b'\n') else {
            return false;
        };
        self.read_up_to += complete as u64 + 1;
        let said = String::from_utf8_lossy(&written[..=complete]);
        zyr_screen::engine::could_not_put_the_screens_back(&said)
    }
}

/// What a screen is showing, for the journal.
///
/// The size and not merely on or off. Whether the host's screen came home
/// after a session is what this product is asked about most, the engine
/// writes its list at every one of its starts, and this turns that list
/// into the answer instead of half of it.
fn showing(screen: &zyr_screen::Screen) -> String {
    match screen.size {
        Some((width, height)) if screen.active => format!("on at {width}x{height}"),
        _ if screen.active => "on, size unsaid".to_string(),
        _ => "off".to_string(),
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const GAVE_UP: &str = "[2026-08-25 21:16:26]: Warning: Failed to revert display device \
                           configuration (will retry once devices are added or removed).\n";

    fn a_log(what: &str) -> (PathBuf, PathBuf) {
        let folder = std::env::temp_dir().join(format!(
            "zyrdeskd-screen-{}-{what}",
            zyr_proto::random::alphanumeric_string(8)
        ));
        std::fs::create_dir_all(&folder).unwrap();
        (folder.join("engine.log"), folder)
    }

    fn add(log: &Path, said: &str) {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log)
            .unwrap();
        file.write_all(said.as_bytes()).unwrap();
    }

    #[test]
    fn what_the_engine_said_before_the_watch_began_is_not_answered_for() {
        // Le journal du moteur porte tous ses démarrages précédents. Une
        // plainte d'il y a trois semaines, c'est un écran que quelqu'un a
        // remis à la main depuis longtemps.
        let (log, folder) = a_log("avant");
        add(&log, GAVE_UP);

        let mut watching = Watching::from_here(&log);
        assert!(!watching.gave_up_on_the_screens());

        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn what_the_engine_says_afterwards_is_heard_once() {
        let (log, folder) = a_log("apres");
        add(&log, "[2026-08-25 21:16:23]: Info: Session ended\n");

        let mut watching = Watching::from_here(&log);
        add(&log, GAVE_UP);
        assert!(watching.gave_up_on_the_screens());
        // Et pas une deuxième fois : ce qui a été lu est derrière nous.
        assert!(!watching.gave_up_on_the_screens());

        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_line_caught_half_written_is_read_whole_at_the_next_look() {
        // Le moteur écrit dans ce fichier pendant qu'on le lit. Une
        // phrase coupée en deux et lue en deux morceaux ne ressemble plus
        // à rien, et c'est justement celle qu'il ne faut pas manquer.
        let (log, folder) = a_log("coupe");
        let mut watching = Watching::from_here(&log);

        let (start, rest) = GAVE_UP.split_at(40);
        add(&log, start);
        assert!(!watching.gave_up_on_the_screens());
        add(&log, rest);
        assert!(watching.gave_up_on_the_screens());

        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_log_cut_back_from_its_top_does_not_leave_the_watch_lost() {
        // Le journal est rogné quand il devient trop gros. Repartir d'une
        // position qui n'existe plus, c'est lire le milieu d'une ligne
        // pour toujours.
        let (log, folder) = a_log("rogne");
        add(
            &log,
            "[2026-08-25 21:16:23]: Info: Session ended\n"
                .repeat(20)
                .as_str(),
        );
        let mut watching = Watching::from_here(&log);

        std::fs::write(&log, b"[2026-08-25 21:20:00]: Info: fresh\n").unwrap();
        assert!(!watching.gave_up_on_the_screens());
        add(&log, GAVE_UP);
        assert!(watching.gave_up_on_the_screens());

        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_log_that_is_not_there_says_nothing_rather_than_failing() {
        let (log, folder) = a_log("absent");
        let mut watching = Watching::from_here(&log);
        assert!(!watching.gave_up_on_the_screens());
        let _ = std::fs::remove_dir_all(&folder);
    }
}
