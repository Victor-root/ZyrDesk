//! Diagnostic de la machine : chaque vérification rend un état et un détail.

use std::fmt;
use std::path::PathBuf;
use std::process::ExitCode;

use zyr_engine_host::{SunshineConfig, ports};
use zyr_proto::net::{ENGINE_BASE_PORT_MAX, ENGINE_BASE_PORT_MIN};
use zyr_proto::paths;

enum Etat {
    Ok,
    Attention,
    Echec,
}

impl fmt::Display for Etat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let symbole = match self {
            Etat::Ok => "[ OK ]",
            Etat::Attention => "[ !  ]",
            Etat::Echec => "[ÉCHEC]",
        };
        write!(f, "{symbole}")
    }
}

struct Verification {
    nom: &'static str,
    etat: Etat,
    detail: String,
}

pub fn run() -> ExitCode {
    let verifications = [
        plateforme(),
        gpu(),
        ports_moteur(),
        dossier_donnees(),
        configuration_moteur(),
        moteurs(),
        service(),
    ];

    println!("Diagnostic ZyrDesk v{}\n", zyr_proto::PRODUCT_VERSION);
    let mut echec = false;
    for v in &verifications {
        println!("{} {:24} {}", v.etat, v.nom, v.detail);
        if matches!(v.etat, Etat::Echec) {
            echec = true;
        }
    }
    println!();
    if echec {
        println!("Au moins une vérification a échoué.");
        ExitCode::FAILURE
    } else {
        println!("Machine prête pour ce stade du projet.");
        ExitCode::SUCCESS
    }
}

fn plateforme() -> Verification {
    let detail = format!("{} ({})", std::env::consts::OS, std::env::consts::ARCH);
    if cfg!(windows) {
        Verification {
            nom: "Plateforme",
            etat: Etat::Ok,
            detail,
        }
    } else {
        Verification {
            nom: "Plateforme",
            etat: Etat::Attention,
            detail: format!(
                "{detail} : environnement de développement, non supporté en production"
            ),
        }
    }
}

#[cfg(windows)]
fn gpu() -> Verification {
    let sortie = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "Get-CimInstance -ClassName Win32_VideoController | Select-Object -ExpandProperty Name",
        ])
        .output();
    match sortie {
        Ok(s) if s.status.success() => {
            let noms: Vec<String> = String::from_utf8_lossy(&s.stdout)
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect();
            if noms.is_empty() {
                // Toute machine Windows réelle expose un adaptateur : une liste
                // vide traduit une requête sans réponse, pas une absence de GPU.
                Verification {
                    nom: "Processeur graphique",
                    etat: Etat::Attention,
                    detail: "aucun adaptateur listé".to_string(),
                }
            } else {
                Verification {
                    nom: "Processeur graphique",
                    etat: Etat::Ok,
                    detail: noms.join(" ; "),
                }
            }
        }
        _ => Verification {
            nom: "Processeur graphique",
            etat: Etat::Attention,
            detail: "détection impossible (PowerShell indisponible ?)".to_string(),
        },
    }
}

#[cfg(not(windows))]
fn gpu() -> Verification {
    Verification {
        nom: "Processeur graphique",
        etat: Etat::Attention,
        detail: "détection non disponible hors Windows".to_string(),
    }
}

fn ports_moteur() -> Verification {
    match ports::free_base() {
        Some(ports) => Verification {
            nom: "Ports moteur",
            etat: Etat::Ok,
            detail: format!(
                "base {} disponible (plage {}-{})",
                ports.base(),
                ENGINE_BASE_PORT_MIN,
                ENGINE_BASE_PORT_MAX
            ),
        },
        None => Verification {
            nom: "Ports moteur",
            etat: Etat::Echec,
            detail: format!(
                "aucune base libre dans {}-{}",
                ENGINE_BASE_PORT_MIN, ENGINE_BASE_PORT_MAX
            ),
        },
    }
}

fn dossier_donnees() -> Verification {
    let dossier = paths::data_dir();
    let essai = || -> std::io::Result<()> {
        std::fs::create_dir_all(&dossier)?;
        let temoin = dossier.join(".doctor-ecriture");
        std::fs::write(&temoin, b"ok")?;
        std::fs::remove_file(&temoin)?;
        Ok(())
    };
    match essai() {
        Ok(()) => Verification {
            nom: "Dossier de données",
            etat: Etat::Ok,
            detail: format!("{} accessible en écriture", dossier.display()),
        },
        Err(e) => Verification {
            nom: "Dossier de données",
            etat: Etat::Echec,
            detail: format!("{} : {e}", dossier.display()),
        },
    }
}

fn configuration_moteur() -> Verification {
    match ports::free_base() {
        Some(ports) => {
            let config = SunshineConfig::new(ports, paths::host_state_dir(), paths::logs_dir());
            let directives = config.render_conf().lines().count();
            Verification {
                nom: "Configuration moteur",
                etat: Etat::Ok,
                detail: format!("génération OK ({directives} directives)"),
            }
        }
        None => Verification {
            nom: "Configuration moteur",
            etat: Etat::Echec,
            detail: "impossible sans base de ports libre".to_string(),
        },
    }
}

fn moteurs() -> Verification {
    let manquants: Vec<&str> = [
        ("hôte", paths::host_engine_exe()),
        ("client", paths::client_engine_exe()),
    ]
    .into_iter()
    .filter(|(_, chemin): &(&str, PathBuf)| !chemin.is_file())
    .map(|(role, _)| role)
    .collect();

    if manquants.is_empty() {
        Verification {
            nom: "Moteurs",
            etat: Etat::Ok,
            detail: "hôte et client en place".to_string(),
        }
    } else {
        Verification {
            nom: "Moteurs",
            etat: Etat::Attention,
            detail: format!(
                "absent(s) : {} (voir « zyr-cli engines status »)",
                manquants.join(", ")
            ),
        }
    }
}

#[cfg(windows)]
fn service() -> Verification {
    let sortie = std::process::Command::new("sc.exe")
        .args(["query", "zyrdeskd"])
        .output();
    match sortie {
        Ok(s) if s.status.success() => Verification {
            nom: "Service ZyrDesk",
            etat: Etat::Ok,
            detail: "installé".to_string(),
        },
        Ok(_) => Verification {
            nom: "Service ZyrDesk",
            etat: Etat::Attention,
            detail: "non installé (normal : arrive au jalon M3)".to_string(),
        },
        Err(e) => Verification {
            nom: "Service ZyrDesk",
            etat: Etat::Attention,
            detail: format!("état indéterminé : {e}"),
        },
    }
}

#[cfg(not(windows))]
fn service() -> Verification {
    Verification {
        nom: "Service ZyrDesk",
        etat: Etat::Attention,
        detail: "sans objet hors Windows".to_string(),
    }
}
