//! Keyboard and mouse, as the client sends them and the host plays them.
//!
//! Keys travel as set-1 scan codes, the layout-free numbering of the
//! physical keys, so that the host's own layout gives them their meaning
//! exactly as it would for a keyboard plugged into it.
//!
//! Both ends keep track of what is held down. A key left down at the
//! host because its release was never seen (the window lost the focus,
//! the link dropped) would repeat there forever, so each end can release
//! everything it knows to be down.

/// A mouse button. The value is what travels on the wire.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Button {
    Left = 1,
    Right = 2,
    Middle = 3,
    X1 = 4,
    X2 = 5,
}

impl Button {
    pub const ALL: [Button; 5] = [
        Button::Left,
        Button::Right,
        Button::Middle,
        Button::X1,
        Button::X2,
    ];

    fn bit(self) -> u8 {
        1 << (self as u8)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputEvent {
    /// A key, by its set-1 scan code; `extended` for the E0 prefix.
    Key {
        scancode: u8,
        extended: bool,
        down: bool,
    },
    /// Where the pointer is, from 0 to 65535 across the whole picture.
    PointerAt {
        x: u16,
        y: u16,
    },
    /// How far the pointer moved, for games that capture it.
    PointerBy {
        dx: i16,
        dy: i16,
    },
    Button {
        button: Button,
        down: bool,
    },
    /// In wheel units, 120 to a notch.
    Wheel {
        vertical: i16,
        horizontal: i16,
    },
    /// Everything held down goes up.
    ReleaseAll,
}

/// What is held down, keys and buttons.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Pressed {
    /// One bit per scan code, extended or not.
    keys: [u64; 8],
    buttons: u8,
}

impl Pressed {
    fn key_bit(scancode: u8, extended: bool) -> (usize, u64) {
        let at = usize::from(scancode) * 2 + usize::from(extended);
        (at / 64, 1 << (at % 64))
    }

    fn is_down(&self, event: &InputEvent) -> bool {
        match *event {
            InputEvent::Key {
                scancode, extended, ..
            } => {
                let (word, bit) = Self::key_bit(scancode, extended);
                self.keys[word] & bit != 0
            }
            InputEvent::Button { button, .. } => self.buttons & button.bit() != 0,
            _ => false,
        }
    }

    fn note(&mut self, event: &InputEvent) {
        match *event {
            InputEvent::Key {
                scancode,
                extended,
                down,
            } => {
                let (word, bit) = Self::key_bit(scancode, extended);
                if down {
                    self.keys[word] |= bit;
                } else {
                    self.keys[word] &= !bit;
                }
            }
            InputEvent::Button { button, down } => {
                if down {
                    self.buttons |= button.bit();
                } else {
                    self.buttons &= !button.bit();
                }
            }
            InputEvent::ReleaseAll => *self = Self::default(),
            _ => {}
        }
    }

    /// The releases for everything down, keys first, and nothing down
    /// afterwards.
    fn ups(&mut self) -> Vec<InputEvent> {
        let mut ups = Vec::new();
        for scancode in 0..=u8::MAX {
            for extended in [false, true] {
                let key = InputEvent::Key {
                    scancode,
                    extended,
                    down: false,
                };
                if self.is_down(&key) {
                    ups.push(key);
                }
            }
        }
        for button in Button::ALL {
            let up = InputEvent::Button {
                button,
                down: false,
            };
            if self.is_down(&up) {
                ups.push(up);
            }
        }
        *self = Self::default();
        ups
    }
}

/// On the host: what the client has pressed and not yet released.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Held(Pressed);

impl Held {
    pub fn new() -> Self {
        Self::default()
    }

    /// Remembers a key or button going down or up, once it was played.
    ///
    /// `ReleaseAll` is answered with [`release_all`](Self::release_all),
    /// whose releases are the ones to play.
    pub fn note(&mut self, event: &InputEvent) {
        if *event != InputEvent::ReleaseAll {
            self.0.note(event);
        }
    }

    /// The releases for everything still down, which is then forgotten.
    pub fn release_all(&mut self) -> Vec<InputEvent> {
        self.0.ups()
    }
}

/// On the client: what is sent, without a second press for a key the
/// host already holds down.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outgoing(Pressed);

impl Outgoing {
    pub fn new() -> Self {
        Self::default()
    }

    /// The event to send, if any: a key or button already down is not
    /// pressed again. Releases always go, even for what was never seen
    /// going down: a key stuck at the host costs more than a release
    /// too many.
    pub fn pass(&mut self, event: InputEvent) -> Option<InputEvent> {
        let pressed_again = matches!(
            event,
            InputEvent::Key { down: true, .. } | InputEvent::Button { down: true, .. }
        ) && self.0.is_down(&event);
        if pressed_again {
            return None;
        }
        self.0.note(&event);
        Some(event)
    }

