mod banc;
mod connect;
mod doctor;
mod engines;
mod host;
mod identite;
mod mesure;
mod sonde;

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
    commande: Commande,
}

#[derive(Subcommand)]
enum Commande {
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
    Identite,
    /// Mesure ce que coûte le tunnel entre deux ordinateurs
    Banc {
        #[command(subcommand)]
        action: banc::Action,
    },
}

fn main() -> std::process::ExitCode {
    match Cli::parse().commande {
        Commande::Doctor => doctor::executer(),
        Commande::Engines { action } => engines::executer(action),
        Commande::Host { action } => host::executer(action),
        Commande::Connect(args) => connect::executer(args),
        Commande::Identite => identite::executer(),
        Commande::Banc { action } => banc::executer(action),
    }
}

/// Signale un échec de façon uniforme sur la sortie d'erreur.
pub fn echec(contexte: &str, erreur: impl std::fmt::Display) -> std::process::ExitCode {
    eprintln!("Échec : {contexte}");
    eprintln!("  {erreur}");
    std::process::ExitCode::FAILURE
}
