//! ZyrDesk's own channel inside the tunnel.
//!
//! Everything said here is the product talking to itself, never an
//! engine. Two things travel on it today: the ports the far engine
//! listens on, which the client cannot guess because the engine picks
//! them when it starts, and the code the two engines have to agree on
//! before they will speak to each other. Tomorrow it will carry the
//! clipboard and the statistics.
//!
//! The pairing code is the reason nobody has to walk to the other
//! computer any more. The engines demand that a code shown on one be
//! typed on the other; the tunnel already knows both computers, having
//! recognised them by fingerprint before a single byte passed, so it
//! carries the code itself and the person is never shown one.
//!
//! One question, one stream, one message each way, in plain text: a
//! channel that can be read with the eyes is a channel that can be
//! diagnosed. Every message opens with the version of this dialect, so
//! two halves of the product installed at different times say so rather
//! than misread each other.

use std::fmt;
use std::io;
use std::sync::Arc;

use tokio::io::AsyncWriteExt;
use zyr_proto::net::{BasePortOutOfRange, EnginePorts};
use zyr_transport::{Connection, RecvStream, SendStream};

use crate::channel::StreamChannel;
use crate::pump;

/// Version of this dialect.
///
/// Version 1 was three bytes carrying nothing but the ports.
pub const VERSION: u32 = 2;

/// Longest message this channel takes.
///
/// It carries port numbers, a four-digit code and a machine name.
/// Anything longer is not one of ours.
const LIMIT: usize = 512;

/// What the host side answers on ZyrDesk's own channel.
///
/// The tunnel knows engines by their ports and nothing else. Whoever
/// holds an engine hands over this and keeps the engine to itself, which
/// is what stops the tunnel from having to know how an engine is driven.
pub trait Answers: Send + Sync + 'static {
    /// Ports the local engine listens on.
    fn engine(&self) -> EnginePorts;

    /// Hands a pairing code to the local engine.
    ///
    /// It hands back once the engine has taken the code, and not before:
    /// the computer asking has already started its own engine and is
    /// waiting on this. Blocking is expected, and it is called where
    /// blocking is allowed.
    fn hand_over_the_code(&self, pin: &str, name: &str) -> Result<(), String>;
}

/// What one ZyrDesk asks the other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Question {
    /// Which ports your engine listens on.
    Ports,
    /// Take this code and hand it to your engine. The name is what the
    /// far computer will file this one under.
    Pair { pin: String, name: String },
}

/// What comes back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Told {
    Ports(EnginePorts),
    /// The far engine took the code.
    Paired,
}

impl fmt::Display for Question {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // The name comes last and takes the whole of what is left,
            // spaces and all: no escaping, and nothing to get wrong.
            Question::Pair { pin, name } => write!(f, "{VERSION} pair {pin} {name}"),
            Question::Ports => write!(f, "{VERSION} ports"),
        }
    }
}

impl fmt::Display for Told {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Told::Ports(engine) => write!(f, "{VERSION} ports {}", engine.base()),
            Told::Paired => write!(f, "{VERSION} paired"),
        }
    }
}

impl Question {
    fn parse(message: &str) -> Result<Self, String> {
        let said = after_the_version(message)?;
        let (verb, rest) = split_first(said);
        match verb {
            "ports" => Ok(Question::Ports),
            "pair" => {
                let (pin, name) = split_first(rest);
                if pin.is_empty() || name.is_empty() {
                    return Err("appairage sans code ni nom".to_string());
                }
                Ok(Question::Pair {
                    pin: pin.to_string(),
                    name: name.to_string(),
                })
            }
            other => Err(format!("question inconnue « {other} »")),
        }
    }
}

impl Told {
    fn parse(message: &str) -> io::Result<Result<Self, String>> {
        let said = after_the_version(message).map_err(unreadable)?;
        let (verb, rest) = split_first(said);
        match verb {
            "ports" => {
                let base: u16 = rest
                    .parse()
                    .map_err(|_| unreadable(format!("port « {rest} »")))?;
                let engine = EnginePorts::new(base)
                    .map_err(|e: BasePortOutOfRange| unreadable(e.to_string()))?;
                Ok(Ok(Told::Ports(engine)))
            }
            "paired" => Ok(Ok(Told::Paired)),
            "no" => Ok(Err(rest.to_string())),
            other => Err(unreadable(format!("réponse inconnue « {other} »"))),
        }
    }
}

