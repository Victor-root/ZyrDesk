//! Service ZyrDesk.
//!
//! Le même exécutable sert de deux façons. Lancé par Windows avec son
//! argument réservé, il devient le service. Lancé à la main, il sert à
//! l'installer, le démarrer, l'arrêter ou le retirer.

mod journal;
mod superviseur;
mod surveillance;

#[cfg(windows)]
mod service;

use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "zyrdeskd",
    version = zyr_proto::PRODUCT_VERSION,
    about = "Service ZyrDesk",
    long_about = "Service ZyrDesk.\n\n\
                  Il rend cet ordinateur accessible à distance, y compris \
                  avant qu'une session Windows ne soit ouverte. \
                  L'installation et le retrait demandent les droits \
                  administrateur."
)]
struct Cli {
    #[command(subcommand)]
    commande: Option<Commande>,
}

#[derive(Subcommand)]
enum Commande {
    /// Inscrit le service auprès de Windows, démarrage automatique
    Install,
    /// Retire le service
    Uninstall,
    /// Démarre le service
    Start,
    /// Arrête le service
    Stop,
    /// Affiche l'état du service
    Status,
}

fn main() -> ExitCode {
    // Windows lance le service avec un argument réservé, que clap n'a
    // pas à connaître : c'est un signal, pas une commande.
    #[cfg(windows)]
    if std::env::args().any(|a| a == service::ARGUMENT_SERVICE) {
        return match service::ceder_a_windows() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => echec("le service n'a pas pu démarrer", e),
        };
    }

    match Cli::parse().commande {
        Some(commande) => executer(commande),
        None => {
            eprintln!("Ce programme est le service ZyrDesk.");
            eprintln!("Lancez « zyrdeskd --help » pour voir ce qu'il sait faire.");
            ExitCode::FAILURE
        }
    }
}

#[cfg(windows)]
fn executer(commande: Commande) -> ExitCode {
    match commande {
        Commande::Install => match service::installer() {
            Ok(()) => {
                println!("Service installé. Il démarrera avec Windows.");
                println!("  Pour le lancer tout de suite : zyrdeskd start");
                ExitCode::SUCCESS
            }
            Err(e) => echec("installation du service", e),
        },
        Commande::Uninstall => match service::desinstaller() {
            Ok(()) => {
                println!("Service retiré.");
                ExitCode::SUCCESS
            }
            Err(e) => echec("retrait du service", e),
        },
        Commande::Start => match service::demarrer() {
            Ok(()) => {
                println!("Service démarré.");
                ExitCode::SUCCESS
            }
            Err(e) => echec("démarrage du service", e),
        },
        Commande::Stop => match service::arreter() {
            Ok(()) => {
                println!("Service arrêté.");
                ExitCode::SUCCESS
            }
            Err(e) => echec("arrêt du service", e),
        },
        Commande::Status => match service::etat() {
            Ok(etat) => {
                println!("{}", lisible(etat));
                println!("  Journal : {}", journal_du_service().display());
                ExitCode::SUCCESS
            }
            Err(e) => echec("état du service", e),
        },
    }
}

#[cfg(windows)]
fn lisible(etat: windows_service::service::ServiceState) -> &'static str {
    use windows_service::service::ServiceState::*;
    match etat {
        Stopped => "Arrêté",
        StartPending => "En cours de démarrage",
        StopPending => "En cours d'arrêt",
        Running => "En marche",
        ContinuePending => "Reprise en cours",
        PausePending => "Mise en pause",
        Paused => "En pause",
    }
}

#[cfg(windows)]
fn journal_du_service() -> std::path::PathBuf {
    zyr_proto::paths::logs_dir().join("service.log")
}

/// Hors de Windows, le service n'a pas d'objet : il n'existe pas de
/// gestionnaire de services à qui parler, ni de session console à servir.
#[cfg(not(windows))]
fn executer(_commande: Commande) -> ExitCode {
    echec(
        "service indisponible",
        "le service ZyrDesk n'existe que sous Windows",
    )
}

fn echec(contexte: &str, erreur: impl std::fmt::Display) -> ExitCode {
    eprintln!("Échec : {contexte}");
    eprintln!("  {erreur}");
    ExitCode::FAILURE
}
