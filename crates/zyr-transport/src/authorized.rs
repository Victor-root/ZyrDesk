//! The devices this computer lets in.
//!
//! Until the rendezvous server exists, the list is kept in a plain file
//! and filled in by hand: each computer shows its fingerprint, the other
//! one writes it down. Nothing about the mechanism changes once the
//! server hands over the same fingerprints inside a session ticket.
//!
//! The file is meant to be readable and correctable in a text editor. It
//! holds no secret: a fingerprint is public, and knowing one grants
//! nothing at all.

use std::fs;
use std::io;
use std::path::Path;

use crate::identity::Fingerprint;

/// Reads the authorised devices. A missing file simply means none.
pub fn read(path: &Path) -> io::Result<Vec<Fingerprint>> {
    let contents = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };

    let mut devices = Vec::new();
    for (number, line) in contents.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // A mistyped fingerprint is reported rather than skipped: an
        // authorisation silently ignored would look like a network fault
        // for as long as it takes to doubt this file.
        let device = line.parse::<Fingerprint>().map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{} ligne {} : {e}", path.display(), number + 1),
            )
        })?;
        if !devices.contains(&device) {
            devices.push(device);
        }
    }
    Ok(devices)
}

/// Adds a device. `false` when it was already there.
pub fn add(path: &Path, device: Fingerprint) -> io::Result<bool> {
    let mut devices = read(path)?;
    if devices.contains(&device) {
        return Ok(false);
    }
    devices.push(device);
    write(path, &devices)?;
    Ok(true)
}

/// Removes a device. `false` when it was not there.
pub fn remove(path: &Path, device: Fingerprint) -> io::Result<bool> {
    let mut devices = read(path)?;
    let before = devices.len();
    devices.retain(|known| *known != device);
    if devices.len() == before {
        return Ok(false);
    }
    write(path, &devices)?;
    Ok(true)
}

fn write(path: &Path, devices: &[Fingerprint]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut contents = String::from(
        "# Ordinateurs autorisés à joindre celui-ci, une empreinte par ligne.\n\
         # Géré par « zyr-cli host authorize ».\n",
    );
    for device in devices {
        contents.push_str(&device.to_string());
        contents.push('\n');
    }
    fs::write(path, contents)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temporary_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "zyrdesk-authorized-{}.conf",
            zyr_proto::random::alphanumeric_string(12)
        ))
    }

    fn a_device() -> Fingerprint {
        "11".repeat(32).parse().unwrap()
    }

    fn another_device() -> Fingerprint {
        "22".repeat(32).parse().unwrap()
    }

    #[test]
    fn a_machine_that_authorised_nobody_lets_nobody_in() {
        assert!(read(&temporary_path()).unwrap().is_empty());
    }

    #[test]
    fn what_is_written_is_read_back() {
        let path = temporary_path();
        assert!(add(&path, a_device()).unwrap());
        assert!(add(&path, another_device()).unwrap());
        assert_eq!(read(&path).unwrap(), vec![a_device(), another_device()]);

        // Authorising twice is not an error, and does not duplicate.
        assert!(!add(&path, a_device()).unwrap());
        assert_eq!(read(&path).unwrap().len(), 2);

        assert!(remove(&path, a_device()).unwrap());
        assert_eq!(read(&path).unwrap(), vec![another_device()]);
        assert!(!remove(&path, a_device()).unwrap());

        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn comments_and_blank_lines_are_ignored() {
        let path = temporary_path();
        fs::write(&path, format!("# un commentaire\n\n  {}  \n\n", a_device())).unwrap();
        assert_eq!(read(&path).unwrap(), vec![a_device()]);
        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn a_mistyped_fingerprint_is_reported_rather_than_skipped() {
        let path = temporary_path();
        fs::write(&path, format!("{}\nnimportequoi\n", a_device())).unwrap();
        let failure = read(&path).unwrap_err();
        assert_eq!(failure.kind(), io::ErrorKind::InvalidData);
        // The line number is what makes the file correctable.
        assert!(failure.to_string().contains("ligne 2"), "{failure}");
        fs::remove_file(&path).unwrap();
    }
}
