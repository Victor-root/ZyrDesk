//! What the tests share: FFmpeg itself, loaded once.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use crate::library::Ffmpeg;
use crate::sys;

/// Names the folder of a Linux build of FFmpeg for the tests.
const DIR_VARIABLE: &str = "ZYR_FFMPEG_DIR";

/// FFmpeg, from `ZYR_FFMPEG_DIR` or else `vendor/ffmpeg`.
///
/// A test that needs it fails when it cannot be loaded, saying where it
/// looked and how to get it, rather than passing without having run.
pub(crate) fn ffmpeg() -> Arc<Ffmpeg> {
    static LOADED: OnceLock<Arc<Ffmpeg>> = OnceLock::new();
    Arc::clone(LOADED.get_or_init(|| {
        let dir = std::env::var_os(DIR_VARIABLE)
            .filter(|dir| !dir.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(zyr_proto::paths::ffmpeg_dir);
        Ffmpeg::load(&dir).unwrap_or_else(|e| {
            panic!(
                "these tests need FFmpeg {version} and could not load it from {dir}: {e}\n\
                 Build it for this system with `packaging/ffmpeg/build.sh linux <out-dir>` \
                 and run the tests with {DIR_VARIABLE}=<out-dir>/lib.",
                version = sys::FFMPEG_VERSION.to_string_lossy(),
                dir = dir.display(),
            )
        })
    }))
}
