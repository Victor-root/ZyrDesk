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
//!
//! # What a line is filed under
//!
//! Every line carries a tag, written between brackets after the date:
//! the part of the product that wrote it. One tag to a module, which is
//! the only rule that keeps them worth anything: a tag somebody has to
//! look up is a tag nobody types.
//!
//! It is what makes a journal answerable. Six lines about the clipboard
//! sit inside four thousand about a session, and the difference between
//! reading those six and reading all four thousand is that they can be
//! asked for by name. `zyr_proto::journal` is where the asking happens.
//!
//! # The two voices
//!
//! Everything is written, always. What the two voices decide is how a
//! line is found again, not whether it exists.
//!
//! [`Log::write`] is the product saying what it did, what it refused,
//! what it found.
//!
//! [`Log::debug`] is what counts, measures, or narrates a piece of
//! plumbing that worked. Twenty streams opening and closing cleanly, a
//! socket going quiet for a second while two computers find each other:
//! true, and drowning everything else when one is looking for something
//! else entirely.
//!
//! It was gated once, first on the kind of build and then on a file, and
//! both were wrong for the same reason: a line that has to be turned on
//! is a line that is not there on the evening it is wanted, and turning
//! it on is a chore asked of somebody who is already stuck. The sifting
//! is what separates the two now, by the letter each line carries, and it
//! separates them after the fact rather than before: `-level:debug`
//! leaves the plumbing out, and nothing had to be decided in advance.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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

/// What a line is filed under when nobody said.
///
/// A real word rather than a blank, because a blank tag is a line that
/// no filter can ever name: whoever is looking for it would have to ask
/// for everything, which is the thing tags exist to avoid.
pub const OTHERWISE: &str = "zyrdesk";

/// The two voices a line can be written in, one letter each.
///
/// One says what the product did, and is there in every build. The other
/// is what a hunt wants and nothing else, and only a build made for
/// hunting carries it. They sit in the same file, in the order things
/// happened, because a hunt is exactly the moment the two are read
/// against each other.
pub const SAYS: char = 'I';
pub const HUNTS: char = 'D';

/// Log opened in append mode, shared by the whole service.
///
/// Copies share the one open file: the tunnel's tasks write to the same
/// place as the supervisor, and their lines interleave in order.
#[derive(Debug, Clone)]
pub struct Log {
    file: Arc<Mutex<File>>,
    /// What lines written through this copy are filed under.
    ///
    /// On the copy and not on the call, which is the whole of what makes
    /// tags stay right: a module takes its own copy once and everything
    /// it writes afterwards is filed under it, with nothing to remember
    /// and nothing to keep in step at three hundred call sites.
    tag: &'static str,
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
            tag: OTHERWISE,
        })
    }

    /// The same journal, writing under that tag.
    ///
    /// The same file and the same lock, so lines from every part of the
    /// product still land in one file in the order they happened. What
    /// changes is only what each of them is filed under.
    ///
    /// Taken once where a module holds its journal, rather than at each
    /// line: a tag chosen afresh three hundred times is three hundred
    /// chances to spell it differently.
    pub fn about(&self, tag: &'static str) -> Self {
        Self {
            file: Arc::clone(&self.file),
            tag,
        }
    }

    /// What lines written through this copy are filed under.
    pub fn tag(&self) -> &'static str {
        self.tag
    }

    /// Writes one timestamped line at the end of the file.
    ///
    /// What the product says of itself: what it did, what it refused,
    /// what it found. Kept in every build, because this is the line
    /// somebody sends when something goes wrong on their machine, and a
    /// product that explains itself only to its own author explains
    /// itself to nobody.
    ///
    /// The end is sought every time: the journal screen can empty this
    /// file from another program while the service runs, and a line must
    /// then land at the new top rather than at a remembered place.
    ///
    /// Never fails: a log that refuses to write must not stop the
    /// service it is watching.
    pub fn write(&self, message: &str) {
        self.said(SAYS, message);
    }

    /// Writes a line nobody wants except while hunting something.
    ///
    /// What counts, measures, or narrates a piece of plumbing that
    /// worked. Written always, like the other voice: the letter it
    /// carries is what sets it apart, for the sifting to leave it out or
    /// to ask for it alone.
    pub fn debug(&self, message: &str) {
        self.said(HUNTS, message);
    }

    /// Puts one line down, in whichever voice.
    fn said(&self, voice: char, message: &str) {
        let Ok(mut file) = self.file.lock() else {
            return;
        };
        let _ = trimmed(&mut file);
        let _ = file.seek(SeekFrom::End(0));
        let _ = writeln!(file, "{} {voice} [{}] {message}", now(), self.tag);
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
    file.write_all("(the beginning of this journal was removed)\n".as_bytes())?;
    file.write_all(&end[from..])
}

/// How often a line about something that repeats is written, at most.
const SELDOM_EVERY: Duration = Duration::from_secs(10);

