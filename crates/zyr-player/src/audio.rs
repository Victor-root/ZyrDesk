//! The sound: datagrams in, decoded ten milliseconds at a time as the
//! sound card asks for them.
//!
//! Packets wait in a small jitter buffer that puts them back in order.
//! One that never came is played as silence, ten milliseconds of it,
//! rather than waited for: the delay a wait would add is heard for the
//! rest of the session, a gap only once. When nothing is there at all,
//! the host being silent, the card is given silence and nothing is
//! counted.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bytes::Bytes;
use zyr_codec::{CodecError, Ffmpeg, OPUS_FRAME, OpusDecoder};
use zyr_media::audio::{JitterBuffer, Popped, read_audio};
use zyr_proto::log::Log;

use crate::lock;
use crate::seldom::Seldom;
use crate::tallies::{SoundTallies, Tallies};

/// The tag of the sound's lines in the journal.
pub const TAG: &str = "sound";

/// Samples in one packet, both channels interleaved.
pub const PACKET: usize = OPUS_FRAME * 2;

/// What one packet plays for.
const TURN: Duration = Duration::from_millis(10);

/// Whether the person asked for silence, read by the sound card's
/// thread each time it plays.
#[derive(Debug, Default)]
pub struct Muted(AtomicBool);

impl Muted {
    pub fn set(&self, muted: bool) {
        self.0.store(muted, Ordering::Relaxed);
    }

