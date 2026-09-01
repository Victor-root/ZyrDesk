//! The one thing left to try on the button's edge: build that window
//! five different ways and look at each of them.
//!
//! What is known and measured: a pixel of that window the page does not
//! paint is not empty, it shows frosted glass that lightens whatever is
//! behind it. It came out at 203,209,216 over black and 215,228,241 over
//! a brown desktop, and it was made permanent once, on purpose, to prove
//! it. What is not known is **which** of the three things this product
//! does to that window turns the glass on: declaring it layered, asking
//! the toolkit for transparency, or giving it a ground to erase itself
//! with.
//!
//! Reading cannot settle it, and it has been tried. The toolkit's
//! transparency is a blur-behind over an empty region, documented to blur
//! nothing; the layered attribute is documented to set one opacity for
//! the whole window; the ground is painted by a message the web view
//! covers. All three read as harmless, and either one of them is not, or
//! none of them is and the glass comes from somewhere else. So they are
//! switched off one at a time and looked at.
//!
//! Every trial but the first cuts the window to a plain square four logos
//! wide instead of to the drawing, and that is the whole trick. The
//! defect is two pixels wide along the edge of a drawing, which is why
//! eight explanations have been argued from photographs of it and eight
//! were wrong. A square turns it into two hundred pixels of nothing but
//! unpainted window, and nobody has to squint at anything.
//!
//! One trial per session, in order, and the journal says which one is
//! running. This is an instrument, not a feature: it goes the day it has
//! answered, like the camera before it.

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::journal::note;

/// How the button's window is built and cut for one trial.
#[derive(Clone, Copy)]
pub struct Trial {
    /// Cut to a plain square around the logo instead of to the drawing.
    pub square: bool,
    /// Declare the window layered, with one opacity for all of it.
    pub layered: bool,
    /// Ask the toolkit for transparency.
    pub transparent: bool,
    /// Give the window a ground of pure black to erase itself with.
    pub ground: bool,
}

/// The five, in the order they are run.
///
/// The first is the product exactly as it stands, so that the session
/// which follows a new build says whether anything moved before any of
/// this touches it. Each of the others switches off one thing and one
/// only, which is the whole of what makes an answer readable.
const TRIALS: [Trial; 5] = [
    Trial {
        square: false,
        layered: true,
        transparent: true,
        ground: true,
    },
    Trial {
        square: true,
        layered: true,
        transparent: true,
        ground: true,
    },
    Trial {
        square: true,
        layered: false,
        transparent: true,
        ground: true,
    },
    Trial {
        square: true,
        layered: true,
        transparent: false,
        ground: true,
    },
    Trial {
        square: true,
        layered: true,
        transparent: true,
        ground: false,
    },
];

/// How many have been started, which is also the number of the one
/// running.
static STARTED: AtomicUsize = AtomicUsize::new(0);

/// Takes the next trial and says which it is.
///
/// Called where the button's window is built, which happens once per
/// session: the window is closed when the session ends, so closing the
/// session and opening another is what moves this on. After the fifth it
/// starts over at the first.
pub fn starts() -> Trial {
    let which = STARTED.load(Ordering::Relaxed) % TRIALS.len();
    STARTED.store(which + 1, Ordering::Relaxed);
    let trial = TRIALS[which];
    note(&format!(
        "essai du bord {}/{} : {}. Regarde le bouton, prends une capture, \
         puis ferme la session et rouvre-la pour passer au suivant.",
        which + 1,
        TRIALS.len(),
        named(trial)
    ));
    trial
}

/// The trial in force, for everything that reaches the window later than
/// the moment it was built.
#[cfg(windows)]
pub fn now() -> Trial {
    let started = STARTED.load(Ordering::Relaxed);
    // Nothing started yet is the product as it stands, which is what
    // every one of these places did before this file existed.
    TRIALS[started.saturating_sub(1) % TRIALS.len()]
}

/// What a trial is, in words, for the journal.
///
/// Read from the trial rather than written beside it: a name and a set of
/// switches kept apart is a name that ends up describing the wrong
/// session, and the whole worth of these five lines is that they can be
/// trusted.
fn named(trial: Trial) -> String {
    let mut what = vec![if trial.square {
        "découpé en carré"
    } else {
        "le bouton tel qu'il est"
    }];
    if !trial.layered {
        what.push("sans calque");
    }
    if !trial.transparent {
        what.push("sans transparence");
    }
    if !trial.ground {
        what.push("sans fond noir");
    }
    what.join(", ")
}
