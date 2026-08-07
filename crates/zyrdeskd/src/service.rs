//! Le service Windows : installation, cycle de vie, arrêt propre.
//!
//! Windows démarre ce programme, lui parle par le gestionnaire de
//! services, et attend qu'il réponde. Répondre est une obligation : un
//! service qui met trop longtemps à confirmer un arrêt est tué, et son
//! moteur avec, sans que rien ne soit rangé.
//!
//! Tout ce qui touche au gestionnaire de services est ici. Le travail
//! réel est dans le superviseur, qui ne sait pas qu'il est un service.

use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

use windows_service::service::{
    ServiceAccess, ServiceControl, ServiceControlAccept, ServiceErrorControl, ServiceExitCode,
    ServiceInfo, ServiceStartType, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
use windows_service::{Result as ResultatService, define_windows_service, service_dispatcher};

use zyr_proto::paths;

use crate::journal::Journal;
use crate::superviseur::{self, Consigne, Fin};

/// Nom interne du service, celui qu'emploie Windows.
pub const NOM: &str = "ZyrDesk";

/// Nom affiché dans la console des services.
const NOM_AFFICHE: &str = "ZyrDesk";

const DESCRIPTION: &str =
    "Rend cet ordinateur accessible à distance, y compris avant l'ouverture de session.";

/// Argument par lequel Windows lance ce programme en tant que service.
///
/// Sans lui, le même exécutable sert d'outil d'installation en ligne de
/// commande. Le distinguer explicitement évite de deviner d'où vient le
/// lancement.
pub const ARGUMENT_SERVICE: &str = "--execute-comme-service";

/// Un service ne tourne qu'en un seul exemplaire : ce qu'il partage avec
/// son gestionnaire est donc légitimement global.
static CONSIGNE: std::sync::OnceLock<Consigne> = std::sync::OnceLock::new();

/// Rend la main à Windows, qui appellera le point d'entrée du service.
pub fn ceder_a_windows() -> ResultatService<()> {
    service_dispatcher::start(NOM, ffi_service_principal)
}

define_windows_service!(ffi_service_principal, service_principal);

fn service_principal(_arguments: Vec<OsString>) {
    let journal = match Journal::ouvrir(&chemin_journal()) {
        Ok(j) => j,
        // Sans journal, il ne reste rien à dire à personne : mieux vaut
        // ne pas démarrer qu'un service muet et invisible.
        Err(_) => return,
    };

    if let Err(e) = tenir_le_service(&journal) {
        journal.ecrire(&format!("le service s'est arrêté sur une erreur : {e}"));
    }
}

fn tenir_le_service(journal: &Journal) -> ResultatService<()> {
    let consigne = CONSIGNE.get_or_init(Consigne::nouvelle).clone();

    let a_la_demande = {
        let consigne = consigne.clone();
        move |controle| match controle {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                consigne.demander_l_arret();
                ServiceControlHandlerResult::NoError
            }
            // Windows demande parfois l'état courant : y répondre est ce
            // qui distingue un service vivant d'un service bloqué.
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };

    let etat = service_control_handler::register(NOM, a_la_demande)?;
    etat.set_service_status(annonce(ServiceState::Running, ServiceExitCode::Win32(0)))?;
    journal.ecrire("service démarré");

    let fin = superviseur::tourner(&consigne, journal);
    journal.ecrire(&format!("service arrêté : {}", motif(fin)));

    // Un service qui renonce doit le dire à Windows autrement qu'en
    // partant sans bruit, sans quoi la console des services le montre
    // arrêté sans raison.
    let sortie = match fin {
        Fin::Demandee | Fin::ExtinctionDeWindows => ServiceExitCode::Win32(0),
        Fin::MoteurIntenable | Fin::RienALancer => ServiceExitCode::ServiceSpecific(1),
    };
    etat.set_service_status(annonce(ServiceState::Stopped, sortie))?;
    Ok(())
}

fn motif(fin: Fin) -> &'static str {
    match fin {
        Fin::Demandee => "arrêt demandé",
        Fin::ExtinctionDeWindows => "extinction de Windows",
        Fin::MoteurIntenable => "le moteur hôte ne tient pas debout",
        Fin::RienALancer => "aucun moteur hôte à lancer",
    }
}

fn annonce(etat: ServiceState, sortie: ServiceExitCode) -> ServiceStatus {
    ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: etat,
        controls_accepted: ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
        exit_code: sortie,
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    }
}

fn chemin_journal() -> PathBuf {
    paths::logs_dir().join("service.log")
}

/// Inscrit le service auprès de Windows, démarrage automatique.
pub fn installer() -> Result<(), Box<dyn std::error::Error>> {
    let gestionnaire = ServiceManager::local_computer(
        None::<&str>,
        ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
    )?;

    let description = ServiceInfo {
        name: OsString::from(NOM),
        display_name: OsString::from(NOM_AFFICHE),
        service_type: ServiceType::OWN_PROCESS,
        // Le PC doit être joignable dès l'allumage, sans que personne
        // n'ouvre de session : c'est tout l'objet du service.
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: std::env::current_exe()?,
        launch_arguments: vec![OsString::from(ARGUMENT_SERVICE)],
        dependencies: vec![],
        // Compte par défaut : LocalSystem, seul à pouvoir atteindre le
        // bureau sécurisé et à survivre aux changements de session.
        account_name: None,
        account_password: None,
    };

    let service = gestionnaire.create_service(&description, ServiceAccess::CHANGE_CONFIG)?;
    service.set_description(DESCRIPTION)?;
    Ok(())
}

/// Retire le service. Il disparaît une fois arrêté.
pub fn desinstaller() -> ResultatService<()> {
    let gestionnaire = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;
    let service = gestionnaire.open_service(NOM, ServiceAccess::STOP | ServiceAccess::DELETE)?;
    // Un service en marche ne se retire qu'après son arrêt ; l'échec ici
    // veut dire qu'il était déjà arrêté, ce qui convient.
    let _ = service.stop();
    service.delete()?;
    Ok(())
}

pub fn demarrer() -> ResultatService<()> {
    let gestionnaire = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;
    let service = gestionnaire.open_service(NOM, ServiceAccess::START)?;
    service.start::<&str>(&[])
}

pub fn arreter() -> ResultatService<()> {
    let gestionnaire = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;
    let service = gestionnaire.open_service(NOM, ServiceAccess::STOP)?;
    service.stop()?;
    Ok(())
}

/// État du service tel que Windows le rapporte.
pub fn etat() -> ResultatService<ServiceState> {
    let gestionnaire = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;
    let service = gestionnaire.open_service(NOM, ServiceAccess::QUERY_STATUS)?;
    Ok(service.query_status()?.current_state)
}
