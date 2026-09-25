//! The sound card, through WASAPI in shared mode, woken by an event
//! each time it has room.
//!
//! Opened once for the session, on the device the desktop plays to, in
//! the smallest period the system allows when the card's own format is
//! the engine's (48 kHz stereo, float), and in 10 ms periods with
//! Windows converting otherwise. The session shows in the volume mixer
//! as "ZyrDesk", and muting is done there, on its own strip, where the
//! person sees it.
//!
//! A computer with no sound card has no sound thread: Windows says at
//! once that there is no default device. A device that goes away
//! mid-session (headphones unplugged) is looked for again every second,
//! the packets being taken at the card's pace meanwhile.

use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

use bytes::Bytes;
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_FAILED, WAIT_TIMEOUT};
use windows::Win32::Media::Audio::{
    AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
    AUDCLNT_STREAMFLAGS_EVENTCALLBACK, AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, IAudioClient,
    IAudioClient3, IAudioRenderClient, IAudioSessionControl, IMMDeviceEnumerator,
    ISimpleAudioVolume, MMDeviceEnumerator, WAVEFORMATEX, WAVEFORMATEXTENSIBLE, eConsole, eRender,
};
use windows::Win32::Media::KernelStreaming::WAVE_FORMAT_EXTENSIBLE;
use windows::Win32::Media::Multimedia::{KSDATAFORMAT_SUBTYPE_IEEE_FLOAT, WAVE_FORMAT_IEEE_FLOAT};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
    CoUninitialize,
};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
use windows::core::{Interface, w};
use zyr_proto::log::Log;

use super::{Multimedia, failure};
use crate::audio::{Muted, Sound, pace};
use crate::seldom::Seldom;

/// The engine's sound: 48 kHz, two channels, 32-bit float.
const RATE: u32 = 48_000;
const CHANNELS: u16 = 2;
const BITS: u16 = 32;

/// The period asked for when the smallest cannot be had, in units of
/// 100 ns: 10 ms.
const PERIOD: i64 = 100_000;

/// How long to wait for the card's event before looking at it anyway,
/// in milliseconds.
const PATIENCE_MS: u32 = 200;

/// How long to wait before looking for a device again once one went
/// away.
const LOOK_AGAIN: Duration = Duration::from_secs(1);

/// The sound thread on the sound card, until the link lets go of
/// `input`.
pub fn run(mut sound: Sound, input: &Receiver<Bytes>, muted: &Muted, log: &Log) {
    let _com = Com::up();
    let _pro_audio = Multimedia::join(w!("Pro Audio"), log);
    let mut output = match Output::open(log) {
        Ok(output) => output,
        Err(Opening::Nothing(reason)) => {
            log.debug(|| format!("no sound card to play on: {reason}"));
            return;
        }
        Err(Opening::Refused(reason)) => {
            log.write(&format!("the sound card refuses to play: {reason}"));
            return;
        }
    };
    loop {
        let reason = match output.play(&mut sound, input, muted) {
            Played::LinkGone => return,
            Played::DeviceGone(reason) => reason,
        };
        log.write(&format!("the sound card went away: {reason}"));
        drop(output);
        output = loop {
            if !pace(&mut sound, input, Some(Instant::now() + LOOK_AGAIN)) {
                return;
            }
            match Output::open(log) {
                Ok(output) => break output,
                Err(Opening::Nothing(reason) | Opening::Refused(reason)) => {
                    log.debug(|| format!("still no sound card: {reason}"));
                }
            }
        };
    }
}

/// Why no sound card could be opened.
enum Opening {
    /// There is none.
    Nothing(String),
    /// There is one, which refused.
    Refused(String),
}

/// Why playing stopped.
enum Played {
    LinkGone,
    DeviceGone(String),
}

/// A sound card, playing.
struct Output {
    client: IAudioClient,
    render: IAudioRenderClient,
    volume: ISimpleAudioVolume,
    /// Set by the card each time it has room.
    wake: Event,
    /// Frames the card's buffer holds.
    frames: u32,
    /// The mute the mixer shows now, once set.
    muted: Option<bool>,
    silent: Seldom,
    log: Log,
}

