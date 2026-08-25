//! Where the engine's process is started.
//!
//! Started from a console, the engine is simply a child of the program
//! that wants it, and nothing more is needed. Started by the Windows
//! service it is another matter: a service lives in a session with no
//! screen and no desktop, so an engine started there would capture
//! nothing. It has to be pushed into the session carrying the display,
//! and only the service knows how.
//!
//! This seam is what lets the rest of the lifecycle be written once:
//! preparing, provisioning, watching and stopping are the same either
//! way, only the moment of starting differs.

use std::io;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

/// What has to be started, whoever starts it.
#[derive(Debug, Clone)]
pub struct Launch {
    pub exe: PathBuf,
    pub arguments: Vec<String>,
    /// Folder to start from. The engine resolves its resources relative
    /// to it, not to its own executable.
    pub working_dir: Option<PathBuf>,
    /// File the program's output is poured into.
    pub log: PathBuf,
}

/// How the engine came to be gone.
///
/// Worth telling apart and worth writing down. The engine puts the far
/// computer's screen back the size it found it at as it goes, and only as
/// it goes: taken outright, it leaves that screen at the size of whoever
/// was watching it. Which of the two happened is the first question asked
/// of a computer that came back wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Parting {
    /// It was asked, and went by itself.
    OfItsOwnAccord,
    /// It was not going, and was taken.
    Taken,
}

impl std::fmt::Display for Parting {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Parting::OfItsOwnAccord => "the engine went by itself, having put the screen back",
            Parting::Taken => {
                "the engine would not go and was taken, so the screen stays as the session left it"
            }
        })
    }
}

/// A started process, seen by whoever watches over it.
pub trait Running: Send {
    /// Identifier the task manager shows. Enough to find the engine
    /// again when reading a log after the fact.
    fn identifier(&self) -> u32;

    /// Exit code, once the process has stopped on its own. The inner
    /// value is absent when it was interrupted instead of returning.
    fn exit_seen(&mut self) -> io::Result<Option<Option<i32>>>;

    /// Stops the process, waits for it to be gone, and says how it went.
    fn stop(&mut self) -> io::Result<Parting>;
}

/// How the engine's process is started.
pub trait Launcher: Send {
    fn launch(&self, launch: &Launch) -> io::Result<Box<dyn Running>>;
}

/// Starts the engine in the session of whoever asks for it.
#[derive(Debug, Clone, Copy, Default)]
pub struct SameSession;

impl Launcher for SameSession {
    fn launch(&self, launch: &Launch) -> io::Result<Box<dyn Running>> {
        let log = std::fs::File::create(&launch.log)?;
        let log_for_errors = log.try_clone()?;

        let mut command = Command::new(&launch.exe);
        if let Some(folder) = &launch.working_dir {
            command.current_dir(folder);
        }
        let child = command
            .args(&launch.arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(log_for_errors))
            .spawn()?;
        Ok(Box::new(child))
    }
}

impl Running for Child {
    fn identifier(&self) -> u32 {
        self.id()
    }

    fn exit_seen(&mut self) -> io::Result<Option<Option<i32>>> {
        Ok(self.try_wait()?.map(|status| status.code()))
    }

    /// A child of this program has no console of its own to be
    /// interrupted through: this is the console supervisor, used while
    /// developing, where the engine is a child like any other.
    fn stop(&mut self) -> io::Result<Parting> {
        self.kill()?;
        self.wait()?;
        Ok(Parting::Taken)
    }
}
