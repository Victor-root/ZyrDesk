//! The mouse and the keyboard over the home window.
//!
//! What is under the hand is read from what the last frame laid down,
//! the last laid winning, since it is drawn on top. A click is a press
//! and a release on the same thing; the keyboard goes from one thing to
//! the next with Tab and does it with Enter or Space, and a ring shows
//! where it is, but only once a key has been used.

use std::time::Instant;

use windows_sys::Win32::Foundation::{HWND, LPARAM};

use crate::home::act::act;
use crate::home::state::{Hit, Scroller, Target};
use crate::home::window::{invalidate, program, scale};
use crate::shell::shortcuts::{Combination, Doing, Held};

/// How far one notch of the wheel scrolls, in page pixels.
const NOTCH: f32 = 72.0;

/// What is under this point: its place among the hits.
fn under(hits: &[Hit], (x, y): (f32, f32)) -> Option<usize> {
    hits.iter().rposition(|hit| {
        x >= hit.at.left && x < hit.at.right && y >= hit.at.top && y < hit.at.bottom
    })
}

/// Whether two hits are the same thing to the hand: the same target, or
/// the same bubble over a thing that answers nothing.
fn same(one: Option<&Hit>, other: Option<&Hit>) -> bool {
    match (one, other) {
        (None, None) => true,
        (Some(one), Some(other)) => one.target == other.target && one.tip == other.tip,
        _ => false,
    }
}

/// Puts the hand on what is under it, and says whether that changed.
fn hover(point: Option<(f32, f32)>) -> bool {
    let mut state = crate::home::state();
    state.pointer = point;
    let before = state.hover.and_then(|at| state.hits.get(at)).cloned();
    let now = point.and_then(|point| under(&state.hits, point));
    state.hover = now;
    let changed = !same(before.as_ref(), now.and_then(|at| state.hits.get(at)));
    if changed {
        state.hover_since = Instant::now();
    }
    changed
}

/// What is under the hand again, after a frame laid things down anew.
pub fn hover_again() {
    let point = crate::home::state().pointer;
    if hover(point) {
        invalidate();
    }
}

pub fn moves(window: HWND, (x, y): (i32, i32)) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
    };

    // Asked for at every move: without it nothing ever says the hand has
    // left, and what it was last on would stay lit.
    let mut tracking = TRACKMOUSEEVENT {
        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
        dwFlags: TME_LEAVE,
        hwndTrack: window,
        dwHoverTime: 0,
    };
    // SAFETY: a window of ours, and the structure the call asks for.
    unsafe { TrackMouseEvent(&mut tracking) };

    let point = (x as f32, y as f32);
    let dragged = {
        let mut state = crate::home::state();
        match state.held {
            Some((which, from)) => {
                let scrolled = *state.scrolled(which);
                if scrolled.travel > 0.0 {
                    let by =
                        (point.1 - from) * (scrolled.content - scrolled.room) / scrolled.travel;
                    state.scrolled(which).go(scrolled.goal + by);
                }
                state.held = Some((which, point.1));
                true
            }
            None => false,
        }
    };
    if dragged || hover(Some(point)) {
        invalidate();
    }
}

pub fn left() {
    if hover(None) {
        invalidate();
    }
}

pub fn presses(window: HWND, (x, y): (i32, i32)) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;

    // The keyboard comes back to the window, so that Escape, Enter and
    // the combinations arrive here. Before anything is held: the field
    // losing it says so to this window, which reads what it holds.
    // SAFETY: a window of ours, on its thread.
    unsafe { SetFocus(window) };
    let point = (x as f32, y as f32);
    hover(Some(point));
    let mut state = crate::home::state();
    state.focus_shown = false;
    let target = state.hovered().cloned();
    // A click beside an open menu only closes it: what is under it was
    // not aimed at.
    if let Some((open, _)) = state.menu.clone() {
        let in_the_menu = matches!(&target, Some(Target::Offer(fingerprint, _) | Target::CardMenu(fingerprint)) if *fingerprint == open);
        if !in_the_menu {
            state.menu = None;
            drop(state);
            invalidate();
            return;
        }
    }
    match target {
        Some(Target::Scrollbar(which)) => state.held = Some((which, point.1)),
        target => state.pressed = target,
    }
    drop(state);
    invalidate();
}

