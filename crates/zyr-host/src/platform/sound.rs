//! What the computer plays, through WASAPI loopback.
//!
//! The mix Windows hands the default sound card is copied before the
//! card applies its own volume and mute, which is what lets the viewer
//! silence the speakers here and keep the sound of the session. It comes
//! in the mixer's own format, 32-bit floats at the card's rate and
//! channels, and is brought to Opus's 48 kHz stereo. Silence is left
//! out. The default card is looked at every second: another one chosen,
//! or this one gone, and it is opened again.

use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use windows::Win32::Media::Audio::{
    AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_E_DEVICE_INVALIDATED, AUDCLNT_SHAREMODE_SHARED,
    AUDCLNT_STREAMFLAGS_LOOPBACK, IAudioCaptureClient, IAudioClient, IMMDevice,
    IMMDeviceEnumerator, MMDeviceEnumerator, WAVEFORMATEX, WAVEFORMATEXTENSIBLE, eConsole, eRender,
};
use windows::Win32::Media::KernelStreaming::WAVE_FORMAT_EXTENSIBLE;
use windows::Win32::Media::Multimedia::{KSDATAFORMAT_SUBTYPE_IEEE_FLOAT, WAVE_FORMAT_IEEE_FLOAT};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
    CoUninitialize,
};
use windows::core::w;
use zyr_codec::{AudioFormat, Ffmpeg, OPUS_FRAME, Resampler};
use zyr_proto::log::Log;

use super::tuning::ThreadTask;
use super::{Counter, failed};
use crate::parts::{Sound, SoundBlock, SoundCapture, SoundError};
use crate::throttle::Throttle;

/// How often the card is asked for what it has.
const POLL: Duration = Duration::from_millis(5);

/// How often the default card is looked at.
const LOOK_EVERY: Duration = Duration::from_secs(1);

/// What the mixer keeps for us between two looks, in 100 ns units:
/// 200 ms, far more than the 5 ms between two looks.
const BUFFER: i64 = 2_000_000;

/// Interleaved samples in one block.
const BLOCK: usize = OPUS_FRAME * AudioFormat::OPUS.channels as usize;

/// The default sound card of this computer.
pub(super) struct Loopback {
    ffmpeg: Arc<Ffmpeg>,
    log: Log,
}

impl Loopback {
    pub(super) fn new(ffmpeg: Arc<Ffmpeg>, log: Log) -> Self {
        Self { ffmpeg, log }
    }
}

impl Sound for Loopback {
    fn open(&mut self) -> Result<Box<dyn SoundCapture>, SoundError> {
        Listening::open(&self.ffmpeg, &self.log).map(|listening| Box::new(listening) as _)
    }
}

/// COM on this thread, for as long as it is held.
struct Com;

impl Com {
    fn join() -> Result<Self, SoundError> {
        // SAFETY: no reserved pointer; balanced by the drop below when it
        // succeeds, S_FALSE included.
        let joined = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if joined.is_err() {
            return Err(SoundError::Failed(format!(
                "COM refuse ce fil (0x{:08X})",
                joined.0 as u32
            )));
        }
        Ok(Self)
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        // SAFETY: balances the CoInitializeEx that made this.
        unsafe { CoUninitialize() };
    }
}

/// The card, being listened to.
struct Listening {
    capture: IAudioCaptureClient,
    client: IAudioClient,
    enumerator: IMMDeviceEnumerator,
    card: String,
    channels: usize,
    resampler: Option<Resampler>,
    /// Sound at 48 kHz stereo, not yet a whole block.
    pending: Vec<f32>,
    /// When the first sample pending was heard.
    pending_at: Option<Instant>,
    next_look: Instant,
    counter: Counter,
    log: Log,
    left_out: Throttle,
    /// Dropped after the interfaces above, in this order.
    _task: ThreadTask,
    _com: Com,
}

