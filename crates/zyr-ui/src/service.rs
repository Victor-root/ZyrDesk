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
