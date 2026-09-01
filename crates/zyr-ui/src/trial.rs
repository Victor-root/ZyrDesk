//! Where the button's white is made: six ways of cutting and redrawing
//! that window, one per session.
//!
//! What is known, and measured rather than reasoned.
//!
//! A pixel of that window the page does not paint shows **nothing** at
//! rest: two hundred pixels of bare window, put there on purpose, stayed
//! invisible over black. The frosted glass died with that measurement.
//!
//! The pale edging shows **only on a click and on a drag**, never at
//! rest. And cut to a plain rectangle instead of to the drawing, the same
//! thing becomes **a white square behind the logo**. So the edging is not
//! an edging: the whole window turns white for the length of a cut, and a
//! cut hugging the drawing only ever let a hairline of it through.
//!
//! What is not known: **which layer paints that white**. This window's
//! own ground is pure black, so it is not this one. What is left is the
//! web view it carries, which is a window of its own with its own
//! background brush, and the layer Windows keeps of it. The trials switch
//! them off one at a time, on the one thing that sets them going: the cut
//! and the redraw that follows it.
//!
//! One trial per session, in order, and the journal says which one is
//! running. This is an instrument, not a feature: it goes the day it has
//! answered, like the camera before it.

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::journal::note;

/// What the window is asked for after a cut.
#[derive(Clone, Copy, PartialEq)]
pub enum Redraw {
    /// What the product does: the window and everything it carries.
    AsToday,
    /// The same, saying this time that nothing is to be wiped first.
    NoErase,
    /// The window alone, without the web view it carries.
    WindowOnly,
    /// Nothing at all.
    None,
}

/// How the button is cut and redrawn for one trial.
#[derive(Clone, Copy)]
pub struct Trial {
    /// One cut per shape the page draws, worked out the first time and
    /// never made again: a logo growing, shrinking or travelling cuts
    /// nothing any more.
    pub frozen: bool,
    /// What the system is asked for once a cut is made.
    pub redraw: Redraw,
    /// Declare the window layered, with one opacity for all of it.
    pub layered: bool,
}

/// The six, in the order they are run.
///
/// The first is the product exactly as it stands, so that the session
/// which follows a new build says whether anything moved before any of
/// this touches it. Each of the others switches off one thing and one
/// only, which is the whole difference between an experiment and a
/// fiddle.
const TRIALS: [Trial; 6] = [
    Trial {
        frozen: false,
        redraw: Redraw::AsToday,
        layered: true,
    },
    Trial {
        frozen: true,
        redraw: Redraw::AsToday,
        layered: true,
    },
    Trial {
        frozen: false,
        redraw: Redraw::NoErase,
        layered: true,
    },
    Trial {
        frozen: false,
        redraw: Redraw::WindowOnly,
        layered: true,
    },
    Trial {
        frozen: false,
        redraw: Redraw::None,
        layered: true,
    },
    Trial {
        frozen: false,
        redraw: Redraw::AsToday,
        layered: false,
    },
];

/// How many have been started, which is also the number of the one
/// running.
static STARTED: AtomicUsize = AtomicUsize::new(0);

/// Takes the next trial and says which it is.
///
/// Called where the button's window is built, which happens once per
/// session: the window is closed when the session ends, so closing the
/// session and opening another is what moves this on. After the sixth it
/// starts over at the first.
pub fn starts() -> Trial {
    let which = STARTED.load(Ordering::Relaxed) % TRIALS.len();
    STARTED.store(which + 1, Ordering::Relaxed);
    let trial = TRIALS[which];
    note(&format!(
        "essai du bord {}/{} : {}. Clique sur le bouton et déplace-le, prends une capture, \
         puis ferme la session et rouvre-la pour passer au suivant.",
        which + 1,
        TRIALS.len(),
        named(trial)
    ));
    trial
}

/// The trial in force, for everything that reaches the window later than
/// the moment it was built.
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
/// session, and the whole worth of these six lines is that they can be
/// trusted.
fn named(trial: Trial) -> String {
    let mut what = vec![if trial.frozen {
        "découpe figée, une par forme"
    } else {
        "le bouton tel qu'il est"
    }];
    match trial.redraw {
        Redraw::AsToday => {}
        Redraw::NoErase => what.push("sans effacement au redessin"),
        Redraw::WindowOnly => what.push("sans redessin de la vue web"),
        Redraw::None => what.push("sans redessin du tout"),
    }
    if !trial.layered {
        what.push("sans calque");
    }
    what.join(", ")
}
