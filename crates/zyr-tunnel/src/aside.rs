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
use zyr_proto::session::WantedScreen;
use zyr_transport::{Connection, RecvStream, SendStream};

use crate::channel::StreamChannel;
use crate::pump;

/// Version of this dialect.
///
/// Version 1 was three bytes carrying nothing but the ports. Version 2
/// added the pairing code. Version 3 added the one keystroke no keyboard
/// can carry. Version 4 added the far computer's speakers. Version 7
/// added the virtual screen, which now sleeps between sessions and has
/// to be asked for. Version 8 added how large that screen draws, without
/// which it is the right size and nobody's desk.
pub const VERSION: u32 = 8;

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

    /// Presses Ctrl+Alt+Suppr on this computer.
    ///
    /// It travels here and not through the engines, and that is not a
    /// preference. Windows reserves this one combination for itself at
    /// both ends: the computer watching never sees it, because its own
    /// Windows takes it first, and the computer being watched could not
    /// be made to feel it, because the way an engine types is the way
    /// Windows refuses for this. The one door is a call reserved for
    /// programs the system trusts, and on the host that is our own
    /// service. So the ask crosses on the product's own channel, between
    /// the two halves of ZyrDesk, and no engine is any the wiser.
    fn secure_attention(&self) -> Result<(), String>;

    /// Silences this computer's speakers for as long as the session
    /// lasts, or lets them play again.
    ///
    /// Asked by whoever is watching and never decided here. A person
    /// taking control of a machine in another room is the only one who
    /// knows whether that room should go quiet, and they are not in it
    /// to walk over and say so. What this end owes in return is that the
    /// sound comes back when the session goes, whatever became of the
    /// computer that asked.
    fn hush_the_speakers(&self, quiet: bool) -> Result<(), String>;

    /// Puts this computer's lock screen up.
    ///
    /// The other half of Ctrl+Alt+Suppr and the mirror of it. That one
    /// only a service may press; this one only a program sitting on the
    /// interactive desktop may ask for. The two refusals are the same
    /// idea seen from both sides: a lock screen nobody can raise or
    /// lower from outside the desk is a lock screen worth something.
    fn lock_the_screen(&self) -> Result<(), String>;

    /// Decides whether this computer resends a still screen at full rate
    /// while somebody is watching it.
    ///
    /// Asked from the far end and not settled here, for the same reason
    /// the speakers are: the only person who can tell whether the picture
    /// feels smooth is the one looking at it, and they are not in front
    /// of the machine that would have to be told. What it costs is paid
    /// here, so the ask is a request and not an order: an answer of no is
    /// an answer.
    ///
    /// The engine reads it when it starts, so saying yes to a change
    /// starts that engine over.
    fn serve_steady(&self, rate: bool) -> Result<(), String>;

    /// Wakes this computer's virtual screen for a session that wants a
    /// picture like that one, or puts it back to sleep.
    ///
    /// The virtual screen is what lets a computer be asked for a picture
    /// its own screen could not draw. It sleeps whenever no session wants
    /// it, and that is the whole point of asking: a machine nobody is
    /// looking at has the screens its owner plugged in and no others, and
    /// a second screen sitting on somebody's desk all day is not
    /// something a remote desktop is entitled to leave behind.
    ///
    /// Asked at the opening of a session and answered before the picture
    /// is opened, because the far engine has to find that screen. A no is
    /// an answer and never fails a session: a computer with no virtual
    /// screen serves what its own screen can draw, which is what every
    /// computer did before this existed.
    ///
    /// Answers the size this computer is going to be showing, which is
    /// the size that was asked for when there is a virtual screen to
    /// wake, and this machine's own when there is not or when none was
    /// wanted. That answer is the whole of what makes « leave that
    /// computer as it is » possible: nothing at the other end can know
    /// what is plugged in here.
    ///
    /// How large that screen draws comes with the size and is honoured
    /// with it, since the two are one ask: a screen the size of the panel
    /// somebody is watching, drawn the way that panel draws. Nought names
    /// none, and takes what this computer's own Windows recommends.
    fn screen_for_a_session(
        &self,
        wanted: Option<WantedScreen>,
    ) -> Result<Option<(u32, u32)>, String>;
}

/// What one ZyrDesk asks the other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Question {
    /// Which ports your engine listens on.
    Ports,
    /// Take this code and hand it to your engine. The name is what the
    /// far computer will file this one under.
    Pair { pin: String, name: String },
    /// Press Ctrl+Alt+Suppr on yourself.
    SecureAttention,
    /// Go quiet, or play again, for as long as this session lasts.
    Hush { quiet: bool },
    /// Put your lock screen up.
    Lock,
    /// Resend a still screen at full rate, or stop doing it.
    Steady { rate: bool },
    /// Wake your virtual screen for a picture like this one, or, with
    /// nothing asked for, put it back to sleep.
    Screen { wanted: Option<WantedScreen> },
}

