//! Checks this machine: every check gives back a status and a detail.

use std::fmt;
use std::path::PathBuf;
use std::process::ExitCode;

use zyr_engine_host::{SunshineConfig, ports};
use zyr_proto::net::{ENGINE_BASE_PORT_MAX, ENGINE_BASE_PORT_MIN};
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
            Status::Failure => "[ÉCHEC]",
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
    let verifications = [
        platform(),
        gpu(),
        engine_ports(),
        data_folder(),
        engine_configuration(),
        engines(),
        service(),
    ];

    println!("Diagnostic ZyrDesk v{}\n", zyr_proto::PRODUCT_VERSION);
    let mut failed = false;
    for v in &verifications {
        println!("{} {:24} {}", v.status, v.name, v.detail);
        if matches!(v.status, Status::Failure) {
            failed = true;
        }
    }
    println!();
    if failed {
        println!("Au moins une vérification a échoué.");
        ExitCode::FAILURE
    } else {
        println!("Machine prête pour ce stade du projet.");
        ExitCode::SUCCESS
    }
}

fn platform() -> Verification {
    let detail = format!("{} ({})", std::env::consts::OS, std::env::consts::ARCH);
    if cfg!(windows) {
        Verification {
            name: "Plateforme",
            status: Status::Ok,
            detail,
        }
    } else {
        Verification {
            name: "Plateforme",
            status: Status::Warning,
            detail: format!(
                "{detail} : environnement de développement, non supporté en production"
            ),
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
                    name: "Processeur graphique",
                    status: Status::Warning,
                    detail: "aucun adaptateur listé".to_string(),
                }
            } else {
                Verification {
                    name: "Processeur graphique",
                    status: Status::Ok,
                    detail: names.join(" ; "),
                }
            }
        }
        _ => Verification {
            name: "Processeur graphique",
            status: Status::Warning,
            detail: "détection impossible (PowerShell indisponible ?)".to_string(),
        },
    }
}

#[cfg(not(windows))]
fn gpu() -> Verification {
    Verification {
        name: "Processeur graphique",
        status: Status::Warning,
        detail: "détection non disponible hors Windows".to_string(),
    }
}

fn engine_ports() -> Verification {
    match ports::free_base() {
        Some(ports) => Verification {
            name: "Ports moteur",
            status: Status::Ok,
            detail: format!(
                "base {} disponible (plage {}-{})",
                ports.base(),
                ENGINE_BASE_PORT_MIN,
                ENGINE_BASE_PORT_MAX
            ),
        },
        None => Verification {
            name: "Ports moteur",
            status: Status::Failure,
            detail: format!(
                "aucune base libre dans {}-{}",
                ENGINE_BASE_PORT_MIN, ENGINE_BASE_PORT_MAX
            ),
        },
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
            name: "Dossier de données",
            status: Status::Ok,
            detail: format!("{} accessible en écriture", folder.display()),
        },
        Err(e) => Verification {
            name: "Dossier de données",
            status: Status::Failure,
            detail: format!("{} : {e}", folder.display()),
        },
    }
}

fn engine_configuration() -> Verification {
    match ports::free_base() {
        Some(ports) => {
            let config = SunshineConfig::new(ports, paths::host_state_dir(), paths::logs_dir());
            let directives = config.render_conf().lines().count();
            Verification {
                name: "Configuration moteur",
                status: Status::Ok,
                detail: format!("génération OK ({directives} directives)"),
            }
        }
        None => Verification {
            name: "Configuration moteur",
            status: Status::Failure,
            detail: "impossible sans base de ports libre".to_string(),
        },
    }
}

fn engines() -> Verification {
    let missing: Vec<&str> = [
        ("hôte", paths::host_engine_exe()),
        ("client", paths::client_engine_exe()),
    ]
    .into_iter()
    .filter(|(_, path): &(&str, PathBuf)| !path.is_file())
    .map(|(role, _)| role)
    .collect();

    if missing.is_empty() {
        Verification {
            name: "Moteurs",
            status: Status::Ok,
            detail: "hôte et client en place".to_string(),
        }
    } else {
        Verification {
            name: "Moteurs",
            status: Status::Warning,
            detail: format!(
                "absent(s) : {} (voir « zyr-cli engines status »)",
                missing.join(", ")
            ),
        }
    }
}

#[cfg(windows)]
fn service() -> Verification {
    let output = std::process::Command::new("sc.exe")
        .args(["query", "zyrdeskd"])
        .output();
    match output {
        Ok(s) if s.status.success() => Verification {
            name: "Service ZyrDesk",
            status: Status::Ok,
            detail: "installé".to_string(),
        },
        Ok(_) => Verification {
            name: "Service ZyrDesk",
            status: Status::Warning,
            detail: "non installé (normal : arrive au jalon M3)".to_string(),
        },
        Err(e) => Verification {
            name: "Service ZyrDesk",
            status: Status::Warning,
            detail: format!("état indéterminé : {e}"),
        },
    }
}

#[cfg(not(windows))]
fn service() -> Verification {
    Verification {
        name: "Service ZyrDesk",
        status: Status::Warning,
        detail: "sans objet hors Windows".to_string(),
    }
}
