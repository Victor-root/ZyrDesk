//! Asking the service, from a window.
//!
//! Every question opens the channel and closes it again. An interface
//! that held one open would have to notice the service restarting and
//! reconnect, for a channel that answers in less time than a frame.
//!
//! Nothing here interprets an answer: what comes back is handed to
//! whoever asked, a refusal included, as the fact it is. Whoever shows it
//! to the person puts it into words.

use zyr_control::{Answer, Request, Service};
use zyr_proto::fact::Fact;

/// What this module files its journal lines under.
const TAG: &str = "service";

/// Writes a line under this module's tag.
fn note(what: &str) {
    crate::shell::journal::note_about(TAG, what);
}

/// Asks one thing, and waits for the one answer.
pub async fn ask(request: &Request) -> Result<Answer, Fact> {
    let mut service = Service::join().await.map_err(|e| e.fact())?;
    match service.ask(request).await.map_err(|e| e.fact())? {
        Answer::Refused(fact) => Err(fact),
        answer => Ok(answer),
    }
}

/// Asks for a list, and reads what came back.
pub async fn list<T>(
    request: &Request,
    read: impl Fn(Answer) -> Option<T>,
) -> Result<Vec<T>, Fact> {
    let mut service = Service::join().await.map_err(|e| e.fact())?;
    let found = service
        .ask_for_a_list(request)
        .await
        .map_err(|e| e.fact())?;
    Ok(found.into_iter().filter_map(read).collect())
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
    crate::shell::app::spawn(async {
        // Already standing: opening the window a second time must not
        // shake a service that is holding a session.
        if Service::join().await.is_ok() {
            return;
        }
        note("service silent, start asked for");
        let outcome = crate::shell::app::spawn_blocking(zyr_launch::start_the_service).await;
        note(&match outcome {
            Ok(Ok(())) => "service asked to start".to_string(),
            Ok(Err(e)) => format!("service not started: {e}"),
            Err(e) => format!("service not started: {e}"),
        });
    });
}
