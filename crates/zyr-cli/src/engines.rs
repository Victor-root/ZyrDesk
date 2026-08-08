//! Inspecting the engines dropped on the machine.
//!
//! The client engine is ours and comes out of our own build, already
//! carrying the product's name. The host engine is still the upstream
//! one, placed by hand and renamed. This command says exactly what is
//! expected and what is missing.

use std::path::Path;
use std::process::ExitCode;

use clap::Subcommand;
use zyr_proto::paths;

#[derive(Subcommand)]
pub enum Action {
    /// Says where the engines are expected and which ones are in place
    Status,
}

pub fn run(action: Action) -> ExitCode {
    match action {
        Action::Status => status(),
    }
}

fn status() -> ExitCode {
    let host = paths::host_engine_exe();
    let client = paths::client_engine_exe();

    println!("Moteurs ZyrDesk\n");
    let host_ok = line("Moteur hôte", &host);
    let client_ok = line("Moteur client", &client);
    println!();

    if host_ok && client_ok {
        println!("Les deux moteurs sont en place.");
        return ExitCode::SUCCESS;
    }

    println!("Mise en place attendue :\n");
    if !host_ok {
        println!("  Moteur hôte");
        println!("    1. Récupérer la version épinglée dans patches/MANIFEST.md.");
        println!(
            "    2. Copier son contenu dans {}",
            paths::host_engine_dir().display()
        );
        println!(
            "    3. Renommer son exécutable en {}",
            host.file_name().unwrap_or_default().to_string_lossy()
        );
    }
    if !client_ok {
        println!("  Moteur client");
        println!("    1. Récupérer le moteur produit par le workflow « Moteurs ».");
        println!(
            "    2. Copier son contenu dans {}",
            paths::client_engine_dir().display()
        );
        println!("       Il porte déjà le nom du produit, rien à renommer.");
    }
    ExitCode::FAILURE
}

fn line(role: &str, path: &Path) -> bool {
    let present = path.is_file();
    let mark = if present { "[ OK ]" } else { "[ !  ]" };
    let detail = if present { "présent" } else { "absent" };
    println!("{mark} {role:14} {detail} : {}", path.display());
    present
}
