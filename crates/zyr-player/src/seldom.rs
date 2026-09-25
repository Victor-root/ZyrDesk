//! Lines about what may happen once a frame, written once in a while.
//!
//! Nothing is thrown away without a word, but a word per datagram would
//! drown the journal: the first time is said at once, then at most once
//! every ten seconds, with how many times it happened meanwhile.

use std::time::{Duration, Instant};

use zyr_proto::log::Log;

const SPAN: Duration = Duration::from_secs(10);

#[derive(Debug, Default)]
pub struct Seldom {
    said_at: Option<Instant>,
    /// Times it happened since the last line, this one included.
    unsaid: u64,
}

impl Seldom {
    pub fn new() -> Self {
        Self::default()
    }

    /// Notes one more time it happened, and writes `line`, given how many
    /// times that makes since the last line, when a line is due.
    pub fn note(&mut self, log: &Log, now: Instant, line: impl FnOnce(u64) -> String) {
        self.unsaid += 1;
        if self
            .said_at
            .is_some_and(|at| now.saturating_duration_since(at) < SPAN)
        {
            return;
        }
        log.write(&line(self.unsaid));
        self.said_at = Some(now);
        self.unsaid = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_time_is_said_then_once_in_a_while_with_the_count() {
        let path = std::env::temp_dir()
            .join(format!("zyr-player-seldom-{}", std::process::id()))
            .join("player.log");
        let _ = std::fs::remove_file(&path);
        let log = Log::open(&path).unwrap();
        let mut seldom = Seldom::new();
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
}