impl Listening {
    /// Opens the default card. What Windows refused, and its code, is in
    /// the error, which the engine writes to the log at a measured pace:
    /// a card that is not there is tried every second.
    fn open(ffmpeg: &Arc<Ffmpeg>, log: &Log) -> Result<Self, SoundError> {
        let com = Com::join()?;
        let task = ThreadTask::join(w!("Pro Audio"), log);
        let refused = |what: &str, e: windows::core::Error| {
            SoundError::Failed(format!("la carte son ne répond pas ({})", failed(what, &e)))
        };
        // SAFETY: a plain constructor on this COM thread.
        let enumerator: IMMDeviceEnumerator =
            unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
                .map_err(|e| refused("listing the sound cards", e))?;
        // SAFETY: plain values; the device comes back owned.
        let device =
            unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole) }.map_err(|e| {
                SoundError::Failed(format!(
                    "aucune carte son n'est active sur l'ordinateur d'en face ({})",
                    failed("finding the default sound card", &e)
                ))
            })?;
        let card = id_of(&device).map_err(|e| refused("naming the default sound card", e))?;
        // SAFETY: a plain activation; the client comes back owned.
        let client: IAudioClient = unsafe { device.Activate(CLSCTX_ALL, None) }
            .map_err(|e| refused("activating the sound card", e))?;
        // SAFETY: the format comes back allocated by COM, freed below.
        let mix = unsafe { client.GetMixFormat() }
            .map_err(|e| refused("asking the sound card its format", e))?;
        let format = read_format(mix);
        let initialised = match &format {
            // SAFETY: the mixer's own format, given back as it was given,
            // alive until freed right after.
            Ok(_) => unsafe {
                client.Initialize(
                    AUDCLNT_SHAREMODE_SHARED,
                    AUDCLNT_STREAMFLAGS_LOOPBACK,
                    BUFFER,
                    0,
                    mix,
                    None,
                )
            },
            Err(_) => Ok(()),
        };
        // SAFETY: allocated by COM for us, used by nobody after this.
        unsafe { CoTaskMemFree(Some(mix.cast_const().cast())) };
        let (rate, channels) = format.map_err(SoundError::Failed)?;
        initialised.map_err(|e| refused("listening to the sound card", e))?;
        // SAFETY: a client initialised above.
        let capture: IAudioCaptureClient = unsafe { client.GetService() }
            .map_err(|e| refused("reaching what the sound card plays", e))?;
        // SAFETY: as above.
        unsafe { client.Start() }.map_err(|e| refused("starting to listen", e))?;
        let heard = AudioFormat { rate, channels };
        let resampler = if heard == AudioFormat::OPUS {
            None
        } else {
            let resampler = Resampler::new(ffmpeg, heard, AudioFormat::OPUS).map_err(|e| {
                SoundError::Failed(format!(
                    "le son ne peut pas être ramené à 48 kHz stéréo ({e})"
                ))
            })?;
            Some(resampler)
        };
        log.write(&format!(
            "listening to the sound card {card}: {rate} Hz, {channels} channels"
        ));
        Ok(Self {
            capture,
            client,
            enumerator,
            card,
            channels: usize::from(channels),
            resampler,
            pending: Vec::with_capacity(BLOCK * 4),
            pending_at: None,
            next_look: Instant::now() + LOOK_EVERY,
            counter: Counter::new(),
            log: log.clone(),
            left_out: Throttle::new(LOOK_EVERY * 10),
            _task: task,
            _com: com,
        })
    }

    /// Takes in everything the card has, leaving out silence.
    fn drain(&mut self) -> Result<(), SoundError> {
        loop {
            // SAFETY: a question to a started client.
            let frames = unsafe { self.capture.GetNextPacketSize() }.map_err(|e| self.lost(e))?;
            if frames == 0 {
                return Ok(());
            }
            let mut data = std::ptr::null_mut();
            let mut count = 0u32;
            let mut flags = 0u32;
            let mut stamp = 0u64;
            // SAFETY: plain out values; the buffer is released below.
            unsafe {
                self.capture
                    .GetBuffer(&mut data, &mut count, &mut flags, None, Some(&mut stamp))
            }
            .map_err(|e| self.lost(e))?;
            let silent = flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0;
            if silent || data.is_null() {
                // A silence cuts the sound: what came before it is let go.
                self.pending.clear();
                self.pending_at = None;
            } else {
                // SAFETY: the buffer holds `count` frames of `channels`
                // floats, as the mix format says, until released below.
                let samples = unsafe {
                    std::slice::from_raw_parts(data.cast::<f32>(), count as usize * self.channels)
                };
                let converted = match &mut self.resampler {
                    Some(resampler) => resampler.convert(samples),
                    None => Ok(samples.to_vec()),
                };
                match converted {
                    Ok(converted) => {
                        if self.pending.is_empty() {
                            self.pending_at = Some(self.heard_at(stamp));
                        }
                        self.pending.extend_from_slice(&converted);
                    }
                    Err(e) => {
                        if let Some(unsaid) = self.left_out.allow(Instant::now()) {
                            self.log
                                .write(&format!("sound left out ({unsaid} more unsaid): {e}"));
                        }
                    }
                }
            }
            // SAFETY: the frames taken above, all of them.
            unsafe { self.capture.ReleaseBuffer(count) }.map_err(|e| self.lost(e))?;
        }
    }

    /// When a packet stamped `stamp` (the performance counter in 100 ns
    /// units) was heard.
    fn heard_at(&self, stamp: u64) -> Instant {
        let now = Instant::now();
        let behind = self
            .counter
            .now_in_hundreds_of_nanoseconds()
            .saturating_sub(i64::try_from(stamp).unwrap_or(i64::MAX));
        now.checked_sub(Duration::from_nanos(
            u64::try_from(behind).unwrap_or(0).saturating_mul(100),
        ))
        .unwrap_or(now)
    }

    /// Whether another card is the default now.
    fn changed(&self) -> bool {
        // SAFETY: a plain question to a live enumerator.
        let now = unsafe { self.enumerator.GetDefaultAudioEndpoint(eRender, eConsole) }
            .and_then(|device| id_of(&device));
        !matches!(now, Ok(card) if card == self.card)
    }

    fn lost(&self, e: windows::core::Error) -> SoundError {
        if e.code() == AUDCLNT_E_DEVICE_INVALIDATED {
            return SoundError::Changed;
        }
        SoundError::Failed(format!(
            "la carte son ne répond plus ({})",
            failed("reading what the sound card plays", &e)
        ))
    }
}

