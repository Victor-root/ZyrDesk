//! Stand-ins for the screen, the keyboard and mouse and the sound card,
//! with which the whole engine runs anywhere.
//!
//! The screen draws a pattern that moves with every image, and carries
//! the number of the image in a corner block so that a test can tell
//! pictures apart after they went through an encoder and a decoder. The
//! keyboard and mouse remember what they were given. The sound card is
//! silent, or plays a tone.

use std::f32::consts::TAU;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use zyr_codec::{Frame, GpuVendor, Input, OPUS_FRAME, VideoEncoder};
use zyr_media::service::Display;

use crate::color::{BLACK, bgra_to_nv12};
use crate::parts::{
    Aimed, Captured, Drawing, Feed, InjectError, Injected, Injector, Screen, ScreenError, Sound,
    SoundBlock, SoundCapture, SoundError,
};
use crate::picture::{Rect, Size};

/// Side of the block, in the top left corner of the image, whose
/// brightness says which image it is.
pub const MARKER: u32 = 32;

/// The brightness of the marker of image number `index`, as luma.
pub fn marker_luma(index: u64) -> u8 {
    // Grey levels far enough apart to survive encoding, in limited range.
    16 + ((index * 23) % 200) as u8
}

/// A screen that changes `rate` times a second, or never for 0.
pub struct SyntheticScreen {
    displays: Vec<(Display, Rect)>,
    aimed: usize,
    period: Option<Duration>,
    next_image: Instant,
    /// Number of the latest image, if any was captured yet.
    latest: Option<u64>,
    /// The picture drawn last, reused.
    bgra: Vec<u8>,
}

impl SyntheticScreen {
    pub fn new(rate: u32) -> Self {
        let period = (rate > 0).then(|| Duration::from_secs(1) / rate);
        Self {
            displays: Self::screens(),
            aimed: 0,
            period,
            next_image: Instant::now(),
            latest: None,
            bgra: Vec::new(),
        }
    }

    /// The two screens it has: a main one at 1280x720, and one at 640x480
    /// right of it.
    pub fn screens() -> Vec<(Display, Rect)> {
        vec![
            (
                Display {
                    id: r"FAKE\MAIN".to_string(),
                    main: true,
                    width: 1280,
                    height: 720,
                    name: "Écran principal".to_string(),
                },
                Rect::new(0, 0, 1280, 720),
            ),
            (
                Display {
                    id: r"FAKE\SIDE".to_string(),
                    main: false,
                    width: 640,
                    height: 480,
                    name: "Écran d'appoint".to_string(),
                },
                Rect::new(1280, 0, 640, 480),
            ),
        ]
    }

    /// The desktop spanning both screens.
    pub fn desktop() -> Rect {
        Rect::new(0, 0, 1920, 720)
    }

    fn aimed(&self) -> Aimed {
        let (display, area) = self.displays[self.aimed].clone();
        Aimed {
            display,
            area,
            desktop: Self::desktop(),
            device: 1,
        }
    }

    /// Draws image `index` at `size`, the image where `drawing` puts it.
    fn paint(&mut self, index: u64, size: Size, drawing: &Drawing) {
        let (width, height) = (size.width as usize, size.height as usize);
        self.bgra.resize(width * height * 4, 0);
        let place = drawing.placement;
        let marker = marker_luma(index);
        for y in 0..height {
            for x in 0..width {
                let inside_x = x as i64 - i64::from(place.x);
                let inside_y = y as i64 - i64::from(place.y);
                let inside = (0..i64::from(place.width)).contains(&inside_x)
                    && (0..i64::from(place.height)).contains(&inside_y);
                let grey = if !inside {
                    0
                } else if inside_x < i64::from(MARKER) && inside_y < i64::from(MARKER) {
                    // The luma asked for, back to the grey that gives it.
                    ((f32::from(marker) - 16.0) * 255.0 / 219.0).round() as u8
                } else {
                    ((inside_x + inside_y + index as i64) % 200) as u8 + 30
                };
                let at = (y * width + x) * 4;
                self.bgra[at..at + 4].copy_from_slice(&[grey, grey, grey, 255]);
            }
        }
    }
}

impl Screen for SyntheticScreen {
    fn displays(&mut self) -> Vec<Display> {
        self.displays
            .iter()
            .map(|(display, _)| display.clone())
            .collect()
    }

