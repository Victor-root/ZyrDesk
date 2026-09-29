//! What the client measures of a session, and shows.
//!
//! Each measure is taken over the last second, from samples kept with
//! the time they were taken. The window is moved on by a timer rather
//! than by the samples themselves, so that a stream that stops shows it:
//! the frame rate falls to nothing instead of staying at its last value.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Span of the windows the measures are taken over.
pub const WINDOW: Duration = Duration::from_secs(1);

/// Samples a window holds at most, the oldest going first: enough for
/// four thousand events a second.
const MOST_SAMPLES: usize = 4096;

/// A snapshot of a session, as the client shows it. A measure not known
/// yet is `None`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Measures {
    /// "H.264", "HEVC" or "AV1".
    pub codec: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Frames received a second.
    pub fps: Option<f64>,
    pub decode_ms: Option<f64>,
    pub render_ms: Option<f64>,
    /// On the host, from capture to sending.
    pub host_ms: Option<f64>,
    /// The round trip through the network.
    pub network_ms: Option<f64>,
    pub network_variance_ms: Option<f64>,
    /// Bits of the frames assembled, a second.
    pub bitrate_mbps: Option<f64>,
    /// Frames lost, out of the frames expected.
    pub dropped_network_pct: Option<f64>,
    /// Frames decoded but never shown, a newer one taking their turn:
    /// not those the host sent again, its screen unchanged.
    pub dropped_jitter_pct: Option<f64>,
    /// Since the last frame reached the decoder, or since the host last
    /// said its screen had not changed from that frame.
    pub since_frame_ms: Option<f64>,
    /// From capture on the host to display here.
    pub latency_ms: Option<f64>,
    /// Between frames shown, the 99th percentile.
    pub frame_interval_p99_ms: Option<f64>,
}

impl Measures {
    /// Every figure on one line, for the journal: what the person reads
    /// in the statistics banner, and the few it does not show.
    pub fn in_a_line(&self) -> String {
        let figure = |value: Option<f64>, decimals: usize, unit: &str| match value {
            Some(value) => format!("{value:.decimals$} {unit}"),
            None => "-".to_string(),
        };
        let stream = match (&self.codec, self.width, self.height) {
            (Some(codec), Some(width), Some(height)) => format!("{codec} {width}x{height}, "),
            _ => String::new(),
        };
        format!(
            "{stream}{} frames/s, {}; host {}, network {} (variance {}), decode {}, display {}, \
             capture to screen {}; lost {}, replaced {}; last frame {} ago, interval 99th {}",
            figure(self.fps, 1, "").trim_end(),
            figure(self.bitrate_mbps, 2, "Mb/s"),
            figure(self.host_ms, 1, "ms"),
            figure(self.network_ms, 1, "ms"),
            figure(self.network_variance_ms, 1, "ms"),
            figure(self.decode_ms, 1, "ms"),
            figure(self.render_ms, 1, "ms"),
            figure(self.latency_ms, 0, "ms"),
            figure(self.dropped_network_pct, 1, "%"),
            figure(self.dropped_jitter_pct, 1, "%"),
            figure(self.since_frame_ms, 0, "ms"),
            figure(self.frame_interval_p99_ms, 1, "ms"),
        )
    }
}

/// Samples of one measure over a sliding window.
#[derive(Debug, Clone)]
pub struct Rolling {
    span: Duration,
    samples: VecDeque<(Instant, f64)>,
}

impl Default for Rolling {
    fn default() -> Self {
        Self::new(WINDOW)
    }
}

impl Rolling {
    pub fn new(span: Duration) -> Self {
        Self {
            span,
            samples: VecDeque::new(),
        }
    }

    pub fn add(&mut self, at: Instant, value: f64) {
        if self.samples.len() == MOST_SAMPLES {
            self.samples.pop_front();
        }
        self.samples.push_back((at, value));
    }

