//! What the hand does on the card: hovering, clicking, pushing a slider,
//! and what each of those asks of the session.

use super::*;

/// What the window answers when the system speaks to it.
///
/// SAFETY: called by the system on the thread that made this window,
/// with the arguments it documents.
pub(super) unsafe extern "system" fn answer(
    window: windows_sys::Win32::Foundation::HWND,
    message: u32,
    holding: windows_sys::Win32::Foundation::WPARAM,
    with: windows_sys::Win32::Foundation::LPARAM,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::UI::Controls::WM_MOUSELEAVE;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DefWindowProcW, HTCLIENT, IDC_ARROW, IDC_HAND, LoadCursorW, SetCursor, WM_LBUTTONDOWN,
        WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_SETCURSOR,
    };

    match message {
        WM_MOUSEACTIVATE => crate::session::floating::NO_ACTIVATION,
        WM_MOUSEMOVE => {
            if !HAND_INSIDE.swap(true, Ordering::Relaxed) {
                // Asked for as soon as a hand arrives: without that
                // nothing ever says it has left, and the last line
                // hovered would stay lit under a mouse that is no
                // longer there.
                let mut tracking = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: window,
                    dwHoverTime: 0,
                };
                // SAFETY: a window of ours, and the request is ours.
                unsafe { TrackMouseEvent(&mut tracking) };
            }
            if pushes(window, pointer_in(with)) {
                return 0;
            }
            hovers(window, under(pointer_in(with)));
            0
        }
        WM_MOUSELEAVE => {
            HAND_INSIDE.store(false, Ordering::Relaxed);
            hovers(window, None);
            0
        }
        WM_SETCURSOR if (with as u32 & 0xFFFF) == HTCLIENT => {
            // The line is asked of the system rather than taken from
            // the last hover: the pointer's shape is decided before the
            // move is announced, and the hand would then be one move
            // late.
            let cursor = if under_the_mouse(window).is_some() {
                IDC_HAND
            } else {
                IDC_ARROW
            };
            // SAFETY: one of the system's pointer shapes, asked
            // for by its name.
            unsafe { SetCursor(LoadCursorW(std::ptr::null_mut(), cursor)) };
            1
        }
        WM_LBUTTONDOWN => {
            let target = under(pointer_in(with));
            *PRESSED.lock().expect("menu's press") = target;
            // A slider is taken and pushed: the gesture starts here and
            // only ends on release, where only the notch it arrives at is
            // written.
            if matches!(target, Some(Target::Bar(_))) {
                pushes(window, pointer_in(with));
            }
            0
        }
        // On release, and where the press began: that is what a click
        // means, and it is what lets one slip away from a button one
        // should not have aimed at.
        WM_LBUTTONUP => {
            let pressed = PRESSED.lock().expect("menu's press").take();
            if let Some(Target::Bar(rank)) = pressed {
                released(window, rank);
                return 0;
            }
            if let Some(target) = under(pointer_in(with))
                && Some(target) == pressed
            {
                acts(target);
            }
            0
        }
        // SAFETY: the system's answer to everything not answered here.
        _ => unsafe { DefWindowProcW(window, message, holding, with) },
    }
}

/// What is under the pointer, asked of the system.
fn under_the_mouse(window: windows_sys::Win32::Foundation::HWND) -> Option<Target> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

    let mut at = POINT { x: 0, y: 0 };
    // SAFETY: a point of ours, and a window of ours it is brought into.
    let read = unsafe { GetCursorPos(&mut at) != 0 && ScreenToClient(window, &mut at) != 0 };
    if !read {
        return None;
    }
    under((at.x, at.y))
}

/// Lights up what is under the mouse, and redraws when it is no longer
/// the same thing.
fn hovers(window: windows_sys::Win32::Foundation::HWND, target: Option<Target>) {
    let mut hover = HOVER.lock().expect("menu's hover");
    if *hover == target {
        return;
    }
    *hover = target;
    drop(hover);
    repaint(window);
}

