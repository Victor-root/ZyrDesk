//! Keys and pointer, played through SendInput.
//!
//! Keys go by their scan code, the extended ones with the E0 flag, as a
//! keyboard plugged into this computer would send them: the layout here
//! gives them their meaning. The pointer goes to an absolute place over
//! the desktop spanning every screen, or by a relative move for games.
//!
//! SendInput reaches only the desktop receiving input, and refuses
//! without a word when the thread is attached to another: when it does,
//! the thread follows the input desktop and tries once more. Nothing
//! said here ever names a key.

use windows::Win32::Foundation::GetLastError;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBD_EVENT_FLAGS, KEYBDINPUT,
    KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, MOUSE_EVENT_FLAGS,
    MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN,
    MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_VIRTUALDESK, MOUSEEVENTF_WHEEL, MOUSEEVENTF_XDOWN,
    MOUSEEVENTF_XUP, MOUSEINPUT, SendInput, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{XBUTTON1, XBUTTON2};
use zyr_media::input::Button;
use zyr_proto::log::Log;

use super::desktop::InputDesktop;
use crate::parts::{InjectError, Injected, Injector};

pub(super) struct SendInputInjector {
    desktop: InputDesktop,
}

impl SendInputInjector {
    /// Made on the thread that plays the input, which it attaches to the
    /// input desktop.
    pub(super) fn new(log: Log) -> Self {
        let mut desktop = InputDesktop::new();
        desktop.follow_saying(&log);
        Self { desktop }
    }
}

impl Injector for SendInputInjector {
    fn inject(&mut self, what: Injected) -> Result<(), InjectError> {
        let inputs = inputs(what);
        let inputs = &inputs[..];
        if sent(inputs) {
            return Ok(());
        }
        // SAFETY: a plain getter, right after the call that failed.
        let first = unsafe { GetLastError() }.0;
        let followed = self.desktop.follow();
        if sent(inputs) {
            return Ok(());
        }
        // SAFETY: as above.
        let second = unsafe { GetLastError() }.0;
        Err(InjectError(match followed {
            Ok(()) => format!(
                "SendInput refused (0x{first:08X}, then 0x{second:08X} on the desktop {})",
                self.desktop.name()
            ),
            Err(e) => format!("SendInput refused (0x{first:08X}), and {e}"),
        }))
    }
}

/// Whether Windows took all of `inputs`.
fn sent(inputs: &[INPUT]) -> bool {
    // SAFETY: complete INPUT structures, whose size is what is given.
    let taken = unsafe { SendInput(inputs, size_of::<INPUT>() as i32) };
    taken as usize == inputs.len()
}

/// The events SendInput takes for one input: two for a wheel turned both
/// ways at once, else one.
fn inputs(what: Injected) -> Vec<INPUT> {
    match what {
        Injected::Key {
            scancode,
            extended,
            down,
        } => {
            let mut flags = KEYEVENTF_SCANCODE;
            if extended {
                flags |= KEYEVENTF_EXTENDEDKEY;
            }
            if !down {
                flags |= KEYEVENTF_KEYUP;
            }
            vec![key(u16::from(scancode), flags)]
        }
        Injected::PointerTo { x, y } => vec![mouse(
            x,
            y,
            0,
            MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
        )],
        Injected::PointerBy { dx, dy } => {
            vec![mouse(i32::from(dx), i32::from(dy), 0, MOUSEEVENTF_MOVE)]
        }
        Injected::Button { button, down } => {
            let (flags, data) = match (button, down) {
                (Button::Left, true) => (MOUSEEVENTF_LEFTDOWN, 0),
                (Button::Left, false) => (MOUSEEVENTF_LEFTUP, 0),
                (Button::Right, true) => (MOUSEEVENTF_RIGHTDOWN, 0),
                (Button::Right, false) => (MOUSEEVENTF_RIGHTUP, 0),
                (Button::Middle, true) => (MOUSEEVENTF_MIDDLEDOWN, 0),
                (Button::Middle, false) => (MOUSEEVENTF_MIDDLEUP, 0),
                (Button::X1, true) => (MOUSEEVENTF_XDOWN, XBUTTON1),
                (Button::X1, false) => (MOUSEEVENTF_XUP, XBUTTON1),
                (Button::X2, true) => (MOUSEEVENTF_XDOWN, XBUTTON2),
                (Button::X2, false) => (MOUSEEVENTF_XUP, XBUTTON2),
            };
            vec![mouse(0, 0, u32::from(data), flags)]
        }
        Injected::Wheel {
            vertical,
            horizontal,
        } => {
            // A turn back is a negative amount, carried in the unsigned
            // field as Windows reads it back.
            let turn = |amount: i16| i32::from(amount) as u32;
            let mut turns = Vec::with_capacity(2);
            if vertical != 0 {
                turns.push(mouse(0, 0, turn(vertical), MOUSEEVENTF_WHEEL));
            }
            if horizontal != 0 {
                turns.push(mouse(0, 0, turn(horizontal), MOUSEEVENTF_HWHEEL));
            }
            turns
        }
    }
}

fn key(scancode: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: scancode,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn mouse(dx: i32, dy: i32, data: u32, flags: MOUSE_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: data,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}