    pub fn get(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// Packets in, samples out.
pub struct Sound {
    jitter: JitterBuffer,
    decoder: OpusDecoder,
    /// Samples decoded and not played yet, from `at` on.
    pending: Vec<f32>,
    at: usize,
    counters: SoundTallies,
    malformed: Seldom,
    broken: Seldom,
    concealed: Seldom,
    tallies: Arc<Mutex<Tallies>>,
    log: Log,
}

impl Sound {
    pub fn open(
        ff: &Arc<Ffmpeg>,
        tallies: Arc<Mutex<Tallies>>,
        log: Log,
    ) -> Result<Self, CodecError> {
        Ok(Self {
            jitter: JitterBuffer::default(),
            decoder: OpusDecoder::open(ff)?,
            pending: Vec::with_capacity(PACKET),
            at: 0,
            counters: SoundTallies::default(),
            malformed: Seldom::new(),
            broken: Seldom::new(),
            concealed: Seldom::new(),
            tallies,
            log,
        })
    }

    /// Takes one datagram in.
    pub fn take(&mut self, datagram: &[u8], now: Instant) {
        match read_audio(datagram) {
            Ok((header, opus)) => self.jitter.push(header.sequence, opus),
            Err(e) => {
                self.counters.malformed += 1;
                self.malformed.note(&self.log, now, |times| {
                    format!("sound datagram refused: {e} ({times} since last said)")
                });
            }
        }
    }

    /// Fills `out`, stereo interleaved, with what is due now, and
    /// silence where there is nothing.
    pub fn fill(&mut self, out: &mut [f32], now: Instant) {
        let mut filled = 0;
        while filled < out.len() {
            if self.at < self.pending.len() {
                let count = (out.len() - filled).min(self.pending.len() - self.at);
                out[filled..filled + count]
                    .copy_from_slice(&self.pending[self.at..self.at + count]);
                self.at += count;
                filled += count;
                continue;
            }
            match self.jitter.pop() {
                Popped::Packet(packet) => {
                    match self.decoder.decode(&packet) {
                        Ok(samples) => {
                            self.counters.decoded += 1;
                            self.pending = samples;
                            self.at = 0;
                        }
                        Err(e) => {
                            self.counters.broken += 1;
                            self.broken.note(&self.log, now, |times| {
                            format!("sound packet refused by the decoder: {e} ({times} since last said)")
                        });
                            self.silence();
                        }
                    }
                }
                Popped::Missing => {
                    self.counters.concealed += 1;
                    self.concealed.note(&self.log, now, |times| {
                        format!("sound packet missing, played as silence ({times} since last said)")
                    });
                    self.silence();
                }
                Popped::Empty => {
                    out[filled..].fill(0.0);
                    break;
                }
            }
        }
        let mut tallies = lock(&self.tallies);
        tallies.jitter = self.jitter.counters();
        tallies.sound = self.counters;
    }

    fn silence(&mut self) {
        self.pending.clear();
        self.pending.resize(PACKET, 0.0);
        self.at = 0;
    }
}

/// The sound thread with no sound card: packets are decoded at the pace
/// a card would ask for them, and go nowhere. What the tests and the
/// diagnostic command line hear.
pub fn run_headless(mut sound: Sound, input: &Receiver<Bytes>) {
    pace(&mut sound, input, None);
}

/// Takes packets and plays them into nothing, ten milliseconds at a
/// time, until `until` if given. False once the link has let go of
/// `input`.
pub fn pace(sound: &mut Sound, input: &Receiver<Bytes>, until: Option<Instant>) -> bool {
    let mut out = vec![0.0f32; PACKET];
    let mut next = Instant::now() + TURN;
    loop {
        let wake = until.map_or(next, |until| until.min(next));
        match input.recv_timeout(wake.saturating_duration_since(Instant::now())) {
            Ok(datagram) => sound.take(&datagram, Instant::now()),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return false,
        }
        let now = Instant::now();
        if now >= next {
            sound.fill(&mut out, now);
            next += TURN;
            // Fallen behind: a card plays on from now, it never plays
            // faster to catch up.
            if next < now {
                next = now + TURN;
            }
        }
        if until.is_some_and(|until| now >= until) {
            return true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    fn opened() -> (Sound, Arc<Mutex<Tallies>>) {
        let tallies = Arc::new(Mutex::new(Tallies::default()));
        let sound =
            Sound::open(&testing::ffmpeg(), Arc::clone(&tallies), testing::log(TAG)).unwrap();
        (sound, tallies)
    }

    #[test]
    fn a_missing_packet_is_played_as_silence_and_counted() {
        let (mut sound, tallies) = opened();
        let now = Instant::now();
        for (sequence, datagram) in testing::sound(6).iter().enumerate() {
            if sequence != 3 {
                sound.take(datagram, now);
            }
        }
        let mut played = Vec::new();
        let mut out = vec![1.0f32; PACKET];
        for _ in 0..7 {
            sound.fill(&mut out, now);
            played.push(out.clone());
        }
        let tallies = lock(&tallies).clone();
        assert_eq!(tallies.sound.decoded, 5);
        assert_eq!(tallies.sound.concealed, 1);
        let silent = |samples: &[f32]| samples.iter().all(|sample| *sample == 0.0);
        let heard = |samples: &[f32]| samples.iter().any(|sample| sample.abs() > 0.1);
        assert!(heard(&played[2]));
        assert!(silent(&played[3]));
        assert!(heard(&played[4]));
        assert!(heard(&played[5]));
        // Every packet played, the buffer then runs dry: silence, and
        // nothing concealed.
        assert!(silent(&played[6]));
        assert_eq!(tallies.jitter.underruns, 1);
    }

    #[test]
    fn a_card_asking_for_odd_amounts_gets_every_sample_in_order() {
        let (mut sound, _) = opened();
        let (mut whole, _) = opened();
        let now = Instant::now();
        for datagram in testing::sound(4) {
            sound.take(&datagram, now);
            whole.take(&datagram, now);
        }
        let mut expected = vec![0.0f32; PACKET * 3];
        whole.fill(&mut expected, now);
        let mut got = Vec::new();
        for size in [7, 480, 1, 959, 320, 1_113] {
            let mut out = vec![0.0f32; size];
            sound.fill(&mut out, now);
            got.extend(out);
        }
        assert_eq!(got[..PACKET * 3], expected[..]);
    }

    #[test]
    fn what_is_not_a_sound_packet_is_counted_and_passed_over() {
        let (mut sound, tallies) = opened();
        let now = Instant::now();
        sound.take(&[1, 0, 0], now);
        sound.take(&[9, 0, 0, 0, 0, 0, 0, 0, 1], now);
        let mut out = vec![1.0f32; PACKET];
        sound.fill(&mut out, now);
        assert!(out.iter().all(|sample| *sample == 0.0));
        assert_eq!(lock(&tallies).sound.malformed, 2);
    }
}
