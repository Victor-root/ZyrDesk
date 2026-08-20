//! The screen the host computer grows for a session to be shown on.
//!
//! A session can only ever be as sharp as the desktop it was captured
//! from. Asked for a picture larger than anything its own screens can
//! show, the host engine captures what it has and blows it up before
//! encoding: the stream carries the requested number of pixels and not
//! one extra piece of detail. A laptop with a common screen therefore
//! cannot serve a large screen properly, however much rate is thrown at
//! it, and no care taken at the other end can put back what was never
//! drawn.
//!
//! What fixes it is giving the host a screen of the size being asked
//! for. Windows lets a driver declare a screen that no cable leads to;
//! the desktop is really drawn at that size, and the engine really
//! captures it. Windows will not load such a driver unless it is signed,
//! and this product buys no certificate, so the driver is a signed one
//! somebody else publishes and ZyrDesk carries, installs, points at its
//! own folder, and takes away again.
//!
//! # The border
//!
//! Everything true of one particular driver and of no other lives behind
//! [`Driver`], in one file. This crate's own code knows a hardware
//! identifier, a folder of files and a list of sizes, and nothing else.
//! Swapping the driver for another means writing one more file next to
//! [`mtt`], not touching anything here or anywhere else in the product,
//! which is the same border the engines are held behind.

pub mod driver;
pub mod engine;
pub mod mtt;

#[cfg(windows)]
mod place;
#[cfg(windows)]
mod vouching;

use std::fmt;
use std::path::Path;

pub use driver::{Driver, Guid, Mode};
pub use engine::Screen;

/// Sizes the virtual screen always offers, whatever a session asks for.
///
/// A screen with no size at all is not a screen: Windows needs one it
/// can show before anybody has asked for anything.
///
/// Long on purpose. A size that is already offered costs a session
/// nothing; one that is not costs it a screen restart, which the person
/// sitting at the host computer sees. These are the shapes screens are
/// actually sold in, so the restart is what an unusual screen pays and
/// everybody else never meets.
pub const ALWAYS_OFFERED: &[Mode] = &[
    // Sixteen by nine.
    Mode::new(1280, 720, 60),
    Mode::new(1366, 768, 60),
    Mode::new(1600, 900, 60),
    Mode::new(1920, 1080, 60),
    Mode::new(2560, 1440, 60),
    Mode::new(3200, 1800, 60),
    Mode::new(3840, 2160, 60),
    // Sixteen by ten, which most laptops are again.
    Mode::new(1280, 800, 60),
    Mode::new(1680, 1050, 60),
    Mode::new(1920, 1200, 60),
    Mode::new(2560, 1600, 60),
    Mode::new(3840, 2400, 60),
    // Three by two, the other laptop shape.
    Mode::new(2256, 1504, 60),
    Mode::new(3000, 2000, 60),
    // Wide and very wide.
    Mode::new(2560, 1080, 60),
    Mode::new(3440, 1440, 60),
    Mode::new(3840, 1600, 60),
    Mode::new(5120, 1440, 60),
];

/// What went wrong, said in the language of the person who reads it.
#[derive(Debug)]
pub enum Trouble {
    /// This system has no such thing as a driver-declared screen.
    NotHere,
    /// The folder handed over does not hold the driver's own files.
    PackageIncomplete { missing: String },
    /// A call into the system refused, with the number it refused by.
    System { doing: String, code: u32 },
    /// A file could not be read or written.
    File { path: String, reason: String },
}

impl fmt::Display for Trouble {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Trouble::NotHere => f.write_str("l'écran virtuel n'existe que sous Windows"),
            Trouble::PackageIncomplete { missing } => {
                write!(f, "fichier manquant dans le pilote fourni : {missing}")
            }
            Trouble::System { doing, code } => {
                write!(f, "Windows a refusé ({doing}), code {code}")
            }
            Trouble::File { path, reason } => write!(f, "{path} : {reason}"),
        }
    }
}

impl std::error::Error for Trouble {}

/// What an operation did, step by step, for the log to carry.
///
/// Every one of these steps is a place a virtual screen can quietly fail
/// to appear, and none of them says anything on its own. Written down so
/// a screen that never showed up can be traced to the step that let it
/// down rather than guessed at.
#[derive(Debug, Default)]
pub struct Done {
    /// Whether anything about the machine actually changed.
    pub changed: bool,
    pub steps: Vec<String>,
}