/// Checks the version at the head of a message and hands back the rest.
fn after_the_version(message: &str) -> Result<&str, String> {
    let (head, rest) = split_first(message.trim());
    match head.parse::<u32>() {
        Ok(VERSION) => Ok(rest),
        Ok(other) => Err(format!(
            "l'autre ordinateur parle la version {other} du tunnel, celui-ci la version {VERSION}"
        )),
        Err(_) => Err("l'autre ordinateur ne parle pas le langage du tunnel".to_string()),
    }
}

/// First word, and everything after it, whitespace and all.
fn split_first(said: &str) -> (&str, &str) {
    match said.trim_start().split_once(char::is_whitespace) {
        Some((first, rest)) => (first, rest.trim()),
        None => (said.trim(), ""),
    }
}

fn unreadable(reason: impl fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, reason.to_string())
}

/// A refusal cut to fit this channel, and flattened onto one message.
///
/// A reason longer than the channel takes would not arrive at all, and
/// the other computer would show a transport fault where it should have
/// shown the reason. Better shortened than lost. Cut on a character and
/// never inside one: these reasons are written in French.
fn shortened(reason: &str) -> String {
    let flat = reason.replace('\n', " ");
    if flat.len() <= ROOM {
        return flat;
    }
    let mut kept = flat;
    let upto = (0..=ROOM).rev().find(|at| kept.is_char_boundary(*at));
    kept.truncate(upto.unwrap_or(0));
    kept
}

/// How long a refusal may be, the rest of its message deducted.
const ROOM: usize = LIMIT - 32;

/// Asks the far ZyrDesk something. Client side.
///
/// A refusal comes back as a failure carrying what the other computer
/// said, which is written to be read by the person.
pub async fn ask(connection: &Connection, question: &Question) -> io::Result<Told> {
    let (mut sending, mut receiving) = connection.open_stream().await.map_err(io::Error::other)?;
    pump::announce(&mut sending, StreamChannel::ZyrDesk).await?;
    sending.write_all(question.to_string().as_bytes()).await?;
    sending.shutdown().await?;

    let heard = receiving
        .read_to_end(LIMIT)
        .await
        .map_err(io::Error::other)?;
    match Told::parse(&String::from_utf8_lossy(&heard))? {
        Ok(told) => Ok(told),
        Err(refusal) => Err(io::Error::other(refusal)),
    }
}

/// Asks for the far engine's ports, and refuses anything else.
pub async fn ask_the_ports(connection: &Connection) -> io::Result<EnginePorts> {
    match ask(connection, &Question::Ports).await? {
        Told::Ports(engine) => Ok(engine),
        other => Err(unreadable(format!("réponse hors sujet : {other}"))),
    }
}

/// Hands the far computer the code its engine is waiting for.
pub async fn ask_to_pair(connection: &Connection, pin: &str, name: &str) -> io::Result<()> {
    let question = Question::Pair {
        pin: pin.to_string(),
        name: name.to_string(),
    };
    match ask(connection, &question).await? {
        Told::Paired => Ok(()),
        other => Err(unreadable(format!("réponse hors sujet : {other}"))),
    }
}

/// Answers whatever the other ZyrDesk asks. Host side.
pub async fn answer(
    sending: SendStream,
    mut receiving: RecvStream,
    answering: Arc<dyn Answers>,
) -> io::Result<()> {
    let asked = receiving
        .read_to_end(LIMIT)
        .await
        .map_err(io::Error::other)?;
    let said = String::from_utf8_lossy(&asked).to_string();

    let told = match Question::parse(&said) {
        Ok(question) => attended(question, answering).await,
        Err(refusal) => Err(refusal),
    };
    say(sending, told).await
}

/// Does what was asked, on a thread where waiting is allowed.
///
/// Handing a code to an engine talks to it over the network and waits
/// for its answer: doing that on the runtime's own threads would hold up
/// every session this computer is serving.
async fn attended(question: Question, answering: Arc<dyn Answers>) -> Result<Told, String> {
    match question {
        Question::Ports => Ok(Told::Ports(answering.engine())),
        Question::Pair { pin, name } => {
            tokio::task::spawn_blocking(move || answering.hand_over_the_code(&pin, &name))
                .await
                .map_err(|e| format!("l'appairage n'a pas pu être mené : {e}"))?
                .map(|()| Told::Paired)
        }
    }
}

