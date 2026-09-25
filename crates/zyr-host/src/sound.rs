//! What this computer plays, carried to the viewer 10 ms at a time.
//!
//! The sound card is listened to only while the viewer asks for sound.
//! Each 10 ms heard becomes one Opus packet behind a sequence number; a
//! silent computer sends nothing at all, and the player hears that as
//! the silence it is. A sound card that changes (headphones plugged in,
//! another default chosen) is opened again; one that cannot be listened
//! to is said to the viewer once and tried again every second.

use std::sync::Arc;
use std::sync::mpsc::{self, RecvTimeoutError, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use zyr_codec::{Ffmpeg, OpusEncoder};
use zyr_media::audio::write_audio;
use zyr_media::control::{NoticeKind, ToPlayer};
use zyr_proto::log::Log;

use crate::clock::HostClock;
use crate::link::{Outbox, Sent};
use crate::parts::{Sound, SoundCapture, SoundError};
use crate::throttle::Throttle;

/// Opus's rate for the session's sound, in bits a second.
pub(crate) const OPUS_BITRATE: u32 = 128_000;

/// Longest wait for sound before looking whether the engine said
/// something.
const LISTENING: Duration = Duration::from_millis(20);

/// How long before a sound card that could not be listened to is tried
/// again.
const RETRY: Duration = Duration::from_secs(1);

/// How often the counts are written.
const REPORT_EVERY: Duration = Duration::from_secs(10);

pub(crate) enum Command {
    /// Carry the sound.
    Listen,
    /// Stop carrying it.
    Hush,
    Quit,
}

pub(crate) struct Shared {
    pub(crate) ffmpeg: Arc<Ffmpeg>,
    pub(crate) outbox: Outbox,
    pub(crate) clock: HostClock,
    pub(crate) log: Log,
}

pub(crate) fn start(
    sound: Box<dyn Sound>,
    shared: Shared,
) -> std::io::Result<(mpsc::Sender<Command>, JoinHandle<()>)> {
    let (commands, received) = mpsc::channel();
    let thread = thread::Builder::new()
        .name("engine sound".to_string())
        .spawn(move || Listener::new(sound, shared).run(&received))?;
    Ok((commands, thread))
}

#[derive(Debug, Default)]
struct Counts {
    blocks: u64,
    packets: u64,
    bytes: u64,
    crowded: u64,
    failed: u64,
    reopened: u64,
}

impl Counts {
    fn said(&self) -> String {
        format!(
            "sound: {} blocks heard, {} packets sent ({} bytes), {} dropped on a full link, \
             {} failed to encode, {} reopenings",
            self.blocks, self.packets, self.bytes, self.crowded, self.failed, self.reopened
        )
    }
}

/// The sound card being listened to, and its encoder.
struct Listening {
    capture: Box<dyn SoundCapture>,
    opus: OpusEncoder,
}

struct Listener {
    sound: Box<dyn Sound>,
    shared: Shared,
    wanted: bool,
    listening: Option<Listening>,
    /// When to try the sound card again after it failed.
    retry_at: Option<Instant>,
    /// Whether the viewer was told the sound card fails, since it last
    /// worked.
    told: bool,
    sequence: u16,
    counts: Counts,
    crowded: Throttle,
}

impl Listener {
    fn new(sound: Box<dyn Sound>, shared: Shared) -> Self {
        Self {
            sound,
            shared,
            wanted: false,
            listening: None,
            retry_at: None,
            told: false,
            sequence: 0,
            counts: Counts::default(),
            crowded: Throttle::new(REPORT_EVERY),
        }
    }

    fn run(mut self, commands: &mpsc::Receiver<Command>) {
        let mut report = Instant::now() + REPORT_EVERY;
        loop {
            let command = if self.listening.is_some() {
                match commands.try_recv() {
                    Ok(command) => Some(command),
                    Err(TryRecvError::Empty) => None,
                    Err(TryRecvError::Disconnected) => Some(Command::Quit),
                }
            } else {
                let wake = self.retry_at.filter(|_| self.wanted).unwrap_or(report);
                match commands.recv_timeout(wake.saturating_duration_since(Instant::now())) {
                    Ok(command) => Some(command),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => Some(Command::Quit),
                }
            };
            match command {
                Some(Command::Listen) => self.wanted = true,
                Some(Command::Hush) => {
                    self.wanted = false;
                    if self.listening.take().is_some() {
                        self.shared.log.write("sound stopped");
                    }
                }
                Some(Command::Quit) => break,
                None => {}
            }
            if self.wanted {
                self.listen();
            }
            let now = Instant::now();
            if now >= report {
                self.shared.log.debug(|| self.counts.said());
                report = now + REPORT_EVERY;
            }
        }
        self.shared.log.write(&self.counts.said());
    }

    /// Opens the sound card if need be, and carries what it gives.
    fn listen(&mut self) {
        if self.listening.is_none() {
            if self.retry_at.is_some_and(|at| Instant::now() < at) {
                return;
            }
            match self.open() {
                Ok(listening) => {
                    self.shared.log.write("sound started");
                    self.listening = Some(listening);
                    self.retry_at = None;
                    self.told = false;
                }
                Err(e) => {
                    self.failed(&e.to_string());
                    return;
                }
            }
        }
        let Some(listening) = &mut self.listening else {
            return;
        };
        match listening.capture.next_block(Instant::now() + LISTENING) {
            Ok(Some(block)) => {
                self.counts.blocks += 1;
                match listening.opus.encode(&block.samples) {
                    Ok(packet) => self.send(&packet, self.shared.clock.micros(block.at)),
                    Err(e) => {
                        self.counts.failed += 1;
                        self.shared
                            .log
                            .write(&format!("10 ms of sound could not be encoded: {e}"));
                    }
                }
            }
            Ok(None) => {}
            Err(SoundError::Changed) => {
                self.counts.reopened += 1;
                self.shared
                    .log
                    .write("the sound card changed: listening to it again");
                self.listening = None;
            }
            Err(SoundError::Failed(why)) => {
                self.listening = None;
                self.failed(&why);
            }
        }
    }

    fn open(&mut self) -> Result<Listening, String> {
        let capture = self.sound.open().map_err(|e| e.to_string())?;
        let opus =
            OpusEncoder::open(&self.shared.ffmpeg, OPUS_BITRATE).map_err(|e| e.to_string())?;
        Ok(Listening { capture, opus })
    }

    /// Says once that the sound fails, and tries again later.
    fn failed(&mut self, why: &str) {
        self.shared
            .log
            .write(&format!("sound cannot be listened to: {why}"));
        if !self.told {
            self.shared.outbox.player(&ToPlayer::Notice {
                kind: NoticeKind::AudioTrouble,
                text: format!("Le son de l'ordinateur d'en face n'est pas transmis : {why}"),
            });
            self.told = true;
        }
        self.retry_at = Some(Instant::now() + RETRY);
    }

    fn send(&mut self, packet: &[u8], captured_us: u64) {
        let mut datagram = Vec::new();
        // The low 32 bits, as the header carries them.
        write_audio(self.sequence, captured_us as u32, packet, &mut datagram);
        self.sequence = self.sequence.wrapping_add(1);
        match self.shared.outbox.audio(datagram) {
            Sent::Queued => {
                self.counts.packets += 1;
                self.counts.bytes += packet.len() as u64;
            }
            Sent::Crowded => {
                self.counts.crowded += 1;
                if let Some(unsaid) = self.crowded.allow(Instant::now()) {
                    self.shared.log.write(&format!(
                        "a sound packet found the link full and was dropped ({unsaid} more unsaid)"
                    ));
                }
            }
            Sent::Closed => {}
        }
    }
}