pub fn releases((x, y): (i32, i32)) {
    let point = (x as f32, y as f32);
    let pressed = {
        let mut state = crate::home::state();
        state.held = None;
        state.pressed.take()
    };
    invalidate();
    let Some(pressed) = pressed else {
        return;
    };
    hover(Some(point));
    if crate::home::state().hovered() != Some(&pressed) {
        return;
    }
    if let Some(app) = program() {
        act(&app, pressed);
    }
}

/// Scrolls what the wheel is over: the open dialog, or the page.
///
/// The journal's lines scroll inside their dialog, since they are what
/// is read there and the dialog is made to fit the window; unless it does
/// not fit all the same, on a very short screen, where the wheel first
/// reaches what sticks out, or the buttons at its bottom could not be
/// reached.
pub fn wheel(notches: f32, across: bool) {
    use crate::home::state::Dialog;

    let by = -notches * scale() * NOTCH;
    {
        let mut state = crate::home::state();
        let which = match &state.dialog {
            None => Scroller::Page,
            Some(Dialog::Journal { .. }) => {
                let dialog = state.dialog_scroll;
                if dialog.content > dialog.room + 1.0 && !across {
                    Scroller::Dialog
                } else {
                    Scroller::Lines
                }
            }
            Some(_) => Scroller::Dialog,
        };
        if across {
            // Sideways: a line of the journal does not wrap, and reading
            // all of it is moving along it.
            if which == Scroller::Lines {
                state.lines_across = (state.lines_across - by).max(0.0);
            }
        } else {
            let goal = state.scrolled(which).goal;
            state.scrolled(which).go(goal + by);
        }
    }
    invalidate();
}

/// What a key does, and whether it was taken.
pub fn key_down(key: u32, with: LPARAM) -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyState, VK_END, VK_ESCAPE, VK_HOME, VK_NEXT, VK_PRIOR, VK_RETURN, VK_SHIFT, VK_SPACE,
        VK_TAB,
    };

    let Some(app) = program() else {
        return false;
    };
    // Read and let go before acting on it: writing the combination
    // takes the window's memory again.
    let listening = crate::home::state().listening;
    if let Some(doing) = listening {
        return combination(&app, doing, key, with);
    }
    match key as u16 {
        VK_TAB => {
            // SAFETY: a question about this thread's keyboard.
            let backwards = unsafe { GetKeyState(i32::from(VK_SHIFT)) } < 0;
            step_focus(backwards);
            true
        }
        VK_RETURN | VK_SPACE => {
            let (focus, dialog) = {
                let state = crate::home::state();
                let focus = state.focus_shown.then(|| state.focus.clone()).flatten();
                (focus, state.dialog.is_some())
            };
            match focus {
                Some(target) => act(&app, target),
                None if dialog && key as u16 == VK_RETURN => crate::home::dialogs::confirm(&app),
                None => return false,
            }
            true
        }
        VK_ESCAPE => {
            let mut state = crate::home::state();
            if state.menu.take().is_some() {
                drop(state);
                invalidate();
            } else if state.dialog.is_some() {
                drop(state);
                crate::home::dialogs::close(&app);
            } else if state.drawer {
                state.drawer = false;
                drop(state);
                invalidate();
            } else {
                return false;
            }
            true
        }
        VK_PRIOR | VK_NEXT | VK_HOME | VK_END => {
            let mut state = crate::home::state();
            let which = if state.dialog.is_some() {
                Scroller::Dialog
            } else {
                Scroller::Page
            };
            let scrolled = *state.scrolled(which);
            let goal = match key as u16 {
                VK_PRIOR => scrolled.goal - scrolled.room * 0.9,
                VK_NEXT => scrolled.goal + scrolled.room * 0.9,
                VK_HOME => 0.0,
                _ => scrolled.content,
            };
            state.scrolled(which).go(goal);
            drop(state);
            invalidate();
            true
        }
        _ => false,
    }
}

