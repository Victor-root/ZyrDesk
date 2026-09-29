//! What shape the pointer has on the desktop this program stands on.
//!
//! A desktop says what a click is about to do through the shape of the
//! pointer and through almost nothing else: an upright bar means the
//! click lands between two letters, a hand means something to follow, a
//! ring means wait. It is read from a program standing on the interactive
//! desktop, since the pointer belongs to that desktop and a service's
//! window station carries none.
//!
//! One moment answers no shape at all, and it is the one moment the
//! computer being watched draws its pointer into the picture itself: a
//! window being dragged. See `a_window_is_being_dragged`.

use zyr_proto::session::Pointer;

/// The shape the pointer has on the desktop this program stands on.
pub fn pointer_shape() -> Pointer {
    use windows_sys::Win32::UI::WindowsAndMessaging::{CURSOR_SHOWING, CURSORINFO, GetCursorInfo};

    if a_window_is_being_dragged() {
        return Pointer::Theirs;
    }
    let mut about = CURSORINFO {
        cbSize: std::mem::size_of::<CURSORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: the block is ours with its own size written in it as the
    // call requires.
    if unsafe { GetCursorInfo(&mut about) } == 0 {
        return Pointer::Arrow;
    }
    // Hidden is the ordinary pointer and not a shape of its own. A
    // machine hides it while somebody types and shows it again on the
    // first movement, and a session that answered « nothing » there would
    // blink the pointer out under a hand that had not moved.
    if about.flags & CURSOR_SHOWING == 0 {
        return Pointer::Arrow;
    }
    named_shape(about.hCursor)
}

/// Whether a window on this desktop is being dragged or resized right
/// now.
///
/// It is asked for one reason, and the answer is not a shape but the
/// absence of one. While that lasts, Windows stops letting the graphics
/// card carry the pointer and composes it with the window instead, so
/// that the two move together and neither is a frame behind the other.
/// The picture this computer sends is filmed after that composing, so
/// the pointer is already in it, and the switch that keeps the engine
/// from drawing one has nothing left to switch off. The session watching
/// would show two: its own, where the hand is, and this one, a round
/// trip behind, dragging the window.
///
/// So it is told to draw none for as long as this lasts, and what it
/// shows is the one Windows drew, moving with the window it drags.
///
/// The system says it plainly: a window being dragged or resized puts
/// the thread that owns it in a loop of its own, and that is what is
/// read here.
fn a_window_is_being_dragged() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GUI_INMOVESIZE, GUITHREADINFO, GetGUIThreadInfo,
    };

    let mut about = GUITHREADINFO {
        cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: the block is ours with its own size written in it as the
    // call requires. Naming no thread means the one in front, which is
    // the only one that can be dragging anything.
    if unsafe { GetGUIThreadInfo(0, &mut about) } == 0 {
        return false;
    }
    about.flags & GUI_INMOVESIZE != 0
}

/// Which of the shapes this computer knows that pointer is.
///
/// Compared against the system's own, which are shared: a program asking
/// for the ordinary arrow is handed the very same pointer as every other
/// program that asked, so the two can be told apart by identity alone.
/// A pointer a program drew for itself matches none of them and comes
/// back as the arrow, which is what a system falls back to as well.
fn named_shape(cursor: windows_sys::Win32::UI::WindowsAndMessaging::HCURSOR) -> Pointer {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        IDC_APPSTARTING, IDC_ARROW, IDC_CROSS, IDC_HAND, IDC_IBEAM, IDC_NO, IDC_SIZEALL,
        IDC_SIZENESW, IDC_SIZENS, IDC_SIZENWSE, IDC_SIZEWE, IDC_WAIT, LoadCursorW,
    };

    if cursor.is_null() {
        return Pointer::Arrow;
    }
    for (which, shape) in [
        (IDC_IBEAM, Pointer::Text),
        (IDC_HAND, Pointer::Hand),
        (IDC_WAIT, Pointer::Wait),
        (IDC_APPSTARTING, Pointer::WaitingArrow),
        (IDC_CROSS, Pointer::Cross),
        (IDC_SIZEWE, Pointer::SizeAcross),
        (IDC_SIZENS, Pointer::SizeDown),
        (IDC_SIZENWSE, Pointer::SizeFalling),
        (IDC_SIZENESW, Pointer::SizeRising),
        (IDC_SIZEALL, Pointer::SizeAll),
        (IDC_NO, Pointer::Refused),
        (IDC_ARROW, Pointer::Arrow),
    ] {
        // SAFETY: a shape of the system's own, asked for by the number
        // the system reserves for it. Nothing is loaded from a file and
        // nothing is ours to free: these are shared and outlive us.
        if unsafe { LoadCursorW(std::ptr::null_mut(), which) } == cursor {
            return shape;
        }
    }
    Pointer::Arrow
}
