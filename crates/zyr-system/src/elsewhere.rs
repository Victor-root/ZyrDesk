//! What there is of all this outside Windows: no session holds a screen,
//! so nothing is started in one, locked, pressed or read there.

use std::io;
use std::path::Path;

use zyr_proto::log::Log;
use zyr_proto::session::Pointer;

use crate::{Errand, Whose};

fn not_here() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "this is not Windows: no session holds a screen here",
    )
}

pub fn session_on_screen() -> Option<u32> {
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
