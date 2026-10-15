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
}
