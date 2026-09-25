//! Finding FFmpeg's libraries and opening them.
//!
//! Three libraries, opened by their full path from one folder, in the
//! order they lean on each other: avutil, then swresample, then avcodec.
//! On Windows each one is opened so that what it needs in turn is looked
//! for in its own folder and in the system's, and nowhere else: a DLL
//! of the same name lying in the current folder or on the PATH is never
//! picked up by mistake. Every function the bindings use is looked up
//! before anything is called, and each library must be the major
//! version the bindings were generated from.

use std::ffi::{CStr, c_char, c_int, c_uint};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use libloading::Library;
use zyr_proto::log::Log;

use crate::error::CodecError;
use crate::sys;

/// FFmpeg, loaded: the function tables of its three libraries.
///
/// Shared by everything that uses it, behind an `Arc`, and unloaded
/// only when the last encoder, decoder or resampler lets go of it, and
/// the log its lines go to (see [`Ffmpeg::log_into`]).
pub struct Ffmpeg {
    // Dropped in declaration order, so each library is let go of before
    // the ones it leans on.
    pub(crate) avcodec: sys::Avcodec,
    pub(crate) swresample: sys::Swresample,
    pub(crate) avutil: sys::Avutil,
    pub(crate) logging: sys::Logging,
    version: String,
}

impl Ffmpeg {
    /// Opens FFmpeg from `dir`, usually `zyr_proto::paths::ffmpeg_dir()`.
    pub fn load(dir: &Path) -> Result<Arc<Ffmpeg>, CodecError> {
        let (avutil, avutil_path) = open(
            dir,
            AVUTIL,
            &[sys::AVUTIL_FUNCTIONS, sys::Logging::FUNCTIONS],
        )?;
        let (swresample, swresample_path) = open(dir, SWRESAMPLE, &[sys::SWRESAMPLE_FUNCTIONS])?;
        let (avcodec, avcodec_path) = open(dir, AVCODEC, &[sys::AVCODEC_FUNCTIONS])?;

        let unresolved = |path: &Path, e: libloading::Error| CodecError::Library {
            path: path.to_path_buf(),
            reason: described(&e),
        };
        // SAFETY: each library is the FFmpeg library its table was
        // generated from, every name was found just above, and the
        // tables keep their library loaded for as long as they live. The
        // hand-written pair comes from avutil, which the table built
        // right after it keeps loaded.
        let (logging, avutil, swresample, avcodec) = unsafe {
            (
                sys::Logging::from_library(&avutil).map_err(|e| unresolved(&avutil_path, e))?,
                sys::Avutil::from_library(avutil).map_err(|e| unresolved(&avutil_path, e))?,
                sys::Swresample::from_library(swresample)
                    .map_err(|e| unresolved(&swresample_path, e))?,
                sys::Avcodec::from_library(avcodec).map_err(|e| unresolved(&avcodec_path, e))?,
            )
        };

        // SAFETY: returns a static string, never null.
        let version = unsafe { CStr::from_ptr(avutil.av_version_info()) }
            .to_string_lossy()
            .into_owned();
        Ok(Arc::new(Ffmpeg {
            avcodec,
            swresample,
            avutil,
            logging,
            version,
        }))
    }

    /// FFmpeg's version as its libraries report it, such as "9.0.2".
    pub fn version(&self) -> &str {
        &self.version
    }

    /// The files of FFmpeg's libraries that `dir` does not hold, under
    /// the names [`Ffmpeg::load`] opens them by, major version included.
    ///
    /// Looked for and not opened: for whoever only needs to know whether
    /// there is anything to load, without loading it into its own
    /// process. A file of the right name can still fail to load, which
    /// only [`Ffmpeg::load`] finds out.
    pub fn missing_from(dir: &Path) -> Vec<PathBuf> {
        LIBRARIES
            .iter()
            .map(|library| dir.join(file_name(*library)))
            .filter(|path| !path.is_file())
            .collect()
    }

