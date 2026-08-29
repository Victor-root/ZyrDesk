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
            // Left as it is, and that is deliberate. A service killed in
            // the middle of a session leaves the screen awake, and it
            // does have to go back; but not here, and not now. The engine
            // has not been started yet, and an engine starting up spends
            // its first moments putting back an arrangement of screens
            // that a session it never finished had changed. Taking a
            // display device away from under it, a second before it
            // begins, is what left somebody's screens rearranged at every
            // start of the service. The supervisor does it once the
            // engine has had its say.
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

/// Where the engine writes the arrangement of screens it owes this
/// computer back.
///
/// Beside its own executable, in the folder it keeps its papers in, and
/// named by the engine and not by us.
#[cfg(windows)]
fn what_the_engine_owes_back() -> PathBuf {
    paths::host_engine_dir()
        .join("config")
        .join("display_device.state")
}

/// Throws away an arrangement the engine can never put back.
///
/// The engine saves the arrangement of screens it found before a session
/// changed it, and puts that arrangement back at the start of every one
/// of its lives until it succeeds. That is right, and this is the one
/// case where it cannot work.
///
/// Our virtual screen sleeps between sessions. An arrangement that names
/// it therefore names a screen that does not exist at the moment the
/// engine tries, so the attempt fails, and what the engine does when it
/// fails is **switch every screen it can find back on**. It then keeps
/// that arrangement and tries again at its next start, and at every one
/// after that: a screen its owner had switched off came back on every
/// single time the service started, for ever, with nothing to break the
/// circle.
///
/// The engine cannot know any of this. It has no way of telling a screen
/// that is gone from a screen that is asleep, and no reason to suspect
/// that one of them will be back. This side does know, so this is where
/// the circle is broken: an arrangement naming our screen is not a
/// promise worth keeping, it is a trap, and it goes.
///
/// Only that one. An arrangement naming nothing but real screens is a
/// real debt to a real person, and it is left exactly where it is.
#[cfg(windows)]
pub fn forget_what_cannot_be_put_back(log: &Log) {
    let Some(ours) = remembered() else {
        return;
    };
    let owed = what_the_engine_owes_back();
    let Ok(said) = std::fs::read_to_string(&owed) else {
        return;
    };
    if !said.contains(&ours) {
        return;
    }
    match std::fs::remove_file(&owed) {
        Ok(()) => log.write(&format!(
            "the engine was holding an arrangement of screens it can never put back, naming this              computer's virtual screen ({ours}) which sleeps between sessions; it has been              dropped, so it stops switching every screen back on at each start"
        )),
        Err(e) => log.write(&format!(
            "the engine holds an arrangement of screens it can never put back ({}), and it could              not be dropped: {e}",
            owed.display()
        )),
    }
}

/// Where the desk is written down before a session touches it.
///
/// Its presence is the whole of « somebody's screens are not the way
/// they left them ». Written when a session first changes anything,
/// removed once everything is back, and read at the start of the service
/// so a run that never got to finish is caught up with.
const BEFORE: &str = "desk-before.txt";

/// Where what this computer is showing is written down, for the service
/// to read.
///
/// The service cannot see a screen. Everything Windows says about the
/// arrangement of screens is answered for the window station of whoever
/// asks, and a service sits on one with no screens at all: asked from
/// there, this computer has no screens and no sizes, which is exactly
/// what it used to answer a session that asked what it was showing. So
/// the session on screen writes it down and the service reads it.
const SHOWING: &str = "showing.txt";

fn before_path() -> PathBuf {
    paths::virtual_screen_dir().join(BEFORE)
}

fn showing_path() -> PathBuf {
    paths::virtual_screen_dir().join(SHOWING)
}

/// The desk as it was before a session touched it, if one did.
pub fn noted_before() -> Vec<zyr_screen::arrangement::Seat> {
    std::fs::read_to_string(before_path())
        .map(|text| zyr_screen::arrangement::read(&text))
        .unwrap_or_default()
}