    /// Forgets the samples older than the window, seen from `now`.
    pub fn expire(&mut self, now: Instant) {
        while let Some((at, _)) = self.samples.front() {
            if now.saturating_duration_since(*at) <= self.span {
                break;
            }
            self.samples.pop_front();
        }
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// The sum of the samples, spread over the window: events a second
    /// when each counts one, bits a second when each is a size.
    pub fn per_second(&self) -> f64 {
        self.values().sum::<f64>() / self.span.as_secs_f64()
    }

    pub fn mean(&self) -> Option<f64> {
        (!self.is_empty()).then(|| self.values().sum::<f64>() / self.len() as f64)
    }

    /// How far the samples stray from their mean (standard deviation).
    pub fn deviation(&self) -> Option<f64> {
        let mean = self.mean()?;
        let spread = self
            .values()
            .map(|value| (value - mean).powi(2))
            .sum::<f64>();
        Some((spread / self.len() as f64).sqrt())
    }

    /// The value below which `fraction` of the samples lie (nearest rank).
    pub fn percentile(&self, fraction: f64) -> Option<f64> {
        let mut sorted: Vec<f64> = self.values().collect();
        sorted.sort_by(f64::total_cmp);
        let rank = (fraction.clamp(0.0, 1.0) * sorted.len() as f64).ceil() as usize;
        sorted.get(rank.saturating_sub(1)).copied()
    }

    fn values(&self) -> impl Iterator<Item = f64> {
        self.samples.iter().map(|(_, value)| *value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn the_measures_read_on_one_line() {
        let measures = Measures {
            codec: Some("H.264".to_string()),
            width: Some(1920),
            height: Some(1080),
            fps: Some(59.94),
            decode_ms: Some(1.24),
            render_ms: Some(0.9),
            host_ms: Some(2.3),
            network_ms: Some(1.0),
            network_variance_ms: Some(0.4),
            bitrate_mbps: Some(24.31),
            dropped_network_pct: Some(0.0),
            dropped_jitter_pct: Some(1.25),
            since_frame_ms: Some(8.0),
            latency_ms: Some(41.0),
            frame_interval_p99_ms: Some(17.1),
        };
        assert_eq!(
            measures.in_a_line(),
            "H.264 1920x1080, 59.9 frames/s, 24.31 Mb/s; host 2.3 ms, network 1.0 ms (variance \
             0.4 ms), decode 1.2 ms, display 0.9 ms, capture to screen 41 ms; lost 0.0 %, \
             replaced 1.2 %; last frame 8 ms ago, interval 99th 17.1 ms"
        );
        // Before anything is known, every figure is a dash.
        assert_eq!(
            Measures::default().in_a_line(),
            "- frames/s, -; host -, network - (variance -), decode -, display -, capture to \
             screen -; lost -, replaced -; last frame - ago, interval 99th -"
        );
    }

    #[test]
    fn an_empty_window_knows_nothing() {
        let rolling = Rolling::default();
        assert!(rolling.is_empty());
        assert_eq!(rolling.mean(), None);
        assert_eq!(rolling.deviation(), None);
        assert_eq!(rolling.percentile(0.99), None);
        assert_eq!(rolling.per_second(), 0.0);
    }

    #[test]
    fn a_steady_stream_is_counted_per_second() {
        let at = Instant::now();
        let mut frames = Rolling::default();
        let mut bits = Rolling::default();
        for n in 0..300u64 {
            let now = at + Duration::from_micros(n * 16_667);
            frames.add(now, 1.0);
            bits.add(now, 100_000.0 * 8.0);
            frames.expire(now);
            bits.expire(now);
        }
        // Sixty frames fit in the last second, the sixty-first being
        // older than that by 20 us.
        assert_eq!(frames.per_second(), 60.0);
        assert_eq!(bits.per_second() / 1e6, 48.0);
    }

    #[test]
    fn a_stream_that_stops_falls_to_nothing_with_time() {
        let at = Instant::now();
        let mut frames = Rolling::default();
        for n in 0..60u64 {
            frames.add(at + ms(n * 16), 1.0);
        }
        frames.expire(at + ms(1_500));
        assert!(frames.per_second() < 60.0);
        frames.expire(at + ms(3_000));
        assert!(frames.is_empty());
        assert_eq!(frames.per_second(), 0.0);
    }

    #[test]
    fn mean_deviation_and_percentile_follow_the_samples() {
        let at = Instant::now();
        let mut rolling = Rolling::default();
        for (n, value) in [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0]
            .into_iter()
            .enumerate()
        {
            rolling.add(at + ms(n as u64), value);
        }
        assert_eq!(rolling.mean(), Some(5.0));
        assert_eq!(rolling.deviation(), Some(2.0));
        assert_eq!(rolling.percentile(0.5), Some(4.0));
        assert_eq!(rolling.percentile(0.99), Some(9.0));
        assert_eq!(rolling.percentile(0.0), Some(2.0));
    }

    #[test]
    fn the_window_never_holds_more_than_its_share() {
        let at = Instant::now();
        let mut rolling = Rolling::default();
        for n in 0..10_000u64 {
            rolling.add(at + Duration::from_micros(n), 1.0);
        }
        assert_eq!(rolling.len(), MOST_SAMPLES);
    }
}
