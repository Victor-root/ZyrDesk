//! Maintient le moteur hôte en marche tant que le service tourne.
//!
//! Le superviseur enchaîne trois choses : préparer le moteur, le lancer,
//! et décider quoi faire quand il s'arrête. La décision elle-même est
//! prise par la politique du module voisin ; ici on l'applique, on
//! écrit ce qui se passe, et on rend la main dès qu'on demande l'arrêt.

// Hors de Windows, rien n'appelle ce module : le service n'y existe
// pas. Il reste compilé et testé partout, la logique n'ayant rien de
// propre à un système, mais sans appelant il passerait pour du code
// mort. L'exception s'arrête aux plateformes sans service : sous
// Windows, un vrai code mort reste signalé.
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use zyr_engine_host::api::EngineApi;
use zyr_engine_host::{Credentials, Ecoute, EngineRuntime, HostEngine, SunshineConfig, ports};
use zyr_proto::paths;

use crate::journal::Journal;
use crate::surveillance::{Politique, Suite};

/// Marge laissée au moteur pour ouvrir ses ports au démarrage.
const DELAI_DEMARRAGE: Duration = Duration::from_secs(30);

/// Période à laquelle le superviseur reprend la main pour vérifier l'état
/// du moteur et la consigne d'arrêt.
const PERIODE_SURVEILLANCE: Duration = Duration::from_millis(500);

/// Consigne d'arrêt, partagée avec ce qui commande le service.
#[derive(Debug, Clone, Default)]
pub struct Consigne(Arc<AtomicBool>);

impl Consigne {
    pub fn nouvelle() -> Self {
        Self::default()
    }

