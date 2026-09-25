//! ZyrDesk's own channel inside the tunnel.
//!
//! Everything said here is the product talking to itself, never the
//! engine. The first word of every session is said here, the one that
//! has the watched computer bring its engine up, and so is everything
//! the engine has no business carrying: the keys Windows keeps for
//! itself, the speakers, the screens, the journal, the clipboard.
//!
//! One question, one stream, one message each way, in plain text: a
//! channel that can be read with the eyes is a channel that can be
//! diagnosed. Every message opens with the version of this dialect, so
//! two halves of the product installed at different times say so rather
//! than misread each other.

use std::fmt;
use std::io;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use zyr_proto::clipboard::{Clip, Stamp};
use zyr_proto::log::Log;
use zyr_proto::session::WantedScreen;
use zyr_transport::{Connection, MediaProfile, RecvStream, SendStream};

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
/// computer was never going to make. Version 12 asks which screens that
/// computer has and which of them to be served from, a machine with two
/// of them having had no way to offer the second. Version 13 has the
/// cadence of a still screen answered like the screen to film, « already
/// » or « starting over »: it is read once by the far engine too, and a
/// session that asked for it and then opened its picture through the way
/// that was about to go fell over on its first picture. Version 14 asks
/// for a rate while the picture runs, which the far engine takes where it
/// stands, and has the cadence of a still screen taken the same way: what
/// used to start an engine over is asked of the one that runs. Version 15
/// has a session say what it will be served, in its very first word: the
/// computer being watched opens its tunnel when its service starts, long
/// before anybody asks it for a picture, and held a window worked out
/// from a nominal rate whatever the session actually ran at. Version 16
/// asks what shape the far pointer has, which is the first thing here
/// asked several times a second: a desktop says what a click is about to
/// do through that shape and nothing else, the engines carry none of
/// them, and the pointer drawn where the hand actually is had until now
/// no way of being anything but an arrow. Version 17 says whether the far
/// engine draws that pointer into the picture, instead of throwing the
/// key combination that toggles it: a toggle nobody can read, living in
/// an engine that outlives every session and is shared by all of them,
/// left a session turning it the wrong way while believing the opposite,
/// and a screen with two pointers or none. Version 18 asks for what a
/// computer has measured of its own access to the Internet, the one
/// question here worth asking of a computer nobody can even open a
/// session with: it is a second, independent trace, kept apart from the
/// journal, and reading it from the far end is the same errand a remote
/// desktop already exists to spare. Version 19 carries what somebody
/// copied, which the engines' protocol has no channel for and never will:
/// it is the one question here that goes both ways at once, the same
/// message handing over what was copied on this side and asking for what
/// was copied on the other. Version 20 carries the files themselves,
/// piece by piece and only once somebody pastes them: what a clipboard
/// holds of a file is its name, so the names cross at once and weigh
/// nothing, and the bytes follow a piece at a time in whichever direction
/// they are wanted. Version 21 lets the journal be asked for sifted: what
/// somebody typed in the box travels with the question, so that the far
/// computer keeps the lines that answer it before it cuts its journal
/// down to a page, which is the only order in which a sift is worth
/// anything. Version 22 is the product's own engine: the first word of a
/// session opens it rather than asking where one listens, the pairing
/// code is gone with the engines that wanted one, and the rate, the
/// cadence of a still screen, the pointer drawn into the picture and the
/// codecs travel between the player and the engine and no longer here.
pub const VERSION: u32 = 22;

/// Longest question this channel takes.
///
/// It carries a rate, a size and a screen's name. Anything longer is not
/// one of ours, with the one exception below.
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

/// Longest a message carrying a page may be, in either direction.
///
/// Two questions are pages rather than lines, and their answers are
/// pages too. What somebody copied has to travel from whichever of the
/// two computers they copied it on, so the asking end pushes and the
/// answering end answers, and the two ends of the same exchange are
/// allowed the same weight.
///
/// It is read only once the question has named itself, which is what
/// keeps the line above a line for everything else: see `a_question`. And
/// it is larger than what either of them may weigh, because both travel
/// written in base64, which costs a third of it again.
const LONGEST_PAGE: usize = 8 * 1024 * 1024;

/// The two verbs whose messages are allowed that weight.
///
/// Written once and read in three places each: where a question is
/// spelled, where it is read back, and where the ceiling is decided on
/// the first few bytes of it.
const CLIPBOARD: &str = "clipboard";
const PIECES: &str = "pieces";

/// How much of a file travels in one message.
///
/// One piece is asked for and answered at a time, and that is the whole
/// of what keeps a file from eating the session it travels beside: there
/// is never more than one piece of it in the pipe, whatever the file
/// weighs and however fast the link is. Nothing has to be rationed,
/// because nothing is ever asked for twice over.
///
/// Large enough that a fast link is not spending its time waiting for the
/// next ask, small enough that the picture never waits behind it: at a
/// tenth of a second of round trip, which is a bad link, this is still
/// two and a half megabytes a second.
pub const A_PIECE: usize = 256 * 1024;

/// What a message says where a clip or a stamp could have been and there
/// is none.
///
/// A word and not an absent field, for the reason the rest of this
/// channel uses one: a message that names what it does not carry and a
/// message that lost a piece on the way must not look alike.
const NONE: &str = "none";

/// How long a refusal is given to reach the far end before its
/// connection goes: a few round trips of the worst road a session runs
/// on.
const REFUSAL_DELIVERED: Duration = Duration::from_secs(2);

