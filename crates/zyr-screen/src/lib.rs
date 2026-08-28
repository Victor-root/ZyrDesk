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
    // Laid down asleep. Windows starts a device the moment it is
    // declared, so leaving it there is a second screen on somebody's desk
    // from the minute this product is installed until it is removed, and
    // nobody asked for that. It is woken when a session wants it and put
    // back to sleep when that session ends.
    place::sleep(driver, &mut done)?;
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

/// Wakes the virtual screen for a session, able to show that size.
///
/// The screen sleeps whenever no session wants it, which is nearly all
/// the time: a machine nobody is looking at has the screens its owner
/// plugged in and no others. This is the one moment it is woken, and the
/// size the session asked for is settled first, since waking is when the
/// driver reads the sizes written down for it.
///
/// Awake already means a session still has it, or one ended badly. Only a
/// size it does not already offer is worth stopping and starting it for
/// then, because that is what a session in progress would feel.
#[cfg(windows)]
pub fn wake_up(driver: &dyn Driver, home: &Path, wanted: Mode) -> Result<Done, Trouble> {
    let mut done = Done::default();
    // The size wanted goes first, and that is not tidiness. A screen
    // waking up wears the first size on its list, and the engine then
    // finds it at that size: with the list in a fixed order, every
    // session was served a screen born at the smallest size offered, and
    // it took a second rearrangement of the whole desktop to put it
    // right. On a machine with several screens that second rearrangement
    // lands while the engine is already reconfiguring them, and the two
    // undo each other. Born at the size that was asked for, there is
    // nothing left to change.
    let mut modes: Vec<Mode> = vec![wanted];
    modes.extend(
        ALWAYS_OFFERED
            .iter()
            .copied()
            .filter(|mode| *mode != wanted),
    );
    let sizes_changed = driver.write_modes(home, &modes, &mut done)?;
    match place::awake(driver)? {
        None => {
            done.step("no virtual screen on this computer to wake");
            return Ok(done);
        }
        Some(false) => {
            let before = place::screens_on_the_desktop();
            place::wake(driver, &mut done)?;
            done.changed = true;
            // Waited on, and this is not politeness. Windows hands back
            // from starting a device long before that device is a screen
            // anybody can capture: the desktop is rebuilt around it
            // afterwards, and nothing says when. The session asking is
            // told the screen is ready and starts its engine on that
            // word, so answering early is telling it to capture a screen
            // that is not there yet, which is a session opening onto
            // nothing.
            done.step(match settled(before) {
                Some(waited) => format!("virtual screen on the desktop after {waited} ms"),
                None => format!(
                    "the virtual screen was woken but has not joined the desktop after {} ms, the \
                     session opens on what is there",
                    JOINING_THE_DESKTOP.as_millis()
                ),
            });
        }
        Some(true) if sizes_changed => {
            place::restart(driver, &mut done)?;
            done.changed = true;
        }
        Some(true) => done.step(format!(
            "virtual screen already awake and already offers {wanted}"
        )),
    }
    Ok(done)
}

/// How long a woken screen is given to become one of the desktop's, and
/// how often that is asked.
///
/// Generous at the top because the machine being woken is sometimes busy
/// serving a session already, and short at the bottom because everything
/// waiting on this is a person watching an empty window.
#[cfg(windows)]
const JOINING_THE_DESKTOP: std::time::Duration = std::time::Duration::from_secs(5);
#[cfg(windows)]
const ASKING_AGAIN: std::time::Duration = std::time::Duration::from_millis(50);

/// Waits for the desktop to be made of one more screen than it was.
///
/// Answers how long that took, or nothing when it never happened. Counted
/// rather than named: what is being waited on is Windows finishing, and a
/// screen that is on the desktop is exactly what the far engine can find.
#[cfg(windows)]
fn settled(before: usize) -> Option<u128> {
    let start = std::time::Instant::now();
    while start.elapsed() < JOINING_THE_DESKTOP {
        if place::screens_on_the_desktop() > before {
            return Some(start.elapsed().as_millis());
        }
        std::thread::sleep(ASKING_AGAIN);
    }
    None
}

