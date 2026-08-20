//! Replacing a file whole, without a moment where it is neither.
//!
//! A file rewritten in place is empty for the time of the write: a
//! service killed in that instant, or a machine losing power, leaves a
//! blank file behind, and what it held is silently reset to defaults at
//! the next start. The remote-access switch lives in such a file; a
//! choice that can be lost to a power cut is not a choice.
//!
//! The new contents are written beside the file and take its place in
//! one move, which the system does whole or not at all.

use std::fs;
use std::io;
use std::path::Path;

/// What the file in the making is called, beside the real one.
const IN_THE_MAKING: &str = "new";

/// Replaces the file's contents, whole or not at all.
///
/// The folder is created if needed. The half-written file lives beside
/// the real one for the length of the write, under the same name with
/// one more ending, and takes its place in a single move.
pub fn replace(path: &Path, contents: &str) -> io::Result<()> {
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder)?;
    }
    let making = match path.extension() {
        Some(ending) => {
            let mut both = ending.to_os_string();
            both.push(".");
            both.push(IN_THE_MAKING);
            path.with_extension(both)
        }
        None => path.with_extension(IN_THE_MAKING),
    };
    fs::write(&making, contents)?;
    fs::rename(&making, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_folder(name: &str) -> std::path::PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "zyrdesk-files-{}-{name}",
            crate::random::alphanumeric_string(8)
        ));
        let _ = fs::remove_dir_all(&folder);
        folder
    }

    #[test]
    fn the_file_is_replaced_and_nothing_is_left_beside_it() {
        let folder = fresh_folder("remplace");
        let path = folder.join("choix.conf");

        replace(&path, "avant").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "avant");

        replace(&path, "après").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "après");

        // Le fichier en cours d'écriture ne survit pas au remplacement :
        // un reste s'accumulerait à chaque écriture.
        let left: Vec<_> = fs::read_dir(&folder).unwrap().collect();
        assert_eq!(left.len(), 1, "{left:?}");

        fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn the_folder_is_created_on_the_way() {
        let folder = fresh_folder("dossier");
        let path = folder.join("plus").join("loin.conf");
        replace(&path, "écrit").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "écrit");
        fs::remove_dir_all(&folder).unwrap();
    }
}
