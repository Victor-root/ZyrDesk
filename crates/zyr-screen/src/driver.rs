//! The border between the product and whichever driver grows the screen.
//!
//! One trait, and everything on the far side of it is one file. What is
//! on this side never learns the make of the driver: it hands over a
//! folder of files, a folder to live in, and a list of sizes, and gets
//! back whether anything changed. That is the same shape the engines are
//! held at, and for the same reason: a driver that has to be swapped
//! later should cost one new file and nothing else.

use std::fmt;
use std::path::Path;

use crate::{Done, Trouble};

/// One picture size a screen can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mode {
    pub width: u32,
    pub height: u32,
    /// Times a second, whole. Nothing here needs the fractional rates a
    /// television carries, and asking for one would only invite a driver
    /// to round it somewhere we cannot see.
    pub hz: u32,
}

impl Mode {
    pub const fn new(width: u32, height: u32, hz: u32) -> Self {
        Self { width, height, hz }
    }
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}x{} at {} Hz", self.width, self.height, self.hz)
    }
}

/// The number Windows files a kind of device under.
///
/// Ours and not the system's on purpose: a driver describes itself in
/// plain data, which keeps this border free of anything that only exists
/// on one system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Guid {
    pub a: u32,
    pub b: u16,
    pub c: u16,
    pub d: [u8; 8],
}

/// Everything true of one virtual screen driver and of no other.
pub trait Driver: Sync {
    /// What to call it in the log.
    fn name(&self) -> &'static str;

    /// What Windows knows the device by. The device is created by
    /// writing this down, and found again by looking for it.
    fn hardware_id(&self) -> &'static str;

    /// Kind of device it is, and the number Windows files that kind
    /// under. The name is only ever shown; the number is what counts.
    fn class(&self) -> (&'static str, Guid);

    /// The description file inside the package folder.
    fn inf_file(&self) -> &'static str;

    /// Every file the package folder must hold. Checked before anything
    /// is installed: a package short of one file fails halfway through
    /// installing instead, and half an installed driver is worse than
    /// none.
    fn package_files(&self) -> &'static [&'static str];

    /// Refuses a package folder that is not whole.
    fn check_package(&self, package: &Path) -> Result<(), Trouble> {
        for file in self.package_files() {
            if !package.join(file).is_file() {
                return Err(Trouble::PackageIncomplete {
                    missing: package.join(file).display().to_string(),
                });
            }
        }
        Ok(())
    }

    /// Whether a screen introducing itself by that name is this
    /// driver's own.
    fn is_its_screen(&self, friendly_name: &str) -> bool;

    /// Tells the driver to keep its settings under `home` rather than
    /// wherever it would have put them.
    ///
    /// A driver left to its own devices writes into a folder of its
    /// choosing at the root of the disk, which is neither ours to leave
    /// behind nor ours to clean up.
    fn settle_in(&self, home: &Path, done: &mut Done) -> Result<(), Trouble>;

    /// Undoes [`Driver::settle_in`], leaving nothing pointing at us.
    fn move_out(&self, home: &Path, done: &mut Done) -> Result<(), Trouble>;

    /// Writes down the sizes the screen must offer.
    ///
    /// Returns whether that changed what was already written. It
    /// commonly does not, and the caller uses that to spare the machine
    /// a screen restart nobody would have wanted.
    fn write_modes(&self, home: &Path, modes: &[Mode], done: &mut Done) -> Result<bool, Trouble>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_size_says_itself_the_way_a_person_reads_it() {
        assert_eq!(Mode::new(3840, 2160, 60).to_string(), "3840x2160 at 60 Hz");
    }

    #[test]
    fn a_package_short_of_a_file_is_refused_by_name() {
        let nowhere = Path::new("/pas/de/dossier/ici");
        let refusal = crate::mtt::MTT.check_package(nowhere).unwrap_err();
        let said = refusal.to_string();
        assert!(said.contains(crate::mtt::MTT.inf_file()), "{said}");
    }
}
