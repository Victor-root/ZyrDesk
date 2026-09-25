//! The NVIDIA driver told to keep the card at full speed for this
//! program.
//!
//! Left to its own judgement, the driver lowers the card's clocks a few
//! seconds into a session: the desktop asks little of the card, and
//! nothing tells the driver that each picture is waited for. The encoder
//! then takes three to four times as long over each: on a GeForce GTX
//! 1660 Ti, 2 ms a picture for the first two seconds, then 8 to 10 ms for
//! the rest of the session, a picture sent again unchanged included.
//! Sunshine met the same and asks the driver what this asks it: in a
//! profile of the driver's own settings for its program, "Power
//! management mode" at "Prefer maximum performance" (its option
//! `nvenc_latency_over_power`, on unless turned off).
//!
//! The profile lives in the driver's settings, like one made in the
//! NVIDIA Control Panel, and names this program by its file name: it
//! holds for whichever of its processes use the card, the engine's
//! during a session, and costs nothing once that ends. It is written
//! only when missing or changed, and before the engine makes its
//! Direct3D device: the driver reads it when a process first uses the
//! card.
//!
//! NvAPI is the driver's own library, in System32 wherever an NVIDIA
//! driver is installed: a machine without one has nothing to set, and
//! nothing is said. Its functions are reached by the numbers NVIDIA's
//! headers give them, through the one it exports.

use std::ffi::c_void;
use std::ptr;
use std::time::Instant;

use windows::Win32::Foundation::{FreeLibrary, HMODULE};
use windows::Win32::System::LibraryLoader::{
    GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW,
};
use windows::core::{PCWSTR, s, w};
use zyr_proto::log::Log;

/// The profile's name among the driver's.
const PROFILE: &str = "ZyrDesk";

/// The driver's library for this processor.
#[cfg(target_arch = "aarch64")]
const LIBRARY: PCWSTR = w!("nvapia64.dll");
#[cfg(target_arch = "x86")]
const LIBRARY: PCWSTR = w!("nvapi.dll");
#[cfg(not(any(target_arch = "aarch64", target_arch = "x86")))]
const LIBRARY: PCWSTR = w!("nvapi64.dll");

// The functions' numbers, from nvapi_interface.h.
const INITIALIZE: u32 = 0x0150_e828;
const UNLOAD: u32 = 0xd22b_dd7e;
const CREATE_SESSION: u32 = 0x0694_d52e;
const DESTROY_SESSION: u32 = 0xdad9_cff8;
const LOAD_SETTINGS: u32 = 0x375d_bd6b;
const SAVE_SETTINGS: u32 = 0xfcbc_7e14;
const FIND_PROFILE_BY_NAME: u32 = 0x7e4a_9a0b;
const CREATE_PROFILE: u32 = 0xcc17_6068;
const GET_APPLICATION_INFO: u32 = 0xed1f_8c69;
const CREATE_APPLICATION: u32 = 0x4347_a9de;
const GET_SETTING: u32 = 0x73bf_8338;
const SET_SETTING: u32 = 0x577d_d202;

/// "Power management mode", and its "Prefer maximum performance", from
/// NvApiDriverSettings.h.
const PREFERRED_PSTATE: u32 = 0x1057_eb71;
const PREFER_MAX: u32 = 1;

// What the functions answer, from nvapi_lite_common.h.
const OK: i32 = 0;
const SETTING_NOT_FOUND: i32 = -160;
const PROFILE_NOT_FOUND: i32 = -163;
const EXECUTABLE_NOT_FOUND: i32 = -166;

/// A value of 32 bits, held by the profile itself rather than inherited.
const DWORD_TYPE: u32 = 0;
const CURRENT_PROFILE_LOCATION: u32 = 0;

/// NvAPI's strings: UTF-16 with a nought after, in a fixed array.
type UnicodeString = [u16; 2048];

/// NVDRS_PROFILE_V1.
#[repr(C)]
struct Profile {
    version: u32,
    name: UnicodeString,
    gpu_support: u32,
    is_predefined: u32,
    applications: u32,
    settings: u32,
}

/// NVDRS_APPLICATION_V1.
#[repr(C)]
struct Application {
    version: u32,
    is_predefined: u32,
    name: UnicodeString,
    friendly_name: UnicodeString,
    launcher: UnicodeString,
}