/// What the host side answers on ZyrDesk's own channel.
///
/// Whoever holds the engine of a session hands over this and keeps the
/// engine to itself, which is what stops the tunnel from having to know
/// how an engine is driven. Everything but the pointer is called where
/// blocking is allowed.
///
/// The first word of a session is not among these: it is handed back
/// whole by [`until_a_session_opens`], since what answers it is the
/// engine being brought up, and the tunnel is only started once it is.
pub trait Answers: Send + Sync + 'static {
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
    /// is opened, because the picture is made at the size this answers. A
    /// no is an answer and never fails a session: a computer with no
    /// virtual screen serves what its own screen can draw, which is what
    /// every computer did before this existed.
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
    fn journal(&self, sift: &str) -> Result<String, String>;

    /// What this computer has measured of its own access to the
    /// Internet, gathered and handed over whole.
    ///
    /// Kept apart from the journal on purpose: it holds one measurement
    /// a second, and folded into the journal it would drown the very
    /// thing that page exists to be read in one sitting. Asked for the
    /// same reason the journal is: the walk to the other machine is the
    /// errand a remote desktop exists to spare, and a session too
    /// unwell to open is exactly the moment nobody can make that walk
    /// through it.
    fn reach_log(&self) -> Result<String, String>;

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

    /// What shape the pointer has on this computer right now.
    ///
    /// A desktop says what it is about to do through this and almost
    /// nothing else: an upright bar means the click lands in text, a hand
    /// means a link, a ring means wait. That shape is drawn into the
    /// picture by the engine here, so the computer watching sees it a
    /// network away from where its own hand is; drawing its own pointer
    /// instead buys back the whole of that delay and loses every shape,
    /// since nothing in what the engines speak carries one.
    ///
    /// So it travels here instead, as the one word that names it. Asked
    /// often, several times a second while a hand is moving, so the
    /// answer is a reading and never an errand: nothing is started, moved
    /// or written to answer it.
    ///
    /// Read on the desktop that owns the screen and the keyboard rather
    /// than on the one this service happens to sit on, which has no
    /// pointer at all. That desktop changes under a machine being locked
    /// or asking for a password, and the answer follows it.
    fn pointer(&self) -> Result<zyr_proto::session::Pointer, String>;

    /// Which screens this computer is showing on, one to a line.
    ///
    /// Only the ones it is actually showing on: a screen that is switched
    /// off, and the one it grows for itself between sessions, are not
    /// screens anybody asks to be served from, and offering them would
    /// offer a black picture.
    ///
    /// Named by this computer and by nothing else: the far end reads the
    /// list and hands one of the names back. Empty is « it has not said »,
    /// which is an engine that has not finished starting.
    fn screens(&self) -> Result<String, String>;

    /// Serves this computer's picture from that screen from now on.
    ///
    /// Nothing named means the main one, which is what every session asks
    /// for until somebody says otherwise: a machine that went on serving
    /// the screen a previous session picked would be a machine rearranged
    /// by having been looked at.
    ///
    /// The engine of the session changes screen where it stands: nothing
    /// starts over, and the picture is on the other screen within the
    /// second.
    fn film_this_screen(&self, id: Option<String>) -> Result<(), String>;

    /// Takes what was copied over there, and hands back what was copied
    /// here.
    ///
    /// The one thing on this channel that travels both ways in one
    /// message, and it has to: a clipboard is shared or it is not, and
    /// which of the two computers somebody copied on is not something
    /// either end gets to decide. Whoever is watching asks; this end
    /// takes what came with the question and answers with its own.
    ///
    /// `seen` is the stamp of what the asking end believes both
    /// clipboards hold. Nothing is handed back when this computer holds
    /// that very thing, which is almost every turn: a clipboard is asked
    /// about several times a second and changes a few times an hour.
    ///
    /// Nothing is also handed back when this computer holds nothing at
    /// all, and that is not the same statement dressed up: an empty
    /// clipboard here must not empty the one over there. What is never
    /// said is « I have nothing, drop yours ».
    fn clipboard(&self, pushing: Option<Clip>, seen: Option<Stamp>)
    -> Result<Option<Clip>, String>;

    /// Takes a piece of a file that was copied over there, and hands back
    /// a piece of one that was copied here.
    ///
    /// Both ways in one message, exactly like the clipboard above, and
    /// for exactly the same reason: whichever of the two computers
    /// somebody copied files on, it is the other one they may paste them
    /// on, and only the side that opened the way can ask anything at all.
    ///
    /// `asking` is a piece this end is to hand over, out of the files its
    /// own clipboard named. `giving` is a piece of what the far
    /// clipboard named, arriving because this end asked for it.
    ///
    /// What comes back is that piece, and what this end wants next: a
    /// paste in progress here says so by wanting the piece after the one
    /// it was just given, and says it is done by wanting nothing.
    fn pieces(
        &self,
        asking: Option<Wanted>,
        giving: Option<Given>,
    ) -> Result<(Option<Given>, Option<Wanted>), String>;
}

/// A piece of a file somebody copied, asked for.
///
/// The file is named by its rank in the listing that crossed and never by
/// its path: a rank cannot be made to mean another file, where a path
/// handed over by the far computer is a path this one would have to check
/// all over again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wanted {
    pub rank: u32,
    pub from: u64,
    pub how_many: u32,
}

impl fmt::Display for Wanted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} {}", self.rank, self.from, self.how_many)
    }
}

impl Wanted {
    fn read(said: &str) -> Result<Self, String> {
        let mut pieces = said.split_whitespace();
        let mut number = |what: &str| {
            pieces
                .next()
                .and_then(|said| said.parse::<u64>().ok())
                .ok_or_else(|| format!("un morceau de fichier sans {what}"))
        };
        let rank = number("rang")?;
        let from = number("départ")?;
        let how_many = number("longueur")?;
        Ok(Self {
            rank: rank
                .try_into()
                .map_err(|_| "un rang hors de tout".to_string())?,
            from,
            how_many: how_many
                .try_into()
                .map_err(|_| "une longueur hors de tout".to_string())?,
        })
    }
}

/// A piece of a file, handed over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Given {
    pub rank: u32,
    pub from: u64,
    pub bytes: Vec<u8>,
}

impl fmt::Display for Given {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} {}",
            self.rank,
            self.from,
            BASE64.encode(&self.bytes)
        )
    }
}