/// Does what the thing just clicked asks for.
///
/// Said before it goes off, and not only when it refuses. This menu is
/// behind the picture and its lines are few: without this line, an entry
/// that seems to do nothing cannot be told apart from a click that never
/// arrived, and the two are fixed in different places.
fn acts(target: Target) {
    let Some(app) = PROGRAM.lock().expect("menu's program").clone() else {
        return;
    };
    match (target, target.line().and_then(|rank| LINES.get(rank))) {
        (Target::Line(_), Some(Line::Entry(entry))) => {
            say_the_click(entry.label);
            let does = entry.does;
            // Closed before it goes off: what follows takes the time it
            // takes, and a card left open on top would be a tablecloth
            // laid over the picture.
            show(false);
            crate::shell::app::spawn(async move {
                let refusal = match does {
                    Does::Session(session_act) => {
                        crate::session::floating::ask(&app, session_act).await
                    }
                    Does::PutAway => crate::session::floating::hide(&app),
                };
                say_the_refusal(refusal);
            });
        }
        (Target::Line(_), Some(Line::List(list))) => {
            // The same line opens and closes: a list opened beside the
            // menu closes where it was opened, and not only by its title.
            let mut panel = PANEL.lock().expect("menu's panel");
            *panel = (*panel != Some(list.setting)).then_some(list.setting);
            drop(panel);
            redraw(&app);
        }
        (Target::Side(_, side), Some(Line::Toggle(toggle))) => {
            // Pushing a switch to the side it is already on does nothing,
            // like any switch.
            if toggle.current_side() == side {
                return;
            }
            note(&format!(
                "floating button's menu: {} set to {}",
                toggle.label, toggle.sides[side]
            ));
            // The card stays open: one looks at the picture after
            // flipping, and opening it again for the next line would
            // make two gestures for one setting.
            let act = toggle.act;
            crate::shell::app::spawn(async move {
                match crate::session::floating::ask(&app, act).await {
                    // Read again rather than assumed: it is the only
                    // way to show where things really stand.
                    Ok(()) => reread_the_toggles(&app),
                    Err(refusal) => say_the_refusal(Err(refusal)),
                }
            });
        }
        (Target::Side(_, side), Some(Line::Choice(choice))) => {
            let Some(value) = value_of(choice.setting, side) else {
                return;
            };
            // What the far machine cannot do is not a choice: offering
            // it struck through says why, letting it be clicked would
            // say the opposite.
            let refuse = SESSION_MENU
                .lock()
                .expect("menu's settings")
                .as_ref()
                .is_some_and(|menu| choice.setting.out_of_reach(menu, &value));
            if refuse {
                return;
            }
            choose(&app, choice.setting, value);
        }
        (Target::Value(rank), _) => {
            let Some(setting) = *PANEL.lock().expect("menu's panel") else {
                return;
            };
            let Some(value) = value_of(setting, rank) else {
                return;
            };
            // The list closes on the choice: staying in it after choosing
            // would suggest there is something left to do there.
            *PANEL.lock().expect("menu's panel") = None;
            // And the card with it: what is chosen in a list shows at
            // once, what one wants to look at then is the picture, and a
            // card left on top would be a tablecloth laid over it.
            show(false);
            choose(&app, setting, value);
        }
        _ => {}
    }
}

/// A setting's value at that rank.
fn value_of(setting: Setting, rank: usize) -> Option<String> {
    SESSION_MENU
        .lock()
        .expect("menu's settings")
        .as_ref()
        .and_then(|menu| setting.values(menu).get(rank).cloned())
}

/// Writes this choice, gives it to the session where it stands, and reads
/// back what the session says about it.
///
/// Read back and not assumed: choosing a size changes what "client" is
/// worth, and it is the answer that carries it.
pub(super) fn choose(app: &App, setting: Setting, value: String) {
    note(&format!(
        "floating button's menu: {} set to « {value} »",
        setting.name()
    ));
    let app = app.clone();
    crate::shell::app::spawn(async move {
        match crate::shell::settings::choose_session(app.clone(), setting.name().to_string(), value)
            .await
        {
            Ok(choice) => {
                if let Some(menu) = SESSION_MENU.lock().expect("menu's settings").as_mut() {
                    menu.now = choice;
                }
                forget_the_push();
                redraw(&app);
            }
            Err(refusal) => {
                forget_the_push();
                say_the_refusal(Err(refusal));
            }
        }
    });
}