async fn say(mut sending: SendStream, told: Result<Told, String>) -> io::Result<()> {
    let message = match told {
        Ok(told) => told.to_string(),
        Err(reason) => format!("{VERSION} no {}", shortened(&reason)),
    };
    sending.write_all(message.as_bytes()).await?;
    sending.shutdown().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ports(base: u16) -> EnginePorts {
        EnginePorts::new(base).unwrap()
    }

    #[test]
    fn every_question_survives_the_round_trip() {
        for question in [
            Question::Ports,
            Question::Pair {
                pin: "0429".to_string(),
                // Un nom d'ordinateur porte des espaces plus souvent
                // qu'on ne le croit, et il voyage en fin de message
                // pour cette raison précise.
                name: "PC de Victor".to_string(),
            },
        ] {
            let said = question.to_string();
            assert_eq!(Question::parse(&said), Ok(question), "sur « {said} »");
        }
    }

    #[test]
    fn every_answer_survives_the_round_trip() {
        for told in [Told::Ports(ports(42000)), Told::Paired] {
            let said = told.to_string();
            assert_eq!(Told::parse(&said).unwrap(), Ok(told), "sur « {said} »");
        }
    }

    #[test]
    fn a_refusal_comes_back_as_a_refusal_and_not_as_nonsense() {
        let said = format!("{VERSION} no l'accès distant est arrêté sur cet ordinateur");
        let Ok(Err(reason)) = Told::parse(&said) else {
            panic!("« {said} » n'est pas relu comme un refus");
        };
        assert!(reason.contains("accès distant"), "{reason}");
    }

    #[test]
    fn another_version_is_named_rather_than_misread() {
        // La moitié la plus ancienne du produit doit être nommée, pas
        // devinée : c'est la seule panne qui se répare en une phrase.
        let refusal = Question::parse("9 ports").unwrap_err();
        assert!(
            refusal.contains('9') && refusal.contains("version"),
            "{refusal}"
        );

        // Et la version 1, qui n'était pas du texte du tout, ne doit pas
        // passer pour une question valide.
        assert!(Question::parse("\u{1}\u{a4}\u{10}").is_err());
    }

    #[test]
    fn a_pairing_without_a_code_is_refused() {
        assert!(Question::parse(&format!("{VERSION} pair")).is_err());
        assert!(Question::parse(&format!("{VERSION} pair 0429")).is_err());
    }

    #[test]
    fn a_port_outside_the_engine_range_is_refused() {
        // Un port faux enverrait les ports locaux du client n'importe
        // où : mieux vaut le dire que les ouvrir et attendre.
        assert!(Told::parse(&format!("{VERSION} ports 80")).is_err());
        assert!(Told::parse(&format!("{VERSION} ports pas-un-nombre")).is_err());
    }

    #[test]
    fn a_reason_written_over_two_lines_arrives_whole() {
        // Un refus est écrit pour être lu, parfois sur plusieurs lignes.
        // Il voyage à plat et doit rester entièrement lisible.
        let folded = format!("{VERSION} no {}", shortened("deux\nlignes"));
        let Ok(Err(reason)) = Told::parse(&folded) else {
            panic!("« {folded} » n'est pas relu comme un refus");
        };
        assert_eq!(reason, "deux lignes");
    }

    #[test]
    fn a_reason_too_long_for_the_channel_is_shortened_rather_than_lost() {
        // Sans ça, le message dépasserait ce que le canal accepte et
        // l'autre ordinateur verrait une panne de transport là où il
        // devait lire une explication.
        for reason in [
            "é".repeat(600),
            "x".repeat(600),
            format!("{}é", "x".repeat(ROOM - 1)),
        ] {
            let message = format!("{VERSION} no {}", shortened(&reason));
            assert!(message.len() <= LIMIT, "{} octets", message.len());
            assert!(matches!(Told::parse(&message), Ok(Err(_))), "{message}");
        }

        // Et une raison qui tient n'est pas touchée.
        assert_eq!(
            shortened("le moteur n'attend aucun code"),
            "le moteur n'attend aucun code"
        );
    }
}
