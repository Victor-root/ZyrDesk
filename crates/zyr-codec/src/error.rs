//! What can go wrong between the engine and FFmpeg, said in words a
//! person can act on.

use std::ffi::c_int;
use std::fmt;
use std::path::PathBuf;

/// `AVERROR(EAGAIN)`: nothing to hand over yet, or no room to take more.
///
/// A macro FFmpeg cannot export. EAGAIN is 11 in glibc and in the
/// Windows C runtime alike, and a test checks FFmpeg's own words for it.
pub(crate) const AGAIN: c_int = -11;

/// `AVERROR_EOF`: FFmpeg's tag for "EOF ", negated. Everything was
/// handed over and nothing more will come.
pub(crate) const END: c_int = -i32::from_le_bytes(*b"EOF ");

#[derive(Debug)]
pub enum CodecError {
    /// One of FFmpeg's libraries could not be opened.
    Library { path: PathBuf, reason: String },
    /// A library lacks a function the bindings rely on.
    Function {
        path: PathBuf,
        function: &'static str,
    },
    /// A library is another major version than the bindings describe.
    Version {
        path: PathBuf,
        found: u32,
        expected: u32,
    },
    /// This build of FFmpeg has no encoder or decoder by that name.
    Missing { codec: &'static str },
    /// FFmpeg refused: what was being done, its error code, its words.
    Refused {
        what: String,
        code: c_int,
        text: String,
    },
    /// Options the codec does not know. FFmpeg would have gone on
    /// without them, and the latency they were there for with them.
    UnknownOptions {
        codec: &'static str,
        options: Vec<String>,
    },
    /// FFmpeg could not allocate what was asked for.
    OutOfMemory { what: &'static str },
    /// Direct3D refused, in the words Windows gives.
    Graphics { what: &'static str, reason: String },
    /// Something asked of this crate that cannot be done as asked.
    Invalid(String),
}

impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CodecError::Library { path, reason } => {
                write!(
                    f,
                    "FFmpeg not found: {} does not open ({reason})",
                    path.display()
                )
            }
            CodecError::Function { path, function } => write!(
                f,
                "{} has no function {function}: it is not the FFmpeg expected",
                path.display()
            ),
            CodecError::Version {
                path,
                found,
                expected,
            } => write!(
                f,
                "{} is version {found}, the engine expects version {expected}",
                path.display()
            ),
            CodecError::Missing { codec } => {
                write!(f, "this version of FFmpeg has no {codec}")
            }
            CodecError::Refused { what, code, text } => {
                write!(f, "{what}: {text} (code {code})")
            }
            CodecError::UnknownOptions { codec, options } => {
                write!(
                    f,
                    "{codec} does not know the settings {}",
                    options.join(", ")
                )
            }
            CodecError::OutOfMemory { what } => {
                write!(f, "not enough memory for {what}")
            }
            CodecError::Graphics { what, reason } => {
                write!(f, "Direct3D refuses {what}: {reason}")
            }
            CodecError::Invalid(reason) => f.write_str(reason),
        }
    }
}

impl std::error::Error for CodecError {}
