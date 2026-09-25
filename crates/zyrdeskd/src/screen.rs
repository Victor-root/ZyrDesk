//! The service's dealings with the virtual screen and this computer's
//! desk.
//!
//! The screen itself is grown by `zyr-screen`, which knows drivers and
//! nothing about ZyrDesk. This file is the other half: when the screen
//! is put in place, where its papers live, when it is woken and put back
//! to sleep, how this computer's desk is noted and given back around a
//! session, and what all of that writes into the service's log.
//!
//! Everything here is deliberately forgiving. A computer with no virtual
//! screen is a computer that still opens sessions, still reaches other
//! computers and still shows a picture; what it loses is the ability to
//! serve a screen bigger than its own properly. That is worth saying out
//! loud at every step and worth failing not one single thing over.

use std::path::PathBuf;

#[cfg(windows)]
use zyr_proto::log::Log;
use zyr_proto::paths;

/// What this module's lines are filed under.
#[cfg(windows)]
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
            // Written down, because it decides which screen a session is
            // served from. A screen that has once refused a desktop
            // larger than itself refuses every one after it, so a session
            // asking for more than it draws borrows the screen this
            // computer grows for itself instead.
            //
            // Read by the session being opened as well: this is written
            // before that session is answered, so the one that discovers
            // the limit is served through the grown screen too rather
            // than being the one that pays for the discovery.
            said.extend(remember_it_is_stuck(&main));
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

/// Moves this computer's desktop onto the screen it grew for itself, at
/// the size a session asked for.
///
/// The last thing tried, and only where this computer's own screens have
/// refused that size. The service has woken that screen just before this;
/// putting a desktop on it is window station work, so it happens here.
///
/// This computer's own screens are switched off while it lasts, and that
/// is the price. Left on they are a second screen of a desktop nobody can
/// see, windows land on them and vanish from the session and the pointer
/// walks off the edge of the picture: a computer with one screen has to
/// look like one from the other end. It is paid back in full when the
/// desk goes home, which is what the note taken first is for.
#[cfg(windows)]
pub fn take_the_grown_screen_for(wanted: (u32, u32, u32)) -> Vec<String> {
    let (wide, high, scale) = wanted;
    // Refused outright rather than half done: without the note there is
    // nothing that says how to put this computer back, and moving a
    // desktop with no way back is the one thing none of this may do.
    if !before_path().exists() {
        return vec![
            "this computer's desk was never written down, so its desktop is not moved anywhere"
                .to_string(),
        ];
    }
    let Some(grown) = zyr_screen::desktop::the_screen_the_driver_grew(zyr_screen::shipped()) else {
        return vec![
            "the screen this computer grows for itself is not among its screens, so the desktop \
             stays where it is"
                .to_string(),
        ];
    };
    let (moved, mut said) = zyr_screen::arrangement::put_the_desktop_alone_on(&grown, wide, high);
    if moved {
        said.push(zyr_screen::magnify::magnify(&grown, scale));
    }
    // Read again rather than worked out, as everywhere else: the far end
    // is told what this computer ended up showing and never what it was
    // asked for.
    let now = zyr_screen::arrangement::as_it_stands();
    if let Err(e) = write_beside(SHOWING, &zyr_screen::arrangement::written(&now)) {
        said.push(format!(
            "what this computer is showing was not written down: {e}"
        ));
    }
    said
}

/// Where the screens that have refused a desktop larger than themselves
/// are remembered.
///
/// One line per screen, by the name that survives a restart. Read when a
/// session asks for its size: a computer whose own screen cannot take it
/// serves that session from the screen it grows instead.
const STUCK: &str = "screens-stuck-at-their-size.txt";

fn stuck_path() -> PathBuf {
    paths::virtual_screen_dir().join(STUCK)
}

/// Whether that screen has already refused a desktop larger than itself.
fn stuck_at_its_size(screen: &str) -> bool {
    std::fs::read_to_string(stuck_path())
        .map(|text| text.lines().any(|line| line.trim() == screen))
        .unwrap_or(false)
}

/// Whether the screen this computer serves a session from draws nothing
/// larger than itself.
///
/// Asked when a session asks for its size: a computer whose main screen
/// is stuck serves that session from the screen it grows for itself.
pub fn the_main_screen_is_stuck() -> bool {
    std::fs::read_to_string(showing_path())
        .map(|text| zyr_screen::arrangement::read(&text))
        .unwrap_or_default()
        .into_iter()
        .any(|seat| {
            seat.main && seat.on && !seat.screen.is_empty() && stuck_at_its_size(&seat.screen)
        })
}

/// Writes that down, saying so once and never again.
#[cfg(windows)]
fn remember_it_is_stuck(main: &zyr_screen::arrangement::Seat) -> Vec<String> {
    if main.screen.is_empty() || stuck_at_its_size(&main.screen) {
        return Vec::new();
    }
    let known = std::fs::read_to_string(stuck_path()).unwrap_or_default();
    let text = format!("{}{}\n", known, main.screen);
    match write_beside(STUCK, &text) {
        Ok(()) => vec![format!(
            "{} draws nothing larger than itself, so a session asking for more borrows the \
             screen this computer grows instead",
            main.adapter
        )],
        Err(e) => vec![format!(
            "that {} draws nothing larger than itself could not be written down: {e}",
            main.adapter
        )],
    }
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
/// this runs at the opening of every session, and a line each would bury
/// the journal.
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

/// Puts it back to sleep, saying so, whoever asked.
///
/// Answers whether it is asleep now. Windows refuses to stop a display
/// device while something else is rearranging the desktop, which at the
/// end of a session is exactly what the desk going home is doing, so a
/// refusal here is ordinary and means try again in a moment, never give
/// up.
#[cfg(windows)]
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
