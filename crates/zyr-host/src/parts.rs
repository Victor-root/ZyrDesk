//! What the engine needs from the computer it runs on: a screen to film,
//! a keyboard and mouse to play, a sound card to listen to.
//!
//! Each is a trait, so that the engine itself stays the same everywhere:
//! Windows implements them for real, and the tests run the whole engine
//! on stand-ins (`fake`).

use std::fmt;
use std::time::Instant;

use zyr_codec::{Frame, GpuVendor, Input, VideoEncoder};
use zyr_media::input::Button;
use zyr_media::service::Display;

use crate::picture::Rect;

/// Makes the screen, on the thread that captures: whatever belongs to a
/// thread (its priority, the desktop it is attached to) is set up there.
pub type MakeScreen = Box<dyn FnOnce() -> Result<Box<dyn Screen>, ScreenError> + Send>;

/// Makes the keyboard and mouse, on the thread that plays them.
pub type MakeInjector = Box<dyn FnOnce() -> Box<dyn Injector> + Send>;

/// The screens of this computer, and the one being filmed.
pub trait Screen {
    /// The screens that can be filmed now.
    fn displays(&mut self) -> Vec<Display>;

    /// Aims the capture at the screen whose [`Display::id`] is `display`,
    /// or at the main one for `""` or a screen that is not there.
    fn aim(&mut self, display: &str) -> Result<Aimed, ScreenError>;

    /// What an encoder of this screen's pictures is fed with: textures of
    /// the device the screen is captured on, or memory.
    fn encoder_input(&self) -> Input;

    /// Who made the graphics card the screen is captured on.
    fn vendor(&self) -> GpuVendor;

    /// Waits for the screen to change, until `until` at the latest.
    fn wait(&mut self, until: Instant) -> Result<Captured, ScreenError>;

    /// Draws the latest image into a frame for `encoder`, at the
    /// encoder's size, where `drawing` says.
    fn draw(
        &mut self,
        encoder: &VideoEncoder,
        feed: Feed,
        drawing: &Drawing,
    ) -> Result<Frame, ScreenError>;
}

/// The screen the capture is aimed at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Aimed {
    pub display: Display,
    /// Where it sits on the desktop spanning every screen; its size is
    /// that of the image it gives.
    pub area: Rect,
    /// The desktop spanning every screen.
    pub desktop: Rect,
    /// Changes whenever the device that encoders are fed on changes:
    /// an encoder opened before is of no use after.
    pub device: u64,
}

/// What became of a wait for the screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Captured {
    /// A new image, as it was on the screen at `at`.
    Image { at: Instant },
    /// Only the pointer moved or changed shape.
    Pointer,
    /// Nothing new before the deadline.
    Nothing,
    /// The capture had to be aimed again on its own (the screen changed
    /// its size, or went away): this is what it films now.
    Moved(Aimed),
}

/// What the encoder takes its pictures in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feed {
    Memory,
    Texture,
}

/// How to draw the image into a picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Drawing {
    /// Where the image goes in the picture; the rest is black.
    pub placement: Rect,
    /// Whether the pointer is drawn over it.
    pub pointer: bool,
}

/// What went wrong with the screen, in a sentence for the viewer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenError(pub String);

impl fmt::Display for ScreenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ScreenError {}

/// Plays keys and pointer on this computer.
pub trait Injector {
    fn inject(&mut self, what: Injected) -> Result<(), InjectError>;
}

/// An input event as this computer plays it: the pointer already placed
/// on its desktop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Injected {
    /// A key, by its set-1 scan code; `extended` for the E0 prefix.
    Key {
        scancode: u8,
        extended: bool,
        down: bool,
    },
    /// Where the pointer goes, from 0 to 65535 across the desktop
    /// spanning every screen.
    PointerTo {
        x: i32,
        y: i32,
    },
    PointerBy {
        dx: i16,
        dy: i16,
    },
    Button {
        button: Button,
        down: bool,
    },
    /// In wheel units, 120 to a notch.
    Wheel {
        vertical: i16,
        horizontal: i16,
    },
}

/// Why an input could not be played, for the log: never what the input
/// was, which may have been a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InjectError(pub String);

impl fmt::Display for InjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The sound card whose sound the session carries.
pub trait Sound: Send {
    /// Starts listening, on the thread that will read what is heard.
    /// Listening stops when what this gives is dropped.
    fn open(&mut self) -> Result<Box<dyn SoundCapture>, SoundError>;
}

/// The sound card, being listened to.
pub trait SoundCapture {
    /// The next 10 ms of sound, 48 kHz stereo, waiting until `until` at
    /// the latest. Nothing when the computer is silent.
    fn next_block(&mut self, until: Instant) -> Result<Option<SoundBlock>, SoundError>;
}

/// Ten milliseconds of sound.
#[derive(Debug, Clone, PartialEq)]
pub struct SoundBlock {
    /// 48 kHz stereo, interleaved: `zyr_codec::OPUS_FRAME` samples per
    /// channel.
    pub samples: Vec<f32>,
    /// When its first sample was heard.
    pub at: Instant,
}

/// What went wrong with the sound card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SoundError {
    /// Another sound card plays now, or this one went away: open again.
    Changed,
    /// It cannot be listened to, in a sentence for the viewer.
    Failed(String),
}

impl fmt::Display for SoundError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SoundError::Changed => f.write_str("la carte son a changé"),
            SoundError::Failed(why) => f.write_str(why),
        }
    }
}
