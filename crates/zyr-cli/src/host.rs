//! Rend cet ordinateur accessible à distance.
//!
//! Au premier plan et sans service : c'est la forme minimale permettant
//! de mesurer les performances réelles. Le démarrage automatique avec
//! Windows et l'accès avant ouverture de session arrivent au jalon M3.

use std::process::ExitCode;
use std::time::Duration;

use clap::Subcommand;
use zyr_engine_host::api::EngineApi;
use zyr_engine_host::{Credentials, Ecoute, EngineRuntime, HostEngine, SunshineConfig, ports};
use zyr_proto::paths;

use crate::echec;

/// Marge laissée au moteur pour ouvrir ses ports au démarrage.
const DELAI_DEMARRAGE: Duration = Duration::from_secs(20);
/// Période de surveillance du moteur une fois démarré.
const PERIODE_SURVEILLANCE: Duration = Duration::from_secs(1);

#[derive(Subcommand)]
pub enum Action {
    /// Démarre le moteur hôte et le garde actif
    Start,
    /// Accepte un code d'appairage présenté par un ordinateur distant
    Pin {
        /// Code à quatre chiffres affiché sur l'ordinateur qui se connecte
        code: String,
        /// Nom donné à l'ordinateur distant
        #[arg(long, default_value = "Ordinateur ZyrDesk")]
        nom: String,
    },
}

pub fn executer(action: Action) -> ExitCode {
    match action {
        Action::Start => start(),
        Action::Pin { code, nom } => pin(&code, &nom),
    }
}

fn start() -> ExitCode {
    let exe = paths::host_engine_exe();
    if !exe.is_file() {
        return echec(
            "moteur hôte introuvable",
            format!(
                "{}\n  Lancez « zyr-cli engines status » pour la marche à suivre.",
                exe.display()
            ),
        );
    }

    let Some(ports) = ports::base_libre() else {
        return echec(
            "aucun port disponible",
            "toute la plage réservée aux moteurs est occupée",
        );
    };

    let chemin_runtime = EngineRuntime::chemin_standard();
    // Sans tunnel, le moteur doit être joignable depuis le réseau local,
    // sans quoi aucun autre ordinateur ne peut l'atteindre. Le tunnel du
    // jalon M2 permettra de le refermer sur la machine locale.
    let config = SunshineConfig::new(ports, paths::host_state_dir(), paths::logs_dir())
        .avec_ecoute(Ecoute::Reseau);
    let creds = Credentials::aleatoires();
    let mut moteur = HostEngine::nouveau(
        &exe,
        config,
        creds.clone(),
        paths::logs_dir().join("engine-console.log"),
    );

    println!("Démarrage de l'accès distant...");
    if let Err(e) = moteur.preparer() {
        return echec("préparation du moteur", e);
    }
    if let Err(e) = moteur.provisionner_identifiants() {
        return echec("provisionnement des identifiants du moteur", e);
    }
    if let Err(e) = moteur.demarrer() {
        return echec("démarrage du moteur", e);
    }

    let api = EngineApi::nouvelle(ports, creds.clone());
    if let Err(e) = api.attendre_disponible(DELAI_DEMARRAGE) {
        let _ = moteur.arreter();
        return echec(
            "le moteur n'a pas fini de démarrer",
            format!(
                "{e}\n  Journal : {}",
                paths::logs_dir().join("engine-console.log").display()
            ),
        );
    }

    let runtime = EngineRuntime {
        ports,
        credentials: creds,
    };
    if let Err(e) = runtime.ecrire(&chemin_runtime) {
        let _ = moteur.arreter();
        return echec("enregistrement de l'état du moteur", e);
    }

    println!("\nAccès distant actif.");
    println!("  Cet ordinateur est joignable sur le réseau local.");
    println!("  Si un autre ordinateur n'arrive pas à se connecter, autorisez le");
    println!("  moteur dans le pare-feu Windows (voir docs/testing/M1-PROTOCOLE.md).");
    println!("  Pour autoriser un ordinateur qui se connecte pour la première fois,");
    println!("  lancez ici : zyr-cli host pin <code affiché sur l'autre ordinateur>");
    println!("\nCtrl+C pour arrêter.\n");

    let code_retour = surveiller(&mut moteur);
    let _ = EngineRuntime::supprimer(&chemin_runtime);
    code_retour
}

/// Bloque tant que le moteur tourne, et signale un arrêt inattendu.
fn surveiller(moteur: &mut HostEngine) -> ExitCode {
    loop {
        match moteur.arret_constate() {
            Ok(None) => std::thread::sleep(PERIODE_SURVEILLANCE),
            Ok(Some(code)) => {
                let code = code
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "interrompu".to_string());
                return echec(
                    "le moteur hôte s'est arrêté",
                    format!(
                        "code {code}\n  Journal : {}",
                        paths::logs_dir().join("engine-console.log").display()
                    ),
                );
            }
            Err(e) => return echec("surveillance du moteur", e),
        }
    }
}

fn pin(code: &str, nom: &str) -> ExitCode {
    if code.len() != 4 || !code.chars().all(|c| c.is_ascii_digit()) {
        return echec(
            "code d'appairage invalide",
            format!("« {code} » : quatre chiffres attendus"),
        );
    }

    let chemin = EngineRuntime::chemin_standard();
    let runtime = match EngineRuntime::lire(&chemin) {
        Ok(r) => r,
        Err(e) => {
            return echec(
                "aucun accès distant actif",
                format!("{e}\n  Lancez « zyr-cli host start » dans une autre fenêtre."),
            );
        }
    };

    let api = EngineApi::nouvelle(runtime.ports, runtime.credentials);
    match api.soumettre_pin(code, nom) {
        Ok(()) => {
            println!("Ordinateur autorisé : {nom}");
            ExitCode::SUCCESS
        }
        Err(e) => echec("appairage refusé", e),
    }
}
