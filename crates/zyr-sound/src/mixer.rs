//! Windows' sound, reached the way the volume mixer reaches it.
//!
//! Two switches live here and they sit at different heights. What a
//! program plays is muted on that program's own strip, the one the mixer
//! draws beside its icon, and nothing else on the computer hears it. The
//! speakers are muted on the device itself, past the point where Windows
//! has already mixed every program together and copied the mix to
//! whoever is recording it: that is why the room falls silent and a
//! session's sound does not.

use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{
    IAudioSessionControl2, IAudioSessionManager2, IMMDevice, IMMDeviceEnumerator,
    ISimpleAudioVolume, MMDeviceEnumerator, eConsole, eRender,
};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::core::Interface;

use crate::Trouble;

pub fn muted(process: u32) -> Result<bool, Trouble> {
    let _com = Com::up();
    let strip = its_strip(process)?;
    // SAFETY: an interface this call obtained and still holds.
    unsafe { strip.GetMute() }
        .map(|is| is.as_bool())
        .map_err(|e| Trouble::of(format!("le son du programme {process} est illisible : {e}")))
}

pub fn mute(process: u32, quiet: bool) -> Result<(), Trouble> {
    let _com = Com::up();
    let strip = its_strip(process)?;
    // SAFETY: the same, and no event context is offered since nothing of
    // ours is listening for one.
    unsafe { strip.SetMute(quiet, std::ptr::null()) }
        .map_err(|e| Trouble::of(format!("le son du programme {process} n'a pas bougé : {e}")))
}

pub fn speakers_muted() -> Result<bool, Trouble> {
    let _com = Com::up();
    let speakers = the_speakers()?;
    // SAFETY: an interface this call obtained and still holds.
    unsafe { speakers.GetMute() }
        .map(|is| is.as_bool())
        .map_err(|e| Trouble::of(format!("le muet des enceintes est illisible : {e}")))
}

pub fn mute_speakers(quiet: bool) -> Result<(), Trouble> {
    let _com = Com::up();
    let speakers = the_speakers()?;
    // SAFETY: the same, with no event context for the same reason.
    unsafe { speakers.SetMute(quiet, std::ptr::null()) }
        .map_err(|e| Trouble::of(format!("les enceintes n'ont pas obéi : {e}")))
}

/// COM, brought up for as long as one question takes.
///
/// Held across the whole of a question and never handed back with an
/// interface still in hand: everything below is a pointer that only
/// means anything while COM stands.
///
/// A thread that already had COM up in another apartment refuses this
/// and keeps the apartment it had, which is an answer and not a fault:
/// the calls below work either way. What must not happen then is taking
/// COM down at the end, since it belongs to whoever raised it.
struct Com(bool);

impl Com {
    fn up() -> Self {
        // SAFETY: nothing is touched but this thread's own apartment.
        let outcome = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        Self(outcome.is_ok())
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

/// The device the desktop is playing to.
///
/// Whichever it happens to be, asked for by the role the desktop uses,
/// which is the same question the host engine asks about what it
/// captures. The answer depends on who is signed in, so where this runs
/// matters: asked from a service sitting beside the sessions rather than
/// in one, it names that service's device and not the person's.
fn playing_to() -> Result<IMMDevice, Trouble> {
    // SAFETY: a standard class asked of COM, with no aggregation.
    let mixer: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
            .map_err(|e| Trouble::of(format!("le mélangeur audio ne s'ouvre pas : {e}")))?;
    // SAFETY: an interface this call obtained and still holds.
    unsafe { mixer.GetDefaultAudioEndpoint(eRender, eConsole) }
        .map_err(|e| Trouble::of(format!("cet ordinateur n'a pas de sortie audio : {e}")))
}

/// The speakers' own mute, which is the device's and not the mix's.
fn the_speakers() -> Result<IAudioEndpointVolume, Trouble> {
    let device = playing_to()?;
    // SAFETY: an interface this call obtained and still holds; no
    // activation parameters, this one takes none.
    unsafe { device.Activate(CLSCTX_ALL, None) }
        .map_err(|e| Trouble::of(format!("le volume des enceintes est hors d'atteinte : {e}")))
}

/// That program's strip in the mixer.
///
/// Found by process number, which is what the mixer itself shows a name
/// and an icon for. A program that has not played anything yet has no
/// strip at all, and that is said plainly rather than answered with a
/// silence that would read as « already muted ».
fn its_strip(process: u32) -> Result<ISimpleAudioVolume, Trouble> {
    let device = playing_to()?;
    // SAFETY: an interface this call obtained and still holds.
    let manager: IAudioSessionManager2 = unsafe { device.Activate(CLSCTX_ALL, None) }
        .map_err(|e| Trouble::of(format!("le mélangeur ne se laisse pas ouvrir : {e}")))?;
    // SAFETY: the same, on an interface still in hand.
    let playing = unsafe { manager.GetSessionEnumerator() }
        .map_err(|e| Trouble::of(format!("le mélangeur ne dit pas qui joue : {e}")))?;
    // SAFETY: the same.
    let how_many = unsafe { playing.GetCount() }
        .map_err(|e| Trouble::of(format!("le mélangeur ne dit pas combien jouent : {e}")))?;

    for rank in 0..how_many {
        // SAFETY: a rank the count above vouches for.
        let Ok(one) = (unsafe { playing.GetSession(rank) }) else {
            continue;
        };
        // Windows keeps strips of programs that have stopped, and a
        // strip whose owner cannot be named is one of those: skipped
        // rather than fatal, since the one being looked for may be next.
        let Ok(named) = one.cast::<IAudioSessionControl2>() else {
            continue;
        };
        // SAFETY: an interface obtained just above and still in hand.
        if unsafe { named.GetProcessId() }.is_ok_and(|whose| whose == process) {
            return named
                .cast::<ISimpleAudioVolume>()
                .map_err(|e| Trouble::of(format!("ce programme n'a pas de volume : {e}")));
        }
    }
    Err(Trouble::of(format!(
        "le programme {process} ne joue aucun son pour l'instant"
    )))
}