impl Output {
    fn open(log: &Log) -> Result<Output, Opening> {
        let refused = |what: &str, e: &windows::core::Error| Opening::Refused(failure(what, e));
        // SAFETY: a standard class asked of COM, with no aggregation.
        let devices: IMMDeviceEnumerator =
            unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
                .map_err(|e| refused("CoCreateInstance(MMDeviceEnumerator)", &e))?;
        // SAFETY: an interface of ours; no device is an error at once.
        let device = unsafe { devices.GetDefaultAudioEndpoint(eRender, eConsole) }
            .map_err(|e| Opening::Nothing(failure("GetDefaultAudioEndpoint", &e)))?;
        // Windows 10 has IAudioClient3, which gives the smallest period.
        // SAFETY: activation of an interface of the device, no parameter.
        let low_latency: Option<IAudioClient3> = unsafe { device.Activate(CLSCTX_ALL, None) }.ok();
        let client: IAudioClient = match &low_latency {
            Some(client) => client
                .cast()
                .map_err(|e| refused("IAudioClient3 as IAudioClient", &e))?,
            // SAFETY: as above.
            None => unsafe { device.Activate(CLSCTX_ALL, None) }
                .map_err(|e| refused("Activate(IAudioClient)", &e))?,
        };
        // SAFETY: a getter; the format is COM's memory, freed by `Mixed`.
        let mixed =
            Mixed(unsafe { client.GetMixFormat() }.map_err(|e| refused("GetMixFormat", &e))?);
        let native = mixed.is_the_engines();
        // The smallest period needs the card's own format, which must be
        // the engine's; anything else goes in 10 ms periods, Windows
        // converting what needs to be.
        let tried = low_latency
            .as_ref()
            .filter(|_| native)
            .map(|low_latency| smallest_period(low_latency, mixed.0));
        let (client, how) = match tried {
            Some(Ok(frames)) => (
                client,
                format!("in periods of {frames} frames, the smallest"),
            ),
            tried => {
                let client = match tried {
                    Some(Err(e)) => {
                        log.write(&format!(
                            "{}: 10 ms periods instead",
                            failure("InitializeSharedAudioStream", &e)
                        ));
                        // A client whose initialisation failed is not
                        // tried again: a fresh one is.
                        // SAFETY: as above.
                        unsafe { device.Activate::<IAudioClient>(CLSCTX_ALL, None) }
                            .map_err(|e| refused("Activate(IAudioClient)", &e))?
                    }
                    _ => client,
                };
                let how = in_ten_ms_periods(&client, native, &mixed)
                    .map_err(|e| refused("IAudioClient::Initialize", &e))?;
                (client, how.to_string())
            }
        };

        let wake = Event::new().map_err(|e| refused("CreateEventW", &e))?;
        // SAFETY: an event of ours, alive as long as the client.
        unsafe { client.SetEventHandle(wake.0) }.map_err(|e| refused("SetEventHandle", &e))?;
        // SAFETY: getters on an initialised client.
        let frames = unsafe { client.GetBufferSize() }.map_err(|e| refused("GetBufferSize", &e))?;
        let render: IAudioRenderClient =
            unsafe { client.GetService() }.map_err(|e| refused("GetService(render)", &e))?;
        let volume: ISimpleAudioVolume =
            unsafe { client.GetService() }.map_err(|e| refused("GetService(volume)", &e))?;
        // The name in the mixer. A card that will not take it still
        // plays.
        // SAFETY: as above; the name is a static string.
        match unsafe { client.GetService::<IAudioSessionControl>() } {
            Ok(session) => {
                if let Err(e) = unsafe { session.SetDisplayName(w!("ZyrDesk"), std::ptr::null()) } {
                    log.write(&failure("SetDisplayName", &e));
                }
            }
            Err(e) => log.write(&failure("GetService(session)", &e)),
        }
        // SAFETY: an initialised client with its event set.
        unsafe { client.Start() }.map_err(|e| refused("IAudioClient::Start", &e))?;
        log.write(&format!(
            "sound card open: {}, buffer of {frames} frames, mixing at {} Hz in {} channels",
            how,
            mixed.rate(),
            mixed.channels()
        ));
        Ok(Output {
            client,
            render,
            volume,
            wake,
            frames,
            muted: None,
            silent: Seldom::new(),
            log: log.clone(),
        })
    }

    /// Fills the card each time it has room, until the link or the card
    /// goes.
    fn play(&mut self, sound: &mut Sound, input: &Receiver<Bytes>, muted: &Muted) -> Played {
        loop {
            // SAFETY: an event of ours.
            let woken = unsafe { WaitForSingleObject(self.wake.0, PATIENCE_MS) };
            if woken == WAIT_FAILED {
                // Never waiting again would spin: the card is opened
                // anew, with an event of its own.
                return Played::DeviceGone(failure(
                    "WaitForSingleObject on the sound card's event",
                    &windows::core::Error::from_thread(),
                ));
            }
            let now = Instant::now();
            loop {
                match input.try_recv() {
                    Ok(datagram) => sound.take(&datagram, now),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return Played::LinkGone,
                }
            }
            if woken == WAIT_TIMEOUT {
                let log = &self.log;
                self.silent.note(log, now, |times| {
                    format!(
                        "the sound card said nothing for {PATIENCE_MS} ms ({times} since last said)"
                    )
                });
            }
            let quiet = muted.get();
            if self.muted != Some(quiet) {
                // SAFETY: a setter on the session's own volume.
                match unsafe { self.volume.SetMute(quiet, std::ptr::null()) } {
                    Ok(()) => self.muted = Some(quiet),
                    Err(e) => return Played::DeviceGone(failure("SetMute", &e)),
                }
            }
            // SAFETY: a getter on a started client.
            let padding = match unsafe { self.client.GetCurrentPadding() } {
                Ok(padding) => padding,
                Err(e) => return Played::DeviceGone(failure("GetCurrentPadding", &e)),
            };
            let room = self.frames.saturating_sub(padding);
            if room == 0 {
                continue;
            }
            // SAFETY: at most the room the card just gave.
            let data = match unsafe { self.render.GetBuffer(room) } {
                Ok(data) => data,
                Err(e) => return Played::DeviceGone(failure("GetBuffer", &e)),
            };
            // SAFETY: WASAPI hands `room` frames of the format given, two
            // floats each, aligned for them and ours until released.
            let samples = unsafe {
                std::slice::from_raw_parts_mut(
                    data.cast::<f32>(),
                    room as usize * usize::from(CHANNELS),
                )
            };
            sound.fill(samples, now);
            // SAFETY: the frames just written, with no flag.
            if let Err(e) = unsafe { self.render.ReleaseBuffer(room, 0) } {
                return Played::DeviceGone(failure("ReleaseBuffer", &e));
            }
        }
    }
}

