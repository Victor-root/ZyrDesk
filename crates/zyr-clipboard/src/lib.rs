//! The clipboard of a computer: what it holds, and what it is given.
//!
//! One clipboard belongs to one window station. That is the whole reason
//! this is a crate of its own rather than a few lines somewhere: the
//! service sits on a window station that carries no screen, no desktop
//! and no clipboard, so whoever reads or writes one has to be a program
//! standing on the interactive desktop. Both halves of the product need
//! that program, on the computer watching and on the computer watched,
//! and it is the same reading and the same writing on both.
//!
//! # What crosses, and in what shape
//!
//! Text is text. A picture is a PNG, and it is a PNG whichever way it was
//! found: many programs put one on the clipboard themselves, in which
//! case it is taken exactly as it lies; a screenshot is handed over as
//! several million bytes of raw pixels, and the system's own imaging
//! turns those into a PNG here rather than sending them down a session.
//!
//! Given back, a picture is put on the clipboard twice over: as the PNG
//! it came as, for the programs that ask for one, and as the bitmap
//! Windows has always carried, for every other program. Windows works
//! out the older shapes from that second one by itself.
//!
//! # The border
//!
//! This crate knows Windows' clipboard and nothing about ZyrDesk, in the
//! same way `zyr-sound` knows Windows' sound. It answers what is on the
//! clipboard and puts things on it. When that is worth doing, and towards
//! whom, is decided elsewhere.

// Everything below is Windows' clipboard. Elsewhere the crate still
// compiles and still says something true, which is that there is no such
// clipboard to reach: the product is built and tested on machines that
// are not the ones it runs on.
#[cfg_attr(windows, path = "board.rs")]
#[cfg_attr(not(windows), path = "elsewhere.rs")]
mod board;
mod packed;

use std::fmt;

use zyr_proto::clipboard::Clip;

/// Why the clipboard could not be reached.
///
/// One kind and not several, for the reason `zyr-sound` has one: every
/// one of these is a call into Windows that came back with a refusal, and
/// there is nothing a caller would do differently for one rather than
/// another. What a caller does with it is write it down.
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

/// What is on this computer's clipboard right now, when it is something
/// this product carries.
///
/// Nothing is not a fault: a clipboard that has never been used, and one
/// holding something that is neither text nor a picture, both answer
/// that way.
pub fn what_it_holds() -> Result<Option<Clip>, Trouble> {
    board::what_it_holds()
}

/// Puts that on this computer's clipboard, in place of whatever was on
/// it.
///
/// In place of and not beside: a clipboard holds one thing, and leaving
/// the old one under the new would have programs paste whichever of the
/// two they happened to prefer.
pub fn hold_this(clip: &Clip) -> Result<(), Trouble> {
    board::hold_this(clip)
}

/// How many times this computer's clipboard has changed since Windows
/// started.
///
/// The cheapest question there is about a clipboard, and the reason
/// nothing here reads one several times a second: a number that has not
/// moved is a clipboard that has not moved, and reading a picture to
/// discover the same thing would cost a few million bytes each time.
///
/// Nought means the system would not say, which is answered by reading
/// the clipboard itself rather than by believing it never changes.
pub fn times_it_changed() -> u32 {
    board::times_it_changed()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_refus_se_lit_en_toutes_lettres() {
        // Ce texte finit dans le journal : il doit se lire, pas se
        // décoder.
        let ennui = Trouble::of("le presse-papiers était pris");
        assert_eq!(ennui.to_string(), "le presse-papiers était pris");
    }

    #[cfg(not(windows))]
    #[test]
    fn ailleurs_que_sous_windows_la_reponse_est_franche() {
        // Ni un faux « il est vide » ni un faux « c'est posé » : les deux
        // mentiraient à qui décide d'envoyer quelque chose à partir de
        // la réponse.
        assert!(what_it_holds().is_err());
        assert!(hold_this(&Clip::text("bonjour")).is_err());
        assert_eq!(times_it_changed(), 0);
    }
}