    /// Sends what FFmpeg says, from warnings up, to the product's log,
    /// with what this crate finds on its own (encoders the probe had to
    /// leave out, and why).
    ///
    /// FFmpeg has a single log for the whole process: the last call
    /// decides where its lines go, and keeps its FFmpeg loaded until
    /// another one takes its place, since that FFmpeg puts the lines
    /// into words.
    pub fn log_into(self: &Arc<Self>, log: &Log) {
        crate::log::route(self, log);
    }

    /// FFmpeg's own words for an error code.
    pub(crate) fn error_text(&self, code: c_int) -> String {
        let mut text = [0 as c_char; sys::AV_ERROR_MAX_STRING_SIZE as usize];
        // SAFETY: the buffer is as long as the size given, and
        // av_strerror always leaves a terminated string in it, a generic
        // one for a code it does not know.
        unsafe {
            self.avutil.av_strerror(code, text.as_mut_ptr(), text.len());
            CStr::from_ptr(text.as_ptr())
        }
        .to_string_lossy()
        .into_owned()
    }

    /// The error for a negative return of FFmpeg, the value otherwise.
    ///
    /// `what` is only put into words when FFmpeg refused: most calls
    /// succeed, many of them once a frame.
    pub(crate) fn check(&self, code: c_int, what: impl fmt::Display) -> Result<c_int, CodecError> {
        if code < 0 {
            return Err(CodecError::Refused {
                what: what.to_string(),
                code,
                text: self.error_text(code),
            });
        }
        Ok(code)
    }
}