/// Ends what the slider showed for the hand: from here its thumb stands
/// for what the session says.
///
/// Done once the answer to a choice is in, and after it has been taken
/// where it stands, or the thumb would show the old value for an instant.
/// Not while a hand holds the slider again: that push is a new one, and
/// the answer that arrives is the answer to the one before.
fn forget_the_push() {
    if matches!(*PRESSED.lock().expect("menu's press"), Some(Target::Bar(_))) {
        return;
    }
    *PUSHED.lock().expect("menu's slider") = None;
}

/// Says that a line was clicked, by the key of its label: the journal is
/// read in one language whatever the window speaks.
///
/// Said before it goes off, and not only when it refuses. This menu is
/// behind the picture and its lines are few: without this line, an entry
/// that seems to do nothing cannot be told apart from a click that never
/// arrived, and the two are fixed in different places.
fn say_the_click(label: &str) {
    note(&format!("floating button's menu: {label} clicked"));
}

/// And says a refusal, if there is one.
///
/// On the card and in the journal. On the card because that is where the
/// person who has just clicked is looking, and in the journal because the
/// card closes and a sentence read once cannot be found again.
fn say_the_refusal(refusal: Result<(), Fact>) {
    let Err(refusal) = refusal else {
        return;
    };
    note(&format!("floating button's menu: {refusal}"));
    *REFUSAL.lock().expect("menu's refusal") = Some((refusal, Instant::now()));
    if let Some(app) = PROGRAM.lock().expect("menu's program").clone() {
        redraw(&app);
    }
}

/// Pushes the slider to where the hand is, and says whether it was
/// holding one.
///
/// Nothing is written while it holds it: a slider pushed from one end to
/// the other crosses all its notches, and each would be a round trip to
/// the service for a bitrate nobody wanted.
pub(super) fn pushes(window: windows_sys::Win32::Foundation::HWND, at: (i32, i32)) -> bool {
    let Some(Target::Bar(rank)) = *PRESSED.lock().expect("menu's press") else {
        return false;
    };
    let Some(Line::Slider(slider)) = LINES.get(rank) else {
        return false;
    };
    let Some((_, how_many)) = slider.notch() else {
        return false;
    };
    let scale = scale();
    let Some((_, _, place)) = walk(scale).into_iter().find(|(other, _, _)| *other == rank) else {
        return false;
    };
    let bar = slider_bar(place, scale);
    let thumb = layout::THUMB * scale;
    // The thumb does not go from one edge to the other but from one
    // centre to the other: counted over the whole bar, the two end
    // notches could not be reached.
    let travel = (bar.right - bar.left - thumb).max(1.0);
    let part = ((at.0 as f32 - bar.left - thumb / 2.0) / travel).clamp(0.0, 1.0);
    let notch = (part * (how_many.max(1) - 1) as f32).round() as usize;
    let mut pushed = PUSHED.lock().expect("menu's slider");
    if *pushed != Some(notch) {
        *pushed = Some(notch);
        drop(pushed);
        repaint(window);
    }
    true
}

/// Lets the slider go, and writes the notch it was left at.
///
/// The thumb stays where the hand left it until the session has answered:
/// sent back to what is in force for the length of that round trip, it
/// flashed to its old place at every release.
fn released(window: windows_sys::Win32::Foundation::HWND, rank: usize) {
    let app = PROGRAM.lock().expect("menu's program").clone();
    match (app, what_the_slider_left(rank)) {
        (Some(app), Some((setting, value))) => choose(&app, setting, value),
        _ => {
            forget_the_push();
            repaint(window);
        }
    }
}

/// What the hand left the slider on, when that is not what is in force.
fn what_the_slider_left(rank: usize) -> Option<(Setting, String)> {
    let notch = (*PUSHED.lock().expect("menu's slider"))?;
    let Line::Slider(slider) = LINES.get(rank)? else {
        return None;
    };
    let value = value_of(slider.setting, notch)?;
    let in_force = SESSION_MENU
        .lock()
        .expect("menu's settings")
        .as_ref()
        .is_some_and(|menu| slider.setting.current(menu) == value);
    (!in_force).then_some((slider.setting, value))
}

#[cfg(test)]
mod tests {
    use std::sync::{MutexGuard, PoisonError};