/// NVDRS_SETTING_V1, packed on four bytes: each of its two values is a
/// union as large as its binary form, 4100 bytes, whose first 32 bits
/// are the value of a setting of that size.
#[repr(C)]
struct Setting {
    version: u32,
    name: UnicodeString,
    id: u32,
    kind: u32,
    location: u32,
    is_current_predefined: u32,
    is_predefined_valid: u32,
    predefined: [u32; 1025],
    current: [u32; 1025],
}

const _: () = assert!(size_of::<Profile>() == 4116);
const _: () = assert!(size_of::<Application>() == 12296);
const _: () = assert!(size_of::<Setting>() == 12320);

/// A structure's version, as NvAPI checks it: its size, and the version
/// of its layout above.
const fn version<T>(layout: u32) -> u32 {
    size_of::<T>() as u32 | (layout << 16)
}

type Status = i32;
type Handle = *mut c_void;
type Plain = unsafe extern "C" fn() -> Status;
type OnSession = unsafe extern "C" fn(Handle) -> Status;
type CreateSession = unsafe extern "C" fn(*mut Handle) -> Status;
type FindProfileByName = unsafe extern "C" fn(Handle, *const u16, *mut Handle) -> Status;
type CreateProfile = unsafe extern "C" fn(Handle, *mut Profile, *mut Handle) -> Status;
type GetApplicationInfo =
    unsafe extern "C" fn(Handle, Handle, *const u16, *mut Application) -> Status;
type CreateApplication = unsafe extern "C" fn(Handle, Handle, *mut Application) -> Status;
type GetSetting = unsafe extern "C" fn(Handle, Handle, u32, *mut Setting) -> Status;
type SetSetting = unsafe extern "C" fn(Handle, Handle, *mut Setting) -> Status;

/// Makes sure the driver keeps the card at full speed for this program,
/// and says so. Nothing is said on a machine without an NVIDIA driver.
pub(super) fn full_speed(log: &Log) {
    let Some(program) = std::env::current_exe()
        .ok()
        .and_then(|path| Some(path.file_name()?.to_string_lossy().into_owned()))
    else {
        return;
    };
    let started = Instant::now();
    let Some(api) = NvApi::load() else {
        return;
    };
    let kept = api.keep_full_speed(&program);
    let took = started.elapsed().as_millis();
    log.write(&match kept {
        Ok(false) => {
            format!(
                "the NVIDIA driver keeps the card at full speed for {program} (seen in {took} ms)"
            )
        }
        Ok(true) => format!(
            "the NVIDIA driver now keeps the card at full speed for {program} (set in {took} ms)"
        ),
        Err(e) => format!(
            "the NVIDIA driver could not be told to keep the card at full speed for {program}: \
             {e}"
        ),
    });
}

/// The driver's library, loaded and initialised.
struct NvApi {
    library: HMODULE,
    query: unsafe extern "C" fn(u32) -> *mut c_void,
}

impl NvApi {
    /// None where there is no NVIDIA driver, or no card of its own.
    fn load() -> Option<Self> {
        // SAFETY: a library of the system's own, looked for in System32
        // alone.
        let library =
            unsafe { LoadLibraryExW(LIBRARY, None, LOAD_LIBRARY_SEARCH_SYSTEM32) }.ok()?;
        // SAFETY: the library just loaded, and a terminated name.
        let Some(query) = (unsafe { GetProcAddress(library, s!("nvapi_QueryInterface")) }) else {
            // SAFETY: loaded above, and nothing of it is used.
            let _ = unsafe { FreeLibrary(library) };
            return None;
        };
        // SAFETY: `void *nvapi_QueryInterface(NvU32)`, as NVIDIA's headers
        // declare it, in place of the shape every export is given.
        let query = unsafe {
            std::mem::transmute::<
                unsafe extern "system" fn() -> isize,
                unsafe extern "C" fn(u32) -> *mut c_void,
            >(query)
        };
        let api = Self { library, query };
        // SAFETY: NvAPI_Initialize's own type.
        let initialize = unsafe { api.function::<Plain>(INITIALIZE) }?;
        // SAFETY: a function that takes nothing.
        (unsafe { initialize() } == OK).then_some(api)
    }

    /// Whether anything had to be written: the profile, the program in
    /// it and the setting in it are each made if missing.
    fn keep_full_speed(&self, program: &str) -> Result<bool, String> {
        let session = Session::open(self)?;
        let (profile, made) = session.profile()?;
        let placed = session.place(profile, program)?;
        let set = session.set_full_speed(profile)?;
        let written = made || placed || set;
        if written {
            session.save()?;
        }
        Ok(written)
    }