impl Given {
    fn read(said: &str) -> Result<Self, String> {
        let mut pieces = said.trim().splitn(3, char::is_whitespace);
        let mut number = |what: &str| {
            pieces
                .next()
                .and_then(|said| said.parse::<u64>().ok())
                .ok_or_else(|| format!("un morceau de fichier sans {what}"))
        };
        let rank = number("rang")?;
        let from = number("départ")?;
        let bytes = BASE64
            .decode(pieces.next().unwrap_or("").trim())
            .map_err(|_| "un morceau de fichier illisible".to_string())?;
        Ok(Self {
            rank: rank
                .try_into()
                .map_err(|_| "un rang hors de tout".to_string())?,
            from,
            bytes,
        })
    }
}

/// One half of a message about pieces of files, or the word that says
/// there is none.
///
/// Written and read once for the four places it appears: both halves of
/// the question and both halves of the answer.
fn half<T: fmt::Display>(named: &str, what: &Option<T>) -> String {
    match what {
        Some(what) => format!("{named} {what}"),
        None => format!("{named} {NONE}"),
    }
}

/// Splits a message into its two named halves.
///
/// The names are there so that a message missing a half and a message
/// that lost one on the way do not look alike, which is the rule the rest
/// of this channel follows.
fn halves<'a>(said: &'a str, first: &str, second: &str) -> Result<(&'a str, &'a str), String> {
    let said = said.trim();
    let rest = said
        .strip_prefix(first)
        .ok_or_else(|| format!("un message qui ne dit pas « {first} »"))?;
    let (before, after) = rest
        .split_once(second)
        .ok_or_else(|| format!("un message qui ne dit pas « {second} »"))?;
    Ok((before.trim(), after.trim()))
}

/// Reads a half that may say there is nothing.
fn some_of<T>(
    said: &str,
    read: impl FnOnce(&str) -> Result<T, String>,
) -> Result<Option<T>, String> {
    match said {
        NONE | "" => Ok(None),
        carried => read(carried).map(Some),
    }
}

/// What one ZyrDesk asks the other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Question {
    /// Open a session: bring your engine up for a picture served like
    /// that.
    ///
    /// The shape of the picture travels with the question because the
    /// window the far computer holds open is worked out from it, and this
    /// is the first word of every session.
    Open { serving: MediaProfile },
    /// Press Ctrl+Alt+Suppr on yourself.
    SecureAttention,
    /// Go quiet, or play again, for as long as this session lasts.
    Hush { quiet: bool },
    /// Put your lock screen up.
    Lock,
    /// Wake your virtual screen for a picture like this one, or, with
    /// nothing asked for, put it back to sleep.
    Screen { wanted: Option<WantedScreen> },
    /// Hand over your journal, so it can be read from here.
    ///
    /// Sifted through what is carried with the question, and sifted
    /// there rather than here: only the end of each file reaches a page,
    /// so lines dropped before the cut are lines that would never have
    /// crossed at all. Empty asks for the whole of it.
    Journal { sift: String },
    /// Hand over what you have measured of your own access to the
    /// Internet, so it can be read from here.
    ReachLog,
    /// Empty your journal, so what comes after is only what comes after.
    EmptyTheJournal,
    /// What shape your pointer has right now.
    Pointer,
    /// Which screens you are showing on.
    Screens,
    /// Serve your picture from that screen, or, with nothing named, from
    /// your main one.
    FilmThisScreen { id: Option<String> },
    /// Here is what was copied on this computer, if anything new was;
    /// hand back what was copied on yours, unless it is the one I say I
    /// already have.
    Clipboard {
        pushing: Option<Clip>,
        seen: Option<Stamp>,
    },
    /// Hand me that piece of a file you named, and here is a piece of one
    /// I named; tell me what you want next.
    Pieces {
        asking: Option<Wanted>,
        giving: Option<Given>,
    },
}

/// What comes back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Told {
    /// The far computer's engine is up, and waiting for the player.
    Opened,
    /// The far computer pressed it.
    Attended,
    /// The far computer's speakers are as they were asked to be.
    Hushed,
    /// The far computer's screen is being locked.
    Locked,
    /// The far computer's virtual screen is where it was asked to be,
    /// and it is showing this size. Absent when that computer could not
    /// measure itself, which leaves the asking end on what it guessed.
    Screen { size: Option<(u32, u32)> },
    /// The far computer's journal, whole.
    Journal { text: String },
    /// What the far computer has measured of its own access to the
    /// Internet, whole.
    ReachLog { text: String },
    /// The far computer's journal is empty.
    Emptied,
    /// The shape this computer's pointer has right now.
    Pointer { shape: zyr_proto::session::Pointer },
    /// The screens the far computer is showing on, one to a line. Empty
    /// is « it has not said », which is a computer whose engine has not
    /// finished starting.
    Screens { listed: String },
    /// What was copied on the far computer, or nothing when it holds the
    /// very thing the question said it already had, and nothing again
    /// when it holds nothing at all.
    Clipboard { theirs: Option<Clip> },
    /// The piece that was asked for, and what the far computer wants next
    /// of what this one named.
    Pieces {
        given: Option<Given>,
        wanted: Option<Wanted>,
    },
    /// The far computer's engine films the screen it was asked to.
    Filming,
}

impl fmt::Display for Question {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // In the words the rest of the product uses for them:
            // kilobits a second, and pictures a second.
            Question::Open { serving } => write!(
                f,
                "{VERSION} open {} {}",
                serving.bits_per_second / 1_000,
                serving.frames_per_second
            ),
            Question::SecureAttention => write!(f, "{VERSION} sas"),
            Question::Lock => write!(f, "{VERSION} lock"),
            Question::Screen { wanted } => match wanted {
                Some(screen) => write!(f, "{VERSION} screen {screen}"),
                None => write!(f, "{VERSION} screen none"),
            },
            Question::Journal { sift } => write!(f, "{VERSION} journal {sift}"),
            Question::ReachLog => write!(f, "{VERSION} reach"),
            Question::EmptyTheJournal => write!(f, "{VERSION} empty-journal"),
            Question::Pointer => write!(f, "{VERSION} pointer"),
            Question::Screens => write!(f, "{VERSION} screens"),
            // « main » rather than nothing at all, so a question that
            // names no screen still reads as a question: the identifiers
            // themselves are device paths, full of backslashes, and none
            // of them can be that word.
            Question::FilmThisScreen { id } => match id {
                Some(id) => write!(f, "{VERSION} film {id}"),
                None => write!(f, "{VERSION} film main"),
            },
            Question::Hush { quiet } => {
                write!(
                    f,
                    "{VERSION} hush {}",
                    if *quiet { "quiet" } else { "play" }
                )
            }
            // What is already shared first, because it is one word and
            // whatever is being pushed is a page: the reading below takes
            // the two words that name the message, then the one that says
            // what is shared, then the whole of the rest.
            Question::Clipboard { pushing, seen } => write!(
                f,
                "{VERSION} {CLIPBOARD} {} {}",
                match seen {
                    Some(stamp) => stamp.to_string(),
                    None => NONE.to_string(),
                },
                carried(pushing)
            ),
            Question::Pieces { asking, giving } => write!(
                f,
                "{VERSION} {PIECES} {} {}",
                half("asking", asking),
                half("giving", giving)
            ),
        }
    }
}

