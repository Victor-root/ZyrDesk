//! The host engine: what the computer being shown sends, and what it
//! plays of what the viewer does.
//!
//! The service starts it for one session, as `zyrdeskd --serve-a-session
//! <link>`, in the session that owns the screen and with the system's
//! own account, the only one that sees the sign-in screen and the
//! administrator prompts. It talks to nobody but the service, through
//! the local link: the service carries its messages and packets through
//! the tunnel to the player.
//!
//! # Threads
//!
//! - the link, on a small runtime of its own: reads what comes in and
//!   hands it to whoever it is for, writes what the others hand it,
//!   answers the player's pings itself;
//! - the engine, the caller's thread: the conversation with the service
//!   and the player;
//! - the pipeline: capture, conversion, encoding, packets, at the
//!   cadence the viewer asked;
//! - the input: keys and pointer, in order, and everything held let go
//!   of when the session ends or the player falls silent;
//! - the sound: what the computer plays, 10 ms at a time in Opus.
//!
//! No media thread ever waits on the link: packets that find its queue
//! full are counted and dropped, and the next picture is a key frame.
//!
//! # On other systems
//!
//! The screen, the keyboard and mouse and the sound card are traits
//! ([`Screen`], [`Injector`], [`Sound`]). Windows has its own (the
//! `platform` module); the `fake` feature brings stand-ins (`fake`) with
//! which the whole engine runs anywhere, and is tested so.

mod clock;
#[cfg(any(windows, test, feature = "fake"))]
mod color;
#[cfg(any(test, feature = "fake"))]
pub mod fake;
mod input;
mod link;
mod parts;
pub mod picture;
mod pipeline;
#[cfg(windows)]
mod platform;
#[cfg(any(windows, test))]
mod pointer;
mod session;
mod sound;
mod throttle;

#[cfg(test)]
mod end_to_end;

use std::process::ExitCode;
use std::sync::Arc;

use zyr_codec::Ffmpeg;
use zyr_proto::log::Log;

pub use parts::{
    Aimed, Captured, Drawing, Feed, InjectError, Injected, Injector, MakeInjector, MakeScreen,
    Screen, ScreenError, Sound, SoundBlock, SoundCapture, SoundError,
};

/// What the host engine's lines are filed under.
const TAG: &str = "engine";

/// What the engine runs on: FFmpeg, and this computer's screen, keyboard
/// and mouse and sound card.
pub struct Parts {
    pub ffmpeg: Arc<Ffmpeg>,
    /// Made on the thread that captures.
    pub screen: MakeScreen,
    /// Made on the thread that plays the keys and the pointer.
    pub injector: MakeInjector,
    pub sound: Box<dyn Sound>,
}

/// How a session ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ending {
    /// The service said to stop.
    Stopped,
    /// The player said goodbye.
    PlayerLeft,
    /// The link closed with nobody saying goodbye.
    LinkLost,
    /// The link could not be reached at all.
    Unreachable(String),
    /// The engine could not go on; the player was told why.
    Failed(String),
}

/// Exit codes of `zyrdeskd --serve-a-session`.
pub mod exit {
    /// The session ended as asked: the service said to stop, or the
    /// player said goodbye.
    pub const ENDED: u8 = 0;
    /// The link closed with nobody saying goodbye.
    pub const LINK_LOST: u8 = 2;
    /// The link named on the command line could not be reached.
    pub const NO_LINK: u8 = 3;
    /// FFmpeg could not be loaded from `vendor/ffmpeg`.
    pub const NO_FFMPEG: u8 = 4;
    /// The engine could not go on; the log says why.
    pub const FAILED: u8 = 5;
}

/// Serves one session over the link named `link_name`, which the service
/// created, and says how it went as an exit code (see [`exit`]).
///
/// Runs on Windows: elsewhere there is no screen to film, and the answer
/// is [`exit::FAILED`].
pub fn serve(link_name: &str, log: Log) -> ExitCode {
    let log = log.about(TAG);
    log.write(&format!("host engine starting on link {link_name}"));
    let ffmpeg = match Ffmpeg::load(&zyr_proto::paths::ffmpeg_dir()) {
        Ok(ffmpeg) => ffmpeg,
        Err(e) => {
            log.write(&format!("host engine cannot start: {e}"));
            return ExitCode::from(exit::NO_FFMPEG);
        }
    };
    ffmpeg.log_into(&log);
    log.write(&format!("FFmpeg {} loaded", ffmpeg.version()));
    ExitCode::from(match served(link_name, ffmpeg, &log) {
        Ending::Stopped | Ending::PlayerLeft => exit::ENDED,
        Ending::LinkLost => exit::LINK_LOST,
        Ending::Unreachable(_) => exit::NO_LINK,
        Ending::Failed(_) => exit::FAILED,
    })
}

#[cfg(windows)]
fn served(link_name: &str, ffmpeg: Arc<Ffmpeg>, log: &Log) -> Ending {
    let _tuned = platform::Tuned::for_the_session(log);
    run(link_name, platform::parts(ffmpeg, log), log.clone())
}

#[cfg(not(windows))]
fn served(_link_name: &str, _ffmpeg: Arc<Ffmpeg>, log: &Log) -> Ending {
    let why = "the host engine films a Windows screen, and this is not Windows".to_string();
    log.write(&why);
    Ending::Failed(why)
}

/// Connects to the link named `link_name`, which the service created,
/// and runs the engine over it with these parts until the session ends.
///
/// The link is made on the runtime that then carries it, on a thread of
/// its own: what tokio opens belongs to the runtime it was opened on.
pub fn run(link_name: &str, parts: Parts, log: Log) -> Ending {
    let log = log.about(TAG);
    let connected = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .and_then(|runtime| {
            let link = runtime.block_on(zyr_control::link::connect(link_name))?;
            Ok((runtime, link))
        });
    match connected {
        Ok((runtime, link)) => session::run(runtime, link, parts, &log),
        Err(e) => {
            log.write(&format!("host engine cannot reach link {link_name}: {e}"));
            Ending::Unreachable(e.to_string())
        }
    }
}
