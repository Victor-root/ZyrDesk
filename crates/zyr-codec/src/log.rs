//! What FFmpeg says, written in the product's log.
//!
//! FFmpeg logs through one callback for the whole process, with no room
//! for a pointer of ours: where the lines go is therefore kept here, in
//! a static. Only warnings and worse are written, and at most a handful
//! in any ten seconds: an encoder that complains about every frame
//! would otherwise fill the log sixty times a second. What is left out
//! is counted and said once the ten seconds are over.

use std::ffi::{CStr, c_char, c_int, c_void};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use zyr_proto::log::Log;

use crate::library::Ffmpeg;
use crate::sys;

/// The tag FFmpeg's lines are filed under.
const TAG: &str = "ffmpeg";

/// How long an allowance of lines lasts.
const WINDOW: Duration = Duration::from_secs(10);

/// Lines written in one window at most.
const LINES_PER_WINDOW: u32 = 20;

/// Longest line kept, which is far more than FFmpeg ever writes.
const LINE: usize = 1024;

/// Where FFmpeg's lines go, once somebody said.
static SINK: Mutex<Option<Sink>> = Mutex::new(None);

struct Sink {
    log: Log,
    format_line: sys::FormatLogLine,
    allowance: Allowance,
}

/// From now on FFmpeg's lines go to `log`.
pub(crate) fn route(ff: &Ffmpeg, log: &Log) {
    let sink = Sink {
        log: log.about(TAG),
        format_line: ff.logging.format_line,
        allowance: Allowance::new(Instant::now()),
    };
    // A poisoned lock only means a line failed to be written once; the
    // sink itself is replaced whole.
    let mut current = SINK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    *current = Some(sink);
    // SAFETY: the callback matches what av_log_set_callback expects and
    // lives as long as the program.
    unsafe { (ff.logging.set_callback)(Some(write_line)) };
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
    let Ok(mut sink) = SINK.lock() else {
        return;
    };
    let Some(sink) = sink.as_mut() else {
        return;
    };
    match sink.allowance.admit(Instant::now()) {
        Admission::LeaveOut => return,
        Admission::Write { left_out: 0 } => {}
        Admission::Write { left_out } => sink.log.write(&format!(
            "{left_out} FFmpeg lines left out in the last {} s",
            WINDOW.as_secs()
        )),
    }
    let mut line = [0 as c_char; LINE];
    let mut prefix: c_int = 1;
    // SAFETY: the arguments are FFmpeg's own, passed through untouched;
    // the buffer is as long as the size given and comes back terminated.
    let line = unsafe {
        (sink.format_line)(
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
    sink.log.write(line.to_string_lossy().trim_end());
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
    use crate::OpusDecoder;
    use crate::testing;

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
    fn ffmpeg_complaints_reach_the_product_log() {
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

        let written = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
        assert!(written.contains(&format!("[{TAG}] ")), "{written}");
    }
}
