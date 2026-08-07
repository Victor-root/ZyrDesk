//! Makes this computer reachable from elsewhere.
//!
//! In the foreground and without a service: this is the smallest form
//! that allows real performance to be measured. Starting with Windows
//! and access before anyone logs in belong to the service.

use std::process::ExitCode;
use std::time::Duration;

use clap::Subcommand;
use zyr_engine_host::api::EngineApi;
use zyr_engine_host::{Credentials, EngineRuntime, HostEngine, Listening, SunshineConfig, ports};
use zyr_proto::paths;

use crate::failure;

/// Margin given to the engine to open its ports at start-up.
const START_DELAY: Duration = Duration::from_secs(20);
/// How often the engine is checked on once started.
const WATCH_PERIOD: Duration = Duration::from_secs(1);

#[derive(Subcommand)]
pub enum Action {
    /// Starts the host engine and keeps it running
    Start,
    /// Accepts a pairing code shown by a remote computer
    Pin {
        /// Four-digit code shown on the computer that is connecting
        code: String,
        /// Name given to the remote computer
        #[arg(long, default_value = "Ordinateur ZyrDesk")]
        name: String,
    },
}

pub fn run(action: Action) -> ExitCode {
    match action {
        Action::Start => start(),
        Action::Pin { code, name } => pin(&code, &name),
    }
}

fn start() -> ExitCode {
    let exe = paths::host_engine_exe();
    if !exe.is_file() {
        return failure(
            "moteur hôte introuvable",
            format!(
                "{}\n  Lancez « zyr-cli engines status » pour la marche à suivre.",
                exe.display()
            ),
        );
    }

    let Some(ports) = ports::free_base() else {
        return failure(
            "aucun port disponible",
            "toute la plage réservée aux moteurs est occupée",
        );
    };

    let runtime_path = EngineRuntime::standard_path();
    // Without a tunnel, the engine has to be reachable from the local
    // network, or no other computer can get to it at all. The tunnel
    // lets it close back onto the local machine.
    let config = SunshineConfig::new(ports, paths::host_state_dir(), paths::logs_dir())
        .with_listening(Listening::Network);
    let credentials = Credentials::random();
    let mut engine = HostEngine::new(
        &exe,
        config,
        credentials.clone(),
        paths::logs_dir().join("engine-console.log"),
    );

    println!("Démarrage de l'accès distant...");
    if let Err(e) = engine.prepare() {
        return failure("préparation du moteur", e);
    }
    if let Err(e) = engine.provision_credentials() {
        return failure("provisionnement des identifiants du moteur", e);
    }
    if let Err(e) = engine.start() {
        return failure("démarrage du moteur", e);
    }

    let api = EngineApi::new(ports, credentials.clone());
    if let Err(e) = api.wait_until_ready(START_DELAY) {
        let _ = engine.stop();
        return failure(
            "le moteur n'a pas fini de démarrer",
            format!(
                "{e}\n  Journal : {}",
                paths::logs_dir().join("engine-console.log").display()
            ),
        );
    }

    let runtime = EngineRuntime { ports, credentials };
    if let Err(e) = runtime.write(&runtime_path) {
        let _ = engine.stop();
        return failure("enregistrement de l'état du moteur", e);
    }

    println!("\nAccès distant actif.");
    println!("  Cet ordinateur est joignable sur le réseau local.");
    println!("  Si un autre ordinateur n'arrive pas à se connecter, autorisez le");
    println!("  moteur dans le pare-feu Windows (voir docs/testing/M1-PROTOCOLE.md).");
    println!("  Pour autoriser un ordinateur qui se connecte pour la première fois,");
    println!("  lancez ici : zyr-cli host pin <code affiché sur l'autre ordinateur>");
    println!("\nCtrl+C pour arrêter.\n");

    let exit_code = watch(&mut engine);
    let _ = EngineRuntime::remove(&runtime_path);
    exit_code
}

/// Blocks while the engine runs, and reports an unexpected stop.
fn watch(engine: &mut HostEngine) -> ExitCode {
    loop {
        match engine.exit_seen() {
            Ok(None) => std::thread::sleep(WATCH_PERIOD),
            Ok(Some(code)) => {
                let code = code
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "interrompu".to_string());
                return failure(
                    "le moteur hôte s'est arrêté",
                    format!(
                        "code {code}\n  Journal : {}",
                        paths::logs_dir().join("engine-console.log").display()
                    ),
                );
            }
            Err(e) => return failure("surveillance du moteur", e),
        }
    }
}

fn pin(code: &str, name: &str) -> ExitCode {
    if code.len() != 4 || !code.chars().all(|c| c.is_ascii_digit()) {
        return failure(
            "code d'appairage invalide",
            format!("« {code} » : quatre chiffres attendus"),
        );
    }

    let path = EngineRuntime::standard_path();
    let runtime = match EngineRuntime::read(&path) {
        Ok(runtime) => runtime,
        Err(e) => {
            return failure(
                "aucun accès distant actif",
                format!("{e}\n  Lancez « zyr-cli host start » dans une autre fenêtre."),
            );
        }
    };

    let api = EngineApi::new(runtime.ports, runtime.credentials);
    match api.submit_pin(code, name) {
        Ok(()) => {
            println!("Ordinateur autorisé : {name}");
            ExitCode::SUCCESS
        }
        Err(e) => failure("appairage refusé", e),
    }
}