/// Puts it back to sleep, which is where it belongs between sessions.
///
/// Doing nothing when it is asleep already, and that is the ordinary
/// case: this is asked at every start of the service and at the end of
/// every session, so that a computer whose service was killed mid-session
/// does not keep a screen its owner never asked for.
///
/// It waits for the desktop to stop changing first, and that wait is the
/// point of this whole function. A session ending is the far engine
/// putting back the screens it had switched off for it, and the
/// arrangement it puts back is the one that had this screen in it. Taking
/// the screen away while it is halfway through leaves it restoring a
/// screen that no longer exists: it gives up, switches every screen it
/// can find back on to be safe, and the person at this computer finds
/// monitors they had switched off themselves lit up again. Waiting costs
/// a few seconds nobody is looking at.
#[cfg(windows)]
pub fn go_to_sleep(driver: &dyn Driver) -> Result<Done, Trouble> {
    let mut done = Done::default();
    match place::awake(driver)? {
        None => done.step("no virtual screen on this computer to put to sleep"),
        Some(false) => done.step("virtual screen already asleep"),
        Some(true) => {
            if let Some(waited) = the_desktop_settled() {
                done.step(format!("the desktop stopped changing after {waited} ms"));
            } else {
                done.step(format!(
                    "the desktop was still changing after {} ms, putting the screen to sleep \
                     anyway",
                    SETTLING.as_millis()
                ));
            }
            place::sleep(driver, &mut done)?;
            done.changed = true;
        }
    }
    Ok(done)
}

/// How long the desktop is given to stop changing, and how still it has
/// to be before it counts as settled.
///
/// The long one covers a far engine putting several screens back one at a
/// time, which is the slowest thing that happens here. The short one is
/// what says it has finished: it changes the count every time it moves a
/// screen, so a stretch with no change at all is a stretch with nothing
/// happening.
#[cfg(windows)]
const SETTLING: std::time::Duration = std::time::Duration::from_secs(10);
#[cfg(windows)]
const STILLNESS: std::time::Duration = std::time::Duration::from_millis(800);

/// Waits for the desktop to stop being rearranged, and says how long that
/// took. Nothing when it never stopped.
#[cfg(windows)]
fn the_desktop_settled() -> Option<u128> {
    let start = std::time::Instant::now();
    let mut seen = place::screens_on_the_desktop();
    let mut since = start;
    while start.elapsed() < SETTLING {
        std::thread::sleep(ASKING_AGAIN);
        let now = place::screens_on_the_desktop();
        if now == seen {
            if since.elapsed() >= STILLNESS {
                return Some(start.elapsed().as_millis());
            }
            continue;
        }
        seen = now;
        since = std::time::Instant::now();
    }
    None
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
pub fn wake_up(_driver: &dyn Driver, _home: &Path, _wanted: Mode) -> Result<Done, Trouble> {
    Err(Trouble::NotHere)
}

#[cfg(not(windows))]
pub fn go_to_sleep(_driver: &dyn Driver) -> Result<Done, Trouble> {
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

/// Whether it is awake, `None` when this machine has none at all.
#[cfg(windows)]
pub fn awake(driver: &dyn Driver) -> Result<Option<bool>, Trouble> {
    place::awake(driver)
}

#[cfg(not(windows))]
pub fn awake(_driver: &dyn Driver) -> Result<Option<bool>, Trouble> {
    Ok(None)
}

/// Size of this machine's main screen, as it stands.
///
/// What a session asks for when it wants this computer left exactly as it
/// is. That size cannot be worked out at the other end: nothing there
/// knows what is plugged in here, and guessing it wrong is a picture
/// scaled twice for nothing.
///
/// Asked of the system's own display configuration rather than of a
/// window, because the one asking is a service and a service has no
/// window and no desktop to put one on.
#[cfg(windows)]
pub fn the_main_screen() -> Option<(u32, u32)> {
    place::the_main_screen()
}

#[cfg(not(windows))]
pub fn the_main_screen() -> Option<(u32, u32)> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_size_a_session_asked_for_is_the_one_the_screen_is_born_at() {
        // Un écran qui se réveille porte la première taille de sa liste.
        // Laissée dans un ordre fixe, chaque session recevait un écran né
        // à la plus petite taille offerte, et il fallait réarranger tout
        // le bureau une deuxième fois pour la corriger, pendant que le
        // moteur le réarrangeait déjà.
        let wanted = Mode::new(1920, 1080, 60);
        let mut modes: Vec<Mode> = vec![wanted];
        modes.extend(
            ALWAYS_OFFERED
                .iter()
                .copied()
                .filter(|mode| *mode != wanted),
        );
        assert_eq!(modes[0], wanted);
        // Et elle n'y est qu'une fois, même quand elle est déjà offerte.
        assert_eq!(modes.iter().filter(|mode| **mode == wanted).count(), 1);
        assert_eq!(modes.len(), ALWAYS_OFFERED.len());

        // Une taille que personne n'offrait s'ajoute sans en chasser une.
        let unusual = Mode::new(2048, 1152, 60);
        let mut modes: Vec<Mode> = vec![unusual];
        modes.extend(
            ALWAYS_OFFERED
                .iter()
                .copied()
                .filter(|mode| *mode != unusual),
        );
        assert_eq!(modes[0], unusual);
        assert_eq!(modes.len(), ALWAYS_OFFERED.len() + 1);
    }

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