    fn aim(&mut self, display: &str) -> Result<Aimed, ScreenError> {
        self.aimed = self
            .displays
            .iter()
            .position(|(each, _)| each.id == display)
            .unwrap_or(0);
        Ok(self.aimed())
    }

    fn encoder_input(&self) -> Input {
        Input::Cpu
    }

    fn vendor(&self) -> GpuVendor {
        GpuVendor::Other
    }

    fn wait(&mut self, until: Instant) -> Result<Captured, ScreenError> {
        let Some(period) = self.period else {
            thread::sleep(until.saturating_duration_since(Instant::now()));
            return Ok(Captured::Nothing);
        };
        if self.next_image > until {
            thread::sleep(until.saturating_duration_since(Instant::now()));
            return Ok(Captured::Nothing);
        }
        let at = self.next_image;
        thread::sleep(at.saturating_duration_since(Instant::now()));
        self.next_image = at + period;
        self.latest = Some(self.latest.map_or(0, |index| index + 1));
        Ok(Captured::Image { at })
    }

    fn draw(
        &mut self,
        encoder: &VideoEncoder,
        feed: Feed,
        drawing: &Drawing,
    ) -> Result<Frame, ScreenError> {
        if feed != Feed::Memory {
            return Err(ScreenError(
                "cet écran d'essai ne dessine qu'en mémoire".to_string(),
            ));
        }
        let mut frame = encoder
            .frame_for_cpu()
            .map_err(|e| ScreenError(e.to_string()))?;
        let size = Size::new(frame.width(), frame.height());
        let mut planes = frame.planes();
        match self.latest {
            Some(index) => {
                self.paint(index, size, drawing);
                bgra_to_nv12(&self.bgra, size.width as usize * 4, size, &mut planes);
            }
            None => {
                planes.luma.fill(BLACK.0);
                planes.chroma.fill(BLACK.1);
            }
        }
        Ok(Frame::Cpu(frame))
    }
}

/// A keyboard and mouse that remember what they were given.
pub struct RecordingInjector {
    played: Arc<Mutex<Vec<Injected>>>,
}

/// What a [`RecordingInjector`] was given, to look at from elsewhere.
#[derive(Clone)]
pub struct Recorded {
    played: Arc<Mutex<Vec<Injected>>>,
}

impl RecordingInjector {
    pub fn new() -> (Self, Recorded) {
        let played = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                played: Arc::clone(&played),
            },
            Recorded { played },
        )
    }
}

impl Recorded {
    /// Everything played so far, in order.
    pub fn taken(&self) -> Vec<Injected> {
        self.played
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Injector for RecordingInjector {
    fn inject(&mut self, what: Injected) -> Result<(), InjectError> {
        self.played
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(what);
        Ok(())
    }
}

/// A sound card with nothing to play.
pub struct SilentSound;

struct Silence;

impl Sound for SilentSound {
    fn open(&mut self) -> Result<Box<dyn SoundCapture>, SoundError> {
        Ok(Box::new(Silence))
    }
}

impl SoundCapture for Silence {
    fn next_block(&mut self, until: Instant) -> Result<Option<SoundBlock>, SoundError> {
        thread::sleep(until.saturating_duration_since(Instant::now()));
        Ok(None)
    }
}

/// A sound card playing a 440 Hz tone, in real time.
pub struct ToneSound;

struct Tone {
    next: Instant,
    played: u64,
}

impl Sound for ToneSound {
    fn open(&mut self) -> Result<Box<dyn SoundCapture>, SoundError> {
        Ok(Box::new(Tone {
            next: Instant::now(),
            played: 0,
        }))
    }
}

/// Ten milliseconds.
const BLOCK: Duration = Duration::from_millis(10);

impl SoundCapture for Tone {
    fn next_block(&mut self, until: Instant) -> Result<Option<SoundBlock>, SoundError> {
        if self.next > until {
            thread::sleep(until.saturating_duration_since(Instant::now()));
            return Ok(None);
        }
        thread::sleep(self.next.saturating_duration_since(Instant::now()));
        let at = self.next;
        self.next += BLOCK;
        let mut samples = Vec::with_capacity(OPUS_FRAME * 2);
        for n in 0..OPUS_FRAME as u64 {
            let t = (self.played + n) as f32 / 48_000.0;
            let value = 0.25 * (TAU * 440.0 * t).sin();
            samples.extend_from_slice(&[value, value]);
        }
        self.played += OPUS_FRAME as u64;
        Ok(Some(SoundBlock { samples, at }))
    }
}
