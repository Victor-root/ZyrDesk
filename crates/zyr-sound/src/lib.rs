//! The sound of a computer, as a session needs to move it.
//!
//! Two silences, and they are not the same silence.
//!
//! One is here, at the screen somebody is watching: the picture arrives
//! with its sound and the person wants the sound gone, without touching
//! anything else the computer is playing. Windows has kept a volume and
//! a mute per program since the volume mixer existed, and this is that
//! same switch asked for from a program rather than from the mixer.
//!
//! The other is over there, at the computer being controlled: it goes on
//! playing out loud into an empty room while its sound is also travelling
//! down the session. Muting its speakers is what stops that, and it works
//! because of a detail of how Windows captures a computer's own output:
//! what the engine records is the mix the audio engine hands to the
//! device, copied before the device applies its own volume and mute. The
//! speakers therefore fall silent and the stream keeps its sound. The
//! usual answer to this, the engines' own included, is a second sound
//! card that no cable leads to, published by somebody else and installed
//! behind the person's back; this needs nothing of the sort.
//!
//! # The border
//!
//! This crate knows Windows' sound and nothing about ZyrDesk, in the same
//! way `zyr-screen` knows drivers and nothing about ZyrDesk. It takes a
//! process number or nothing at all, and answers whether something is
//! muted. What is worth muting, and when, is decided elsewhere.

// Everything below is Windows' sound. Elsewhere the crate still
// compiles and still says something true, which is that there is no such
// sound to reach: the product is built and tested on machines that are
// not the ones it runs on.
#[cfg_attr(windows, path = "mixer.rs")]
#[cfg_attr(not(windows), path = "elsewhere.rs")]
mod mixer;

use std::fmt;

/// Why the sound could not be reached.
///
/// One kind and not several: everything here is a call into Windows'
/// sound that came back with a refusal, and there is nothing a caller
/// would do differently for one refusal rather than another. What a
/// caller does with it is write it down.
#[derive(Debug)]
pub struct Trouble(String);

impl Trouble {
    fn of(said: impl fmt::Display) -> Self {
        Self(said.to_string())
    }
}

impl fmt::Display for Trouble {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Trouble {}

/// Whether the sound that program is playing is muted right now.
///
/// Asked rather than remembered: the mixer is open to anybody, the
/// person may have used it, and a switch that shows what it believes
/// instead of what is true is a switch nobody trusts twice.
pub fn muted(process: u32) -> Result<bool, Trouble> {
    mixer::muted(process)
}

/// Mutes, or unmutes, everything that program plays.
///
/// It reaches the program's own sound and nothing else on the computer,
/// which is the whole point: the picture can be watched in silence while
/// the music that was already playing goes on.
pub fn mute(process: u32, quiet: bool) -> Result<(), Trouble> {
    mixer::mute(process, quiet)
}

/// Whether this computer's speakers are muted right now.
pub fn speakers_muted() -> Result<bool, Trouble> {
    mixer::speakers_muted()
}

/// Mutes, or unmutes, this computer's speakers.
///
/// The device the desktop is playing to, whichever it happens to be,
/// asked for the same way the engine asks for what it captures. Which
/// device that is depends on who is signed in, so this has to be asked
/// from the session that is on screen and not from a service sitting on
/// the side.
pub fn mute_speakers(quiet: bool) -> Result<(), Trouble> {
    mixer::mute_speakers(quiet)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_refus_se_lit_en_toutes_lettres() {
        // Ce texte finit dans le journal et parfois sous les yeux d'une
        // personne : il doit se lire, pas se décoder.
        let ennui = Trouble::of("le mélangeur n'a pas répondu");
        assert_eq!(ennui.to_string(), "le mélangeur n'a pas répondu");
    }

    #[cfg(not(windows))]
    #[test]
    fn ailleurs_que_sous_windows_la_reponse_est_franche() {
        // Ni un faux « c'est coupé » ni un faux « c'est actif » : les
        // deux mentiraient à qui affiche un interrupteur.
        assert!(muted(1).is_err());
        assert!(mute(1, true).is_err());
        assert!(speakers_muted().is_err());
        assert!(mute_speakers(true).is_err());
    }
}
