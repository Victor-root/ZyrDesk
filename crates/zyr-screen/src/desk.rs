//! This computer's desk, lent to a session and given back.
//!
//! A session may ask for a size this computer's main screen is not
//! showing, and for the magnification that goes with it. Before anything
//! is touched the desk is written down, what it ends up showing is
//! written down beside it, and when the session goes the desk is put back
//! exactly as it was noted and the note is forgotten. The notes live in
//! the folder the caller names, as plain text: they are what somebody
//! opens when a desk came back wrong.
//!
//! What moves a screen runs in the session that owns the screen and
//! nowhere else, since everything Windows says about the arrangement of
//! screens is answered for the window station of whoever asks. What only
//! reads the notes may be asked from anywhere.

use std::path::{Path, PathBuf};

use crate::arrangement::{self, Seat};

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
///
/// Written at every look, and also when no screen is switched on: that is
/// how the service learns a computer has none.
const SHOWING: &str = "showing.txt";

fn before_path(home: &Path) -> PathBuf {
    home.join(BEFORE)
}

fn showing_path(home: &Path) -> PathBuf {
    home.join(SHOWING)
}

/// The desk as it was before a session touched it, if one did.
pub fn noted_before(home: &Path) -> Vec<Seat> {
    std::fs::read_to_string(before_path(home))
        .map(|text| arrangement::read(&text))
        .unwrap_or_default()
}

/// The screens as the session on screen last wrote them down, when it
/// wrote them at all.
fn last_written(home: &Path) -> Option<Vec<Seat>> {
    std::fs::read_to_string(showing_path(home))
        .ok()
        .map(|text| arrangement::read(&text))
}

/// The screen a desk is served from: its main one, if it is switched on.
fn its_main_screen(desk: &[Seat]) -> Option<&Seat> {
    desk.iter().find(|seat| seat.main && seat.on)
}

/// What this computer's main screen is showing, as the session on screen
/// last wrote it down.
pub fn showing_now(home: &Path) -> Option<(u32, u32)> {
    let desk = last_written(home)?;
    its_main_screen(&desk).map(|seat| (seat.wide, seat.high))
}

