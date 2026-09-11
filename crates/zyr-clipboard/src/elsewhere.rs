//! The same questions, on a computer that is not Windows.
//!
//! Answered with a refusal and never with a guess, for the reason
//! `zyr-sound` answers the same way: a false « it is empty » would have
//! the far computer's clipboard quietly emptied, and a false « it is
//! posted » would have somebody paste what never arrived.

use std::path::Path;

use zyr_proto::clipboard::{Clip, Listing};

use crate::{Found, Trouble};

fn nowhere<T>() -> Result<T, Trouble> {
    Err(Trouble::of(
        "le presse-papiers ne se lit ainsi que sous Windows",
    ))
}

pub fn what_it_holds() -> Result<Option<Found>, Trouble> {
    nowhere()
}

pub fn hold_this(_clip: &Clip) -> Result<Vec<String>, Trouble> {
    nowhere()
}

pub fn stand_in_for(_listed: &Listing, _folder: &Path) -> Result<(), Trouble> {
    nowhere()
}

/// There is no place to take up beside a clipboard there is none of.
pub struct Attending;

pub fn attend() -> Result<Attending, Trouble> {
    nowhere()
}

/// A plain wait, there being no clipboard here to answer for.
pub fn answer_for(how_long: std::time::Duration) {
    std::thread::sleep(how_long);
}

/// Nothing was ever stood in for, the refusal above having seen to it, so
/// nothing stands and nobody can have pasted it.
pub fn still_standing() -> bool {
    false
}

pub fn somebody_pasted() -> bool {
    false
}

pub fn let_go() {}

/// Said in the same words as the refusals above, since it is the same
/// answer: there is no clipboard here to look at.
pub fn what_is_offered() -> String {
    "le presse-papiers ne se lit ainsi que sous Windows".to_string()
}

/// Nought, which is the answer this crate documents as « the system
/// would not say »: whoever asks reads the clipboard itself instead, and
/// reading it here refuses in turn.
pub fn times_it_changed() -> u32 {
    0
}
