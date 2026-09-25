//! What FFmpeg says, written in the product's log.
//!
//! FFmpeg logs through one callback for the whole process, with no room
//! for a pointer of ours: where the lines go is therefore kept here, in
//! a static. Only warnings and worse are written, and at most a handful
//! in any ten seconds: an encoder that complains about every frame
//! would otherwise fill the log sixty times a second. What is left out
//! is counted, and the count written ahead of the first line of the
//! next ten seconds.
//!
//! The crate's own findings go to the same log, under a tag of their
//! own.

use std::ffi::{CStr, c_char, c_int, c_void};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use zyr_proto::log::Log;

use crate::library::Ffmpeg;
use crate::sys;

/// The tag FFmpeg's lines are filed under.
const TAG: &str = "ffmpeg";

/// The tag of the crate's own lines.
const OWN_TAG: &str = "codec";

/// How long an allowance of lines lasts.
const WINDOW: Duration = Duration::from_secs(10);

/// Lines written in one window at most.
const LINES_PER_WINDOW: u32 = 20;

/// Longest line kept, which is far more than FFmpeg ever writes.
const LINE: usize = 1024;

/// Where FFmpeg's lines go, once somebody said.
static SINK: Mutex<Option<Sink>> = Mutex::new(None);

struct Sink {
    /// The FFmpeg whose function puts each line into words, kept loaded
    /// for as long as that function may be called.
    ff: Arc<Ffmpeg>,
    ffmpeg: Log,
    own: Log,
    allowance: Allowance,
}

/// From now on FFmpeg's lines go to `log`.
pub(crate) fn route(ff: &Arc<Ffmpeg>, log: &Log) {
    let sink = Sink {
        ff: Arc::clone(ff),
        ffmpeg: log.about(TAG),
        own: log.about(OWN_TAG),
        allowance: Allowance::new(Instant::now()),
    };
    // A poisoned lock only means a line once failed to be written: the
    // sink is still whole.
    let mut current = SINK.lock().unwrap_or_else(PoisonError::into_inner);
    let replaced = current.replace(sink);
    // SAFETY: the callback matches what av_log_set_callback expects and
    // lives as long as the program.
    unsafe { (ff.logging.set_callback)(Some(write_line)) };
    drop(current);
    // Let go of outside the lock: should that be the last hold on another
    // FFmpeg, it unloads here, and anything it logs on its way out must
    // find the lock free.
    drop(replaced);
}

/// Writes one of the crate's own lines, once FFmpeg's go somewhere.
///
/// Not rate-limited: only called for things that happen a handful of
/// times, never once a frame.
pub(crate) fn note(line: &str) {
    let sink = SINK.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(sink) = sink.as_ref() {
        sink.own.write(line);
    }
}

/// Called by FFmpeg, from any of its threads, for every line it logs.
unsafe extern "C" fn write_line(
    context: *mut c_void,
    level: c_int,
    format: *const c_char,
    args: *mut c_void,
) {
    if level > sys::AV_LOG_WARNING as c_int {
        return;
    }
    let mut sink = SINK.lock().unwrap_or_else(PoisonError::into_inner);
    let Some(sink) = sink.as_mut() else {
        return;
    };
    match sink.allowance.admit(Instant::now()) {
        Admission::LeaveOut => return,
        Admission::Write { left_out: 0 } => {}
        Admission::Write { left_out } => sink.ffmpeg.write(&format!(
            "{left_out} FFmpeg lines left out in the last {} s",
            WINDOW.as_secs()
        )),
    }
    let mut line = [0 as c_char; LINE];
    let mut prefix: c_int = 1;
    // SAFETY: the arguments are FFmpeg's own, passed through untouched;
    // the buffer is as long as the size given and comes back terminated.
    let line = unsafe {
        (sink.ff.logging.format_line)(
            context,
            level,
            format,
            args,
            line.as_mut_ptr(),
            LINE as c_int,
            &mut prefix,
        );
        CStr::from_ptr(line.as_ptr())
    };
    sink.ffmpeg.write(line.to_string_lossy().trim_end());
}

/// How many lines may be written now.
struct Allowance {
    since: Instant,
    written: u32,
    left_out: u32,
}

#[derive(Debug, PartialEq, Eq)]
enum Admission {
    /// Write the line, after saying how many were left out before it.
    Write {
        left_out: u32,
    },
    LeaveOut,
}

impl Allowance {
    fn new(now: Instant) -> Self {
        Self {
            since: now,
            written: 0,
            left_out: 0,
        }
    }

    fn admit(&mut self, now: Instant) -> Admission {
        let mut left_out = 0;
        if now.duration_since(self.since) >= WINDOW {
            left_out = self.left_out;
            *self = Self::new(now);
        }
        if self.written >= LINES_PER_WINDOW {
            self.left_out += 1;
            return Admission::LeaveOut;
        }
        self.written += 1;
        Admission::Write { left_out }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;
    use crate::{GpuVendor, Input, OpusDecoder, probe};

    #[test]
    fn a_burst_is_cut_short_and_what_was_left_out_is_said_after() {
        let start = Instant::now();
        let mut allowance = Allowance::new(start);
        for _ in 0..LINES_PER_WINDOW {
            assert_eq!(allowance.admit(start), Admission::Write { left_out: 0 });
        }
        for _ in 0..5 {
            assert_eq!(allowance.admit(start), Admission::LeaveOut);
        }
        let later = start + WINDOW;
        assert_eq!(allowance.admit(later), Admission::Write { left_out: 5 });
        assert_eq!(allowance.admit(later), Admission::Write { left_out: 0 });
    }

    #[test]
    fn ffmpeg_complaints_and_refused_encoders_reach_the_product_log() {
        let path = std::env::temp_dir()
            .join(format!("zyr-codec-log-{}", std::process::id()))
            .join("engine.log");
        let log = Log::open(&path).unwrap();
        let ff = testing::ffmpeg();
        ff.log_into(&log);

        // A packet announcing a count of frames in a byte that is not
        // there: FFmpeg says so, at error level.
        let mut decoder = OpusDecoder::open(&ff).unwrap();
        assert!(decoder.decode(&[0xff]).is_err());
        // The Linux build has no NVENC: the probe says it left it out.
        probe(&ff, &Input::Cpu, GpuVendor::Nvidia);

        let written = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
        assert!(written.contains(&format!("[{TAG}] ")), "{written}");
        let refused = written
            .lines()
            .find(|line| line.contains(&format!("[{OWN_TAG}] ")) && line.contains("h264_nvenc"));
        assert!(refused.is_some(), "{written}");
    }
}