/// Whether the session on screen last found none of this computer's own
/// screens switched on.
///
/// Not the same question as « showing nothing ». A note that was never
/// written is a session that never got to look, and says nothing about
/// what is plugged in; only a note that was written, and lists no screen
/// that is on, says this computer has none.
pub fn nothing_is_switched_on(home: &Path) -> bool {
    last_written(home).is_some_and(|desk| its_main_screen(&desk).is_none())
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
pub fn hold_the_desk_for(home: &Path, wanted: Option<(u32, u32, u32)>) -> Vec<String> {
    hold_this_desk(home, arrangement::as_it_stands(), wanted)
}

/// [`hold_the_desk_for`] on a desk already read, so that it can be tried
/// on desks the machine at hand does not have.
fn hold_this_desk(
    home: &Path,
    mut desk: Vec<Seat>,
    wanted: Option<(u32, u32, u32)>,
) -> Vec<String> {
    let mut said = Vec::new();
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
    if !before_path(home).exists() {
        said.extend(remember_what_can_be_read(home, &desk));
    }
    said.extend(fill_in_what_cannot(home, &mut desk));
    let Some(main) = its_main_screen(&desk).cloned() else {
        // Written all the same, and that is why this branch does more
        // than say so. What the service reads is the last thing written
        // here: a note left by a session that found a screen would go on
        // saying this computer has one for as long as nobody wrote
        // another, and the service, which trusts it, would never grow the
        // screen a computer with nothing switched on has to be filmed on.
        said.extend(note_what_is_showing(home, &desk));
        said.push(
            "no screen of this computer's own is switched on, so there is nothing to put at a \
             size; the session is served what the engine finds"
                .to_string(),
        );
        return said;
    };
    // Noted before anything is touched.
    if wanted.is_some() {
        match write_the_desk_down(home, &desk) {
            // The main screen is spelled out beside the count, because it
            // is the one the session changes and the one whose way back
            // is read out of this note. A count alone says a note was
            // written; this says what it will put back.
            Ok(true) => said.push(format!(
                "this computer's desk is written down before the session touches it ({} screens); \
                 the one it will change is {main}",
                desk.len()
            )),
            Ok(false) => {}
            // Worth saying loudly. Everything else here can be undone by
            // hand in a minute; this is the note that says what to undo.
            Err(e) => said.push(format!(
                "this computer's desk could not be written down, so a session must not change it: \
                 {e}"
            )),
        }
    }
    if let Some((wide, high, scale)) = wanted.filter(|_| before_path(home).exists()) {
        if (wide, high) != (main.wide, main.high) {
            said.push(arrangement::put_at(&main.adapter, wide, high));
        }
        // Only once the size is really there, and read rather than
        // assumed. A screen that cannot draw the size it was asked for
        // says so and keeps the one it has, and the magnification that
        // came with that size then belongs to nothing: a 1920x1200 laptop
        // asked for 3840x2160 at 175 % stayed at 1920x1200 and got the
        // 175 %, so its owner was handed back a desk with everything on
        // it a third too large and had to put it right by hand.
        let got = arrangement::as_it_stands()
            .into_iter()
            .find(|seat| seat.adapter == main.adapter);
        if got.is_some_and(|seat| (seat.wide, seat.high) == (wide, high)) {
            said.push(crate::magnify::magnify(&main.adapter, scale));
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
            said.extend(remember_it_is_stuck(home, &main));
        }
    }
    // Read again rather than worked out: what was asked for and what
    // Windows did are two different things, and the far end is told the
    // second.
    said.extend(note_what_is_showing(home, &arrangement::as_it_stands()));
    said
}

/// Writes the desk down before anything touches it, once, and says
/// whether it was written now.
///
/// Only once: a second session that follows the first must not note a
/// desk the first one had already changed, or what is put back is the
/// middle of a session rather than somebody's desk.
fn write_the_desk_down(home: &Path, desk: &[Seat]) -> std::io::Result<bool> {
    if before_path(home).exists() {
        return Ok(false);
    }
    write_beside(home, BEFORE, &arrangement::written(desk)).map(|()| true)
}

/// Writes down what this computer's screens are doing, for the service
/// to read, and says so if it could not.
fn note_what_is_showing(home: &Path, desk: &[Seat]) -> Vec<String> {
    match write_beside(home, SHOWING, &arrangement::written(desk)) {
        Ok(()) => Vec::new(),
        Err(e) => vec![format!(
            "what this computer is showing was not written down: {e}"
        )],
    }
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
pub fn take_the_grown_screen_for(home: &Path, wanted: (u32, u32, u32)) -> Vec<String> {
    let (wide, high, scale) = wanted;
    // Noted first if nobody did. A session that asked for a size has
    // written the desk down already; one that left this computer as it
    // was has not, and finding its main screen gives nothing is the first
    // time it has to be. Refused outright when it cannot be, rather than
    // half done: without the note there is nothing that says how to put
    // this computer back, and moving a desktop with no way back is the
    // one thing none of this may do.
    let mut desk = arrangement::as_it_stands();
    let mut said = fill_in_what_cannot(home, &mut desk);
    match write_the_desk_down(home, &desk) {
        Ok(true) => {
            said.push("this computer's desk is written down before its desktop moves".to_string())
        }
        Ok(false) => {}
        Err(e) => {
            return vec![format!(
                "this computer's desk could not be written down, so its desktop is not moved \
                 anywhere: {e}"
            )];
        }
    }
    let Some(grown) = crate::desktop::the_screen_the_driver_grew(crate::shipped()) else {
        said.push(
            "the screen this computer grows for itself is not among its screens, so the desktop \
             stays where it is"
                .to_string(),
        );
        return said;
    };
    let (moved, moving) = arrangement::put_the_desktop_alone_on(&grown, wide, high);
    said.extend(moving);
    if moved {
        said.push(crate::magnify::magnify(&grown, scale));
    }
    // Read again rather than worked out, as everywhere else: the far end
    // is told what this computer ended up showing and never what it was
    // asked for.
    said.extend(note_what_is_showing(home, &arrangement::as_it_stands()));
    said
}

/// Where the screens that have refused a desktop larger than themselves
/// are remembered.
///
/// One line per screen, by the name that survives a restart. Read when a
/// session asks for its size: a computer whose own screen cannot take it
/// serves that session from the screen it grows instead.
const STUCK: &str = "screens-stuck-at-their-size.txt";

fn stuck_path(home: &Path) -> PathBuf {
    home.join(STUCK)
}

/// Whether that screen has already refused a desktop larger than itself.
fn stuck_at_its_size(home: &Path, screen: &str) -> bool {
    std::fs::read_to_string(stuck_path(home))
        .map(|text| text.lines().any(|line| line.trim() == screen))
        .unwrap_or(false)
}

/// Whether the screen this computer serves a session from draws nothing
/// larger than itself.
///
/// Asked when a session asks for its size: a computer whose main screen
/// is stuck serves that session from the screen it grows for itself.
pub fn the_main_screen_is_stuck(home: &Path) -> bool {
    last_written(home).is_some_and(|desk| {
        its_main_screen(&desk)
            .is_some_and(|seat| !seat.screen.is_empty() && stuck_at_its_size(home, &seat.screen))
    })
}

/// Writes that down, saying so once and never again.
fn remember_it_is_stuck(home: &Path, main: &Seat) -> Vec<String> {
    if main.screen.is_empty() || stuck_at_its_size(home, &main.screen) {
        return Vec::new();
    }
    let known = std::fs::read_to_string(stuck_path(home)).unwrap_or_default();
    let text = format!("{}{}\n", known, main.screen);
    match write_beside(home, STUCK, &text) {
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
pub fn give_the_desk_back(home: &Path) -> Vec<String> {
    let noted = noted_before(home);
    if noted.is_empty() {
        return vec!["no desk was written down, so there is nothing to put back".to_string()];
    }
    let (back, mut said) = arrangement::put_back(&noted);
    if !back {
        said.push(
            "this computer's desk is not back yet, so what it was is kept for another try"
                .to_string(),
        );
        return said;
    }
    if let Err(e) = std::fs::remove_file(before_path(home)) {
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
const KNOWN: &str = "screen-scales.txt";

fn known_path(home: &Path) -> PathBuf {
    home.join(KNOWN)
}

/// What was last read for each screen, by the name that survives a
/// restart.
///
/// A line each, `screen percent`, and a line that will not read is
/// skipped rather than failing the rest: this is a memory, and half a
/// memory beats none.
fn what_was_known(home: &Path) -> Vec<(String, u32)> {
    let Ok(text) = std::fs::read_to_string(known_path(home)) else {
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
fn remember_what_can_be_read(home: &Path, desk: &[Seat]) -> Vec<String> {
    let mut known = what_was_known(home);
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
    match write_beside(home, KNOWN, &text) {
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
fn fill_in_what_cannot(home: &Path, desk: &mut [Seat]) -> Vec<String> {
    let known = what_was_known(home);
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

/// Writes one of this folder's notes whole, making the folder if it is
/// not there yet: a note caught half written would be read as another.
fn write_beside(home: &Path, name: &str, text: &str) -> std::io::Result<()> {
    zyr_proto::files::replace(&home.join(name), text)
}

/// Outside Windows there is no desk to move or put back, and saying so is
/// the whole of what can be done about it.
#[cfg(not(windows))]
fn no_desk_here() -> Vec<String> {
    vec!["this is not Windows: there is no desk here to set or put back".to_string()]
}

#[cfg(not(windows))]
pub fn take_the_grown_screen_for(_home: &Path, _wanted: (u32, u32, u32)) -> Vec<String> {
    no_desk_here()
}

#[cfg(not(windows))]
pub fn give_the_desk_back(_home: &Path) -> Vec<String> {
    no_desk_here()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_folder(what: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "zyr-screen-desk-{}-{what}",
            zyr_proto::random::alphanumeric_string(8)
        ));
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    fn a_screen(adapter: &str, on: bool, main: bool) -> Seat {
        Seat {
            adapter: adapter.to_string(),
            screen: if on {
                format!(r"MONITOR\GSM5B7F\{{4d36e96e}}\{adapter}")
            } else {
                String::new()
            },
            on,
            wide: if on { 1920 } else { 0 },
            high: if on { 1080 } else { 0 },
            refresh: if on { 60 } else { 0 },
            at: (0, 0),
            turned: 0,
            main,
            scale: if on { 100 } else { 0 },
        }
    }

    fn note(home: &Path, desk: &[Seat]) {
        write_beside(home, SHOWING, &arrangement::written(desk)).unwrap();
    }

    #[test]
    fn a_note_never_written_says_nothing_about_what_is_plugged_in() {
        let home = a_folder("unwritten");
        assert_eq!(showing_now(&home), None);
        assert!(!nothing_is_switched_on(&home));
        assert!(!the_main_screen_is_stuck(&home));
    }

    #[test]
    fn a_screen_that_is_on_is_what_the_note_says_this_computer_shows() {
        let home = a_folder("on");
        note(&home, &[a_screen(r"\\.\DISPLAY1", true, true)]);
        assert_eq!(showing_now(&home), Some((1920, 1080)));
        assert!(!nothing_is_switched_on(&home));
    }

    #[test]
    fn a_computer_with_no_screen_on_stops_saying_it_has_one() {
        let home = a_folder("unplugged");
        // What the last session found, while the screen was plugged in.
        note(&home, &[a_screen(r"\\.\DISPLAY1", true, true)]);
        assert_eq!(showing_now(&home), Some((1920, 1080)));

        // Then the screen is unplugged: its adapter is still listed, off.
        let said = hold_this_desk(
            &home,
            vec![a_screen(r"\\.\DISPLAY1", false, true)],
            Some((1920, 1080, 100)),
        );

        assert_eq!(showing_now(&home), None);
        assert!(nothing_is_switched_on(&home));
        assert!(!the_main_screen_is_stuck(&home));
        assert!(
            said.iter()
                .any(|line| line.contains("no screen of this computer's own is switched on")),
            "{said:?}"
        );
        // And nothing was noted to be given back: nothing was touched.
        assert!(noted_before(&home).is_empty());
    }

    #[test]
    fn a_computer_that_lists_no_screen_at_all_has_none_on_either() {
        let home = a_folder("empty");
        note(&home, &[a_screen(r"\\.\DISPLAY1", true, true)]);

        hold_this_desk(&home, Vec::new(), None);

        assert!(nothing_is_switched_on(&home));
        assert_eq!(showing_now(&home), None);
    }

    #[test]
    fn the_desk_is_written_down_once_and_the_first_note_is_kept() {
        let home = a_folder("once");
        let before = vec![a_screen(r"\\.\DISPLAY1", true, true)];
        let changed = vec![a_screen(r"\\.\DISPLAY2", true, true)];

        assert!(write_the_desk_down(&home, &before).unwrap());
        // A second session finds the desk already lent: what it sees is the
        // middle of the first one, never somebody's desk.
        assert!(!write_the_desk_down(&home, &changed).unwrap());

        assert_eq!(noted_before(&home), before);
    }
}
