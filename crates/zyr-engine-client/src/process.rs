//! Lancement du moteur client.

use std::fmt;
use std::fs;
use std::io;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use zyr_proto::session::SessionSettings;

use crate::command;
use crate::state::DeviceState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IssueSession {
    /// Le moteur s'est arrêté normalement.
    Terminee,
    /// Le moteur s'est arrêté sur une erreur.
    ///
    /// Les causes ne sont pas distinguables tant que le moteur ne rend
    /// pas de codes de sortie différenciés : c'est l'objet du patch
    /// P-M5, sans lequel la reprise automatique ne peut pas décider
    /// seule s'il faut relancer.
    Echec { code: Option<i32> },
}

#[derive(Debug)]
pub enum ErreurMoteur {
    ExecutableIntrouvable(PathBuf),
    Io(io::Error),
    AppairageEchoue { code: Option<i32>, sortie: String },
}

impl fmt::Display for ErreurMoteur {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErreurMoteur::ExecutableIntrouvable(p) => {
                write!(f, "moteur client introuvable : {}", p.display())
            }
            ErreurMoteur::Io(e) => write!(f, "erreur système : {e}"),
            ErreurMoteur::AppairageEchoue { code, sortie } => {
                let code = code.map(|c| c.to_string()).unwrap_or("interrompu".into());
                write!(f, "appairage échoué ({code}) : {sortie}")
            }
        }
    }
}

impl std::error::Error for ErreurMoteur {}

impl From<io::Error> for ErreurMoteur {
    fn from(e: io::Error) -> Self {
        ErreurMoteur::Io(e)
    }
}

pub struct ClientEngine {
    exe: PathBuf,
    etat: DeviceState,
    journal: Option<PathBuf>,
}

impl ClientEngine {
    pub fn nouveau(exe: impl Into<PathBuf>, etat: DeviceState) -> Self {
        Self {
            exe: exe.into(),
            etat,
            journal: None,
        }
    }

    /// Recueille tout ce que le moteur écrit.
    ///
    /// Sans cela, ses messages d'erreur ne vivent que dans ses propres
    /// fenêtres : une session qui échoue ne laisse aucune trace
    /// exploitable.
    pub fn avec_journal(mut self, journal: impl Into<PathBuf>) -> Self {
        self.journal = Some(journal.into());
        self
    }

    /// Ouvre le journal en ajout, en créant l'arborescence au besoin.
    fn ouvrir_journal(&self) -> io::Result<Option<fs::File>> {
        let Some(chemin) = &self.journal else {
            return Ok(None);
        };
        if let Some(parent) = chemin.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(chemin)
            .map(Some)
    }

    pub fn etat(&self) -> &DeviceState {
        &self.etat
    }

    fn commande(&self, arguments: &[String]) -> Result<Command, ErreurMoteur> {
        if !self.exe.is_file() {
            return Err(ErreurMoteur::ExecutableIntrouvable(self.exe.clone()));
        }
        let mut commande = Command::new(&self.exe);
        // Le répertoire de travail décide de l'emplacement de l'état.
        commande.current_dir(self.etat.dossier()).args(arguments);
        Ok(commande)
    }

    /// Appaire avec un hôte, sans interaction.
    pub fn appairer(&self, hote: &str, pin: &str) -> Result<(), ErreurMoteur> {
        self.etat.preparer()?;
        let sortie = self
            .commande(&command::arguments_appairage(hote, pin))?
            .stdin(Stdio::null())
            .output()?;

        // La sortie est consignée quelle que soit l'issue : le moteur
        // signale un succès même quand l'appairage n'a pas abouti.
        if let Some(mut journal) = self.ouvrir_journal()? {
            let _ = writeln!(journal, "--- appairage avec {hote} ---");
            let _ = journal.write_all(&sortie.stdout);
            let _ = journal.write_all(&sortie.stderr);
        }

        if sortie.status.success() {
            return Ok(());
        }
        let mut texte = String::from_utf8_lossy(&sortie.stderr).trim().to_string();
        if texte.is_empty() {
            texte = String::from_utf8_lossy(&sortie.stdout).trim().to_string();
        }
        texte.truncate(500);
        Err(ErreurMoteur::AppairageEchoue {
            code: sortie.status.code(),
            sortie: texte,
        })
    }

    /// Démarre une session et attend qu'elle se termine.
    pub fn lancer_session(
        &self,
        hote: &str,
        reglages: &SessionSettings,
    ) -> Result<IssueSession, ErreurMoteur> {
        self.etat.preparer()?;
        let mut commande = self.commande(&command::arguments_session(hote, reglages))?;
        commande.stdin(Stdio::null());
        if let Some(mut journal) = self.ouvrir_journal()? {
            let _ = writeln!(journal, "--- session vers {hote} ---");
            let erreurs = journal.try_clone()?;
            commande
                .stdout(Stdio::from(journal))
                .stderr(Stdio::from(erreurs));
        }
        let statut = commande.spawn()?.wait()?;
        Ok(if statut.success() {
            IssueSession::Terminee
        } else {
            IssueSession::Echec {
                code: statut.code(),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn moteur_absent() -> ClientEngine {
        let dossier = std::env::temp_dir().join(format!(
            "zyrdesk-client-{}",
            zyr_proto::alea::chaine_alphanumerique(12)
        ));
        ClientEngine::nouveau("/introuvable/zyrdesk-session", DeviceState::dans(dossier))
    }

    #[test]
    fn un_moteur_absent_est_signale_avant_toute_tentative() {
        let moteur = moteur_absent();
        assert!(matches!(
            moteur.appairer("127.0.0.1", "1234"),
            Err(ErreurMoteur::ExecutableIntrouvable(_))
        ));
        assert!(matches!(
            moteur.lancer_session("127.0.0.1", &SessionSettings::default()),
            Err(ErreurMoteur::ExecutableIntrouvable(_))
        ));
        let _ = moteur.etat().oublier();
    }

    #[test]
    fn la_preparation_de_l_etat_precede_le_lancement() {
        let moteur = moteur_absent();
        assert!(!moteur.etat().est_prepare());
        let _ = moteur.appairer("127.0.0.1", "1234");
        assert!(moteur.etat().est_prepare());
        moteur.etat().oublier().unwrap();
    }
}
