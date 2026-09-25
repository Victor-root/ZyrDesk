//! The measures a session shows, taken apart from the pictures.
//!
//! The video and link threads note what happens as it happens; a timer
//! of its own turns those notes into [`Measures`] five times a second.
//! The picture stopping is exactly when there is something to say, so
//! nothing here waits for a picture: the frame rate falls and the time
//! since the last frame grows while the decoder has nothing to do.

use std::sync::Mutex;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use zyr_media::clock::ClockOffset;
use zyr_media::codec::VideoCodec;
use zyr_media::stats::{Measures, Rolling};
use zyr_proto::log::Log;

use crate::lock;
use crate::tallies::Tallies;

/// The tag of the measures' lines in the journal.
pub const TAG: &str = "measures";

/// How often the measures are taken again.
pub const EVERY: Duration = Duration::from_millis(200);

/// How often the counters go to the journal, as one line.
const SUMMARY_EVERY: Duration = Duration::from_secs(10);

/// Round trips are few (two a second): their spread is taken over
/// this much time to mean anything.
const ROUND_TRIPS_OVER: Duration = Duration::from_secs(5);

/// How long the tunnel's own round trip stands once told.
const TUNNEL_FRESH: Duration = Duration::from_secs(3);

/// Pictures decoded in the window before the share replaced says
/// anything: over fewer, one picture alone is several percent, and a
/// single one replaced reads as a link that shakes.
const REPLACED_OUT_OF: usize = 30;

/// The player's clock, in microseconds since it started: what pings
/// carry and what the host's clock is placed against.
#[derive(Debug, Clone, Copy)]
pub struct Clock {
    epoch: Instant,
}

impl Clock {
    pub fn new() -> Self {
        Self {
            epoch: Instant::now(),
        }
    }

    pub fn us(&self, at: Instant) -> u64 {
        u64::try_from(at.saturating_duration_since(self.epoch).as_micros()).unwrap_or(u64::MAX)
    }
}

/// What happened lately, noted by the threads that saw it.
#[derive(Debug)]
pub struct Tally {
    clock: Clock,
    offset: ClockOffset,
    codec: Option<VideoCodec>,
    size: Option<(u32, u32)>,
    /// One per frame assembled.
    frames: Rolling,
    /// The bits of each frame assembled.
    bits: Rolling,
    /// One per frame lost.
    lost: Rolling,
    /// One per frame decoded.
    decoded: Rolling,
    /// One per frame decoded and never shown.
    unshown: Rolling,
    decode_ms: Rolling,
    render_ms: Rolling,
    host_ms: Rolling,
    latency_ms: Rolling,
    /// Between two pictures shown.
    interval_ms: Rolling,
    round_trip_ms: Rolling,
    tunnel_ms: Option<(Instant, f64)>,
    last_frame: Option<Instant>,
    last_shown: Option<Instant>,
    /// Whether any frame came yet, whole or lost: from then on, a second
    /// without any is a rate of nothing, not an unknown one.
    began: bool,
}

impl Tally {
    pub fn new(clock: Clock) -> Self {
        Self {
            clock,
            offset: ClockOffset::new(),
            codec: None,
            size: None,
            frames: Rolling::default(),
            bits: Rolling::default(),
            lost: Rolling::default(),
            decoded: Rolling::default(),
            unshown: Rolling::default(),
            decode_ms: Rolling::default(),
            render_ms: Rolling::default(),
            host_ms: Rolling::default(),
            latency_ms: Rolling::default(),
            interval_ms: Rolling::default(),
            round_trip_ms: Rolling::new(ROUND_TRIPS_OVER),
            tunnel_ms: None,
            last_frame: None,
            last_shown: None,
            began: false,
        }
    }

    /// A new stream: its codec and the size of its pictures.
    pub fn streaming(&mut self, codec: VideoCodec, width: u32, height: u32) {
        self.codec = Some(codec);
        self.size = Some((width, height));
    }