impl fmt::Display for Told {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Told::Opened => write!(f, "{VERSION} opened"),
            Told::Attended => write!(f, "{VERSION} attended"),
            Told::Hushed => write!(f, "{VERSION} hushed"),
            Told::Locked => write!(f, "{VERSION} locked"),
            Told::Screen { size } => match size {
                Some((wide, high)) => write!(f, "{VERSION} screen {wide}x{high}"),
                None => write!(f, "{VERSION} screen none"),
            },
            // Whole, lines and all: this channel ends a message by
            // closing the stream, so nothing here has to be folded onto
            // one line the way the control channel folds a refusal.
            Told::Journal { text } => write!(f, "{VERSION} journal {text}"),
            // Whole as well, and for the same reason.
            Told::ReachLog { text } => write!(f, "{VERSION} reach {text}"),
            Told::Emptied => write!(f, "{VERSION} emptied"),
            Told::Pointer { shape } => write!(f, "{VERSION} pointer {shape}"),
            // One screen to a line, whole, for the reason the journal
            // above travels whole: this channel ends a message by closing
            // the stream.
            Told::Screens { listed } => write!(f, "{VERSION} screens {listed}"),
            // Whole, like the journal above: this channel ends a message
            // by closing the stream, so a page needs no folding.
            Told::Clipboard { theirs } => {
                write!(f, "{VERSION} {CLIPBOARD} {}", carried(theirs))
            }
            Told::Pieces { given, wanted } => write!(
                f,
                "{VERSION} {PIECES} {} {}",
                half("given", given),
                half("wanted", wanted)
            ),
            Told::Filming => write!(f, "{VERSION} filming"),
        }
    }
}

impl Question {
    /// How long the answer to this question may be.
    ///
    /// One ceiling for all of them and one exception, which is the
    /// clipboard: a clip going the other way is the same page as a clip
    /// coming this way, and a channel that let one through and not the
    /// other would share a clipboard in one direction only.
    fn longest_answer(&self) -> usize {
        match self {
            Question::Clipboard { .. } | Question::Pieces { .. } => LONGEST_PAGE,
            _ => LONGEST_ANSWER,
        }
    }

    fn parse(message: &str) -> Result<Self, String> {
        let said = after_the_version(message)?;
        let (verb, rest) = split_first(said);
        match verb {
            "open" => served(rest).map(|serving| Question::Open { serving }),
            "sas" => Ok(Question::SecureAttention),
            "lock" => Ok(Question::Lock),
            "screen" => match rest {
                "none" => Ok(Question::Screen { wanted: None }),
                asked => asked.parse().map(|screen| Question::Screen {
                    wanted: Some(screen),
                }),
            },
            "journal" => Ok(Question::Journal {
                sift: rest.to_string(),
            }),
            "reach" => Ok(Question::ReachLog),
            "empty-journal" => Ok(Question::EmptyTheJournal),
            "pointer" => Ok(Question::Pointer),
            "screens" => Ok(Question::Screens),
            "film" => Ok(Question::FilmThisScreen {
                id: match rest {
                    "main" | "" => None,
                    named => Some(named.to_string()),
                },
            }),
            CLIPBOARD => {
                let (said, pushing) = split_first(rest);
                Ok(Question::Clipboard {
                    pushing: what_was_carried(pushing)?,
                    seen: match said {
                        NONE | "" => None,
                        stamp => Some(
                            stamp
                                .parse()
                                .map_err(|_| format!("« {stamp} » ne nomme rien"))?,
                        ),
                    },
                })
            }
            PIECES => {
                let (asking, giving) = halves(rest, "asking", "giving")?;
                Ok(Question::Pieces {
                    asking: some_of(asking, Wanted::read)?,
                    giving: some_of(giving, Given::read)?,
                })
            }
            "hush" => match rest {
                "quiet" => Ok(Question::Hush { quiet: true }),
                "play" => Ok(Question::Hush { quiet: false }),
                other => Err(format!("« {other} » ne dit ni de se taire ni de jouer")),
            },
            other => Err(format!("question inconnue « {other} »")),
        }
    }
}

