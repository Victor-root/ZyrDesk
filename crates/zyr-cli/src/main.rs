mod bench;
mod connect;
mod cpu;
mod doctor;
mod engines;
mod host;
mod identity;
mod measurement;
mod probe;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "zyr-cli",
    version = zyr_proto::PRODUCT_VERSION,
    about = "Outil technique ZyrDesk",
    long_about = "Outil technique ZyrDesk.\n\n\
                  À ce stade du projet, il pilote directement les moteurs \
                  pour valider les performances en réseau local. Le service \
                  et l'interface prennent le relais aux jalons suivants."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Vérifie que cette machine est prête pour ZyrDesk
    Doctor,
    /// Inspecte les moteurs installés
    Engines {
        #[command(subcommand)]
        action: engines::Action,
    },
    /// Rend cet ordinateur accessible à distance
    Host {
        #[command(subcommand)]
        action: host::Action,
    },
    /// Ouvre une session sur un ordinateur distant
    Connect(connect::Args),
    /// Affiche l'empreinte de cette machine
    Identity,
    /// Mesure ce que coûte le tunnel entre deux ordinateurs
    Bench {
        #[command(subcommand)]
        action: bench::Action,
    },
}

fn main() -> std::process::ExitCode {
    match Cli::parse().command {
        Command::Doctor => doctor::run(),
        Command::Engines { action } => engines::run(action),
        Command::Host { action } => host::run(action),
        Command::Connect(args) => connect::run(args),
        Command::Identity => identity::run(),
        Command::Bench { action } => bench::run(action),
    }
}

/// Reports a failure the same way everywhere, on the error stream.
pub fn failure(context: &str, error: impl std::fmt::Display) -> std::process::ExitCode {
    eprintln!("Échec : {context}");
    eprintln!("  {error}");
    std::process::ExitCode::FAILURE
}