    /// The function numbered `id`, or a line naming it if the driver
    /// lacks it.
    ///
    /// # Safety
    ///
    /// `F` must be that function's own type.
    unsafe fn call<F: Copy>(&self, id: u32, name: &str) -> Result<F, String> {
        // SAFETY: as the caller vouches.
        unsafe { self.function(id) }.ok_or_else(|| format!("this driver has no {name}"))
    }

    /// The function numbered `id`, if the driver has it.
    ///
    /// # Safety
    ///
    /// `F` must be that function's own type.
    unsafe fn function<F: Copy>(&self, id: u32) -> Option<F> {
        // SAFETY: NvAPI's own lookup, which answers null for a number it
        // does not know.
        let found = unsafe { (self.query)(id) };
        if found.is_null() || size_of::<F>() != size_of::<*mut c_void>() {
            return None;
        }
        // SAFETY: a function's address, of the type the caller vouches
        // for, which is as large as a pointer.
        Some(unsafe { std::mem::transmute_copy::<*mut c_void, F>(&found) })
    }
}

impl Drop for NvApi {
    fn drop(&mut self) {
        // SAFETY: NvAPI_Unload takes nothing; the library, loaded in
        // `load`, is not used after.
        unsafe {
            if let Some(unload) = self.function::<Plain>(UNLOAD) {
                unload();
            }
            let _ = FreeLibrary(self.library);
        }
    }
}

/// The driver's settings, loaded, for as long as this lives.
struct Session<'a> {
    api: &'a NvApi,
    handle: Handle,
}

impl<'a> Session<'a> {
    fn open(api: &'a NvApi) -> Result<Self, String> {
        let mut handle: Handle = ptr::null_mut();
        // SAFETY: the function's own type, and a place for the handle.
        let status = unsafe {
            api.call::<CreateSession>(CREATE_SESSION, "NvAPI_DRS_CreateSession")?(&mut handle)
        };
        checked("NvAPI_DRS_CreateSession", status)?;
        let session = Self { api, handle };
        // SAFETY: the function's own type, and the session just made.
        let status = unsafe {
            api.call::<OnSession>(LOAD_SETTINGS, "NvAPI_DRS_LoadSettings")?(session.handle)
        };
        checked("NvAPI_DRS_LoadSettings", status)?;
        Ok(session)
    }

    /// The profile, and whether it had to be made.
    fn profile(&self) -> Result<(Handle, bool), String> {
        let name = unicode(PROFILE);
        let mut profile: Handle = ptr::null_mut();
        // SAFETY: the function's own type; a live session, a full string
        // and a place for the handle.
        let found =
            unsafe {
                self.api.call::<FindProfileByName>(
                    FIND_PROFILE_BY_NAME,
                    "NvAPI_DRS_FindProfileByName",
                )?(self.handle, name.as_ptr(), &mut profile)
            };
        match found {
            OK => return Ok((profile, false)),
            PROFILE_NOT_FOUND => {}
            other => return Err(said("NvAPI_DRS_FindProfileByName", other)),
        }
        let mut made = Profile {
            version: version::<Profile>(1),
            name,
            gpu_support: 0,
            is_predefined: 0,
            applications: 0,
            settings: 0,
        };
        // SAFETY: the function's own type; a live session, a profile
        // described in full and a place for its handle.
        let status = unsafe {
            self.api
                .call::<CreateProfile>(CREATE_PROFILE, "NvAPI_DRS_CreateProfile")?(
                self.handle,
                &mut made,
                &mut profile,
            )
        };
        checked("NvAPI_DRS_CreateProfile", status)?;
        Ok((profile, true))
    }

    /// Whether `program` had to be put in `profile`.
    fn place(&self, profile: Handle, program: &str) -> Result<bool, String> {
        let name = unicode(program);
        let mut found = Application {
            version: version::<Application>(1),
            is_predefined: 0,
            name: [0; 2048],
            friendly_name: [0; 2048],
            launcher: [0; 2048],
        };
        // SAFETY: the function's own type; a live session and profile, a
        // full string and a description for the answer.
        let status =
            unsafe {
                self.api.call::<GetApplicationInfo>(
                    GET_APPLICATION_INFO,
                    "NvAPI_DRS_GetApplicationInfo",
                )?(self.handle, profile, name.as_ptr(), &mut found)
            };
        match status {
            OK => return Ok(false),
            EXECUTABLE_NOT_FOUND => {}
            other => return Err(said("NvAPI_DRS_GetApplicationInfo", other)),
        }
        let mut made = Application {
            version: version::<Application>(1),
            is_predefined: 0,
            name,
            friendly_name: name,
            launcher: [0; 2048],
        };
        // SAFETY: the function's own type; a live session and profile,
        // and a program described in full.
        let status = unsafe {
            self.api
                .call::<CreateApplication>(CREATE_APPLICATION, "NvAPI_DRS_CreateApplication")?(
                self.handle,
                profile,
                &mut made,
            )
        };
        checked("NvAPI_DRS_CreateApplication", status)?;
        Ok(true)
    }