    /// Demande l'arrêt. Le superviseur rend la main à la prochaine
    /// vérification, après avoir arrêté le moteur.
    pub fn demander_l_arret(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn arret_demande(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// Raison pour laquelle le superviseur a rendu la main.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fin {
    /// L'arrêt a été demandé.
    Demandee,
    /// Windows s'éteint.
    ExtinctionDeWindows,
    /// Le moteur ne tient pas debout, même après plusieurs relances.
    MoteurIntenable,
    /// Rien n'a pu être lancé.
    RienALancer,
}

/// Tourne jusqu'à ce qu'on demande l'arrêt, ou que le moteur renonce.
pub fn tourner(consigne: &Consigne, journal: &Journal) -> Fin {
    let exe = paths::host_engine_exe();
    if !exe.is_file() {
        journal.ecrire(&format!("moteur hôte introuvable : {}", exe.display()));
        return Fin::RienALancer;
    }

    let mut politique = Politique::nouvelle();
    let chemin_runtime = EngineRuntime::chemin_standard();

    loop {
        if consigne.arret_demande() {
            return Fin::Demandee;
        }

        let debut = Instant::now();
        let arret = match une_vie_du_moteur(&exe, &chemin_runtime, consigne, journal) {
            Ok(code) => code,
            Err(raison) => {
                journal.ecrire(&raison);
                // Un moteur qui ne démarre pas est un échec comme un
                // autre : la politique décidera s'il vaut la peine
                // d'insister.
                None
            }
        };

        if consigne.arret_demande() {
            return Fin::Demandee;
        }

        let vie = debut.elapsed();
        match politique.apres_arret(arret, vie) {
            Suite::Terminer => {
                journal.ecrire("Windows s'éteint, le moteur n'est pas relancé");
                return Fin::ExtinctionDeWindows;
            }
            Suite::Renoncer => {
                journal.ecrire(&format!(
                    "le moteur est retombé {} fois de suite sans tenir, abandon",
                    politique.echecs()
                ));
                return Fin::MoteurIntenable;
            }
            Suite::Relancer(delai) => {
                let code = arret
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "interrompu".to_string());
                journal.ecrire(&format!(
                    "moteur arrêté (code {code}) après {} s, relance dans {} s",
                    vie.as_secs(),
                    delai.as_secs()
                ));
                if !attendre(delai, consigne) {
                    return Fin::Demandee;
                }
            }
        }
    }
}

/// Lance le moteur et le suit jusqu'à son arrêt.
///
/// Rend le code de sortie, ou le motif pour lequel il n'a pas pu vivre.
fn une_vie_du_moteur(
    exe: &std::path::Path,
    chemin_runtime: &std::path::Path,
    consigne: &Consigne,
    journal: &Journal,
) -> Result<Option<i32>, String> {
    let Some(ports) = ports::base_libre() else {
        return Err("aucun port disponible dans la plage réservée aux moteurs".to_string());
    };

    // Le tunnel n'existe pas encore côté service : le moteur reste
    // joignable depuis le réseau local, comme au jalon M1. Il passera en
    // loopback strict quand le service portera l'extrémité de tunnel.
    let config = SunshineConfig::new(ports, paths::host_state_dir(), paths::logs_dir())
        .avec_ecoute(Ecoute::Reseau);
    let creds = Credentials::aleatoires();
    let mut moteur = HostEngine::nouveau(
        exe,
        config,
        creds.clone(),
        paths::logs_dir().join("engine-console.log"),
    );

    moteur.preparer().map_err(|e| e.to_string())?;
    moteur
        .provisionner_identifiants()
        .map_err(|e| e.to_string())?;
    moteur.demarrer().map_err(|e| e.to_string())?;
    journal.ecrire(&format!(
        "moteur démarré sur le port de base {}",
        ports.base()
    ));

    let api = EngineApi::nouvelle(ports, creds.clone());
    if let Err(e) = api.attendre_disponible(DELAI_DEMARRAGE) {
        let _ = moteur.arreter();
        return Err(format!("le moteur n'a pas fini de démarrer : {e}"));
    }

    let runtime = EngineRuntime {
        ports,
        credentials: creds,
    };
    if let Err(e) = runtime.ecrire(chemin_runtime) {
        let _ = moteur.arreter();
        return Err(format!("état du moteur non enregistré : {e}"));
    }
    journal.ecrire("accès distant actif");

    let code = attendre_l_arret_du_moteur(&mut moteur, consigne, journal);
    let _ = EngineRuntime::supprimer(chemin_runtime);
    Ok(code)
}

/// Attend que le moteur s'arrête, ou l'arrête si on le demande.
fn attendre_l_arret_du_moteur(
    moteur: &mut HostEngine,
    consigne: &Consigne,
    journal: &Journal,
) -> Option<i32> {
    loop {
        if consigne.arret_demande() {
            journal.ecrire("arrêt demandé, le moteur est arrêté");
            let _ = moteur.arreter();
            return None;
        }
        match moteur.arret_constate() {
            Ok(Some(code)) => return code,
            Ok(None) => std::thread::sleep(PERIODE_SURVEILLANCE),
            Err(e) => {
                journal.ecrire(&format!("surveillance du moteur impossible : {e}"));
                return None;
            }
        }
    }
}

/// Attend le délai demandé, en restant à l'écoute de la consigne d'arrêt.
///
/// Rend `false` si l'arrêt a été demandé pendant l'attente : un service
/// qui dort une minute avant de répondre est un service que Windows tue.
fn attendre(delai: Duration, consigne: &Consigne) -> bool {
    let echeance = Instant::now() + delai;
    while Instant::now() < echeance {
        if consigne.arret_demande() {
            return false;
        }
        std::thread::sleep(PERIODE_SURVEILLANCE.min(delai));
    }
    !consigne.arret_demande()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_consigne_se_partage_entre_deux_mains() {
        let consigne = Consigne::nouvelle();
        let copie = consigne.clone();
        assert!(!copie.arret_demande());
        consigne.demander_l_arret();
        assert!(copie.arret_demande());
    }

    #[test]
    fn l_attente_est_interrompue_par_la_consigne() {
        let consigne = Consigne::nouvelle();
        let copie = consigne.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            copie.demander_l_arret();
        });

        let debut = Instant::now();
        // Sans réactivité à la consigne, Windows tuerait le service bien
        // avant la fin de cette attente.
        assert!(!attendre(Duration::from_secs(30), &consigne));
        assert!(debut.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn une_attente_nulle_laisse_passer() {
        let consigne = Consigne::nouvelle();
        assert!(attendre(Duration::ZERO, &consigne));
    }

    #[test]
    fn sans_moteur_le_superviseur_le_dit_au_lieu_de_boucler() {
        let dossier = std::env::temp_dir().join(format!("zyrdeskd-{}-sans", std::process::id()));
        let journal = Journal::ouvrir(&dossier.join("service.log")).unwrap();
        // Le moteur n'est pas installé sur la machine de test : c'est
        // exactement le cas que le service doit signaler sans insister.
        if !paths::host_engine_exe().is_file() {
            assert_eq!(tourner(&Consigne::nouvelle(), &journal), Fin::RienALancer);
        }
        let _ = std::fs::remove_dir_all(&dossier);
    }
}
