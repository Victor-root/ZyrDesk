//! Cycle de vie du processus moteur hôte.
//!
//! Le moteur est lancé tel quel, sans modification de son code : tout se
//! joue dans le fichier de configuration produit et les arguments passés.
//!
//! Au jalon M1 le superviseur tourne au premier plan, dans la console de
//! l'utilisateur : une interruption clavier atteint donc aussi le moteur,
//! qui s'arrête proprement par son propre mécanisme. Le service du jalon
//! M3 remplacera cela par un arrêt commandé explicitement.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use crate::config::SunshineConfig;
use crate::credentials::Credentials;

#[derive(Debug)]
pub enum ErreurMoteur {
    ExecutableIntrouvable(PathBuf),
    Io(io::Error),
    ProvisionEchouee { code: Option<i32>, sortie: String },
    DejaDemarre,
}

impl fmt::Display for ErreurMoteur {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErreurMoteur::ExecutableIntrouvable(p) => {
                write!(f, "moteur hôte introuvable : {}", p.display())
            }
            ErreurMoteur::Io(e) => write!(f, "erreur système : {e}"),
            ErreurMoteur::ProvisionEchouee { code, sortie } => {
                let code = code.map(|c| c.to_string()).unwrap_or("interrompu".into());
                write!(
                    f,
                    "provisionnement des identifiants échoué ({code}) : {sortie}"
                )
            }
            ErreurMoteur::DejaDemarre => write!(f, "le moteur est déjà démarré"),
        }
    }
}

impl std::error::Error for ErreurMoteur {}

impl From<io::Error> for ErreurMoteur {
    fn from(e: io::Error) -> Self {
        ErreurMoteur::Io(e)
    }
}

/// Arguments de lancement du moteur.
pub fn arguments_demarrage(config: &SunshineConfig) -> Vec<String> {
    vec![chemin_en_argument(&config.chemin_conf())]
}

/// Arguments d'écriture des identifiants de l'API locale.
///
/// Le moteur écrit le fichier d'identifiants désigné par la
/// configuration, puis se termine immédiatement.
pub fn arguments_provision(config: &SunshineConfig, creds: &Credentials) -> Vec<String> {
    vec![
        chemin_en_argument(&config.chemin_conf()),
        "--creds".to_string(),
        creds.utilisateur.clone(),
        creds.mot_de_passe.clone(),
    ]
}

fn chemin_en_argument(chemin: &Path) -> String {
    chemin.to_string_lossy().into_owned()
}

pub struct HostEngine {
    exe: PathBuf,
    config: SunshineConfig,
    creds: Credentials,
    journal: PathBuf,
    processus: Option<Child>,
}

impl HostEngine {
    pub fn nouveau(
        exe: impl Into<PathBuf>,
        config: SunshineConfig,
        creds: Credentials,
        journal: impl Into<PathBuf>,
    ) -> Self {
        Self {
            exe: exe.into(),
            config,
            creds,
            journal: journal.into(),
            processus: None,
        }
    }

    pub fn config(&self) -> &SunshineConfig {
        &self.config
    }

    pub fn credentials(&self) -> &Credentials {
        &self.creds
    }

    /// Écrit la configuration et prépare l'arborescence attendue.
    pub fn preparer(&self) -> Result<(), ErreurMoteur> {
        if !self.exe.is_file() {
            return Err(ErreurMoteur::ExecutableIntrouvable(self.exe.clone()));
        }
        for dossier in [
            self.chemin_parent(&self.config.chemin_conf()),
            self.chemin_parent(&self.journal),
        ] {
            fs::create_dir_all(&dossier)?;
        }
        fs::write(self.config.chemin_conf(), self.config.rendu_conf())?;
        fs::write(self.config.chemin_apps(), self.config.rendu_apps())?;
        Ok(())
    }

    fn chemin_parent(&self, chemin: &Path) -> PathBuf {
        chemin.parent().map(PathBuf::from).unwrap_or_default()
    }