    /// Whether "Prefer maximum performance" had to be written into
    /// `profile`: it counts only if the profile holds it itself.
    fn set_full_speed(&self, profile: Handle) -> Result<bool, String> {
        let mut setting = Setting::empty();
        // SAFETY: the function's own type; a live session and profile,
        // and a setting of the version given for the answer.
        let status = unsafe {
            self.api
                .call::<GetSetting>(GET_SETTING, "NvAPI_DRS_GetSetting")?(
                self.handle,
                profile,
                PREFERRED_PSTATE,
                &mut setting,
            )
        };
        match status {
            OK if setting.location == CURRENT_PROFILE_LOCATION
                && setting.current[0] == PREFER_MAX =>
            {
                return Ok(false);
            }
            OK | SETTING_NOT_FOUND => {}
            other => return Err(said("NvAPI_DRS_GetSetting", other)),
        }
        let mut setting = Setting::empty();
        setting.id = PREFERRED_PSTATE;
        setting.kind = DWORD_TYPE;
        setting.location = CURRENT_PROFILE_LOCATION;
        setting.current[0] = PREFER_MAX;
        // SAFETY: the function's own type; a live session and profile, and
        // a setting described in full.
        let status = unsafe {
            self.api
                .call::<SetSetting>(SET_SETTING, "NvAPI_DRS_SetSetting")?(
                self.handle,
                profile,
                &mut setting,
            )
        };
        checked("NvAPI_DRS_SetSetting", status)?;
        Ok(true)
    }

    /// Writes what changed into the driver's settings.
    fn save(&self) -> Result<(), String> {
        // SAFETY: the function's own type, and a live session.
        let status = unsafe {
            self.api
                .call::<OnSession>(SAVE_SETTINGS, "NvAPI_DRS_SaveSettings")?(self.handle)
        };
        checked("NvAPI_DRS_SaveSettings", status)
    }
}

impl Drop for Session<'_> {
    fn drop(&mut self) {
        // SAFETY: the function's own type.
        if let Some(destroy) = unsafe { self.api.function::<OnSession>(DESTROY_SESSION) } {
            // SAFETY: the session made in `open`, not used after.
            unsafe { destroy(self.handle) };
        }
    }
}

impl Setting {
    /// A setting of this version with nothing in it, for the driver to
    /// fill or for the caller to describe.
    fn empty() -> Self {
        Self {
            version: version::<Setting>(1),
            name: [0; 2048],
            id: 0,
            kind: 0,
            location: 0,
            is_current_predefined: 0,
            is_predefined_valid: 0,
            predefined: [0; 1025],
            current: [0; 1025],
        }
    }
}

/// `text` as NvAPI takes a string: cut to what its array holds, with the
/// nought after.
fn unicode(text: &str) -> UnicodeString {
    let mut string = [0; 2048];
    for (unit, letter) in string[..2047].iter_mut().zip(text.encode_utf16()) {
        *unit = letter;
    }
    string
}

fn checked(what: &str, status: Status) -> Result<(), String> {
    if status == OK {
        Ok(())
    } else {
        Err(said(what, status))
    }
}

fn said(what: &str, status: Status) -> String {
    format!("{what} answered {status}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_string_is_cut_to_what_the_array_holds_and_always_ends() {
        let short = unicode("zyrdeskd.exe");
        assert_eq!(String::from_utf16_lossy(&short[..12]), "zyrdeskd.exe");
        assert_eq!(short[12], 0);
        let long = unicode(&"é".repeat(3000));
        assert!(long[..2047].iter().all(|&unit| unit == 0x00e9));
        assert_eq!(long[2047], 0);
    }

    #[test]
    fn versions_carry_the_size_and_the_layout() {
        assert_eq!(version::<Setting>(1), 0x0001_3020);
        assert_eq!(version::<Profile>(1), 0x0001_1014);
        assert_eq!(version::<Application>(1), 0x0001_3008);
    }
}