    use super::*;
    use crate::shell::settings::SessionChoice;
    use windows_sys::Win32::UI::WindowsAndMessaging::WM_MOUSEACTIVATE;
    use zyr_proto::session::RATES_OFFERED;

    /// The card's state is the process's own, so the tests that move it
    /// take turns.
    static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

    /// A card as a session opens it: the bitrate in force at `kbps`, and no
    /// hand on the slider.
    fn a_card_at(kbps: u32) -> MutexGuard<'static, ()> {
        let turn = ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner);
        *SESSION_MENU.lock().expect("menu's settings") = Some(SessionMenu {
            sizes: Vec::new(),
            rates: RATES_OFFERED.to_vec(),
            codecs: Vec::new(),
            screens: Vec::new(),
            beyond_it: None,
            now: SessionChoice {
                asked: "client".to_string(),
                bitrate_kbps: kbps,
                codec: "auto".to_string(),
                steady: false,
                width: 0,
                height: 0,
                screen: String::new(),
            },
        });
        *PUSHED.lock().expect("menu's slider") = None;
        *PRESSED.lock().expect("menu's press") = None;
        turn
    }

    fn the_slider() -> usize {
        LINES
            .iter()
            .position(|line| matches!(line, Line::Slider(_)))
            .expect("the card has a slider")
    }

    /// Where the thumb is drawn, as the rate it stands for.
    fn the_thumb() -> u32 {
        let Some(Line::Slider(slider)) = LINES.get(the_slider()) else {
            unreachable!("the_slider names a slider");
        };
        let (notch, _) = slider.notch().expect("the card knows its rates");
        RATES_OFFERED[notch]
    }

    /// A hand pushes the slider to this rate.
    fn push_to(kbps: u32) {
        let notch = RATES_OFFERED
            .iter()
            .position(|rate| *rate == kbps)
            .expect("a rate on offer");
        *PUSHED.lock().expect("menu's slider") = Some(notch);
    }

    /// The session says this rate is in force.
    fn the_session_answers(kbps: u32) {
        SESSION_MENU
            .lock()
            .expect("menu's settings")
            .as_mut()
            .expect("a card")
            .now
            .bitrate_kbps = kbps;
    }

    #[test]
    fn a_click_on_the_card_goes_through_without_making_it_the_active_window() {
        // SAFETY: the answer to this message is given before any window is
        // looked at, so the null one is never touched.
        let answered = unsafe { answer(std::ptr::null_mut(), WM_MOUSEACTIVATE, 0, 0) };
        assert_eq!(answered, crate::session::floating::NO_ACTIVATION);
    }

    #[test]
    fn a_thumb_let_go_of_stays_where_the_hand_left_it_until_the_session_answers() {
        let _turn = a_card_at(20_000);
        push_to(35_000);
        // The hand lets go: there is a rate to write, and the thumb has not
        // gone back to the old one.
        assert_eq!(
            what_the_slider_left(the_slider()).map(|(setting, value)| (setting.name(), value)),
            Some(("bitrate", "35000".to_string()))
        );
        assert_eq!(the_thumb(), 35_000);
        // The session answers: it is where it was, and stands for the
        // session from now on.
        the_session_answers(35_000);
        forget_the_push();
        assert_eq!(the_thumb(), 35_000);
        assert!(PUSHED.lock().expect("menu's slider").is_none());
    }

    #[test]
    fn a_refused_rate_puts_the_thumb_back_where_the_session_stands() {
        let _turn = a_card_at(20_000);
        push_to(35_000);
        assert_eq!(the_thumb(), 35_000);
        forget_the_push();
        assert_eq!(the_thumb(), 20_000);
    }

    #[test]
    fn letting_go_where_the_session_stands_asks_for_nothing() {
        let _turn = a_card_at(20_000);
        push_to(20_000);
        assert!(what_the_slider_left(the_slider()).is_none());
    }

    #[test]
    fn the_answer_to_a_push_does_not_take_the_thumb_from_a_hand_that_holds_it_again() {
        let _turn = a_card_at(20_000);
        push_to(50_000);
        *PRESSED.lock().expect("menu's press") = Some(Target::Bar(the_slider()));
        // The answer to the push before.
        the_session_answers(35_000);
        forget_the_push();
        assert_eq!(the_thumb(), 50_000);
    }
}