/// What this computer's main screen is showing, as the session on screen
/// last wrote it down.
pub fn showing_now() -> Option<(u32, u32)> {
    let text = std::fs::read_to_string(showing_path()).ok()?;
    zyr_screen::arrangement::read(&text)
        .into_iter()
        .find(|seat| seat.main && seat.on)
        .map(|seat| (seat.wide, seat.high))
}

/// Notes this computer's desk, puts its main screen at the size and
/// magnification a session asked for, and writes down what it ends up
/// showing.
///
/// Runs in the session that owns the screen and nowhere else, which is
/// the whole reason this is an errand rather than a function call.
///
/// Nothing here fails a session. A computer that will not take the size
/// serves the one it has and the picture is stretched at the other end,
/// which is what every session did before any of this existed.
#[cfg(windows)]
pub fn hold_the_desk_for(wanted: Option<(u32, u32, u32)>) -> Vec<String> {
    let mut said = Vec::new();
    let mut desk = zyr_screen::arrangement::as_it_stands();
    // Before anything else is done with it, and in that order: what can
    // be read is remembered, then what cannot is filled in from what was
    // remembered before. This is what keeps somebody who chose 150 % on a
    // screen Windows would have drawn at 125 % from being handed 125 %
    // back, which is the whole difference between putting a desk back and
    // putting back a desk that resembles it.
    //
    // Remembered only while this desk is still its owner's. A note means
    // a session already has it, and what the screens draw at then is that
    // session's doing: writing it down would hand somebody, at the next
    // session that cannot read a screen, the magnification a stranger
    // asked for.
    if !before_path().exists() {
        said.extend(remember_what_can_be_read(&desk));
    }
    said.extend(fill_in_what_cannot(&mut desk));
    let Some(main) = desk.iter().find(|seat| seat.main && seat.on).cloned() else {
        said.push(
            "no screen of this computer's own is switched on, so there is nothing to put at a \
             size; the session is served what the engine finds"
                .to_string(),
        );
        return said;
    };
    // Noted before anything is touched, and only once: a second session
    // that follows the first must not note a desk the first one had
    // already changed, or what is put back is the middle of a session
    // rather than somebody's desk.
    if wanted.is_some() && !before_path().exists() {
        match write_beside(BEFORE, &zyr_screen::arrangement::written(&desk)) {
            // The main screen is spelled out beside the count, because it
            // is the one the session changes and the one whose way back
            // is read out of this note. A count alone says a note was
            // written; this says what it will put back.
            Ok(()) => said.push(format!(
                "this computer's desk is written down before the session touches it ({} screens); \
                 the one it will change is {main}",
                desk.len()
            )),
            // Worth saying loudly. Everything else here can be undone by
            // hand in a minute; this is the note that says what to undo.
            Err(e) => said.push(format!(
                "this computer's desk could not be written down, so a session must not change it: \
                 {e}"
            )),
        }
    }
    if let Some((wide, high, scale)) = wanted.filter(|_| before_path().exists()) {
        if (wide, high) != (main.wide, main.high) {
            said.push(zyr_screen::arrangement::put_at(&main.adapter, wide, high));
        }
        // Only once the size is really there, and read rather than
        // assumed. A screen that cannot draw the size it was asked for
        // says so and keeps the one it has, and the magnification that
        // came with that size then belongs to nothing: a 1920x1200 laptop
        // asked for 3840x2160 at 175 % stayed at 1920x1200 and got the
        // 175 %, so its owner was handed back a desk with everything on
        // it a third too large and had to put it right by hand.
        let got = zyr_screen::arrangement::as_it_stands()
            .into_iter()
            .find(|seat| seat.adapter == main.adapter);
        if got.is_some_and(|seat| (seat.wide, seat.high) == (wide, high)) {
            said.push(zyr_screen::magnify::magnify(&main.adapter, scale));
        } else {
            said.push(format!(
                "{} cannot draw {wide}x{high}, so it keeps its own size and its own magnification",
                main.adapter
            ));
        }
    }
    // Read again rather than worked out: what was asked for and what
    // Windows did are two different things, and the far end is told the
    // second.
    let now = zyr_screen::arrangement::as_it_stands();
    if let Err(e) = write_beside(SHOWING, &zyr_screen::arrangement::written(&now)) {
        said.push(format!(
            "what this computer is showing was not written down: {e}"
        ));
    }
    said
}

