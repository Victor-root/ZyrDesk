//! The service's log.
//!
//! A service has no console. Without a written trace, a start-up that
//! fails before anyone logs in leaves nothing to examine: no message, no
//! window, nobody to read it. Everything the service does goes through
//! here.
//!
//! Timestamps are in universal time, without exception. A log that
//! follows local time steps back an hour once a year, and the lines end
//! up out of order at the exact moment one is trying to understand a
//! nighttime incident.

// Outside Windows nothing calls this module: the service does not exist
// there. It stays compiled and tested everywhere, the logic having
// nothing platform-specific about it, but with no caller it would pass
// for dead code. The exception stops at platforms without a service: on
// Windows, genuinely dead code is still reported.
#![cfg_attr(not(windows), allow(dead_code))]

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::Mutex;

use time::OffsetDateTime;
use time::format_description::BorrowedFormatItem;
use time::macros::format_description;

const TIMESTAMP: &[BorrowedFormatItem<'static>] =
    format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");

/// Log opened in append mode, shared by the whole service.
#[derive(Debug)]
pub struct Log {
    file: Mutex<File>,
}

impl Log {
    /// Opens the log, creating its folder if needed.
    pub fn open(path: &Path) -> io::Result<Self> {
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Self {
            file: Mutex::new(file),
        })
    }

    /// Writes one timestamped line.
    ///
    /// Never fails: a log that refuses to write must not stop the
    /// service it is watching.
    pub fn write(&self, message: &str) {
        let Ok(mut file) = self.file.lock() else {
            return;
        };
        let _ = writeln!(file, "{} {message}", now());
        let _ = file.flush();
    }
}

/// Universal timestamp, or an explicit marker when the clock is
/// unreadable.
fn now() -> String {
    OffsetDateTime::now_utc()
        .format(TIMESTAMP)
        .unwrap_or_else(|_| "date unavailable".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_path(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir()
            .join(format!("zyrdeskd-{}-{name}", std::process::id()))
            .join("service.log");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
        path
    }

    #[test]
    fn the_log_creates_its_folder_and_appends_its_lines() {
        let path = fresh_path("log");
        {
            let log = Log::open(&path).unwrap();
            log.write("first");
            log.write("second");
        }
        // A second opening must not erase the first: a service restarted
        // by Windows would otherwise lose the trace of what felled it.
        {
            let log = Log::open(&path).unwrap();
            log.write("after a restart");
        }

        let contents = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].ends_with("first"), "{}", lines[0]);
        assert!(lines[2].ends_with("after a restart"), "{}", lines[2]);

        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn every_line_carries_a_readable_date() {
        let timestamp = now();
        assert_eq!(timestamp.len(), 19, "{timestamp}");
        assert!(timestamp.contains('-') && timestamp.contains(':'));
    }
}