    /// A frame came whole out of the assembler.
    pub fn assembled(&mut self, now: Instant, bytes: usize, host_latency_us: u32) {
        self.began = true;
        self.frames.add(now, 1.0);
        self.bits.add(now, bytes as f64 * 8.0);
        self.host_ms.add(now, f64::from(host_latency_us) / 1000.0);
    }

    /// A frame could not be completed.
    pub fn lost(&mut self, now: Instant) {
        self.began = true;
        self.lost.add(now, 1.0);
    }

    /// A frame reached the decoder, which took that long with it.
    pub fn decoded(&mut self, now: Instant, took: Duration) {
        self.last_frame = Some(now);
        self.decoded.add(now, 1.0);
        self.decode_ms.add(now, ms(took));
    }

    /// Pictures decoded that a newer one replaced before they were
    /// shown.
    ///
    /// Not counted before the first picture is shown: the surface is
    /// still being made then, and the pictures it lets pass are the
    /// start of the session, not ones the link brought too late.
    pub fn unshown(&mut self, now: Instant, count: u64) {
        if self.last_shown.is_none() {
            return;
        }
        for _ in 0..count {
            self.unshown.add(now, 1.0);
        }
    }

    /// A picture captured at `captured_us` on the host's clock was shown
    /// at `now`, drawing it having taken `render`.
    pub fn shown(&mut self, now: Instant, render: Duration, captured_us: u32) {
        self.render_ms.add(now, ms(render));
        if let Some(before) = self.last_shown.replace(now) {
            self.interval_ms
                .add(now, ms(now.saturating_duration_since(before)));
        }
        let local_now = self.clock.us(now);
        if let Some(captured) = self.offset.captured_to_local(captured_us, local_now) {
            // Below zero is the estimate of the host's clock erring by
            // more than the latency itself, never a picture from the
            // future.
            let latency_us = (i128::from(local_now) - i128::from(captured)).max(0);
            self.latency_ms.add(now, latency_us as f64 / 1000.0);
        }
    }

    /// A pong came back at `now` for a ping sent at `sent_us`, the
    /// engine having answered at `host_us` on its own clock.
    pub fn pong(&mut self, now: Instant, sent_us: u64, host_us: u64) {
        if let Some(rtt_us) = self.offset.add(sent_us, host_us, self.clock.us(now)) {
            self.round_trip_ms.add(now, rtt_us as f64 / 1000.0);
        }
    }

    /// The tunnel's own round trip, as the service measures it.
    pub fn tunnel(&mut self, now: Instant, rtt_us: u32) {
        self.tunnel_ms = Some((now, f64::from(rtt_us) / 1000.0));
    }

    /// The measures as they stand at `now`.
    pub fn measures(&mut self, now: Instant) -> Measures {
        for rolling in [
            &mut self.frames,
            &mut self.bits,
            &mut self.lost,
            &mut self.decoded,
            &mut self.unshown,
            &mut self.decode_ms,
            &mut self.render_ms,
            &mut self.host_ms,
            &mut self.latency_ms,
            &mut self.interval_ms,
            &mut self.round_trip_ms,
        ] {
            rolling.expire(now);
        }
        let expected = self.frames.len() + self.lost.len();
        let tunnel = self
            .tunnel_ms
            .filter(|(at, _)| now.saturating_duration_since(*at) <= TUNNEL_FRESH)
            .map(|(_, rtt)| rtt);
        Measures {
            codec: self.codec.map(|codec| codec.name().to_string()),
            width: self.size.map(|(width, _)| width),
            height: self.size.map(|(_, height)| height),
            fps: self.began.then(|| self.frames.per_second()),
            decode_ms: self.decode_ms.mean(),
            render_ms: self.render_ms.mean(),
            host_ms: self.host_ms.mean(),
            network_ms: tunnel.or_else(|| self.round_trip_ms.mean()),
            network_variance_ms: self.round_trip_ms.deviation(),
            bitrate_mbps: self.began.then(|| self.bits.per_second() / 1e6),
            dropped_network_pct: (expected > 0)
                .then(|| self.lost.len() as f64 * 100.0 / expected as f64),
            dropped_jitter_pct: (self.decoded.len() >= REPLACED_OUT_OF)
                .then(|| self.unshown.len() as f64 * 100.0 / self.decoded.len() as f64),
            since_frame_ms: self
                .last_frame
                .map(|at| ms(now.saturating_duration_since(at))),
            latency_ms: self.latency_ms.mean(),
            frame_interval_p99_ms: self.interval_ms.percentile(0.99),
        }
    }
}

