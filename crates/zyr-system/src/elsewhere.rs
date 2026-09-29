//! What there is of all this outside Windows: no session holds a screen,
//! so nothing is started in one, locked, pressed or read there.

use std::convert::Infallible;
use std::io;
use std::net::IpAddr;
use std::path::Path;
use std::time::Duration;

use zyr_proto::log::Log;
use zyr_proto::session::Pointer;

use crate::{Errand, Launch, Refusal, Whose};

fn not_here() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "this is not Windows: no session holds a screen here",
    )
}

pub fn session_on_screen() -> Option<u32> {
    None
}

/// The computer's own counters, which are not read here.
#[derive(Debug)]
pub struct Vitals(Infallible);

impl Vitals {
    pub fn start() -> Option<Self> {
        None
    }

    pub fn second(&mut self, _peer: Option<IpAddr>) -> Option<String> {
        match self.0 {}
    }
}

/// What this computer is, which is not read here.
pub fn describe_this_computer() -> Option<String> {
    None
}

pub fn runs_in(_session: u32, _program: &Path) -> io::Result<bool> {
    Err(not_here())
}

pub fn somebody_signed_in(_session: u32) -> io::Result<bool> {
    Err(not_here())
}

pub fn whoever_this_is() -> String {
    "nobody Windows would name, this is not Windows".to_string()
}

/// A program started in a session on screen, which never exists here.
#[derive(Debug)]
pub struct SessionProcess(Infallible);

impl SessionProcess {
    pub fn process(&self) -> u32 {
        match self.0 {}
    }

    pub fn gone(&self) -> bool {
        match self.0 {}
    }

    pub fn let_go(self, _within: Duration) -> io::Result<Option<u32>> {
        match self.0 {}
    }
}

pub fn start_in_session(_launch: &Launch, _session: u32) -> io::Result<SessionProcess> {
    Err(not_here())
}

pub fn errand(_arguments: &[String], _refused: &str) -> io::Result<Errand> {
    Err(not_here())
}

pub fn errand_code(_arguments: &[String], _refused: &str) -> io::Result<(u32, Errand)> {
    Err(not_here())
}

pub fn start_a_helper(_arguments: &[String], _whose: Whose) -> io::Result<()> {
    Err(not_here())
}

pub fn lock_this_desktop() -> bool {
    false
}

pub fn press(_log: &Log) -> io::Result<()> {
    Err(io::Error::other(
        "this computer has no Ctrl+Alt+Del to press",
    ))
}

pub fn pointer_shape() -> Pointer {
    Pointer::Arrow
}

pub fn still_running(_process: u32) -> bool {
    false
}

pub fn run_as_administrator(_program: &Path, _arguments: &str) -> Result<u32, Refusal> {
    Err(Refusal::NotStarted(io::Error::new(
        io::ErrorKind::Unsupported,
        "this is not Windows: there are no administrator rights to ask for here",
    )))
}

pub fn start_at_sign_in(_entry: &str, _command: Option<&str>) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "this is not Windows: nothing is started at sign-in here",
    ))
}
