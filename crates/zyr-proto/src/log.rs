//! What the product writes down.
//!
//! A service has no console. Without a written trace, a start-up that
//! fails before anyone logs in leaves nothing to examine: no message, no
//! window, nobody to read it. The window is barely better off: during a
//! session it sits behind the picture, where a message would be seen by
//! nobody. Both write here, in the same shape, so that the two traces
//! can be read side by side.
//!
//! Timestamps are in universal time, without exception. A log that
//! follows local time steps back an hour once a year, and the lines end
//! up out of order at the exact moment one is trying to understand a
//! nighttime incident.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

use time::OffsetDateTime;
use time::format_description::BorrowedFormatItem;
use time::macros::format_description;

const TIMESTAMP: &[BorrowedFormatItem<'static>] =
    format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");

/// Size past which the log is cut back.
///
/// A service runs for months, and a file nothing ever trims grows for
/// exactly that long: reading it back into a window, or asking someone
/// to send it, stops being reasonable long before anyone notices.
const AT_MOST: u64 = 4 * 1024 * 1024;

/// What is kept of the old lines when it is.
///
/// The end, where whatever is being investigated lives.
const KEPT: u64 = 256 * 1024;

/// Log opened in append mode, shared by the whole service.
///
/// Copies share the one open file: the tunnel's tasks write to the same
/// place as the supervisor, and their lines interleave in order.
#[derive(Debug, Clone)]
pub struct Log {
    file: Arc<Mutex<File>>,
}

impl Log {
    /// Opens the log, creating its folder if needed.
    ///
    /// Read and write, not the system's own append. Appending is what
    /// every write does, but done by seeking to the end under the lock
    /// rather than by the file's mode: an append-only file on Windows
    /// may not be cut shorter, and trimming is exactly that. One handle
    /// behind one lock keeps the lines in order all the same.
    pub fn open(path: &Path) -> io::Result<Self> {
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)?;
        Ok(Self {
            file: Arc::new(Mutex::new(file)),
        })
    }

    /// Writes one timestamped line at the end of the file.
    ///
    /// The end is sought every time: the journal screen can empty this
    /// file from another program while the service runs, and a line must
    /// then land at the new top rather than at a remembered place.
    ///
    /// Never fails: a log that refuses to write must not stop the
    /// service it is watching.
    pub fn write(&self, message: &str) {
        let Ok(mut file) = self.file.lock() else {
            return;
        };
        let _ = trimmed(&mut file);
        let _ = file.seek(SeekFrom::End(0));
        let _ = writeln!(file, "{} {message}", now());
        let _ = file.flush();
    }
}

/// Cuts the log back once it has grown past reason, keeping its end.
///
/// Done through the open handle and never by replacing the file: other
/// copies of this log hold the same file, and a file swapped out from
/// under them would take their lines to a ghost.
fn trimmed(file: &mut File) -> io::Result<()> {
    let written = file.metadata()?.len();
    if written <= AT_MOST {
        return Ok(());
    }
    let mut end = vec![0u8; KEPT as usize];
    file.seek(SeekFrom::Start(written - KEPT))?;
    file.read_exact(&mut end)?;
    // From the first whole line: the cut lands mid-line, and half a line
    // at the top would read as a corrupted file.
    let from = end
        .iter()
        .position(|byte| *byte == b'\n')
        .map_or(0, |at| at + 1);
    file.set_len(0)?;
    file.seek(SeekFrom::Start(0))?;
    // Said out loud, so the cut is never taken for a loss.
    file.write_all("(le début de ce journal a été retiré)\n".as_bytes())?;
    file.write_all(&end[from..])
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

    #[test]
    fn a_log_grown_past_reason_is_cut_back_to_its_end() {
        let path = fresh_path("taille");
        let log = Log::open(&path).unwrap();

        // Grossi au-delà de la limite par le fichier directement : y
        // aller ligne à ligne prendrait le plus clair du test.
        {
            let mut file = log.file.lock().unwrap();
            let line = format!("{} du remplissage sans intérêt\n", now());
            let times = (AT_MOST / line.len() as u64) + 2;
            for _ in 0..times {
                file.write_all(line.as_bytes()).unwrap();
            }
        }

        log.write("la ligne qui compte");

        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.len() as u64 <= KEPT + 256, "{}", contents.len());
        // La fin est là, le début est parti, et la coupe est annoncée.
        assert!(contents.ends_with("la ligne qui compte\n"));
        assert!(contents.starts_with("(le début"), "{}", &contents[..60]);
        // Et jamais de demi-ligne en tête : la coupe tombe sur une
        // frontière.
        let second = contents.lines().nth(1).unwrap();
        assert!(second.starts_with(char::is_numeric), "{second}");

        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
