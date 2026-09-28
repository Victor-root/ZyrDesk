//! Checks this machine: every check gives back a status and a detail.

use std::fmt;
use std::process::ExitCode;
use std::sync::Arc;

use zyr_player::Ffmpeg;
use zyr_proto::paths;

enum Status {
    Ok,
    Warning,
    Failure,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let symbol = match self {
            Status::Ok => "[ OK ]",
            Status::Warning => "[ !  ]",
            Status::Failure => "[FAIL]",
        };
        write!(f, "{symbol}")
    }
}

struct Verification {
    name: &'static str,
    status: Status,
    detail: String,
}

pub fn run() -> ExitCode {
    let (ffmpeg, loaded) = ffmpeg();
    let verifications = [
        platform(),
        gpu(),
        data_folder(),
        ffmpeg,
        encoders(loaded.as_ref()),
        service(),
    ];

    println!("ZyrDesk diagnosis v{}\n", zyr_proto::PRODUCT_VERSION);
    let mut failed = false;
    for v in &verifications {
        println!("{} {:24} {}", v.status, v.name, v.detail);
        if matches!(v.status, Status::Failure) {
            failed = true;
        }
    }
    println!();
    if failed {
        println!("At least one check failed.");
        ExitCode::FAILURE
    } else {
        println!("Machine ready for ZyrDesk.");
        ExitCode::SUCCESS
    }
}

fn platform() -> Verification {
    let detail = format!("{} ({})", std::env::consts::OS, std::env::consts::ARCH);
    if cfg!(windows) {
        Verification {
            name: "Platform",
            status: Status::Ok,
            detail,
        }
    } else {
        Verification {
            name: "Platform",
            status: Status::Warning,
            detail: format!("{detail}: development environment, not supported in production"),
        }
    }
}

#[cfg(windows)]
fn gpu() -> Verification {
    let output = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "Get-CimInstance -ClassName Win32_VideoController | Select-Object -ExpandProperty Name",
        ])
        .output();
    match output {
        Ok(s) if s.status.success() => {
            let names: Vec<String> = String::from_utf8_lossy(&s.stdout)
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect();
            if names.is_empty() {
                // Every real Windows machine shows an adapter: an empty list
                // means a query that got no answer, not a missing GPU.
                Verification {
                    name: "Graphics processor",
                    status: Status::Warning,
                    detail: "no adapter listed".to_string(),
                }
            } else {
                Verification {
                    name: "Graphics processor",
                    status: Status::Ok,
                    detail: names.join("; "),
                }
            }
        }
        _ => Verification {
            name: "Graphics processor",
            status: Status::Warning,
            detail: "could not detect (PowerShell unavailable?)".to_string(),
        },
    }
}

#[cfg(not(windows))]
fn gpu() -> Verification {
    Verification {
        name: "Graphics processor",
        status: Status::Warning,
        detail: "detection not available outside Windows".to_string(),
    }
}

fn data_folder() -> Verification {
    let folder = paths::data_dir();
    let attempt = || -> std::io::Result<()> {
        std::fs::create_dir_all(&folder)?;
        let marker = folder.join(".doctor-ecriture");
        std::fs::write(&marker, b"ok")?;
        std::fs::remove_file(&marker)?;
        Ok(())
    };
    match attempt() {
        Ok(()) => Verification {
            name: "Data folder",
            status: Status::Ok,
            detail: format!("{} writable", folder.display()),
        },
        Err(e) => Verification {
            name: "Data folder",
            status: Status::Failure,
            detail: format!("{}: {e}", folder.display()),
        },
    }
}

/// FFmpeg, loaded as the player and the host engine load it.
///
/// Loaded for real rather than looked for: a file of the right name can
/// still be the wrong build, or miss what it leans on, and only loading
/// it says so. Handed back loaded, for the encoders to be tried with.
///
/// Outside Windows the folder holds Windows' libraries and nothing else,
/// so its absence there is said and not failed on: FFmpeg for this
/// system is only ever loaded by the tests.
fn ffmpeg() -> (Verification, Option<Arc<Ffmpeg>>) {
    let folder = paths::ffmpeg_dir();
    match Ffmpeg::load(&folder) {
        Ok(ffmpeg) => (
            Verification {
                name: "FFmpeg",
                status: Status::Ok,
                detail: format!(
                    "present and loadable, version {} ({})",
                    ffmpeg.version(),
                    folder.display()
                ),
            },
            Some(ffmpeg),
        ),
        Err(e) => (
            Verification {
                name: "FFmpeg",
                status: if cfg!(windows) {
                    Status::Failure
                } else {
                    Status::Warning
                },
                detail: e.to_string(),
            },
            None,
        ),
    }
}

/// The encoders that really open on the graphics card of the main screen,
/// tried the way a session tries them.
///
/// A warning and not a failure when none does: this computer can still
/// reach others, it simply cannot be reached for a picture.
#[cfg(windows)]
fn encoders(ffmpeg: Option<&Arc<Ffmpeg>>) -> Verification {
    const NAME: &str = "Video encoders";
    let Some(ffmpeg) = ffmpeg else {
        return Verification {
            name: NAME,
            status: Status::Failure,
            detail: "cannot be tried without FFmpeg".to_string(),
        };
    };
    let journal = crate::journal();
    let log = match zyr_proto::log::Log::open(&journal) {
        Ok(log) => log,
        Err(e) => {
            return Verification {
                name: NAME,
                status: Status::Warning,
                detail: format!("cannot be tried without journal {}: {e}", journal.display()),
            };
        }
    };
    // What FFmpeg says of the encoders it leaves out goes to that
    // journal, which is where anybody wondering why one is missing looks.
    ffmpeg.log_into(&log);
    match zyr_host::encoders(ffmpeg, &log) {
        Ok(found) if found.is_empty() => Verification {
            name: NAME,
            status: Status::Warning,
            detail: format!(
                "none opens: this computer cannot be controlled (see {})",
                journal.display()
            ),
        },
        Ok(found) => Verification {
            name: NAME,
            status: Status::Ok,
            detail: found
                .iter()
                .filter_map(|(codec, backend)| backend.encoder_name(*codec))
                .collect::<Vec<_>>()
                .join(", "),
        },
        Err(e) => Verification {
            name: NAME,
            status: Status::Warning,
            detail: format!("could not be tried: {}", zyr_i18n::fact(&e)),
        },
    }
}

#[cfg(not(windows))]
fn encoders(_ffmpeg: Option<&Arc<Ffmpeg>>) -> Verification {
    Verification {
        name: "Video encoders",
        status: Status::Warning,
        detail: "tried on the main screen's graphics card, on Windows only".to_string(),
    }
}

#[cfg(windows)]
fn service() -> Verification {
    let output = std::process::Command::new("sc.exe")
        .args(["query", "zyrdeskd"])
        .output();
    match output {
        Ok(s) if s.status.success() => Verification {
            name: "ZyrDesk service",
            status: Status::Ok,
            detail: "installed".to_string(),
        },
        Ok(_) => Verification {
            name: "ZyrDesk service",
            status: Status::Warning,
            detail: "not installed: without it, no session opens in either direction".to_string(),
        },
        Err(e) => Verification {
            name: "ZyrDesk service",
            status: Status::Warning,
            detail: format!("state unknown: {e}"),
        },
    }
}

#[cfg(not(windows))]
fn service() -> Verification {
    Verification {
        name: "ZyrDesk service",
        status: Status::Warning,
        detail: "not applicable outside Windows".to_string(),
    }
}