impl Told {
    fn parse(message: &str) -> io::Result<Result<Self, String>> {
        let said = after_the_version(message).map_err(unreadable)?;
        let (verb, rest) = split_first(said);
        match verb {
            "opened" => Ok(Ok(Told::Opened)),
            "attended" => Ok(Ok(Told::Attended)),
            "hushed" => Ok(Ok(Told::Hushed)),
            "locked" => Ok(Ok(Told::Locked)),
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
            "reach" => Ok(Ok(Told::ReachLog {
                text: rest.to_string(),
            })),
            "emptied" => Ok(Ok(Told::Emptied)),
            // A shape this build does not know is the ordinary arrow
            // and never a refusal: the reading cannot fail, and that
            // is on purpose.
            "pointer" => Ok(Ok(Told::Pointer {
                shape: rest.parse().unwrap_or_default(),
            })),
            "screens" => Ok(Ok(Told::Screens {
                listed: rest.to_string(),
            })),
            CLIPBOARD => Ok(Ok(Told::Clipboard {
                theirs: what_was_carried(rest).map_err(unreadable)?,
            })),
            PIECES => {
                let (given, wanted) = halves(rest, "given", "wanted").map_err(unreadable)?;
                Ok(Ok(Told::Pieces {
                    given: some_of(given, Given::read).map_err(unreadable)?,
                    wanted: some_of(wanted, Wanted::read).map_err(unreadable)?,
                }))
            }
            "filming" => Ok(Ok(Told::Filming)),
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

/// What a session says it will be served, as it travels: a rate in
/// kilobits a second and a cadence in pictures a second.
fn served(said: &str) -> Result<MediaProfile, String> {
    let (kbps, fps) = split_first(said);
    match (kbps.parse::<u32>(), fps.parse()) {
        (Ok(kbps), Ok(frames_per_second)) => Ok(MediaProfile {
            bits_per_second: u64::from(kbps) * 1_000,
            frames_per_second,
        }),
        _ => Err(format!("« {said} » ne dit pas ce qu'une session demande")),
    }
}

/// A clip as it travels, or the word that says there is none.
///
/// The same spelling in both directions, since it is the same thing being
/// carried: what somebody copied, going towards whichever computer has
/// not got it.
fn carried(clip: &Option<Clip>) -> String {
    match clip {
        Some(clip) => clip.on_the_wire(),
        None => NONE.to_string(),
    }
}

/// Reads what the two above wrote.
fn what_was_carried(said: &str) -> Result<Option<Clip>, String> {
    match said.trim() {
        NONE | "" => Ok(None),
        carried => Clip::from_the_wire(carried)
            .map(Some)
            .map_err(|e| e.to_string()),
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
        .read_to_end(question.longest_answer())
        .await
        .map_err(io::Error::other)?;
    match Told::parse(&String::from_utf8_lossy(&heard))? {
        Ok(told) => Ok(told),
        Err(refusal) => Err(io::Error::other(refusal)),
    }
}

/// Opens a session on the far computer, which brings its engine up.
///
/// What this session will be served goes with it: it is the first word
/// of a session, and the far computer sizes its tunnel on it. Answered
/// once that engine is up, or with the reason it could not be.
pub async fn ask_to_open(connection: &Connection, serving: MediaProfile) -> io::Result<()> {
    match ask(connection, &Question::Open { serving }).await? {
        Told::Opened => Ok(()),
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

/// Asks the far ZyrDesk to wake its virtual screen for a picture like
/// that one, or, with nothing asked for, to leave its own screen alone.
///
/// Answered before the picture is opened and not alongside it: the far
/// engine films that screen, and it can only film one that is already
/// there.
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
pub async fn ask_for_the_journal(connection: &Connection, sift: &str) -> io::Result<String> {
    let asking = Question::Journal {
        sift: sift.to_string(),
    };
    match ask(connection, &asking).await? {
        Told::Journal { text } => Ok(text),
        other => Err(unreadable(format!("réponse hors sujet : {other}"))),
    }
}

/// Asks the far ZyrDesk for what it has measured of its own access to
/// the Internet.
///
/// Asked outside any session, like the journal above and for the same
/// reason: a computer nobody can reach is exactly the one this is worth
/// asking of.
pub async fn ask_for_the_reach_log(connection: &Connection) -> io::Result<String> {
    match ask(connection, &Question::ReachLog).await? {
        Told::ReachLog { text } => Ok(text),
        other => Err(unreadable(format!("réponse hors sujet : {other}"))),
    }
}

/// Asks the far ZyrDesk what shape its pointer has right now.
///
/// Asked while a session is open and only then: what it is for is to
/// give the pointer drawn on this computer the shape the one over there
/// has, and there is no pointer over there to speak of otherwise.
///
/// Asked often, and that is the whole design of it. The answer is worth
/// nothing a moment later, so nothing is cached and nothing is pushed:
/// one small question on a channel that is already open, whose answer is
/// a single word.
pub async fn ask_for_the_pointer(
    connection: &Connection,
) -> io::Result<zyr_proto::session::Pointer> {
    match ask(connection, &Question::Pointer).await? {
        Told::Pointer { shape } => Ok(shape),
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

/// Asks the far ZyrDesk which screens it is showing on.
///
/// Asked while a session is open, since that is when it can be acted on:
/// the answer fills the line of the menu that offers to be served from
/// another of that machine's screens.
pub async fn ask_what_screens_it_has(connection: &Connection) -> io::Result<String> {
    match ask(connection, &Question::Screens).await? {
        Told::Screens { listed } => Ok(listed),
        other => Err(unreadable(format!("réponse hors sujet : {other}"))),
    }
}

/// Asks the far ZyrDesk to serve its picture from that screen, or, with
/// nothing named, from its main one.
///
/// Its engine changes screen where it stands, so this may be asked at
/// any moment of a session and costs it nothing but the new screen.
pub async fn ask_to_film_this_screen(
    connection: &Connection,
    id: Option<String>,
) -> io::Result<()> {
    match ask(connection, &Question::FilmThisScreen { id }).await? {
        Told::Filming => Ok(()),
        other => Err(unreadable(format!("réponse hors sujet : {other}"))),
    }
}

/// Hands the far ZyrDesk what was copied here, and asks for what was
/// copied there.
///
/// One message for both halves of a shared clipboard, because a clipboard
/// is one thing and the two computers are equals about it: whichever of
/// them somebody copied on, the other has to end up holding it. Only the
/// side watching asks, since only that side has a reason to: nothing is
/// shared between two computers that are not in a session.
///
/// `seen` is the stamp of what both ends last agreed on, and it is what
/// keeps this cheap. Almost every turn is that stamp going out and
/// nothing at all coming back.
pub async fn ask_about_the_clipboard(
    connection: &Connection,
    pushing: Option<Clip>,
    seen: Option<Stamp>,
) -> io::Result<Option<Clip>> {
    match ask(connection, &Question::Clipboard { pushing, seen }).await? {
        Told::Clipboard { theirs } => Ok(theirs),
        other => Err(unreadable(format!("réponse hors sujet : {other}"))),
    }
}

/// Hands the far ZyrDesk a piece of a file copied here, and asks for a
/// piece of one copied there.
///
/// One message for both, like the clipboard it follows from. Only one
/// piece is ever in flight in each direction, and that is the whole of
/// what keeps a file from eating the session it travels beside: a link
/// twice as fast carries the file twice as fast and the picture exactly
/// as it was, because nothing is ever asked for before the last piece
/// arrived.
pub async fn ask_for_pieces(
    connection: &Connection,
    asking: Option<Wanted>,
    giving: Option<Given>,
) -> io::Result<(Option<Given>, Option<Wanted>)> {
    match ask(connection, &Question::Pieces { asking, giving }).await? {
        Told::Pieces { given, wanted } => Ok((given, wanted)),
        other => Err(unreadable(format!("réponse hors sujet : {other}"))),
    }
}

/// The first word of a session, heard and not answered yet.
///
/// Answered once the engine it asks for is up, or once it is known that
/// it will not be: the far end waits on this answer, and whatever it
/// says next goes to that engine.
pub struct Opening {
    serving: MediaProfile,
    answer: SendStream,
}

impl Opening {
    /// What the session opening asks to be served.
    pub fn serving(&self) -> MediaProfile {
        self.serving
    }

    /// Tells the far end its session is open.
    pub async fn opened(self) -> io::Result<()> {
        say(self.answer, Ok(Told::Opened)).await
    }

    /// Tells the far end why it is not, in words written to be read, and
    /// waits until it has them.
    ///
    /// A refused session is a connection let go of the moment this
    /// returns, and a connection let go of takes with it whatever it had
    /// not delivered yet: without the wait, the far end read « connection
    /// lost » instead of the one sentence that says what went wrong here.
    /// Bounded, for a far end that stopped answering.
    pub async fn refused(self, reason: &str) -> io::Result<()> {
        let delivered = self.answer.stopped();
        say(self.answer, Err(reason.to_string())).await?;
        let _ = tokio::time::timeout(REFUSAL_DELIVERED, delivered).await;
        Ok(())
    }
}

/// Answers the far computer until it asks to open a session, and hands
/// that question back unanswered. Host side.
///
/// Most connections never ask: a computer fetching this one's journal
/// asks for it and goes. Every question is answered on a task of its own,
/// as it is once the session is open, so a slow one holds up nothing,
/// and those still being answered when the session opens are answered
/// all the same.
///
/// The engine's own stream has nowhere to go before its engine is up,
/// and is refused.
pub async fn until_a_session_opens(
    connection: &Connection,
    answering: Arc<dyn Answers>,
    log: Option<&Log>,
) -> io::Result<Opening> {
    let (heard, mut opening) = mpsc::channel(1);
    loop {
        tokio::select! {
            Some(opening) = opening.recv() => return Ok(opening),
            accepted = connection.accept_stream() => {
                let (sending, mut receiving) = accepted.map_err(io::Error::other)?;
                let answering = answering.clone();
                let heard = heard.clone();
                let log = log.cloned();
                tokio::spawn(async move {
                    let outcome = match pump::read_announcement(&mut receiving).await {
                        Ok(StreamChannel::ZyrDesk) => {
                            before_the_opening(sending, receiving, answering, heard).await
                        }
                        Ok(StreamChannel::Engine) => Err(io::Error::other(
                            "le flux du moteur est arrivé avant l'ouverture de la session",
                        )),
                        Err(e) => Err(e),
                    };
                    if let (Err(e), Some(log)) = (outcome, &log) {
                        log.write(&format!("a stream before the session opened was refused: {e}"));
                    }
                });
            }
        }
    }
}

/// One question asked before the session opens: the opening itself is
/// handed back, everything else answered here.
async fn before_the_opening(
    sending: SendStream,
    mut receiving: RecvStream,
    answering: Arc<dyn Answers>,
    heard: mpsc::Sender<Opening>,
) -> io::Result<()> {
    let said = a_question(&mut receiving).await?;
    let told = match Question::parse(&said) {
        Ok(Question::Open { serving }) => {
            return heard
                .send(Opening {
                    serving,
                    answer: sending,
                })
                .await
                .map_err(|_| io::Error::other("une autre ouverture est déjà en cours"));
        }
        Ok(question) => attended(question, answering).await,
        Err(refusal) => Err(refusal),
    };
    say(sending, told).await
}

/// Answers whatever the other ZyrDesk asks once its session is open.
/// Host side.
pub async fn answer(
    sending: SendStream,
    mut receiving: RecvStream,
    answering: Arc<dyn Answers>,
) -> io::Result<()> {
    let said = a_question(&mut receiving).await?;

    let told = match Question::parse(&said) {
        Ok(question) => attended(question, answering).await,
        Err(refusal) => Err(refusal),
    };
    say(sending, told).await
}

/// Reads a question, and lets the one that carries a clipboard weigh more
/// than a line.
///
/// Two ceilings, and which of them applies is settled on the first few
/// hundred bytes rather than after the whole of it has been taken in:
/// this computer answers questions from every computer it lets in, so a
/// question is a line, and a line is all that is ever held from a verb
/// this build has never heard of. What somebody copied is the one
/// question that is a page, and it is let past the first ceiling only
/// once its own name has been read.
async fn a_question(receiving: &mut RecvStream) -> io::Result<String> {
    let mut said = Vec::new();
    let mut room = LONGEST_QUESTION;
    let mut heard = vec![0u8; 64 * 1024];
    while let Some(read) = receiving
        .read(&mut heard)
        .await
        .map_err(|e| unreadable(e.to_string()))?
    {
        said.extend_from_slice(&heard[..read]);
        if said.len() <= room {
            continue;
        }
        if room != LONGEST_QUESTION || !carries_a_page(&said) {
            return Err(unreadable(
                "une question plus longue que ce que ce canal porte",
            ));
        }
        room = LONGEST_PAGE;
    }
    Ok(String::from_utf8_lossy(&said).into_owned())
}

/// Whether what has been read so far is the head of one of the two
/// questions that are allowed to be a page.
///
/// Read on the head alone and never on the whole, which is the point of
/// it: at the moment this is asked, the rest has not been taken in yet.
fn carries_a_page(head: &[u8]) -> bool {
    let head = &head[..head.len().min(LONGEST_QUESTION)];
    let said = String::from_utf8_lossy(head);
    after_the_version(&said)
        .is_ok_and(|rest| rest.starts_with(CLIPBOARD) || rest.starts_with(PIECES))
}

/// Does what was asked, on a thread where waiting is allowed.
///
/// Most of what is asked starts a program in another Windows session or
/// reads a disk, and waits for it: doing that on the runtime's own
/// threads would hold up every session this computer is serving.
async fn attended(question: Question, answering: Arc<dyn Answers>) -> Result<Told, String> {
    match question {
        // A session already stands on this tunnel, with its engine: a
        // second one would have nowhere to go.
        Question::Open { .. } => Err("une session est déjà ouverte sur ce tunnel".to_string()),
        // Off the thread that carries the tunnel: pressing this starts a
        // program in another Windows session and waits for it, which is a
        // long time to hold a channel every other session is queueing
        // behind.
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
        Question::Journal { sift } => tokio::task::spawn_blocking(move || answering.journal(&sift))
            .await
            .map_err(|e| format!("le journal n'a pas pu être rassemblé : {e}"))?
            .map(|text| Told::Journal { text }),
        // Off the thread as well, and for the same reason: it is a file
        // read from a disk.
        Question::ReachLog => tokio::task::spawn_blocking(move || answering.reach_log())
            .await
            .map_err(|e| format!("le relevé n'a pas pu être lu : {e}"))?
            .map(|text| Told::ReachLog { text }),
        // Off it too: emptying is four files opened and cut on a disk.
        Question::EmptyTheJournal => {
            tokio::task::spawn_blocking(move || answering.empty_the_journal())
                .await
                .map_err(|e| format!("le journal n'a pas pu être vidé : {e}"))?
                .map(|()| Told::Emptied)
        }
        // On the thread, alone of all of these: it is a reading of what
        // the system already holds, it is asked several times a second
        // for the length of a session, and handing each one to another
        // thread would cost more than the answer.
        Question::Pointer => answering.pointer().map(|shape| Told::Pointer { shape }),
        // Off it as well: what is answered is written down in the
        // journal, which is a disk.
        Question::Screens => tokio::task::spawn_blocking(move || answering.screens())
            .await
            .map_err(|e| format!("les écrans n'ont pas pu être lus : {e}"))?
            .map(|listed| Told::Screens { listed }),
        // And off it too: the engine is told without waiting, and the
        // choice is written down in the journal, which is a disk.
        Question::FilmThisScreen { id } => {
            tokio::task::spawn_blocking(move || answering.film_this_screen(id))
                .await
                .map_err(|e| format!("l'écran à filmer n'a pas pu être choisi : {e}"))?
                .map(|()| Told::Filming)
        }
        // Off the thread as well: it writes what came down on a disk and
        // reads what is there from another file, and a clip is a page.
        Question::Clipboard { pushing, seen } => {
            tokio::task::spawn_blocking(move || answering.clipboard(pushing, seen))
                .await
                .map_err(|e| format!("le presse-papiers n'a pas pu être échangé : {e}"))?
                .map(|theirs| Told::Clipboard { theirs })
        }
        // And off it too, and this one more than any: both halves of it
        // are a disk being read and a disk being written.
        Question::Pieces { asking, giving } => {
            tokio::task::spawn_blocking(move || answering.pieces(asking, giving))
                .await
                .map_err(|e| format!("les morceaux n'ont pas pu être échangés : {e}"))?
                .map(|(given, wanted)| Told::Pieces { given, wanted })
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

    #[test]
    fn every_question_survives_the_round_trip() {
        for question in [
            // What a session says about itself when opening: that is
            // where the watched computer gets the size of its window
            // from.
            Question::Open {
                serving: MediaProfile {
                    bits_per_second: 80_000_000,
                    frames_per_second: 144,
                },
            },
            Question::SecureAttention,
            Question::Hush { quiet: true },
            Question::Hush { quiet: false },
            Question::Lock,
            // The magnification travels stuck to the size: a screen at
            // the right size without it is someone else's desktop at
            // the right resolution.
            Question::Screen {
                wanted: Some(WantedScreen {
                    wide: 1920,
                    high: 1200,
                    scale: 125,
                }),
            },
            // Zero means "none asked for": it is what a session says
            // when it could not measure its own screen.
            Question::Screen {
                wanted: Some(WantedScreen {
                    wide: 3840,
                    high: 2160,
                    scale: 0,
                }),
            },
            Question::Screen { wanted: None },
            Question::Journal {
                sift: String::new(),
            },
            Question::Journal {
                sift: "tag:clipboard".to_string(),
            },
            Question::ReachLog,
            Question::EmptyTheJournal,
            Question::Pointer,
            Question::Screens,
            // Nothing named means the main screen, and that is what every
            // session asks for as long as nobody has said otherwise.
            Question::FilmThisScreen { id: None },
            Question::FilmThisScreen {
                id: Some(
                    r"MONITOR\GSM5B7F\{4d36e96e-e325-11ce-bfc1-08002be10318}\0003".to_string(),
                ),
            },
            Question::FilmThisScreen {
                id: Some(r"\\.\DISPLAY3".to_string()),
            },
            // The clipboard, which is the only question to carry
            // something both ways at once. The four cases are there
            // because all four happen: nothing on either side at the
            // very start of a session, something to give, something
            // already shared, and both together.
            Question::Clipboard {
                pushing: None,
                seen: None,
            },
            Question::Clipboard {
                pushing: Some(Clip::text("l'adresse du serveur : 10.0.0.4")),
                seen: None,
            },
            Question::Clipboard {
                pushing: None,
                seen: Some(Clip::text("déjà partagé").stamp()),
            },
            Question::Clipboard {
                pushing: Some(Clip::picture(vec![0x89, b'P', b'N', b'G', 0x00, 0xff])),
                seen: Some(Clip::text("déjà partagé").stamp()),
            },
            // An empty text is not the absence of text, and the two
            // must stay apart from one end of the channel to the
            // other.
            Question::Clipboard {
                pushing: Some(Clip::text("")),
                seen: None,
            },
            // The pieces of a file, which also go both ways at once:
            // we ask for a piece of what the other end copied, and
            // give a piece of what we copied ourselves.
            Question::Pieces {
                asking: None,
                giving: None,
            },
            Question::Pieces {
                asking: Some(Wanted {
                    rank: 3,
                    from: 8_589_934_592,
                    how_many: A_PIECE as u32,
                }),
                giving: None,
            },
            Question::Pieces {
                asking: None,
                giving: Some(Given {
                    rank: 0,
                    from: 0,
                    bytes: vec![0, 1, 2, 250, 255],
                }),
            },
            // An empty piece is not the absence of a piece: it is the
            // end of a file, and the two must be told apart.
            Question::Pieces {
                asking: Some(Wanted {
                    rank: 0,
                    from: 0,
                    how_many: 0,
                }),
                giving: Some(Given {
                    rank: 9,
                    from: 4096,
                    bytes: Vec::new(),
                }),
            },
        ] {
            let said = question.to_string();
            assert_eq!(Question::parse(&said), Ok(question), "sur « {said} »");
        }
    }

    #[test]
    fn every_answer_survives_the_round_trip() {
        for told in [
            Told::Opened,
            Told::Attended,
            Told::Hushed,
            Told::Locked,
            Told::Screen {
                size: Some((1920, 1200)),
            },
            Told::Screen { size: None },
            // A journal travels whole, lines included: this channel
            // ends a message by closing the stream, so nothing
            // needs to be folded onto one line.
            Told::Journal {
                text: "ZyrDesk 0.1.0\nOrdinateur       : PC de Victor\n\n--- Le service ---\nune \
                       ligne\nune autre"
                    .to_string(),
            },
            // The record travels whole, lines included, for the same
            // reason as the journal just above.
            Told::ReachLog {
                text: "8.8.8.8:53 answered in 8 ms\n8.8.8.8:53 said nothing in 1000 ms".to_string(),
            },
            Told::Emptied,
            Told::Pointer {
                shape: zyr_proto::session::Pointer::Text,
            },
            // The list of screens travels whole, one line per screen,
            // like the journal and for the same reason.
            Told::Screens {
                listed: "MONITOR\\GSM5B7F\\0003 main 2560x1440 ROG PG279Q\n\\\\.\\DISPLAY2 other \
                         1920x1080 Dell U2412M"
                    .to_string(),
            },
            Told::Screens {
                listed: String::new(),
            },
            Told::Filming,
            // Nothing is not the same thing as an empty clipboard:
            // nothing means "you already have it", and emptying the far
            // one is never asked for.
            Told::Clipboard { theirs: None },
            Told::Clipboard {
                theirs: Some(Clip::text("deux lignes\net la seconde")),
            },
            Told::Clipboard {
                theirs: Some(Clip::picture(vec![0x89, b'P', b'N', b'G', 0x00, 0xff])),
            },
            // Nothing wanted is what tells the other end it can stop
            // sending: it has to be told apart from a piece of zero
            // length.
            Told::Pieces {
                given: None,
                wanted: None,
            },
            Told::Pieces {
                given: Some(Given {
                    rank: 2,
                    from: 262_144,
                    bytes: vec![7; 32],
                }),
                wanted: Some(Wanted {
                    rank: 2,
                    from: 262_176,
                    how_many: A_PIECE as u32,
                }),
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
        // The half of the product that does not speak the same version
        // must be named, not guessed: it is the only fault that is
        // repaired in one sentence. Counted from the current version, so
        // that this test does not start talking about today's version
        // every time one is added.
        let newer = VERSION + 1;
        let refusal = Question::parse(&format!("{newer} open 20000 60")).unwrap_err();
        assert!(
            refusal.contains(&newer.to_string()) && refusal.contains("version"),
            "{refusal}"
        );

        // And version 1, which was not text at all, must not pass for a
        // valid question.
        assert!(Question::parse("\u{1}\u{a4}\u{10}").is_err());
    }

    #[test]
    fn an_opening_that_does_not_say_what_it_asks_for_is_refused() {
        // A rate that will not read must not become nought, which would
        // size the far computer's window on nothing: it is refused,
        // saying why.
        for said in ["open", "open 20000", "open beaucoup 60", "open -5 60"] {
            let refusal = Question::parse(&format!("{VERSION} {said}")).unwrap_err();
            assert!(refusal.contains("session"), "sur « {said} » : {refusal}");
        }
    }

    #[test]
    fn the_questions_of_the_engines_that_are_gone_are_refused() {
        // A client of the previous dialect is stopped by the version
        // first; these are what a client of this one could never say.
        for said in ["ports 20000 60", "pair 0429 PC", "bitrate 20000", "codecs"] {
            let refusal = Question::parse(&format!("{VERSION} {said}")).unwrap_err();
            assert!(refusal.contains("question inconnue"), "{refusal}");
        }
    }

    #[test]
    fn a_reason_written_over_two_lines_arrives_whole() {
        // A refusal is written to be read, sometimes over several lines.
        // It travels flat and must stay entirely readable.
        let folded = format!("{VERSION} no {}", shortened("deux\nlignes"));
        let Ok(Err(reason)) = Told::parse(&folded) else {
            panic!("« {folded} » n'est pas relu comme un refus");
        };
        assert_eq!(reason, "deux lignes");
    }

    #[test]
    fn a_reason_too_long_for_the_channel_is_shortened_rather_than_lost() {
        // Without this, the message would go beyond what the channel
        // accepts, and the other computer would see a transport
        // failure where it was meant to read an explanation.
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

        // And a reason that fits is not touched.
        assert_eq!(
            shortened("le moteur n'attend aucun code"),
            "le moteur n'attend aucun code"
        );
    }
}
