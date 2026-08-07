//! What we keep from a burst of packets.
//!
//! The mean says nothing useful about an interactive session: a single
//! packet late out of a thousand shows on screen and vanishes into a
//! mean. It is the tails of the distribution that matter, hence the
//! percentiles.

use std::time::Duration;

/// One measured round trip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct RoundTrip(pub Duration);

/// What one burst produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Outcome {
    pub sent: u64,
    pub returned: u64,
    pub bytes_sent: u64,
    pub duration: Duration,
    pub median: Duration,
    pub percentile_95: Duration,
    pub percentile_99: Duration,
    pub worst: Duration,
}

impl Outcome {
    /// Works out the tally of a burst. The measurements are consumed:
    /// they have to be sorted, and nothing else needs them afterwards.
    pub fn from(
        mut measurements: Vec<RoundTrip>,
        sent: u64,
        bytes_sent: u64,
        duration: Duration,
    ) -> Self {
        measurements.sort_unstable();
        let returned = measurements.len() as u64;
        Self {
            sent,
            returned,
            bytes_sent,
            duration,
            median: percentile(&measurements, 50),
            percentile_95: percentile(&measurements, 95),
            percentile_99: percentile(&measurements, 99),
            worst: measurements.last().map(|m| m.0).unwrap_or_default(),
        }
    }

    pub fn lost(&self) -> u64 {
        self.sent.saturating_sub(self.returned)
    }

    /// Share of packets that never came back, as a percentage.
    pub fn loss(&self) -> f64 {
        if self.sent == 0 {
            return 0.0;
        }
        self.lost() as f64 * 100.0 / self.sent as f64
    }

    /// Rate actually sustained, in megabits per second.
    pub fn rate(&self) -> f64 {
        let seconds = self.duration.as_secs_f64();
        if seconds <= 0.0 {
            return 0.0;
        }
        self.bytes_sent as f64 * 8.0 / seconds / 1_000_000.0
    }
}

/// Value the requested percentile falls under.
///
/// The measurements have to be sorted.
fn percentile(measurements: &[RoundTrip], rank: u32) -> Duration {
    if measurements.is_empty() {
        return Duration::ZERO;
    }
    // Percentile 100 must name the last measurement, not go past it.
    let place = (measurements.len() as u64 * rank as u64 / 100) as usize;
    measurements[place.min(measurements.len() - 1)].0
}

/// Milliseconds to two decimals: below that, measurement noise exceeds
/// the precision shown.
pub fn milliseconds(duration: Duration) -> String {
    format!("{:.2} ms", duration.as_secs_f64() * 1000.0)
}

/// Signed gap between two durations, as a report reads it.
pub fn gap(reference: Duration, measured: Duration) -> String {
    if measured >= reference {
        format!("+{}", milliseconds(measured - reference))
    } else {
        format!("-{}", milliseconds(reference - measured))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn measurements(millis: &[u64]) -> Vec<RoundTrip> {
        millis
            .iter()
            .map(|m| RoundTrip(Duration::from_millis(*m)))
            .collect()
    }

    #[test]
    fn the_percentiles_name_the_right_measurements() {
        let hundred: Vec<u64> = (1..=100).collect();
        let outcome = Outcome::from(measurements(&hundred), 100, 0, Duration::from_secs(1));
        assert_eq!(outcome.median, Duration::from_millis(51));
        assert_eq!(outcome.percentile_95, Duration::from_millis(96));
        assert_eq!(outcome.percentile_99, Duration::from_millis(100));
        assert_eq!(outcome.worst, Duration::from_millis(100));
    }

    #[test]
    fn measurements_arriving_out_of_order_are_put_back_in_order() {
        // Sorted: 1, 2, 4, 30, 50.
        let outcome = Outcome::from(
            measurements(&[50, 1, 30, 2, 4]),
            5,
            0,
            Duration::from_secs(1),
        );
        assert_eq!(outcome.median, Duration::from_millis(4));
        assert_eq!(outcome.worst, Duration::from_millis(50));
    }

    #[test]
    fn packets_that_never_came_back_are_counted() {
        let outcome = Outcome::from(measurements(&[1, 2, 3]), 4, 0, Duration::from_secs(1));
        assert_eq!(outcome.lost(), 1);
        assert_eq!(outcome.loss(), 25.0);
    }

    #[test]
    fn a_burst_with_no_return_blows_nothing_up() {
        let outcome = Outcome::from(Vec::new(), 0, 0, Duration::ZERO);
        assert_eq!(outcome.median, Duration::ZERO);
        assert_eq!(outcome.worst, Duration::ZERO);
        assert_eq!(outcome.loss(), 0.0);
        assert_eq!(outcome.rate(), 0.0);
    }

    #[test]
    fn the_rate_matches_what_went_out() {
        // 1 250 000 bytes in one second, which is 10 Mb/s.
        let outcome = Outcome::from(Vec::new(), 0, 1_250_000, Duration::from_secs(1));
        assert!((outcome.rate() - 10.0).abs() < 0.001, "{}", outcome.rate());
    }

    #[test]
    fn the_gap_reads_in_both_directions() {
        let short = Duration::from_micros(1500);
        let long = Duration::from_micros(2300);
        assert_eq!(gap(short, long), "+0.80 ms");
        assert_eq!(gap(long, short), "-0.80 ms");
        assert_eq!(gap(short, short), "+0.00 ms");
    }
}