/// Puts the desk back exactly as it was noted, and forgets the note.
///
/// The note is removed only once Windows has taken the arrangement back,
/// so a refusal is tried again rather than forgotten: a desk left the way
/// a session left it is the one thing this must never do quietly.
#[cfg(windows)]
pub fn give_the_desk_back() -> Vec<String> {
    let noted = noted_before();
    if noted.is_empty() {
        return vec!["no desk was written down, so there is nothing to put back".to_string()];
    }
    let (back, mut said) = zyr_screen::arrangement::put_back(&noted);
    if !back {
        said.push(
            "this computer's desk is not back yet, so what it was is kept for another try"
                .to_string(),
        );
        return said;
    }
    if let Err(e) = std::fs::remove_file(before_path()) {
        said.push(format!(
            "the desk that was written down could not be forgotten: {e}"
        ));
    }
    said
}

/// Where the last magnification known for each screen is kept.
///
/// Outlives every session and every run of the service, which is the
/// point of it. A screen holding a magnification that can no longer be
/// read has lost what it was, and Windows keeps no history: without this
/// the only answer left is the one Windows recommends, and somebody who
/// deliberately chose otherwise gets handed the default instead of their
/// own desk.
#[cfg(windows)]
const KNOWN: &str = "screen-scales.txt";

#[cfg(windows)]
fn known_path() -> PathBuf {
    paths::virtual_screen_dir().join(KNOWN)
}

/// What was last read for each screen, by the name that survives a
/// restart.
///
/// A line each, `screen percent`, and a line that will not read is
/// skipped rather than failing the rest: this is a memory, and half a
/// memory beats none.
#[cfg(windows)]
fn what_was_known() -> Vec<(String, u32)> {
    let Ok(text) = std::fs::read_to_string(known_path()) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| {
            let (screen, percent) = line.trim().split_once(' ')?;
            Some((screen.to_string(), percent.trim().parse().ok()?))
        })
        .collect()
}

/// Writes down what can be read now, keeping what was known about the
/// screens this desk says nothing about.
///
/// Says nothing at all when nothing changed, which is nearly every time:
/// this runs at every start of the engine and at the opening of every
/// session, and a line each would bury the journal.
#[cfg(windows)]
fn remember_what_can_be_read(desk: &[zyr_screen::arrangement::Seat]) -> Vec<String> {
    let mut known = what_was_known();
    let mut changed = false;
    for seat in desk
        .iter()
        .filter(|seat| seat.on && seat.scale != 0 && !seat.screen.is_empty())
    {
        match known.iter_mut().find(|(screen, _)| *screen == seat.screen) {
            Some((_, percent)) if *percent == seat.scale => {}
            Some((_, percent)) => {
                *percent = seat.scale;
                changed = true;
            }
            None => {
                known.push((seat.screen.clone(), seat.scale));
                changed = true;
            }
        }
    }
    if !changed {
        return Vec::new();
    }
    let text = known
        .iter()
        .map(|(screen, percent)| format!("{screen} {percent}"))
        .collect::<Vec<_>>()
        .join("\n");
    match write_beside(KNOWN, &text) {
        Ok(()) => Vec::new(),
        Err(e) => vec![format!(
            "what this computer's screens draw at could not be written down: {e}"
        )],
    }
}

/// Fills in the magnification of a screen that could not say, from what
/// was known of it before.
///
/// The one case this exists for: a screen left holding a step that means
/// nothing after a session cannot be read at all, so a desk noted while
/// it is in that state would carry no magnification for it and put none
/// back. What it was is not lost, it was simply not asked for at the
/// right moment, and this is the moment.
#[cfg(windows)]
fn fill_in_what_cannot(desk: &mut [zyr_screen::arrangement::Seat]) -> Vec<String> {
    let known = what_was_known();
    let mut said = Vec::new();
    for seat in desk
        .iter_mut()
        .filter(|seat| seat.on && seat.scale == 0 && !seat.screen.is_empty())
    {
        let Some((_, percent)) = known.iter().find(|(screen, _)| *screen == seat.screen) else {
            continue;
        };
        said.push(format!(
            "{} will not say how large it draws, so what it drew at last time is used: {percent} %",
            seat.adapter
        ));
        seat.scale = *percent;
    }
    said
}