/// What comes back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Told {
    Ports(EnginePorts),
    /// The far engine took the code.
    Paired,
    /// The far computer pressed it.
    Attended,
    /// The far computer's speakers are as they were asked to be.
    Hushed,
    /// The far computer's screen is being locked.
    Locked,
    /// The far computer serves the way it was asked to.
    Steady,
    /// The far computer's virtual screen is where it was asked to be,
    /// and it is showing this size. Absent when that computer could not
    /// measure itself, which leaves the asking end on what it guessed.
    Screen {
        size: Option<(u32, u32)>,
    },
}

impl fmt::Display for Question {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // The name comes last and takes the whole of what is left,
            // spaces and all: no escaping, and nothing to get wrong.
            Question::Pair { pin, name } => write!(f, "{VERSION} pair {pin} {name}"),
            Question::Ports => write!(f, "{VERSION} ports"),
            Question::SecureAttention => write!(f, "{VERSION} sas"),
            Question::Lock => write!(f, "{VERSION} lock"),
            Question::Steady { rate } => {
                write!(f, "{VERSION} steady {}", if *rate { "on" } else { "off" })
            }
            Question::Screen { wanted } => match wanted {
                Some(screen) => write!(f, "{VERSION} screen {screen}"),
                None => write!(f, "{VERSION} screen none"),
            },
            Question::Hush { quiet } => {
                write!(
                    f,
                    "{VERSION} hush {}",
                    if *quiet { "quiet" } else { "play" }
                )
            }
        }
    }
}

impl fmt::Display for Told {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Told::Ports(engine) => write!(f, "{VERSION} ports {}", engine.base()),
            Told::Paired => write!(f, "{VERSION} paired"),
            Told::Attended => write!(f, "{VERSION} attended"),
            Told::Hushed => write!(f, "{VERSION} hushed"),
            Told::Locked => write!(f, "{VERSION} locked"),
            Told::Steady => write!(f, "{VERSION} steady"),
            Told::Screen { size } => match size {
                Some((wide, high)) => write!(f, "{VERSION} screen {wide}x{high}"),
                None => write!(f, "{VERSION} screen none"),
            },
        }
    }
}

