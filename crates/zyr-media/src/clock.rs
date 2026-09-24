//! The host's clock, as seen from the client.
//!
//! Each ping carries when it left, on the player's clock; its pong adds
//! when the engine answered, on the host's, and the player notes when
//! the pong came back. Taking the answer to have been given halfway
//! through the round trip places the host's clock against the player's.
//! The halves of a round trip are rarely equal, and the shortest round
//! trip is the one where they can differ least: the estimate is the one
//! from the shortest of the last 16.

/// Round trips remembered.
const SAMPLES: usize = 16;

#[derive(Debug, Clone, Copy)]
struct Sample {
    rtt_us: u64,
    /// Host clock minus local clock.
    offset_us: i64,
}

#[derive(Debug, Clone, Default)]
pub struct ClockOffset {
    samples: [Option<Sample>; SAMPLES],
    /// Where the next sample goes, replacing the oldest.
    next: usize,
}

impl ClockOffset {
    pub fn new() -> Self {
        Self::default()
    }

    /// Takes the times of one ping and its pong, in microseconds, and
    /// gives back its round trip; nothing when the times cannot be (an
    /// answer before the question).
    pub fn add(&mut self, sent_us: u64, host_us: u64, received_us: u64) -> Option<u64> {
        let rtt_us = received_us.checked_sub(sent_us)?;
        let halfway = sent_us + rtt_us / 2;
        let offset_us = i64::try_from(i128::from(host_us) - i128::from(halfway)).ok()?;
        self.samples[self.next] = Some(Sample { rtt_us, offset_us });
        self.next = (self.next + 1) % SAMPLES;
        Some(rtt_us)
    }

    /// The round trip the estimate comes from.
    pub fn rtt_us(&self) -> Option<u64> {
        self.best().map(|sample| sample.rtt_us)
    }

    /// Where a time on the host's clock falls on the local one.
    pub fn host_to_local(&self, host_us: u64) -> Option<i64> {
        let best = self.best()?;
        i64::try_from(i128::from(host_us) - i128::from(best.offset_us)).ok()
    }

    /// Where a host time given by its low 32 bits, as the video and audio
    /// headers carry it, falls on the local clock: the full host time
    /// taken is the one with those bits closest to the host's now.
    pub fn captured_to_local(&self, captured_us: u32, local_now_us: u64) -> Option<i64> {
        let best = self.best()?;
        let host_now = i128::from(local_now_us) + i128::from(best.offset_us);
        let wrap = 1i128 << 32;
        let mut host = host_now - host_now.rem_euclid(wrap) + i128::from(captured_us);
        if host - host_now > wrap / 2 {
            host -= wrap;
        } else if host_now - host > wrap / 2 {
            host += wrap;
        }
        i64::try_from(host - i128::from(best.offset_us)).ok()
    }

    fn best(&self) -> Option<Sample> {
        self.samples
            .iter()
            .flatten()
            .min_by_key(|sample| sample.rtt_us)
            .copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::Noise;

    /// The host's clock runs this far ahead of the player's.
    const AHEAD: u64 = 5_000_000_000;

    /// A ping at `sent` taking `out` to reach the host and `back` to
    /// return.
    fn ping(clock: &mut ClockOffset, sent: u64, out: u64, back: u64) -> Option<u64> {
        clock.add(sent, sent + out + AHEAD, sent + out + back)
    }

    #[test]
    fn nothing_is_known_before_a_first_answer() {
        let clock = ClockOffset::new();
        assert_eq!(clock.rtt_us(), None);
        assert_eq!(clock.host_to_local(1), None);
        assert_eq!(clock.captured_to_local(1, 1), None);
    }

    #[test]
    fn the_shortest_round_trip_gives_the_estimate() {
        let mut noise = Noise::new(60);
        let mut clock = ClockOffset::new();
        for n in 0..16u64 {
            let out = 2_000 + noise.below(20_000) as u64;
            let back = 2_000 + noise.below(20_000) as u64;
            ping(&mut clock, n * 500_000, out, back);
        }
        assert_eq!(ping(&mut clock, 9_000_000, 1_000, 1_000), Some(2_000));
        assert_eq!(clock.rtt_us(), Some(2_000));
        assert_eq!(clock.host_to_local(10_000_000 + AHEAD), Some(10_000_000));
    }

    #[test]
    fn uneven_halves_err_by_at_most_half_their_difference() {
        let mut clock = ClockOffset::new();
        ping(&mut clock, 1_000, 3_000, 1_000);
        let local = clock.host_to_local(50_000 + AHEAD).unwrap();
        assert_eq!(local, 50_000 - (3_000 - 1_000) / 2);
    }

    #[test]
    fn an_old_short_round_trip_is_forgotten_after_sixteen_more() {
        let mut clock = ClockOffset::new();
        ping(&mut clock, 0, 100, 100);
        for n in 1..=16u64 {
            ping(&mut clock, n * 1_000_000, 5_000, 5_000);
        }
        assert_eq!(clock.rtt_us(), Some(10_000));
    }

    #[test]
    fn impossible_times_are_refused() {
        let mut clock = ClockOffset::new();
        assert_eq!(clock.add(10, 5, 9), None);
        assert_eq!(clock.add(0, u64::MAX, 10), None);
        assert_eq!(clock.rtt_us(), None);
    }

    #[test]
    fn a_capture_time_in_32_bits_is_placed_across_the_wrap() {
        let mut clock = ClockOffset::new();
        ping(&mut clock, 1_000_000, 500, 500);
        let wrap = 1u64 << 32;
        // Now on the host is just past a wrap of the low 32 bits, and the
        // capture happened just before it.
        let local_now = 3 * wrap + 100 - AHEAD;
        let captured = wrap - 20_000;
        let local = clock.captured_to_local(captured as u32, local_now).unwrap();
        assert_eq!(local, (3 * wrap - 20_000 - AHEAD) as i64);
        // And the ordinary case, well away from any wrap.
        let local_now = 3 * wrap + wrap / 2 - AHEAD;
        let captured = (wrap / 2 - 30_000) as u32;
        let local = clock.captured_to_local(captured, local_now).unwrap();
        assert_eq!(local, local_now as i64 - 30_000);
    }
}