impl Drop for Output {
    fn drop(&mut self) {
        // SAFETY: stopping a client of ours; one already stopped says so.
        let _ = unsafe { self.client.Stop() };
    }
}

/// Initialises the client in the smallest period the engine allows for
/// the card's own format, which is the engine's; gives that period.
fn smallest_period(
    client: &IAudioClient3,
    format: *const WAVEFORMATEX,
) -> windows::core::Result<u32> {
    let (mut default, mut fundamental, mut smallest, mut largest) = (0, 0, 0, 0);
    // SAFETY: the card's own format, and four counts of ours.
    unsafe {
        client.GetSharedModeEnginePeriod(
            format,
            &mut default,
            &mut fundamental,
            &mut smallest,
            &mut largest,
        )?;
        client.InitializeSharedAudioStream(
            AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
            smallest,
            format,
            None,
        )?;
    }
    Ok(smallest)
}

/// Initialises the client in 10 ms periods: in the card's own format
/// when it is the engine's, else in the engine's with Windows
/// converting. Says which.
fn in_ten_ms_periods(
    client: &IAudioClient,
    native: bool,
    mixed: &Mixed,
) -> windows::core::Result<&'static str> {
    let engines = WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_IEEE_FLOAT as u16,
        nChannels: CHANNELS,
        nSamplesPerSec: RATE,
        nAvgBytesPerSec: RATE * u32::from(CHANNELS * BITS / 8),
        nBlockAlign: CHANNELS * BITS / 8,
        wBitsPerSample: BITS,
        cbSize: 0,
    };
    let (flags, format, how) = if native {
        (0, mixed.0.cast_const(), "in 10 ms periods")
    } else {
        (
            AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
            &raw const engines,
            "in 10 ms periods, Windows converting",
        )
    };
    // SAFETY: a format that outlives the call; shared mode takes no
    // periodicity of its own.
    unsafe {
        client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_EVENTCALLBACK | flags,
            PERIOD,
            0,
            format,
            None,
        )
    }?;
    Ok(how)
}

/// The format the card mixes in, in COM's memory.
struct Mixed(*mut WAVEFORMATEX);

impl Mixed {
    /// Whether the card mixes 48 kHz stereo floats already.
    fn is_the_engines(&self) -> bool {
        // SAFETY: a format GetMixFormat returned, whole; an extensible
        // one is read as such only when its tag and size say it is one.
        unsafe {
            let format = self.0.read_unaligned();
            let float = match u32::from(format.wFormatTag) {
                WAVE_FORMAT_IEEE_FLOAT => true,
                WAVE_FORMAT_EXTENSIBLE if usize::from(format.cbSize) >= extension() => {
                    let extensible = self.0.cast::<WAVEFORMATEXTENSIBLE>().read_unaligned();
                    let sub_format = extensible.SubFormat;
                    sub_format == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT
                }
                _ => false,
            };
            float
                && format.nChannels == CHANNELS
                && format.nSamplesPerSec == RATE
                && format.wBitsPerSample == BITS
        }
    }

    fn rate(&self) -> u32 {
        // SAFETY: as above.
        unsafe { self.0.read_unaligned() }.nSamplesPerSec
    }

    fn channels(&self) -> u16 {
        // SAFETY: as above.
        unsafe { self.0.read_unaligned() }.nChannels
    }
}

impl Drop for Mixed {
    fn drop(&mut self) {
        // SAFETY: memory COM gave, freed once here.
        unsafe { CoTaskMemFree(Some(self.0.cast_const().cast())) };
    }
}

/// What an extensible format adds after the plain one.
fn extension() -> usize {
    size_of::<WAVEFORMATEXTENSIBLE>() - size_of::<WAVEFORMATEX>()
}

/// An event of ours, closed when dropped.
struct Event(HANDLE);

impl Event {
    fn new() -> windows::core::Result<Event> {
        // SAFETY: an unnamed auto-reset event, not set.
        unsafe { CreateEventW(None, false, false, None) }.map(Event)
    }
}

impl Drop for Event {
    fn drop(&mut self) {
        // SAFETY: a handle of ours, closed once here.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// COM, brought up on the sound thread for as long as it plays, and
/// taken down only if this thread brought it up.
struct Com(bool);

impl Com {
    fn up() -> Self {
        // SAFETY: nothing is touched but this thread's own apartment.
        Self(unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok())
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: balances the one call above, and only that one.
            unsafe { CoUninitialize() };
        }
    }
}