/// Saying something that repeats, without saying it at every turn.
///
/// Nothing goes without a word, but a place that fails sixty times a
/// second would drown the journal: the first time is said at once, then
/// at most once every ten seconds, with how many times it happened in
/// between.
#[derive(Debug, Clone, Default)]
pub struct Seldom {
    next: Option<Instant>,
    unsaid: u64,
}

impl Seldom {
    /// Whether to say it now, and if so how many times it happened
    /// unsaid since the last time it was said.
    pub fn allow(&mut self, now: Instant) -> Option<u64> {
        if self.next.is_some_and(|next| now < next) {
            self.unsaid += 1;
            return None;
        }
        self.next = now.checked_add(SELDOM_EVERY);
        Some(std::mem::take(&mut self.unsaid))
    }

    /// Notes one more time it happened, and writes `line` in `log`, given
    /// how many times that makes since the last line, this one included,
    /// when a line is due.
    pub fn note(&mut self, log: &Log, now: Instant, line: impl FnOnce(u64) -> String) {
        if let Some(unsaid) = self.allow(now) {
            log.write(&line(unsaid + 1));
        }
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

    #[test]
    fn what_repeats_is_said_the_first_time_then_once_in_a_while_with_the_count() {
        let start = Instant::now();
        let second = Duration::from_secs(1);
        let mut seldom = Seldom::default();
        assert_eq!(seldom.allow(start), Some(0));
        for n in 1..=5 {
            assert_eq!(seldom.allow(start + n * second), None);
        }
        assert_eq!(seldom.allow(start + 10 * second), Some(5));
        assert_eq!(seldom.allow(start + 11 * second), None);
        assert_eq!(seldom.allow(start + 30 * second), Some(1));
    }

    #[test]
    fn a_line_about_what_repeats_counts_itself_in() {
        let path = fresh_path("seldom");
        let log = Log::open(&path).unwrap();
        let mut seldom = Seldom::default();
        let at = Instant::now();
        for n in 0..25u64 {
            seldom.note(&log, at + Duration::from_secs(n), |times| {
                format!("{times} times")
            });
        }
        let written = std::fs::read_to_string(&path).unwrap();
        let counts: Vec<&str> = written
            .lines()
            .map(|line| line.rsplit("] ").next().unwrap())
            .collect();
        assert_eq!(counts, ["1 times", "10 times", "10 times"]);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

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
    fn every_line_carries_its_tag_and_the_file_stays_the_same() {
        // This is what makes it possible to ask for six lines out of
        // four thousand: the tag is on the line, and a tagged copy
        // writes into the same file and in the same order.
        let path = fresh_path("tags");
        let log = Log::open(&path).unwrap();
        let clipboard = log.about("clipboard");

        log.write("under no particular tag");
        clipboard.write("what this computer holds");
        log.write("and what follows");

        let contents = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 3, "one file for both copies");
        assert!(
            lines[0].contains(&format!("[{OTHERWISE}] ")),
            "{}",
            lines[0]
        );
        assert!(lines[1].contains("[clipboard] "), "{}", lines[1]);
        assert!(lines[1].ends_with("what this computer holds"));
        assert!(
            lines[2].contains(&format!("[{OTHERWISE}] ")),
            "{}",
            lines[2]
        );
        assert_eq!(clipboard.tag(), "clipboard");

        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn both_voices_are_written_in_the_order_things_happened() {
        // Both in the same file and in that order: a hunt is precisely
        // the moment when one is read against the other, and it is the
        // sift that separates them afterwards, by the letter each line
        // carries.
        let path = fresh_path("voices");
        let log = Log::open(&path).unwrap();

        log.write("what the product did");
        log.debug("what only a hunt wants");

        let contents = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 2, "{contents}");
        assert!(lines[0].contains(&format!(" {SAYS} [")), "{}", lines[0]);
        assert!(lines[1].contains(&format!(" {HUNTS} [")), "{}", lines[1]);

        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn a_log_grown_past_reason_is_cut_back_to_its_end() {
        let path = fresh_path("size");
        let log = Log::open(&path).unwrap();

        // Grown past the limit through the file directly: getting
        // there line by line would take up most of the test.
        {
            let mut file = log.file.lock().unwrap();
            let line = format!("{} filler of no interest\n", now());
            let times = (AT_MOST / line.len() as u64) + 2;
            for _ in 0..times {
                file.write_all(line.as_bytes()).unwrap();
            }
        }

        log.write("the line that counts");

        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.len() as u64 <= KEPT + 256, "{}", contents.len());
        // The end is there, the beginning is gone, and the cut is
        // announced.
        assert!(contents.ends_with("the line that counts\n"));
        assert!(
            contents.starts_with("(the beginning"),
            "{}",
            &contents[..60]
        );
        // And never half a line at the top: the cut falls on a
        // boundary.
        let second = contents.lines().nth(1).unwrap();
        assert!(second.starts_with(char::is_numeric), "{second}");

        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
