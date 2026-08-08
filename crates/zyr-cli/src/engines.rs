//! Inspecting the engines dropped on the machine.
//!
//! Both come out of our own build and already carry the product's name.
//! This command says exactly where they are expected and which one is
//! missing.

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
    println!("  Le workflow « Moteurs » produit un artefact par moteur.");
    println!("  Décompresser celui qui manque dans son dossier ; il porte");
    println!("  déjà le nom du produit, rien à renommer.\n");
    if !host_ok {
        println!(
            "  zyrdesk-host-engine   -> {}",
            paths::host_engine_dir().display()
        );
    }
    if !client_ok {
        println!(
            "  zyrdesk-client-engine -> {}",
            paths::client_engine_dir().display()
        );
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
