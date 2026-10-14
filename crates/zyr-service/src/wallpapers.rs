//! The wallpapers of the computers this one reached, kept for their
//! cards on the home screen.
//!
//! Each comes as a thumbnail, asked of the far computer once a session
//! with it is open, and is kept in a file named after its fingerprint
//! until the next session brings another, or says it shows none. When
//! the file last changed is what tells the window to ask for it again.

use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use zyr_proto::fingerprint::Fingerprint;

fn file(folder: &Path, peer: Fingerprint) -> PathBuf {
    folder.join(format!("{peer}.bmp"))
}

/// Keeps what that computer said of its wallpaper: its thumbnail, or
/// nothing, which takes away the one kept before.
pub fn keep(folder: &Path, peer: Fingerprint, picture: Option<&[u8]>) -> io::Result<()> {
    let path = file(folder, peer);
    match picture {
        Some(bytes) => zyr_proto::files::replace_bytes(&path, bytes),
        None => match std::fs::remove_file(&path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        },
    }
}

/// When that computer's thumbnail was last kept, in seconds since 1970.
pub fn stamp(folder: &Path, peer: Fingerprint) -> Option<u64> {
    let changed = std::fs::metadata(file(folder, peer))
        .and_then(|file| file.modified())
        .ok()?;
    changed
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|since| since.as_secs())
}

/// That computer's thumbnail, when one is kept.
pub fn read(folder: &Path, peer: Fingerprint) -> Option<Vec<u8>> {
    std::fs::read(file(folder, peer)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fingerprint(seed: u8) -> Fingerprint {
        format!("{seed:02x}").repeat(32).parse().unwrap()
    }

    #[test]
    fn a_thumbnail_kept_is_read_back_until_its_computer_says_it_has_none() {
        let folder = std::env::temp_dir().join(format!(
            "zyrdeskd-wallpapers-{}",
            zyr_proto::random::alphanumeric_string(8)
        ));
        let peer = fingerprint(1);
        assert_eq!(read(&folder, peer), None);
        assert_eq!(stamp(&folder, peer), None);

        keep(&folder, peer, Some(b"BM picture")).unwrap();
        assert_eq!(read(&folder, peer).as_deref(), Some(&b"BM picture"[..]));
        assert!(stamp(&folder, peer).is_some());
        assert_eq!(read(&folder, fingerprint(2)), None);

        keep(&folder, peer, None).unwrap();
        assert_eq!(read(&folder, peer), None);
        // Saying it again is no fault.
        keep(&folder, peer, None).unwrap();
        let _ = std::fs::remove_dir_all(folder);
    }
}
