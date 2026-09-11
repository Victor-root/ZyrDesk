//! The same three questions, on a computer that is not Windows.
//!
//! Answered with a refusal and never with a guess, for the reason
//! `zyr-sound` answers the same way: a false « it is empty » would have
//! the far computer's clipboard quietly emptied, and a false « it is
//! posted » would have somebody paste what never arrived.

use zyr_proto::clipboard::Clip;

use crate::Trouble;

fn nowhere<T>() -> Result<T, Trouble> {
    Err(Trouble::of(
        "le presse-papiers ne se lit ainsi que sous Windows",
    ))
}

pub fn what_it_holds() -> Result<Option<Clip>, Trouble> {
    nowhere()
}

pub fn hold_this(_clip: &Clip) -> Result<(), Trouble> {
    nowhere()
}

/// Nought, which is the answer this crate documents as « the system
/// would not say »: whoever asks reads the clipboard itself instead, and
/// reading it here refuses in turn.
pub fn times_it_changed() -> u32 {
    0
}