    /// The press the keyboard repeats while a key is held: it goes, so
    /// that the host repeats it too, and it counts as the key being down.
    pub fn pass_repeat(&mut self, scancode: u8, extended: bool) -> InputEvent {
        let event = InputEvent::Key {
            scancode,
            extended,
            down: true,
        };
        self.0.note(&event);
        event
    }

    /// The releases for everything down, for when the window loses the
    /// focus.
    pub fn release_everything(&mut self) -> Vec<InputEvent> {
        self.0.ups()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(scancode: u8, extended: bool, down: bool) -> InputEvent {
        InputEvent::Key {
            scancode,
            extended,
            down,
        }
    }

    fn button(button: Button, down: bool) -> InputEvent {
        InputEvent::Button { button, down }
    }

    #[test]
    fn the_host_releases_what_is_still_down() {
        let mut held = Held::new();
        for event in [
            key(0x1d, false, true),
            key(0x1d, true, true),
            key(0x2a, false, true),
            key(0x2a, false, false),
            button(Button::Left, true),
            button(Button::X2, true),
            button(Button::X2, false),
            InputEvent::PointerAt { x: 5, y: 6 },
        ] {
            held.note(&event);
        }
        assert_eq!(
            held.release_all(),
            vec![
                key(0x1d, false, false),
                key(0x1d, true, false),
                button(Button::Left, false)
            ]
        );
        assert!(held.release_all().is_empty());
    }

    #[test]
    fn every_key_and_button_can_be_held_and_released() {
        let mut held = Held::new();
        for scancode in 0..=u8::MAX {
            held.note(&key(scancode, false, true));
            held.note(&key(scancode, true, true));
        }
        for b in Button::ALL {
            held.note(&button(b, true));
        }
        assert_eq!(held.release_all().len(), 512 + Button::ALL.len());
    }

    #[test]
    fn release_all_is_answered_not_noted_by_the_host() {
        let mut held = Held::new();
        held.note(&key(0x1e, false, true));
        held.note(&InputEvent::ReleaseAll);
        assert_eq!(held.release_all(), vec![key(0x1e, false, false)]);
    }

    #[test]
    fn a_second_press_is_not_sent_but_a_repeat_is() {
        let mut outgoing = Outgoing::new();
        assert!(outgoing.pass(key(0x1e, false, true)).is_some());
        assert_eq!(outgoing.pass(key(0x1e, false, true)), None);
        // The same code with the E0 prefix is another key.
        assert!(outgoing.pass(key(0x1e, true, true)).is_some());
        assert_eq!(outgoing.pass_repeat(0x1e, false), key(0x1e, false, true));
        assert!(outgoing.pass(button(Button::Right, true)).is_some());
        assert_eq!(outgoing.pass(button(Button::Right, true)), None);
        assert!(outgoing.pass(button(Button::Right, false)).is_some());
        assert!(outgoing.pass(button(Button::Right, true)).is_some());
    }

    #[test]
    fn releases_and_moves_always_go() {
        let mut outgoing = Outgoing::new();
        for event in [
            key(0x30, false, false),
            button(Button::Middle, false),
            InputEvent::PointerBy { dx: -3, dy: 4 },
            InputEvent::PointerBy { dx: -3, dy: 4 },
            InputEvent::Wheel {
                vertical: 120,
                horizontal: 0,
            },
        ] {
            assert_eq!(outgoing.pass(event), Some(event));
        }
    }

    #[test]
    fn losing_the_focus_releases_everything_once() {
        let mut outgoing = Outgoing::new();
        outgoing.pass(key(0x38, false, true));
        outgoing.pass_repeat(0x0f, false);
        outgoing.pass(button(Button::X1, true));
        assert_eq!(
            outgoing.release_everything(),
            vec![
                key(0x0f, false, false),
                key(0x38, false, false),
                button(Button::X1, false)
            ]
        );
        assert!(outgoing.release_everything().is_empty());
        // Down again afterwards: it goes.
        assert!(outgoing.pass(key(0x38, false, true)).is_some());
    }

    #[test]
    fn a_release_all_sent_forgets_what_was_down() {
        let mut outgoing = Outgoing::new();
        outgoing.pass(key(0x1d, false, true));
        assert_eq!(
            outgoing.pass(InputEvent::ReleaseAll),
            Some(InputEvent::ReleaseAll)
        );
        assert!(outgoing.release_everything().is_empty());
        assert!(outgoing.pass(key(0x1d, false, true)).is_some());
    }
}
