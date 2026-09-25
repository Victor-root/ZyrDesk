//! The same questions, on a computer that is not Windows.
//!
//! Answered with a refusal and never with a guess. A false « it is
//! muted » and a false « it is playing » would both lie to whoever is
//! drawing a switch from the answer.

use crate::Trouble;

fn nowhere<T>() -> Result<T, Trouble> {
    Err(Trouble::of("le son ne se règle ainsi que sous Windows"))
}

pub fn speakers_muted() -> Result<bool, Trouble> {
    nowhere()
}

pub fn mute_speakers(_quiet: bool) -> Result<(), Trouble> {
    nowhere()
}

/// Yes, since nothing here can say otherwise.
///
/// The one question of the three that answers rather than refusing, and
/// it says yes: what reads it tells a person their session is silent,
/// and a false « there is none » would say so of a machine that has
/// sound.
pub fn anything_to_play_through() -> bool {
    true
}
