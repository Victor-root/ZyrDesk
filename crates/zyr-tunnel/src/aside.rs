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
/// which it is the right size and nobody's desk. Version 9 added the far
/// computer's journal, which is the first thing here worth a page rather
/// than a line, and version 10 the emptying of it, without which reading
/// it means reading three weeks of unrelated lines. Version 11 asks what
/// the far computer can encode, so a menu stops offering what that
/// computer was never going to make.
pub const VERSION: u32 = 11;

/// Longest question this channel takes.
///
/// It carries port numbers, a four-digit code and a machine name.
/// Anything longer is not one of ours.
const LONGEST_QUESTION: usize = 512;

/// Longest answer this channel takes.
///
/// A ceiling protects whoever is listening from whoever is speaking, and
/// the two sides are not exposed to the same thing: this computer takes
/// questions from anyone it lets in, and answers only from the computer
/// it went to. One answer carries a whole journal, which is a page and
/// not a line, so the two ceilings part company here rather than a
/// question being allowed to weigh a page. Four times what a journal can
/// weigh at its very largest, its four files being read from the end and
/// cut.
const LONGEST_ANSWER: usize = 4 * 1024 * 1024;

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

    /// This computer's journal, gathered and handed over whole.
    ///
    /// Asked because the walk to the other machine is the errand a
    /// remote desktop exists to spare, and reading its journal is the
    /// one thing that still made anybody take it: a fault is diagnosed
    /// on both journals at once or on neither.
    ///
    /// Handed to whoever this computer already lets in, and to nobody
    /// else. That is not a small permission granted lightly; it is a
    /// smaller one than the permission those computers already hold,
    /// which is to take the screen, the keyboard and the mouse of this
    /// machine. A page of what it has written down is less than that.
    fn journal(&self) -> Result<String, String>;

    /// Empties this computer's journal.
    ///
    /// The other half of reading one, and useless without it. A fault is
    /// found by emptying both journals, doing the thing that goes wrong,
    /// and reading both: a page carrying three weeks of unrelated lines
    /// is a page nobody reads to the end. Doing that from one side only
    /// leaves the walk to the other machine exactly where it was.
    ///
    /// What it costs if somebody asks for it carelessly is what this
    /// computer had written down, which is why the person who asks is
    /// made to ask twice at the other end. It is still far less than
    /// what the same computer may already do here.
    fn empty_the_journal(&self) -> Result<(), String>;

    /// Which pictures this computer's engine can actually make.
    ///
    /// The codec is chosen by whoever is watching and encoded here, and
    /// this end is the only one that knows whether it can. Asking for one
    /// it cannot make breaks nothing: the two engines agree on another
    /// between themselves and the session opens. What was wrong is that
    /// nothing said so, and the menu over there went on showing a choice
    /// that had not been honoured since the session began.
    ///
    /// An empty answer is « it has not said » and never « none »: a
    /// computer that could encode nothing could not be watched at all, so
    /// that answer would be about the reading and not about the machine.
    fn codecs(&self) -> Result<String, String>;
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
    /// Hand over your journal, so it can be read from here.
    Journal,
    /// Empty your journal, so what comes after is only what comes after.
    EmptyTheJournal,
    /// Which pictures your engine can actually make.
    Codecs,
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
    /// The far computer's journal, whole.
    Journal {
        text: String,
    },
    /// The far computer's journal is empty.
    Emptied,
    /// What the far computer's engine can encode, in the product's own
    /// spelling, one name after another. Empty is « it has not said »
    /// and never « none »: a computer that could encode nothing could
    /// not be watched at all.
    Codecs {
        named: String,
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
            Question::Journal => write!(f, "{VERSION} journal"),
            Question::EmptyTheJournal => write!(f, "{VERSION} empty-journal"),
            Question::Codecs => write!(f, "{VERSION} codecs"),
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
            // Whole, lines and all: this channel ends a message by
            // closing the stream, so nothing here has to be folded onto
            // one line the way the control channel folds a refusal.
            Told::Journal { text } => write!(f, "{VERSION} journal {text}"),
            Told::Emptied => write!(f, "{VERSION} emptied"),
            Told::Codecs { named } => write!(f, "{VERSION} codecs {named}"),
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
            "journal" => Ok(Question::Journal),
            "empty-journal" => Ok(Question::EmptyTheJournal),
            "codecs" => Ok(Question::Codecs),
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
            "journal" => Ok(Ok(Told::Journal {
                text: rest.to_string(),
            })),
            "emptied" => Ok(Ok(Told::Emptied)),
            "codecs" => Ok(Ok(Told::Codecs {
                named: rest.to_string(),
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
///
/// Measured against what a question weighs and not against what an
/// answer may: a refusal is a sentence written to be read by a person,
/// and one that ran to a page would be a page nobody reads.
const ROOM: usize = LONGEST_QUESTION - 32;

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
        .read_to_end(LONGEST_ANSWER)
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

/// Asks the far ZyrDesk for its journal.
///
/// The one question here that is asked outside any session: reading the
/// journal of a computer nobody is watching is exactly the moment it is
/// wanted, since what is being looked for is usually why nobody can
/// watch it.
pub async fn ask_for_the_journal(connection: &Connection) -> io::Result<String> {
    match ask(connection, &Question::Journal).await? {
        Told::Journal { text } => Ok(text),
        other => Err(unreadable(format!("réponse hors sujet : {other}"))),
    }
}

/// Asks the far ZyrDesk to empty its journal.
///
/// The other half of reading one. A fault is found by emptying both
/// journals, doing the thing that goes wrong, and reading both; being
/// able to empty only one of the two leaves the walk to the other
/// machine exactly where it was.
pub async fn ask_to_empty_the_journal(connection: &Connection) -> io::Result<()> {
    match ask(connection, &Question::EmptyTheJournal).await? {
        Told::Emptied => Ok(()),
        other => Err(unreadable(format!("réponse hors sujet : {other}"))),
    }
}

/// Asks the far ZyrDesk which pictures its engine can make.
///
/// Asked while a session is open, since that is when it can be acted on:
/// the answer decides what the menu of that session may offer, and a
/// codec that computer cannot make has no business being offered.
pub async fn ask_what_it_can_encode(connection: &Connection) -> io::Result<String> {
    match ask(connection, &Question::Codecs).await? {
        Told::Codecs { named } => Ok(named),
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
        .read_to_end(LONGEST_QUESTION)
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
        // Off the thread as well: gathering a journal is four files read
        // from a disk, and a disk that has gone to sleep takes its time
        // about waking up.
        Question::Journal => tokio::task::spawn_blocking(move || answering.journal())
            .await
            .map_err(|e| format!("le journal n'a pas pu être rassemblé : {e}"))?
            .map(|text| Told::Journal { text }),
        // Off it too: emptying is four files opened and cut on a disk.
        Question::EmptyTheJournal => {
            tokio::task::spawn_blocking(move || answering.empty_the_journal())
                .await
                .map_err(|e| format!("le journal n'a pas pu être vidé : {e}"))?
                .map(|()| Told::Emptied)
        }
        // And off it as well: the answer is read from what the engine
        // wrote down when it started, which is a file on a disk.
        Question::Codecs => tokio::task::spawn_blocking(move || answering.codecs())
            .await
            .map_err(|e| format!("les codecs n'ont pas pu être lus : {e}"))?
            .map(|named| Told::Codecs { named }),
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
            Question::Journal,
            Question::EmptyTheJournal,
            Question::Codecs,
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
            // Un journal voyage entier, lignes comprises : ce canal
            // termine un message en fermant le flux, donc rien n'a
            // besoin d'être replié sur une ligne.
            Told::Journal {
                text: "ZyrDesk 0.1.0\nOrdinateur       : PC de Victor\n\n--- Le service ---\nune \
                       ligne\nune autre"
                    .to_string(),
            },
            Told::Emptied,
            Told::Codecs {
                named: "H.264 HEVC".to_string(),
            },
            // Rien dit n'est pas « aucun » : il faut que les deux
            // traversent le canal sans se confondre.
            Told::Codecs {
                named: String::new(),
            },
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
        // La moitié du produit qui ne parle pas la même version doit être
        // nommée, pas devinée : c'est la seule panne qui se répare en une
        // phrase. Comptée depuis la version courante, pour que ce test ne
        // se mette pas à parler de la version du jour à chaque fois qu'on
        // en ajoute une.
        let autre = VERSION + 1;
        let refusal = Question::parse(&format!("{autre} ports")).unwrap_err();
        assert!(
            refusal.contains(&autre.to_string()) && refusal.contains("version"),
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
            assert!(
                message.len() <= LONGEST_QUESTION,
                "{} octets",
                message.len()
            );
            assert!(matches!(Told::parse(&message), Ok(Err(_))), "{message}");
        }

        // Et une raison qui tient n'est pas touchée.
        assert_eq!(
            shortened("le moteur n'attend aucun code"),
            "le moteur n'attend aucun code"
        );
    }
}
