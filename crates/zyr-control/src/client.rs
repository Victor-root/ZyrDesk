//! Talking to the service from another program.
//!
//! Everything the interface and the command line ask of this computer
//! goes through here. Neither of them holds a tunnel or an engine: they
//! ask, the service does, and it keeps doing it once they are gone.

use std::fmt;
use std::io;

use crate::message::{Answer, Malformed, Request};
use crate::pipe::{self, CHANNEL, Spoken};

/// Why an exchange with the service did not happen.
#[derive(Debug)]
pub enum ControlError {
    /// Nothing is listening: the service is not running.
    NotRunning,
    /// It was listening, and the exchange broke anyway.
    Broken(io::Error),
    /// It answered something this program cannot read.
    Unreadable(Malformed),
    /// It stopped talking mid-exchange.
    LeftOff,
    /// It understood, and said no. The text is meant to be shown.
    Refused(String),
}

impl fmt::Display for ControlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ControlError::NotRunning => f.write_str(
                "le service ZyrDesk ne tourne pas.\n  \
                 Lancez « zyrdeskd status » pour voir son état.",
            ),
            ControlError::Broken(e) => write!(f, "échange interrompu : {e}"),
            ControlError::Unreadable(e) => write!(
                f,
                "réponse incompréhensible du service : {e}\n  \
                 Le service est probablement plus ancien que ce programme."
            ),
            ControlError::LeftOff => f.write_str("le service a coupé la conversation"),
            ControlError::Refused(reason) => f.write_str(reason),
        }
    }
}

impl std::error::Error for ControlError {}

impl From<io::Error> for ControlError {
    fn from(e: io::Error) -> Self {
        match e.kind() {
            io::ErrorKind::NotFound => ControlError::NotRunning,
            _ => ControlError::Broken(e),
        }
    }
}

/// The service, at the other end of the channel.
pub struct Service {
    talking: Spoken,
}

impl Service {
    /// Joins the service on the channel the product uses.
    pub async fn join() -> Result<Self, ControlError> {
        Self::join_on(CHANNEL).await
    }

    pub async fn join_on(channel: &str) -> Result<Self, ControlError> {
        Ok(Self {
            talking: pipe::call(channel).await?,
        })
    }

    /// Asks one thing, and waits for the answer to it.
    pub async fn ask(&mut self, request: &Request) -> Result<Answer, ControlError> {
        self.talking.say(&request.to_string()).await?;
        self.next_answer().await
    }

    /// Asks for a list, and collects it until the service says it is
    /// done.
    ///
    /// A list travels as one message per item rather than as one long
    /// line: the channel keeps its shape, and it stays readable by eye
    /// when something goes wrong.
    pub async fn ask_for_a_list(&mut self, request: &Request) -> Result<Vec<Answer>, ControlError> {
        self.talking.say(&request.to_string()).await?;
        let mut collected = Vec::new();
        loop {
            match self.next_answer().await? {
                Answer::Done => return Ok(collected),
                Answer::Refused(reason) => return Err(ControlError::Refused(reason)),
                item => collected.push(item),
            }
        }
    }

    async fn next_answer(&mut self) -> Result<Answer, ControlError> {
        let line = self.talking.hear().await?.ok_or(ControlError::LeftOff)?;
        Answer::parse(&line).map_err(ControlError::Unreadable)
    }
}