impl SoundCapture for Listening {
    fn next_block(&mut self, until: Instant) -> Result<Option<SoundBlock>, SoundError> {
        loop {
            if self.pending.len() >= BLOCK {
                let at = self.pending_at.unwrap_or_else(Instant::now);
                let samples: Vec<f32> = self.pending.drain(..BLOCK).collect();
                self.pending_at =
                    (!self.pending.is_empty()).then(|| at + Duration::from_millis(10));
                return Ok(Some(SoundBlock { samples, at }));
            }
            self.drain()?;
            if self.pending.len() >= BLOCK {
                continue;
            }
            let now = Instant::now();
            if now >= self.next_look {
                self.next_look = now + LOOK_EVERY;
                if self.changed() {
                    return Err(SoundError::Changed);
                }
            }
            if now >= until {
                return Ok(None);
            }
            thread::sleep(POLL.min(until - now));
        }
    }
}

impl Drop for Listening {
    fn drop(&mut self) {
        // SAFETY: a client started when this was made.
        if let Err(e) = unsafe { self.client.Stop() } {
            self.log
                .write(&failed("stopping listening to the sound card", &e));
        }
    }
}

/// A sound card's id, which stays the same for as long as it is there.
fn id_of(device: &IMMDevice) -> windows::core::Result<String> {
    // SAFETY: a plain question; the id comes back allocated by COM and is
    // freed once copied.
    unsafe {
        let id = device.GetId()?;
        let text = id.to_string().unwrap_or_default();
        CoTaskMemFree(Some(id.0.cast_const().cast()));
        Ok(text)
    }
}

/// The rate and channels of the mixer's format, if it is the 32-bit
/// floats this reads.
fn read_format(format: *const WAVEFORMATEX) -> Result<(u32, u16), String> {
    // SAFETY: a format COM just gave, packed, read without assuming its
    // alignment; the extensible part exists when the tag says so.
    let base = unsafe { format.read_unaligned() };
    let tag = u32::from(base.wFormatTag);
    let float = tag == WAVE_FORMAT_IEEE_FLOAT
        || (tag == WAVE_FORMAT_EXTENSIBLE && {
            // SAFETY: as above, the tag says the larger structure is there.
            let extensible = unsafe { format.cast::<WAVEFORMATEXTENSIBLE>().read_unaligned() };
            // Copied out of the packed structure before being compared.
            let sub_format = extensible.SubFormat;
            sub_format == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT
        });
    let (rate, channels, bits) = (base.nSamplesPerSec, base.nChannels, base.wBitsPerSample);
    if !float || bits != 32 || channels == 0 || rate == 0 {
        return Err(format!(
            "le mélangeur de sons a un format inattendu : type {tag}, {bits} bits, {channels} \
             canaux, {rate} Hz"
        ));
    }
    Ok((rate, channels))
}
