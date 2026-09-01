//! Asking the service, from a window.
//!
//! Every question opens the channel and closes it again. An interface
//! that held one open would have to notice the service restarting and
//! reconnect, for a channel that answers in less time than a frame.
//!
//! Nothing here interprets an answer: what comes back is handed to
//! whoever asked, refusals included, since those are written for the
//! person and shown as they are.

use zyr_control::{Answer, Request, Service};

/// Asks one thing, and waits for the one answer.
pub async fn ask(request: &Request) -> Result<Answer, String> {
    let mut service = Service::join().await.map_err(|e| e.to_string())?;
    match service.ask(request).await.map_err(|e| e.to_string())? {
        Answer::Refused(reason) => Err(reason),
        answer => Ok(answer),
    }
}

/// Asks for a list, and reads what came back.
pub async fn list<T>(
    request: &Request,
    read: impl Fn(Answer) -> Option<T>,
) -> Result<Vec<T>, String> {
    let mut service = Service::join().await.map_err(|e| e.to_string())?;
    let found = service
        .ask_for_a_list(request)
        .await
        .map_err(|e| e.to_string())?;
    Ok(found.into_iter().filter_map(read).collect())
}

/// What to say when the service answers something else entirely: the two
/// halves of the product were not installed at the same time.
pub fn unexpected(answer: Answer) -> String {
    format!("réponse inattendue du service : {answer}")
}

/// Puts the service back on its feet, if it is not already standing.
///
/// Nothing of this product runs while nobody is using it, so opening the
/// window is what starts it. Without administrator rights: registering
/// the service grants whoever is signed in the right to start and stop
/// it, precisely so that this costs nobody a prompt.
///
/// On a thread of its own and never waited for. A service takes a moment
/// to come up, the home screen already knows how to show a service that
/// is not answering yet, and a window that stayed grey until Windows had
/// finished would look broken.
pub fn wake_the_service() {
    crate::app::spawn(async {
        // Already standing: opening the window a second time must not
        // shake a service that is holding a session.
        if Service::join().await.is_ok() {
            return;
        }
        crate::journal::note("service muet, démarrage demandé");
        let outcome = crate::app::spawn_blocking(started).await;
        crate::journal::note(&match outcome {
            Ok(Ok(())) => "service demandé au démarrage".to_string(),
            Ok(Err(e)) => format!("service non démarré : {e}"),
            Err(e) => format!("service non démarré : {e}"),
        });
    });
}

/// Asks the service program to start the service.
///
/// The program beside this one, which is where it is: the two are built
/// and shipped together.
#[cfg(windows)]
fn started() -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;

    /// Keeps a console window from flashing up behind the interface.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let here = std::env::current_exe()?;
    let program = here.with_file_name(zyr_proto::paths::executable_name("zyrdeskd"));
    let said = std::process::Command::new(program)
        .arg("start")
        .creation_flags(CREATE_NO_WINDOW)
        .output()?;
    if said.status.success() {
        return Ok(());
    }
    // Failures land on the error output; the ordinary one is read only
    // when there is nothing there, so the reason is never an empty line.
    let mut words = String::from_utf8_lossy(&said.stderr).trim().to_string();
    if words.is_empty() {
        words = String::from_utf8_lossy(&said.stdout).trim().to_string();
    }
    Err(std::io::Error::other(words))
}

#[cfg(not(windows))]
fn started() -> std::io::Result<()> {
    Err(std::io::Error::other(
        "le service ZyrDesk n'existe que sous Windows",
    ))
}
