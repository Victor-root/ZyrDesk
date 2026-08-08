//! What this computer is, asked of the service.
//!
//! Everything here is read, never held. When the service does not
//! answer, that is itself the state to show: the product is installed
//! but not running, which is something the person can act on.

use serde::Serialize;
use zyr_control::{Answer, Request, Service};

/// What the home screen shows about this computer.
#[derive(Serialize)]
pub struct Standing {
    /// Name the person knows this machine by.
    pub name: String,
    /// What another computer pins to reach this one.
    pub fingerprint: String,
    /// Whether this computer can be reached right now.
    pub hosting: bool,
    /// Sessions this computer has open towards others.
    pub ways: usize,
    /// Set when the service could not be asked, in words meant to be
    /// shown as they are.
    pub unreachable: Option<String>,
}

impl Standing {
    /// What is still true when the service is not there.
    fn without_the_service(reason: String) -> Self {
        Self {
            name: zyr_proto::machine::name(),
            fingerprint: String::new(),
            hosting: false,
            ways: 0,
            unreachable: Some(reason),
        }
    }
}

#[tauri::command]
pub async fn standing() -> Standing {
    match asked().await {
        Ok(standing) => standing,
        Err(reason) => Standing::without_the_service(reason),
    }
}

async fn asked() -> Result<Standing, String> {
    let mut service = Service::join().await.map_err(|e| e.to_string())?;
    match service
        .ask(&Request::Standing)
        .await
        .map_err(|e| e.to_string())?
    {
        Answer::Standing(standing) => Ok(Standing {
            name: zyr_proto::machine::name(),
            fingerprint: standing.fingerprint.to_string(),
            hosting: standing.hosting,
            ways: standing.ways,
            unreachable: None,
        }),
        Answer::Refused(reason) => Err(reason),
        other => Err(format!("réponse inattendue du service : {other}")),
    }
}
