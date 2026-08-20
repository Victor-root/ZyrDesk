//! Where the product keeps its files.
//!
//! Everything lives under a single `data` folder at the project root.
//! Nothing is scattered elsewhere on the machine: the contents can be
//! read, backed up and erased in one move.
//!
//! The `ZYRDESK_DATA` environment variable moves it. The system-wide
//! location of an installed product comes with the service.

use std::ffi::OsString;
use std::path::PathBuf;

const DATA_VAR: &str = "ZYRDESK_DATA";

/// Root of every file the product owns.
pub fn data_dir() -> PathBuf {
    resolve_data_dir(std::env::var_os(DATA_VAR), project_root)
}

/// The resolution rule, kept apart from the environment so it can be
/// checked.
fn resolve_data_dir(override_value: Option<OsString>, root: impl FnOnce() -> PathBuf) -> PathBuf {
    match override_value {
        Some(path) if !path.is_empty() => PathBuf::from(path),
        _ => root().join("data"),
    }
}

/// Project root: the first ancestor of the executable that holds a
/// `Cargo.toml`. The executable lives under `target/<profile>/`, so
/// walking up lands on the repository root whatever the build profile.
/// Failing that, the executable's own folder.
fn project_root() -> PathBuf {
    let Ok(exe) = std::env::current_exe() else {
        return PathBuf::from(".");
    };
    let mut candidate = exe.parent();
    while let Some(folder) = candidate {
        if folder.join("Cargo.toml").is_file() {
            return folder.to_path_buf();
        }
        candidate = folder.parent();
    }
    exe.parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Executable file name, with the platform's extension.
pub fn executable_name(base: &str) -> String {
    if cfg!(windows) {
        format!("{base}.exe")
    } else {
        base.to_string()
    }
}

/// Engine binaries.
pub fn engines_dir() -> PathBuf {
    data_dir().join("engines")
}

/// Host engine, derived from Sunshine.
pub fn host_engine_dir() -> PathBuf {
    engines_dir().join("host")
}

/// Host engine executable, as expected on disk.
///
/// The name is the product's own, never the upstream project's: it is
/// what the user sees in the task manager.
pub fn host_engine_exe() -> PathBuf {
    host_engine_dir().join(executable_name("zyrdesk-host-engine"))
}

/// Client engine, derived from Moonlight.
pub fn client_engine_dir() -> PathBuf {
    engines_dir().join("client")
}

/// Client engine executable, as expected on disk.
pub fn client_engine_exe() -> PathBuf {
    client_engine_dir().join(executable_name("zyrdesk-session"))
}

/// Cryptographic identity of this machine.
///
/// It has to last: this is what other devices pin.
pub fn identity_dir() -> PathBuf {
    data_dir().join("identity")
}

/// Configuration and state generated for the host engine.
pub fn host_state_dir() -> PathBuf {
    data_dir().join("host")
}

/// Isolated client engine state for one remote device.
pub fn device_state_dir(device_id: &str) -> PathBuf {
    data_dir().join("devices").join(device_id)
}

/// Where the virtual screen keeps its settings and its own log.
///
/// Under our roof and not at the root of the disk, where the driver
/// would have put them if nobody had told it otherwise: what the product
/// leaves behind is what the product knows how to clean up.
pub fn virtual_screen_dir() -> PathBuf {
    data_dir().join("screen")
}

/// The virtual screen driver's own files, carried by the product.
///
/// Beside the program and not under the data folder, because that is
/// what they are: files nobody on this machine owns or may change,
/// signed as a set, where editing, renaming or repacking any one of them
/// makes Windows stop recognising the signature and refuse them all.
///
/// One name answering in both worlds. Run from the repository it lands
/// on `vendor/ecran-virtuel`, where the files are kept; installed, on
/// the same folder next to the program, where the installer lays them.
/// Nothing has to be copied anywhere for a build made on this machine to
/// find them.
pub fn virtual_screen_driver_dir() -> PathBuf {
    project_root().join("vendor").join("ecran-virtuel")
}

/// Logs of every component.
pub fn logs_dir() -> PathBuf {
    data_dir().join("logs")
}

/// Fingerprints of the devices allowed to reach this computer.
pub fn authorized_devices() -> PathBuf {
    data_dir().join("authorized-devices.conf")
}

/// Computers written down by hand, kept on the home screen.
///
/// Apart from the authorised devices on purpose: one decides who may come
/// in, the other only spares somebody a copying.
pub fn known_computers() -> PathBuf {
    data_dir().join("known-computers.conf")
}

/// What the person asked the service to do, kept across restarts.
pub fn preferences() -> PathBuf {
    data_dir().join("preferences.conf")
}

/// Key combinations the window answers to during a session.
///
/// Apart from the preferences because nobody else needs them: they say
/// what a keyboard does on this machine, while the preferences say what
/// a session looks like, which the service has to know.
pub fn keyboard_shortcuts() -> PathBuf {
    data_dir().join("keyboard-shortcuts.conf")
}

/// Where the floating button of a session was last put down.
///
/// Apart for the same reason as the shortcuts: it says where a hand left
/// something on this screen, which is nobody else's business.
pub fn floating_button() -> PathBuf {
    data_dir().join("floating-button.conf")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn everything_lives_under_the_single_root() {
        let root = data_dir();
        for path in [
            engines_dir(),
            host_engine_dir(),
            host_engine_exe(),
            client_engine_dir(),
            client_engine_exe(),
            identity_dir(),
            host_state_dir(),
            device_state_dir("desk-pc"),
            logs_dir(),
            authorized_devices(),
            known_computers(),
            preferences(),
        ] {
            assert!(
                path.starts_with(&root),
                "{} outside the root",
                path.display()
            );
        }
    }

    #[test]
    fn the_override_wins_over_the_project_root() {
        let project = || PathBuf::from("/the/project");
        assert_eq!(
            resolve_data_dir(Some(OsString::from("/elsewhere")), project),
            PathBuf::from("/elsewhere")
        );
    }

    #[test]
    fn without_an_override_the_data_sits_in_the_project() {
        let project = || PathBuf::from("/the/project");
        assert_eq!(
            resolve_data_dir(None, project),
            PathBuf::from("/the/project/data")
        );
        // An empty variable counts as no override at all.
        assert_eq!(
            resolve_data_dir(Some(OsString::new()), project),
            PathBuf::from("/the/project/data")
        );
    }

    #[test]
    fn each_device_gets_its_own_folder() {
        assert_ne!(device_state_dir("a"), device_state_dir("b"));
    }

    #[test]
    fn the_executables_carry_the_product_name() {
        for exe in [host_engine_exe(), client_engine_exe()] {
            let name = exe.file_name().unwrap().to_string_lossy().to_lowercase();
            assert!(name.starts_with("zyrdesk"), "{name}");
            assert!(
                !name.contains("sunshine") && !name.contains("moonlight"),
                "{name}"
            );
        }
    }

    #[test]
    fn the_executable_extension_follows_the_platform() {
        let expected = if cfg!(windows) { "tool.exe" } else { "tool" };
        assert_eq!(executable_name("tool"), expected);
    }
}
