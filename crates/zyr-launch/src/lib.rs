//! How ZyrDesk's programs come to be running.
//!
//! The service is started by whoever opens the window, with no prompt:
//! registering it gave whoever is signed in the right to. It is set up
//! once, with the administrator rights Windows asks for, which is the one
//! prompt the product ever shows. And the window comes back when the
//! person signs in, when they asked for it.
//!
//! What asks Windows for each of these is `zyr-system`'s. What is decided
//! here is which program, with which word, and what its answer means, so
//! all of it is built and tried everywhere.

use std::io;
use std::path::PathBuf;

use zyr_proto::fact::Fact;
use zyr_proto::paths;
use zyr_system::Refusal;

/// What the window is started under when the person signs in, which is
/// what they read in the task manager's start-up tab.
const AT_SIGN_IN: &str = "ZyrDesk";

/// The point from which an exit code is no longer a program's own.
///
/// Windows puts there what it says about a program that stopped by
/// itself: a crash, an overflow, a damaged image. The service program
/// only ever exits with zero or one, so nothing of that form can come
/// from it, and anything that comes from there means it was unable to say
/// anything.
const CRASHED: u32 = 0xC000_0000;

/// Starts the service, when it is registered.
///
/// Without administrator rights: registering the service grants whoever
/// is signed in the right to start and stop it, precisely so that this
/// costs nobody a prompt.
pub fn start_the_service() -> io::Result<()> {
    let said = zyr_system::run_unseen(&the_service_program()?, &["start"])?;
    if said.status.success() {
        return Ok(());
    }
    // Failures land on the error output; the ordinary one is read only
    // when there is nothing there, so the reason is never an empty line.
    let mut words = String::from_utf8_lossy(&said.stderr).trim().to_string();
    if words.is_empty() {
        words = String::from_utf8_lossy(&said.stdout).trim().to_string();
    }
    Err(io::Error::other(words))
}

/// Registers the service with Windows and starts it.
///
/// The one moment the product asks Windows for administrator rights: a
/// service is what makes this computer reachable before anybody has
/// signed in, and Windows lets no program register one on its own.
///
/// Waits for the person to answer the prompt and for the service to be
/// up: whoever asked would otherwise show a service that is not there
/// yet, or one that never started at all.
pub fn set_the_service_up() -> Result<(), Fact> {
    let program =
        the_service_program().map_err(|e| Fact::new("launch.no_location").with("detail", e))?;
    if !program.is_file() {
        return Err(Fact::new("launch.service_missing").with("path", program.display()));
    }
    match zyr_system::run_as_administrator(&program, "setup") {
        Ok(0) => Ok(()),
        Err(refused) => Err(said(refused)),
        // A code of this form is not a report from the program but the
        // way Windows says it stopped abruptly. The service has then
        // written nothing, and sending the person off to read a journal
        // that will say nothing is the worst referral there is: an hour
        // goes on searching the place where there is nothing to find.
        Ok(code) if code >= CRASHED => {
            Err(Fact::new("launch.setup_crashed").with("code", format!("{code:08X}")))
        }
        Ok(code) => Err(Fact::new("launch.setup_failed").with("code", code)),
    }
}

/// Decides whether the window comes back on its own when the person
/// signs in.
pub fn start_with_windows(on: bool) -> Result<(), Fact> {
    let program =
        std::env::current_exe().map_err(|e| Fact::new("launch.no_location").with("detail", e))?;
    let command = format!("\"{}\"", program.display());
    zyr_system::start_at_sign_in(AT_SIGN_IN, on.then_some(command.as_str()))
        .map_err(|e| Fact::new("launch.startup_not_saved").with("detail", e))
}

/// The service program, beside this one.
///
/// The two are built and shipped together, so this is where it is;
/// looking for it anywhere else would be guessing.
fn the_service_program() -> io::Result<PathBuf> {
    Ok(std::env::current_exe()?.with_file_name(paths::executable_name("zyrdeskd")))
}

/// What a refusal to run the setup tells the person.
fn said(refused: Refusal) -> Fact {
    match refused {
        Refusal::Declined => Fact::new("launch.elevation_refused"),
        Refusal::NotStarted(e) => Fact::new("launch.elevation_failed").with("detail", e),
        // Started with nothing to watch it by: saying it worked would be a
        // guess, and whoever asked would go on to show a service that may
        // not be there.
        Refusal::Unwatched => Fact::new("launch.elevation_silent"),
        Refusal::NotWaited => Fact::new("launch.setup_wait_failed"),
        Refusal::NoExitCode => Fact::new("launch.setup_no_code"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_refusal_tells_the_person_something_of_its_own() {
        let told = [
            Refusal::Declined,
            Refusal::NotStarted(io::Error::other("the shell said no")),
            Refusal::Unwatched,
            Refusal::NotWaited,
            Refusal::NoExitCode,
        ]
        .map(|refused| said(refused).code().to_string());
        let mut apart = told.to_vec();
        apart.sort();
        apart.dedup();
        assert_eq!(apart.len(), told.len(), "{told:?}");
    }
}