/// In milliseconds, from whole nanoseconds, so that a whole number of
/// them comes out exact.
fn ms(duration: Duration) -> f64 {
    duration.as_nanos() as f64 / 1e6
}

/// The stats thread: takes the measures every [`EVERY`] until `stop`
/// is dropped, and writes the counters down now and then and at the
/// end.
pub fn run(
    tally: &Mutex<Tally>,
    measures: &Mutex<Measures>,
    tallies: &Mutex<Tallies>,
    stop: &Receiver<()>,
    log: &Log,
) {
    let mut summary_at = Instant::now() + SUMMARY_EVERY;
    // Nothing is ever sent on `stop`: it only goes away.
    while let Err(RecvTimeoutError::Timeout) = stop.recv_timeout(EVERY) {
        let now = Instant::now();
        let taken = lock(tally).measures(now);
        *lock(measures) = taken;
        if now >= summary_at {
            summary_at = now + SUMMARY_EVERY;
            let counters = lock(tallies).clone();
            log.debug(|| format!("so far: {counters}"));
        }
    }
    log.write(&format!("at the end: {}", lock(tallies)));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms_after(at: Instant, n: u64) -> Instant {
        at + Duration::from_millis(n)
    }

    #[test]
    fn nothing_is_known_before_anything_came() {
        let at = Instant::now();
        let mut tally = Tally::new(Clock { epoch: at });
        assert_eq!(tally.measures(at), Measures::default());
    }

    #[test]
    fn a_steady_stream_gives_every_measure() {
        let at = Instant::now();
        let clock = Clock { epoch: at };
        let mut tally = Tally::new(clock);
        tally.streaming(VideoCodec::Hevc, 1920, 1080);
        // The host's clock runs 7 s ahead; the round trip takes 10 ms,
        // evenly split.
        let host_ahead_us = 7_000_000u64;
        tally.pong(
            ms_after(at, 20),
            clock.us(ms_after(at, 10)),
            15_000 + host_ahead_us,
        );
        tally.tunnel(ms_after(at, 20), 4_000);
        for n in 0..60u64 {
            let arrived = ms_after(at, 100 + n * 16);
            tally.assembled(arrived, 12_500, 3_000);
            tally.decoded(arrived, Duration::from_millis(2));
            let shown = arrived + Duration::from_millis(1);
            // Captured 30 ms before being shown, on the host's clock.
            let captured = clock.us(shown) + host_ahead_us - 30_000;
            tally.shown(shown, Duration::from_millis(1), captured as u32);
        }
        tally.lost(ms_after(at, 1_000));
        let measures = tally.measures(ms_after(at, 1_050));
        assert_eq!(measures.codec.as_deref(), Some("HEVC"));
        assert_eq!((measures.width, measures.height), (Some(1920), Some(1080)));
        assert_eq!(measures.fps, Some(60.0));
        assert_eq!(measures.bitrate_mbps, Some(6.0));
        assert_eq!(measures.decode_ms, Some(2.0));
        assert_eq!(measures.render_ms, Some(1.0));
        assert_eq!(measures.host_ms, Some(3.0));
        assert_eq!(measures.network_ms, Some(4.0));
        assert_eq!(measures.network_variance_ms, Some(0.0));
        let dropped = measures.dropped_network_pct.unwrap();
        assert!((dropped - 100.0 / 61.0).abs() < 1e-9, "{dropped}");
        assert_eq!(measures.dropped_jitter_pct, Some(0.0));
        assert_eq!(measures.since_frame_ms, Some(1050.0 - 100.0 - 59.0 * 16.0));
        let latency = measures.latency_ms.unwrap();
        assert!((latency - 30.0).abs() < 0.01, "{latency}");
        assert_eq!(measures.frame_interval_p99_ms, Some(16.0));
    }

    #[test]
    fn a_stream_that_stops_shows_it() {
        let at = Instant::now();
        let mut tally = Tally::new(Clock { epoch: at });
        for n in 0..30u64 {
            let arrived = ms_after(at, n * 33);
            tally.assembled(arrived, 1_000, 0);
            tally.decoded(arrived, Duration::from_millis(1));
        }
        let measures = tally.measures(ms_after(at, 5_000));
        assert_eq!(measures.fps, Some(0.0));
        assert_eq!(measures.bitrate_mbps, Some(0.0));
        assert_eq!(measures.decode_ms, None);
        assert_eq!(measures.dropped_network_pct, None);
        assert_eq!(measures.since_frame_ms, Some(5_000.0 - 29.0 * 33.0));
    }

    #[test]
    fn frames_waiting_for_a_key_frame_are_still_received() {
        let at = Instant::now();
        let mut tally = Tally::new(Clock { epoch: at });
        for n in 0..10u64 {
            tally.assembled(ms_after(at, n * 10), 1_000, 0);
        }
        let measures = tally.measures(ms_after(at, 100));
        assert_eq!(measures.fps, Some(10.0));
        assert_eq!(measures.decode_ms, None);
        assert_eq!(measures.since_frame_ms, None);
    }

    #[test]
    fn the_round_trip_stands_in_for_the_tunnel_until_it_speaks() {
        let at = Instant::now();
        let clock = Clock { epoch: at };
        let mut tally = Tally::new(clock);
        tally.pong(ms_after(at, 8), 0, 1_000_000);
        tally.pong(ms_after(at, 512), clock.us(ms_after(at, 500)), 2_000_000);
        let measures = tally.measures(ms_after(at, 600));
        assert_eq!(measures.network_ms, Some(10.0));
        assert_eq!(measures.network_variance_ms, Some(2.0));
        tally.tunnel(ms_after(at, 600), 7_500);
        assert_eq!(tally.measures(ms_after(at, 700)).network_ms, Some(7.5));
        // Told once and never again: it goes stale.
        assert_eq!(tally.measures(ms_after(at, 4_000)).network_ms, Some(10.0));
    }

    #[test]
    fn pictures_replaced_before_being_shown_count_as_jitter() {
        let at = Instant::now();
        let mut tally = Tally::new(Clock { epoch: at });
        tally.shown(at, Duration::from_millis(1), 0);
        for n in 0..40u64 {
            tally.decoded(ms_after(at, n), Duration::from_millis(1));
        }
        tally.unshown(ms_after(at, 39), 4);
        assert_eq!(
            tally.measures(ms_after(at, 50)).dropped_jitter_pct,
            Some(10.0)
        );
    }

    #[test]
    fn the_start_of_a_session_is_not_jitter() {
        let at = Instant::now();
        let mut tally = Tally::new(Clock { epoch: at });
        for n in 0..8u64 {
            tally.decoded(ms_after(at, n), Duration::from_millis(1));
        }
        // Replaced while the surface was still being made.
        tally.unshown(ms_after(at, 7), 3);
        tally.shown(ms_after(at, 8), Duration::from_millis(1), 0);
        assert_eq!(tally.measures(ms_after(at, 9)).dropped_jitter_pct, None);
        for n in 10..40u64 {
            tally.decoded(ms_after(at, n), Duration::from_millis(1));
        }
        assert_eq!(
            tally.measures(ms_after(at, 50)).dropped_jitter_pct,
            Some(0.0)
        );
    }
}