/// A library of FFmpeg, by its name and the major version the bindings
/// were generated from.
type Named = (&'static str, u32);

const AVUTIL: Named = ("avutil", sys::LIBAVUTIL_VERSION_MAJOR);
const SWRESAMPLE: Named = ("swresample", sys::LIBSWRESAMPLE_VERSION_MAJOR);
const AVCODEC: Named = ("avcodec", sys::LIBAVCODEC_VERSION_MAJOR);

/// The three of them, in the order they are opened.
const LIBRARIES: [Named; 3] = [AVUTIL, SWRESAMPLE, AVCODEC];

/// The file a library has on this system.
fn file_name((library, major): Named) -> String {
    if cfg!(windows) {
        format!("{library}-{major}.dll")
    } else {
        format!("lib{library}.so.{major}")
    }
}

/// The major version out of FFmpeg's packed `AV_VERSION_INT`.
fn major_of(version: c_uint) -> u32 {
    version >> 16
}

/// Opens one library, checks that every function named is there and
/// that the library is the major version expected.
fn open(
    dir: &Path,
    (library, major): Named,
    functions: &[&[&'static str]],
) -> Result<(Library, PathBuf), CodecError> {
    // A full path: Windows only searches a DLL's own folder for what it
    // needs when the DLL was named that way.
    let named = dir.join(file_name((library, major)));
    let path = std::path::absolute(&named).map_err(|e| CodecError::Library {
        path: named,
        reason: e.to_string(),
    })?;
    // SAFETY: loading FFmpeg runs its initialisers, which only set up
    // its own tables.
    let loaded = unsafe { load(&path) }.map_err(|e| CodecError::Library {
        path: path.clone(),
        reason: described(&e),
    })?;
    for function in functions.iter().flat_map(|names| names.iter()) {
        // SAFETY: the symbol is only looked up, never called, so the
        // type it is looked up as does not matter.
        let symbol = unsafe { loaded.get::<unsafe extern "C" fn()>(*function) };
        if symbol.is_err() {
            return Err(CodecError::Function { path, function });
        }
    }
    let getter = format!("{library}_version");
    // SAFETY: every FFmpeg library exports `<name>_version`, which takes
    // nothing and returns its packed version.
    let found = unsafe {
        let version = loaded.get::<unsafe extern "C" fn() -> c_uint>(getter.as_str());
        version.map(|version| major_of(version()))
    }
    .map_err(|e| CodecError::Library {
        path: path.clone(),
        reason: described(&e),
    })?;
    if found != major {
        return Err(CodecError::Version {
            path,
            found,
            expected: major,
        });
    }
    Ok((loaded, path))
}

/// The directories Windows searches for what a DLL needs in turn: its
/// own folder, then System32.
#[cfg(windows)]
unsafe fn load(path: &Path) -> Result<Library, libloading::Error> {
    use libloading::os::windows::{
        LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, LOAD_LIBRARY_SEARCH_SYSTEM32, Library as WindowsLibrary,
    };
    // SAFETY: passed on to the caller.
    unsafe {
        WindowsLibrary::load_with_flags(
            path,
            LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
        )
    }
    .map(Library::from)
}

/// By full path. What avcodec and swresample need of avutil is found by
/// name among the libraries already loaded, which is why avutil comes
/// first.
#[cfg(not(windows))]
unsafe fn load(path: &Path) -> Result<Library, libloading::Error> {
    // SAFETY: passed on to the caller.
    unsafe { Library::new(path) }
}

/// A loader error with the system's own explanation behind it.
fn described(error: &libloading::Error) -> String {
    match std::error::Error::source(error) {
        Some(source) => format!("{error} : {source}"),
        None => error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    #[test]
    fn library_files_carry_the_major_version_of_the_bindings() {
        let name = file_name(AVCODEC);
        let expected = if cfg!(windows) {
            "avcodec-63.dll"
        } else {
            "libavcodec.so.63"
        };
        assert_eq!(name, expected);
        assert_eq!(major_of(61 << 16 | 1 << 8 | 102), 61);
    }

    #[test]
    fn a_folder_without_ffmpeg_is_refused_with_its_path() {
        let empty = std::env::temp_dir().join(format!("zyr-codec-empty-{}", std::process::id()));
        std::fs::create_dir_all(&empty).unwrap();
        let refused = Ffmpeg::load(&empty)
            .err()
            .expect("an empty folder is not FFmpeg");
        std::fs::remove_dir_all(&empty).unwrap();

        match &refused {
            CodecError::Library { path, .. } => {
                assert_eq!(path, &empty.join(file_name(("avutil", 61))));
            }
            other => panic!("unexpected error: {other:?}"),
        }
        let said = refused.to_string();
        assert!(said.starts_with("FFmpeg introuvable"), "{said}");
        assert!(said.contains(&empty.display().to_string()), "{said}");
    }

    #[test]
    fn a_folder_says_which_of_ffmpeg_s_files_it_lacks() {
        let folder = std::env::temp_dir().join(format!("zyr-codec-lacks-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let every = LIBRARIES.map(|library| folder.join(file_name(library)));
        assert_eq!(Ffmpeg::missing_from(&folder), every);

        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(&every[0], b"").unwrap();
        std::fs::write(&every[2], b"").unwrap();
        assert_eq!(Ffmpeg::missing_from(&folder), [every[1].clone()]);

        // A library of another major version is not the one the bindings
        // speak to: it counts as missing.
        std::fs::write(folder.join(file_name(("swresample", 6))), b"").unwrap();
        assert_eq!(Ffmpeg::missing_from(&folder), [every[1].clone()]);

        std::fs::write(&every[1], b"").unwrap();
        assert!(Ffmpeg::missing_from(&folder).is_empty());
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_relative_folder_is_opened_by_its_full_path() {
        let refused = Ffmpeg::load(Path::new("no-ffmpeg-here")).err();
        let Some(CodecError::Library { path, .. }) = refused else {
            panic!("unexpected: {refused:?}");
        };
        assert!(path.is_absolute(), "{}", path.display());
    }

    #[test]
    fn the_loaded_libraries_are_the_ones_the_bindings_describe() {
        let ff = testing::ffmpeg();
        assert_eq!(ff.version(), sys::FFMPEG_VERSION.to_str().unwrap());
    }

    #[test]
    fn error_codes_are_put_in_ffmpeg_words() {
        let ff = testing::ffmpeg();
        assert_eq!(ff.error_text(crate::error::END), "End of file");
        let again = ff.error_text(crate::error::AGAIN);
        assert!(again.contains("temporarily unavailable"), "{again}");
        let refused = ff.check(crate::error::END, "essai").unwrap_err();
        assert_eq!(refused.to_string(), "essai : End of file (code -541478725)");
        assert_eq!(ff.check(3, "essai").unwrap(), 3);
    }
}
