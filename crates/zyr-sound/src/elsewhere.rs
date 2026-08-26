//! The same four questions, on a computer that is not Windows.
//!
//! Answered with a refusal and never with a guess. A false « it is
//! muted » and a false « it is playing » would both lie to whoever is
//! drawing a switch from the answer.

use crate::Trouble;

fn nowhere<T>() -> Result<T, Trouble> {
    Err(Trouble::of("le son ne se règle ainsi que sous Windows"))
}

pub fn muted(_process: u32) -> Result<bool, Trouble> {
    nowhere()
}

pub fn mute(_process: u32, _quiet: bool) -> Result<(), Trouble> {
    nowhere()
}

pub fn speakers_muted() -> Result<bool, Trouble> {
    nowhere()
}

pub fn mute_speakers(_quiet: bool) -> Result<(), Trouble> {
    nowhere()
}