    /// Écrit les identifiants de l'API locale dans l'état du moteur.
    pub fn provisionner_identifiants(&self) -> Result<(), ErreurMoteur> {
        let sortie = Command::new(&self.exe)
            .args(arguments_provision(&self.config, &self.creds))
            .stdin(Stdio::null())
            .output()?;
        if sortie.status.success() {
            return Ok(());
        }
        // La sortie peut contenir les identifiants : seule la sortie
        // d'erreur est remontée, et tronquée.
        let mut texte = String::from_utf8_lossy(&sortie.stderr).trim().to_string();
        texte.truncate(500);
        Err(ErreurMoteur::ProvisionEchouee {
            code: sortie.status.code(),
            sortie: texte,
        })
    }

    /// Lance le moteur, sa sortie étant redirigée vers notre journal.
    pub fn demarrer(&mut self) -> Result<(), ErreurMoteur> {
        if self.processus.is_some() {
            return Err(ErreurMoteur::DejaDemarre);
        }
        let journal = fs::File::create(&self.journal)?;
        let journal_err = journal.try_clone()?;
        let enfant = Command::new(&self.exe)
            .args(arguments_demarrage(&self.config))
            .stdin(Stdio::null())
            .stdout(Stdio::from(journal))
            .stderr(Stdio::from(journal_err))
            .spawn()?;
        self.processus = Some(enfant);
        Ok(())
    }

    /// Code de sortie si le moteur s'est arrêté de lui-même.
    pub fn arret_constate(&mut self) -> Result<Option<Option<i32>>, ErreurMoteur> {
        match self.processus.as_mut() {
            Some(enfant) => Ok(enfant.try_wait()?.map(|statut| statut.code())),
            None => Ok(None),
        }
    }

    pub fn arreter(&mut self) -> Result<(), ErreurMoteur> {
        if let Some(mut enfant) = self.processus.take() {
            enfant.kill()?;
            enfant.wait()?;
        }
        Ok(())
    }
}

impl Drop for HostEngine {
    fn drop(&mut self) {
        let _ = self.arreter();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zyr_proto::net::EnginePorts;

    fn config() -> SunshineConfig {
        SunshineConfig::new(EnginePorts::new(42100).unwrap(), "/data/host")
    }

    #[test]
    fn le_demarrage_ne_passe_que_le_fichier_de_configuration() {
        let args = arguments_demarrage(&config());
        assert_eq!(args.len(), 1);
        assert!(args[0].ends_with("engine.conf"));
    }

    #[test]
    fn la_provision_passe_la_configuration_puis_les_identifiants() {
        let creds = Credentials {
            utilisateur: "u".to_string(),
            mot_de_passe: "p".to_string(),
        };
        let args = arguments_provision(&config(), &creds);
        assert!(args[0].ends_with("engine.conf"));
        assert_eq!(&args[1..], ["--creds", "u", "p"]);
    }

    #[test]
    fn preparer_refuse_un_executable_absent() {
        let creds = Credentials::aleatoires();
        let moteur = HostEngine::nouveau(
            "/introuvable/zyrdesk-host-engine",
            config(),
            creds,
            "/data/logs/host.log",
        );
        assert!(matches!(
            moteur.preparer(),
            Err(ErreurMoteur::ExecutableIntrouvable(_))
        ));
    }

    #[test]
    fn preparer_ecrit_la_configuration_et_la_liste_d_applications() {
        let base = std::env::temp_dir().join(format!(
            "zyrdesk-test-{}",
            zyr_proto::alea::chaine_alphanumerique(12)
        ));
        let faux_exe = base.join("moteur");
        fs::create_dir_all(&base).unwrap();
        fs::write(&faux_exe, b"").unwrap();

        let config = SunshineConfig::new(EnginePorts::new(42100).unwrap(), base.join("host"));
        let moteur = HostEngine::nouveau(
            &faux_exe,
            config.clone(),
            Credentials::aleatoires(),
            base.join("logs/host.log"),
        );
        moteur.preparer().unwrap();

        let conf = fs::read_to_string(config.chemin_conf()).unwrap();
        assert!(conf.contains("bind_address = 127.0.0.1"));
        let apps = fs::read_to_string(config.chemin_apps()).unwrap();
        assert!(apps.contains("Desktop"));
        assert!(base.join("logs").is_dir());

        fs::remove_dir_all(&base).unwrap();
    }
}