/// Takes the keyboard to the next thing that answers it, or the one
/// before, going round.
fn step_focus(backwards: bool) {
    let mut state = crate::home::state();
    let mut reachable: Vec<Target> = Vec::new();
    for hit in &state.hits {
        if let Some(target) = hit
            .target
            .as_ref()
            .filter(|target| answers_the_keyboard(target))
            && !reachable.contains(target)
        {
            reachable.push(target.clone());
        }
    }
    if reachable.is_empty() {
        return;
    }
    let at = state
        .focus
        .as_ref()
        .and_then(|focus| reachable.iter().position(|target| target == focus));
    let next = match (at, backwards) {
        (None, false) => 0,
        (None, true) => reachable.len() - 1,
        (Some(at), false) => (at + 1) % reachable.len(),
        (Some(at), true) => (at + reachable.len() - 1) % reachable.len(),
    };
    state.focus = Some(reachable[next].clone());
    state.focus_shown = true;
    drop(state);
    invalidate();
}

/// What the keyboard can be on: not what is held and dragged, nor the
/// veil that only covers.
fn answers_the_keyboard(target: &Target) -> bool {
    !matches!(target, Target::Scrollbar(_) | Target::Fold)
}

/// What a key is worth when a shortcut is waiting for it.
///
/// The key's place and not the character on it: that is what the product
/// keeps, and what keeps a shortcut under the same finger from one
/// keyboard to another.
fn combination(app: &crate::shell::app::App, doing: Doing, key: u32, with: LPARAM) -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyState, VK_BACK, VK_CONTROL, VK_DELETE, VK_ESCAPE, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
    };

    match key as u16 {
        VK_ESCAPE => {
            crate::home::state().listening = None;
            invalidate();
            return true;
        }
        VK_BACK | VK_DELETE => {
            bind(app, doing, None);
            return true;
        }
        _ => {}
    }
    // The extended key is another key than the one in the same place in
    // the main block: refused rather than mistaken for it.
    if with & (1 << 24) != 0 {
        return true;
    }
    let scan = ((with >> 16) & 0xFF) as u16;
    let Some(place) = crate::shell::shortcuts::placed(scan) else {
        return true;
    };
    // SAFETY: questions to the system about this thread's keyboard.
    let held = unsafe {
        Held {
            ctrl: GetKeyState(i32::from(VK_CONTROL)) < 0,
            alt: GetKeyState(i32::from(VK_MENU)) < 0,
            shift: GetKeyState(i32::from(VK_SHIFT)) < 0,
            win: GetKeyState(i32::from(VK_LWIN)) < 0 || GetKeyState(i32::from(VK_RWIN)) < 0,
        }
    };
    bind(
        app,
        doing,
        Some(Combination {
            held,
            key: place.to_string(),
        }),
    );
    true
}

/// Writes or takes off a combination, and reads the shortcuts again.
fn bind(app: &crate::shell::app::App, doing: Doing, combination: Option<Combination>) {
    crate::home::state().listening = None;
    if let Err(refusal) = crate::shell::shortcuts::bind(doing, combination) {
        crate::home::state().tell_trouble(&refusal);
    }
    crate::home::seen::shortcuts_changed();
    crate::home::redraw(app);
}

#[cfg(test)]
mod tests {
    use super::*;
    use zyr_draw::Rect;

    fn hit(target: Option<Target>, at: Rect) -> Hit {
        Hit {
            target,
            at,
            tip: None,
        }
    }

    #[test]
    fn what_was_laid_last_is_what_the_hand_is_on() {
        let hits = [
            hit(Some(Target::Advanced), Rect::at(0.0, 0.0, 100.0, 100.0)),
            hit(Some(Target::Detach), Rect::at(10.0, 10.0, 20.0, 20.0)),
        ];
        assert_eq!(under(&hits, (15.0, 15.0)), Some(1));
        assert_eq!(under(&hits, (50.0, 50.0)), Some(0));
        assert_eq!(under(&hits, (150.0, 50.0)), None);
    }
}