impl Done {
    fn step(&mut self, said: impl Into<String>) {
        self.steps.push(said.into());
    }
}

/// The driver this product carries.
///
/// One call, so that everywhere else in the product names the screen and
/// not the make of it.
pub fn shipped() -> &'static dyn Driver {
    &mtt::MTT
}

/// Puts the virtual screen on this machine, or leaves it as it is if it
/// is already there.
///
/// `package` is the folder holding the driver's own files, `home` the
/// folder ZyrDesk keeps its settings and its logs in.
#[cfg(windows)]
pub fn install(driver: &dyn Driver, package: &Path, home: &Path) -> Result<Done, Trouble> {
    driver.check_package(package)?;
    let mut done = Done::default();
    done.step(format!(
        "virtual screen driver: {} from {}",
        driver.name(),
        package.display()
    ));
    driver.settle_in(home, &mut done)?;
    driver.write_modes(home, ALWAYS_OFFERED, &mut done)?;
    // Before laying the driver down and not after: what this answers is
    // a question Windows asks while laying it down, on a desktop nobody
    // is watching.
    vouching::vouch_for(&package.join(driver.catalog_file()), &mut done)?;
    place::put_in_place(driver, package, home, &mut done)?;
    Ok(done)
}

/// Takes the virtual screen back off, leaving nothing of it behind.
///
/// `package` is wanted here too, and only to read who signed the driver
/// so that publisher can stop being named as one this computer expects.
#[cfg(windows)]
pub fn uninstall(driver: &dyn Driver, package: &Path, home: &Path) -> Result<Done, Trouble> {
    let mut done = Done::default();
    place::take_away(driver, home, &mut done)?;
    driver.move_out(home, &mut done)?;
    let catalog = package.join(driver.catalog_file());
    if catalog.is_file() {
        vouching::stop_vouching_for(&catalog, &mut done)?;
    } else {
        done.step(format!(
            "the driver's signature is no longer on disk ({}), its publisher stays named as \
             expected",
            catalog.display()
        ));
    }
    Ok(done)
}

/// Makes sure the virtual screen can show that size.
///
/// Nothing happens at all when it already can, which is the ordinary
/// case: the sizes it is born with cover the usual screens. When it
/// cannot, the sizes are written down and the screen is restarted, which
/// is the only moment the driver reads them again.
#[cfg(windows)]
pub fn offer(driver: &dyn Driver, home: &Path, wanted: Mode) -> Result<Done, Trouble> {
    let mut done = Done::default();
    let mut modes: Vec<Mode> = ALWAYS_OFFERED.to_vec();
    if !modes.contains(&wanted) {
        modes.push(wanted);
    }
    if !driver.write_modes(home, &modes, &mut done)? {
        done.step(format!(
            "virtual screen already offers {wanted}, left alone"
        ));
        return Ok(done);
    }
    done.changed = true;
    place::restart(driver, &mut done)?;
    Ok(done)
}

#[cfg(not(windows))]
pub fn install(_driver: &dyn Driver, _package: &Path, _home: &Path) -> Result<Done, Trouble> {
    Err(Trouble::NotHere)
}

#[cfg(not(windows))]
pub fn uninstall(_driver: &dyn Driver, _package: &Path, _home: &Path) -> Result<Done, Trouble> {
    Err(Trouble::NotHere)
}

#[cfg(not(windows))]
pub fn offer(_driver: &dyn Driver, _home: &Path, _wanted: Mode) -> Result<Done, Trouble> {
    Err(Trouble::NotHere)
}

/// Whether the virtual screen is on this machine right now.
#[cfg(windows)]
pub fn present(driver: &dyn Driver) -> Result<bool, Trouble> {
    place::present(driver)
}

#[cfg(not(windows))]
pub fn present(_driver: &dyn Driver) -> Result<bool, Trouble> {
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sizes_a_screen_is_born_with_cover_the_usual_ones() {
        // Chaque taille absente d'ici coûte un redémarrage de l'écran à
        // la première session qui la demande : celles-là couvrent
        // l'écrasante majorité des ordinateurs.
        for common in [(1920, 1080), (2560, 1440), (3840, 2160)] {
            assert!(
                ALWAYS_OFFERED.iter().any(|m| (m.width, m.height) == common),
                "{common:?}"
            );
        }
    }

    #[test]
    fn the_product_carries_exactly_one_driver() {
        assert_eq!(shipped().hardware_id(), mtt::MTT.hardware_id());
        assert!(!shipped().name().is_empty());
    }
}