/// Writes one of this folder's notes, making the folder if it is not
/// there yet.
#[cfg(windows)]
fn write_beside(name: &str, text: &str) -> std::io::Result<()> {
    let home = paths::virtual_screen_dir();
    std::fs::create_dir_all(&home)?;
    std::fs::write(home.join(name), text)
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
#[cfg(windows)]
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
#[cfg(windows)]
pub fn sleep_after_a_session(still_nobody: &dyn Fn() -> bool) -> Result<Vec<String>, String> {
    zyr_screen::go_to_sleep(zyr_screen::shipped(), still_nobody)
        .map(|done| done.steps)
        .map_err(|e| e.to_string())
}

/// Wakes the screen only long enough for the engine to name it.
///
/// The engine's name for a screen is a digest of that screen's own
/// identity, which nothing else on the machine computes the same way, and
/// the engine only says it about screens it can see. A screen that sleeps
/// between sessions is never seen, so on a computer that has never run an
/// engine with it awake the name would never be learned and the virtual
/// screen would never be captured.
///
/// So it is woken for exactly one start of the engine, once in the life
/// of a computer, and put back to sleep as soon as the name is written
/// down. Somebody sitting in front of that computer sees a second screen
/// appear and go, once.
///
/// Answers whether it was woken, which is what says it has to be put back.
#[cfg(windows)]
pub fn wake_to_be_named(log: &Log) -> bool {
    if remembered().is_some() {
        return false;
    }
    match zyr_screen::wake_up(
        zyr_screen::shipped(),
        &paths::virtual_screen_dir(),
        zyr_screen::Mode::new(1920, 1080, SESSION_RATE),
    ) {
        Ok(done) => {
            log.write(
                "the virtual screen has never been named by an engine, waking it for this one \
                 start so it can be",
            );
            for line in done.steps {
                log.write(&line);
            }
            true
        }
        Err(e) => {
            log.write(&format!(
                "the virtual screen could not be woken to be named: {e}"
            ));
            false
        }
    }
}

/// Puts it back to sleep, saying so, whoever asked.
///
/// Answers whether it is asleep now. Windows refuses to stop a display
/// device while something else is rearranging the desktop, which at the
/// end of a session is exactly what the engine is doing, so a refusal
/// here is ordinary and means try again in a moment, never give up.
#[cfg(windows)]
pub fn back_to_sleep(log: &Log, still_nobody: &dyn Fn() -> bool) -> bool {
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
/// both mean the engine has no virtual screen to see.
#[cfg(windows)]
pub fn asleep() -> bool {
    !zyr_screen::awake(zyr_screen::shipped()).is_ok_and(|awake| awake == Some(true))
}

#[cfg(not(windows))]
pub fn asleep() -> bool {
    true
}

/// Rate the virtual screen is offered at.
///
/// Sixty and not the rate the session asked for, and that is not a
/// shortcut. This screen is drawn by software into memory: nothing is
/// ever shown on it, so its rate is only the ceiling on how often the
/// engine can find something new to capture. Sixty covers every desktop,
/// and a session asking for more is served by the engine resending, which
/// is the setting that already exists for it.
#[cfg(windows)]
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
pub fn learn_from(
    engine_log: &std::path::Path,
    started_with: Option<&str>,
    asleep: bool,
    log: &Log,
) -> Learned {
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
        // Asleep is not gone, and telling them apart is the whole of this
        // branch. The screen sleeps at the device between sessions, so
        // the engine cannot see it and is not supposed to: forgetting its
        // name here would throw away, at every start, the one thing that
        // costs an engine restart to learn.
        if asleep {
            log.write(
                "the virtual screen is asleep, as it is between sessions, so the engine does not \
                 see it; its name is kept for the session that wakes it",
            );
            return Learned::NothingToChange;
        }
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
