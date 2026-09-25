//! The host engine on Windows: the screen through Desktop Duplication
//! and Direct3D 11, keys and pointer through SendInput, the sound
//! through WASAPI.
//!
//! Every call Windows can refuse is checked, and a refusal is written
//! to the log with what was being done and Windows' own code, in hex,
//! so that one line of the journal says what broke on which machine.

mod capture;
mod convert;
mod desktop;
mod device;
mod displays;
mod inject;
mod sound;
mod tuning;

use std::sync::Arc;
use std::time::{Duration, Instant};

use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use zyr_codec::Ffmpeg;
use zyr_proto::log::Log;

use crate::Parts;
use crate::parts::{Injector, Screen};

pub use tuning::Tuned;

/// The engine's parts on this computer.
pub fn parts(ffmpeg: Arc<Ffmpeg>, log: &Log) -> Parts {
    let screen_log = log.clone();
    let input_log = log.clone();
    Parts {
        ffmpeg: Arc::clone(&ffmpeg),
        screen: Box::new(move || {
            capture::DuplicatedScreen::new(screen_log)
                .map(|screen| Box::new(screen) as Box<dyn Screen>)
        }),
        injector: Box::new(move || {
            Box::new(inject::SendInputInjector::new(input_log)) as Box<dyn Injector>
        }),
        sound: Box::new(sound::Loopback::new(ffmpeg, log.clone())),
    }
}

/// A refusal of Windows, as the log says it: what was being done, the
/// code in hex, and Windows' own words.
fn failed(what: &str, e: &windows::core::Error) -> String {
    format!("{what} failed: 0x{:08X} {}", e.code().0 as u32, e.message())
}

/// The performance counter, which Windows stamps pictures and sound
/// with.
struct Counter {
    per_second: i64,
}

impl Counter {
    fn new() -> Self {
        let mut per_second = 0;
        // SAFETY: a plain out value. It never fails on a system that
        // runs Windows 10; nought is kept out below all the same.
        let _ = unsafe { QueryPerformanceFrequency(&mut per_second) };
        Self {
            per_second: per_second.max(1),
        }
    }

    /// When the counter read `ticks`, on the monotonic clock.
    fn instant(&self, ticks: i64) -> Instant {
        let mut now_ticks = 0;
        // SAFETY: a plain out value.
        let _ = unsafe { QueryPerformanceCounter(&mut now_ticks) };
        let now = Instant::now();
        let behind = u128::try_from(now_ticks.saturating_sub(ticks)).unwrap_or(0);
        let nanos = behind * 1_000_000_000 / self.per_second as u128;
        now.checked_sub(Duration::from_nanos(
            u64::try_from(nanos).unwrap_or(u64::MAX),
        ))
        .unwrap_or(now)
    }

    /// The counter now, in the 100-nanosecond units sound is stamped in.
    fn now_in_hundreds_of_nanoseconds(&self) -> i64 {
        let mut now_ticks = 0;
        // SAFETY: a plain out value.
        let _ = unsafe { QueryPerformanceCounter(&mut now_ticks) };
        i64::try_from(i128::from(now_ticks) * 10_000_000 / i128::from(self.per_second))
            .unwrap_or(i64::MAX)
    }
}
