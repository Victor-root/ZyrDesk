//! What someone reaching this computer may do on it.
//!
//! Its owner, and anyone it was told to trust, may do everything. A
//! contact may do what the owner chose for this one computer: watch and
//! hear it always, use its keyboard and its mouse if allowed, and come
//! in without the owner's accord at the machine if allowed. Written once
//! here because the window sets them, the service keeps them, and the
//! tunnel holds back what they leave out.

/// What a visitor may do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rights {
    pub keyboard: bool,
    pub mouse: bool,
    /// Coming in without someone at this computer saying yes first.
    pub unattended: bool,
}

impl Rights {
    /// The owner's, or a computer trusted on purpose.
    pub const ALL: Rights = Rights {
        keyboard: true,
        mouse: true,
        unattended: true,
    };

    /// A contact given a computer and nothing more: watching it, once
    /// someone at it said yes.
    pub const WATCHING: Rights = Rights {
        keyboard: false,
        mouse: false,
        unattended: false,
    };

    /// The same, with that one right turned the other way.
    pub fn toggled(self, right: Right) -> Rights {
        match right {
            Right::Keyboard => Rights {
                keyboard: !self.keyboard,
                ..self
            },
            Right::Mouse => Rights {
                mouse: !self.mouse,
                ..self
            },
            Right::Unattended => Rights {
                unattended: !self.unattended,
                ..self
            },
        }
    }
}

/// One of the things a visitor may be allowed, on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Right {
    Keyboard,
    Mouse,
    Unattended,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turning_one_right_leaves_the_others_as_they_were() {
        let given = Rights::WATCHING.toggled(Right::Mouse);
        assert_eq!(
            given,
            Rights {
                keyboard: false,
                mouse: true,
                unattended: false
            }
        );
        let given = given.toggled(Right::Unattended).toggled(Right::Mouse);
        assert_eq!(
            given,
            Rights {
                keyboard: false,
                mouse: false,
                unattended: true
            }
        );
        assert!(!Rights::ALL.toggled(Right::Keyboard).keyboard);
    }
}
