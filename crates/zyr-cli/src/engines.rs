//! Inspection des moteurs déposés sur la machine.
//!
//! Les moteurs ne sont pas encore compilés par le projet : ils sont
//! déposés manuellement, sous un nom ZyrDesk. Cette commande dit
//! exactement ce qui est attendu et ce qui manque.

use std::path::Path;
use std::process::ExitCode;

use clap::Subcommand;
use zyr_proto::paths;

#[derive(Subcommand)]
pub enum Action {
    /// Indique où les moteurs sont attendus et lesquels sont présents
    Status,
}

pub fn executer(action: Action) -> ExitCode {
    match action {
        Action::Status => status(),
    }
}

fn status() -> ExitCode {
    let hote = paths::host_engine_exe();
    let client = paths::client_engine_exe();

    println!("Moteurs ZyrDesk\n");
    let hote_ok = ligne("Moteur hôte", &hote);
    let client_ok = ligne("Moteur client", &client);
    println!();

    if hote_ok && client_ok {
        println!("Les deux moteurs sont en place.");
        return ExitCode::SUCCESS;
    }

    println!("Mise en place attendue :\n");
    if !hote_ok {
        println!("  Moteur hôte");
        println!("    1. Récupérer la version épinglée dans patches/MANIFEST.md.");
        println!(
            "    2. Copier son contenu dans {}",
            paths::host_engine_dir().display()
        );
        println!(
            "    3. Renommer son exécutable en {}",
            hote.file_name().unwrap_or_default().to_string_lossy()
        );
    }
    if !client_ok {
        println!("  Moteur client");
        println!("    1. Récupérer la version épinglée dans patches/MANIFEST.md.");
        println!(
            "    2. Copier son contenu dans {}",
            paths::client_engine_dir().display()
        );
        println!(
            "    3. Renommer son exécutable en {}",
            client.file_name().unwrap_or_default().to_string_lossy()
        );
    }
    ExitCode::FAILURE
}

fn ligne(role: &str, chemin: &Path) -> bool {
    let present = chemin.is_file();
    let etat = if present { "[ OK ]" } else { "[ !  ]" };
    let detail = if present { "présent" } else { "absent" };
    println!("{etat} {role:14} {detail} : {}", chemin.display());
    present
}
