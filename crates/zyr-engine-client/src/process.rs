//! Lancement du moteur client.

use std::fmt;
use std::io;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use zyr_proto::session::SessionSettings;

use crate::command;
use crate::state::DeviceState;

/// Variable d'environnement Qt sélectionnant la couche d'affichage.
///
/// Passée par l'environnement plutôt que par la ligne de commande : le
/// moteur analyse ses propres arguments et refuserait une option qu'il
/// ne connaît pas.
const VAR_PLATEFORME_QT: &str = "QT_QPA_PLATFORM";
const PLATEFORME_SANS_AFFICHAGE: &str = "offscreen";

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
    masquer_fenetre_attente: bool,
}

impl ClientEngine {
    pub fn nouveau(exe: impl Into<PathBuf>, etat: DeviceState) -> Self {
        Self {
            exe: exe.into(),
            etat,
            masquer_fenetre_attente: false,
        }
    }

    /// Tente de supprimer la fenêtre d'attente affichée par le moteur
    /// avant l'ouverture de la fenêtre vidéo.
    ///
    /// La fenêtre d'attente relève de la couche graphique du moteur,
    /// alors que la fenêtre vidéo n'en dépend pas : neutraliser la
    /// première devrait donc laisser la seconde intacte. Reste à le
    /// confirmer sur une machine réelle, faute de quoi le patch P-M1
    /// devient nécessaire.
    pub fn masquer_fenetre_attente(mut self, actif: bool) -> Self {
        self.masquer_fenetre_attente = actif;
        self
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
        if self.masquer_fenetre_attente {
            commande.env(VAR_PLATEFORME_QT, PLATEFORME_SANS_AFFICHAGE);
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