impl Question {
    fn parse(message: &str) -> Result<Self, String> {
        let said = after_the_version(message)?;
        let (verb, rest) = split_first(said);
        match verb {
            "ports" => Ok(Question::Ports),
            "sas" => Ok(Question::SecureAttention),
            "lock" => Ok(Question::Lock),
            "steady" => match rest {
                "on" => Ok(Question::Steady { rate: true }),
                "off" => Ok(Question::Steady { rate: false }),
                other => Err(format!("« {other} » ne dit ni oui ni non")),
            },
            "screen" => match rest {
                "none" => Ok(Question::Screen { wanted: None }),
                asked => asked.parse().map(|screen| Question::Screen {
                    wanted: Some(screen),
                }),
            },
            "hush" => match rest {
                "quiet" => Ok(Question::Hush { quiet: true }),
                "play" => Ok(Question::Hush { quiet: false }),
                other => Err(format!("« {other} » ne dit ni de se taire ni de jouer")),
            },
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
            "attended" => Ok(Ok(Told::Attended)),
            "hushed" => Ok(Ok(Told::Hushed)),
            "locked" => Ok(Ok(Told::Locked)),
            "steady" => Ok(Ok(Told::Steady)),
            "screen" => Ok(Ok(Told::Screen {
                size: match rest {
                    "none" | "" => None,
                    said => Some(
                        zyr_proto::session::parse_resolution(said)
                            .map_err(|e| unreadable(e.to_string()))?,
                    ),
                },
            })),
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

/// Asks the far ZyrDesk to press Ctrl+Alt+Suppr on itself.
pub async fn ask_for_the_secure_attention(connection: &Connection) -> io::Result<()> {
    match ask(connection, &Question::SecureAttention).await? {
        Told::Attended => Ok(()),
        other => Err(unreadable(format!("réponse hors sujet : {other}"))),
    }
}

/// Asks the far ZyrDesk to silence its own speakers, or to let them
/// play again.
///
/// Asked from here because the choice belongs here. Whoever takes
/// control of a computer in another room is the one who knows that the
/// room should go quiet, and a setting on that far machine would have to
/// be walked over to, which is the one thing remote control is for.
pub async fn ask_to_hush(connection: &Connection, quiet: bool) -> io::Result<()> {
    match ask(connection, &Question::Hush { quiet }).await? {
        Told::Hushed => Ok(()),
        other => Err(unreadable(format!("réponse hors sujet : {other}"))),
    }
}

/// Asks the far ZyrDesk to put its lock screen up.
///
/// The nearest thing there is to Windows+L on the far computer, and it
/// exists because that combination itself cannot travel: Windows keeps it
/// where no program can reach it, at both ends. So the ask goes round by
/// the product's own channel, and the far service raises the screen from
/// the one place its Windows will take that order.
pub async fn ask_to_lock(connection: &Connection) -> io::Result<()> {
    match ask(connection, &Question::Lock).await? {
        Told::Locked => Ok(()),
        other => Err(unreadable(format!("réponse hors sujet : {other}"))),
    }
}

/// Asks the far ZyrDesk to resend a still screen at full rate, or to
/// stop doing it.
///
/// Asked at the opening of every session and never in the middle of one:
/// the far engine reads this when it starts, so a change of it starts
/// that engine over, and an engine starting over in the middle of a
/// session is that session going.
pub async fn ask_to_serve_steady(connection: &Connection, rate: bool) -> io::Result<()> {
    match ask(connection, &Question::Steady { rate }).await? {
        Told::Steady => Ok(()),
        other => Err(unreadable(format!("réponse hors sujet : {other}"))),
    }
}

/// Asks the far ZyrDesk to wake its virtual screen for a picture like
/// that one, or, with nothing asked for, to leave its own screen alone.
///
/// Answered before the picture is opened and not alongside it: the far
/// engine has to find that screen, and it can only find one that is
/// already there.
///
/// Answers the size that computer will be showing, which is what makes
/// « leave it as it is » possible at all: nothing this end knows says
/// what is plugged in over there.
pub async fn ask_for_a_screen(
    connection: &Connection,
    wanted: Option<WantedScreen>,
) -> io::Result<Option<(u32, u32)>> {
    match ask(connection, &Question::Screen { wanted }).await? {
        Told::Screen { size } => Ok(size),
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
        // Off the thread that carries the tunnel, like the pairing above
        // and for the same reason: pressing this starts a program in
        // another Windows session and waits for it, which is a long time
        // to hold a channel every other session is queueing behind.
        Question::SecureAttention => {
            tokio::task::spawn_blocking(move || answering.secure_attention())
                .await
                .map_err(|e| format!("la frappe n'a pas pu être menée : {e}"))?
                .map(|()| Told::Attended)
        }
        // Off that thread too: silencing a machine's speakers means
        // starting a program in the session that owns its screen and
        // waiting for it to come back.
        Question::Hush { quiet } => {
            tokio::task::spawn_blocking(move || answering.hush_the_speakers(quiet))
                .await
                .map_err(|e| format!("les enceintes n'ont pas pu être touchées : {e}"))?
                .map(|()| Told::Hushed)
        }
        // And off it again: locking means starting a program in the
        // session that owns the screen and waiting for it.
        Question::Lock => tokio::task::spawn_blocking(move || answering.lock_the_screen())
            .await
            .map_err(|e| format!("le verrouillage n'a pas pu être mené : {e}"))?
            .map(|()| Told::Locked),
        // Off the thread as well: saying yes writes a file and starts an
        // engine over.
        Question::Steady { rate } => {
            tokio::task::spawn_blocking(move || answering.serve_steady(rate))
                .await
                .map_err(|e| format!("la cadence n'a pas pu être réglée : {e}"))?
                .map(|()| Told::Steady)
        }
        // Off it too, and this one takes the longest of them all: waking
        // a screen is Windows starting a device, and the answer is not
        // sent until it has, because the computer asking opens its
        // picture on it.
        Question::Screen { wanted } => {
            tokio::task::spawn_blocking(move || answering.screen_for_a_session(wanted))
                .await
                .map_err(|e| format!("l'écran n'a pas pu être préparé : {e}"))?
                .map(|size| Told::Screen { size })
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
            Question::SecureAttention,
            Question::Hush { quiet: true },
            Question::Hush { quiet: false },
            Question::Lock,
            Question::Steady { rate: true },
            Question::Steady { rate: false },
            // L'agrandissement voyage collé à la taille : un écran à la
            // bonne taille sans lui, c'est le bureau de quelqu'un
            // d'autre à la bonne résolution.
            Question::Screen {
                wanted: Some(WantedScreen {
                    wide: 1920,
                    high: 1200,
                    scale: 125,
                }),
            },
            // Zéro veut dire « aucun demandé » : c'est ce que dit une
            // session qui n'a pas pu mesurer son propre écran.
            Question::Screen {
                wanted: Some(WantedScreen {
                    wide: 3840,
                    high: 2160,
                    scale: 0,
                }),
            },
            Question::Screen { wanted: None },
        ] {
            let said = question.to_string();
            assert_eq!(Question::parse(&said), Ok(question), "sur « {said} »");
        }
    }

    #[test]
    fn every_answer_survives_the_round_trip() {
        for told in [
            Told::Ports(ports(42000)),
            Told::Paired,
            Told::Attended,
            Told::Hushed,
            Told::Locked,
            Told::Steady,
            Told::Screen {
                size: Some((1920, 1200)),
            },
            Told::Screen { size: None },
        ] {
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
