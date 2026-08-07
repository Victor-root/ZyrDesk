//! Description du moteur hôte en cours d'exécution.
//!
//! Le superviseur du jalon M1 et la commande d'appairage sont deux
//! processus distincts : le premier publie ici de quoi joindre le moteur,
//! le second le relit.
//!
//! Le fichier contient les identifiants de l'API locale : il est écrit
//! dans l'espace de l'utilisateur courant, jamais dans un dossier commun
//! à tous les comptes de la machine. Le service du jalon M3 remplacera ce
//! fichier par un état en mémoire et un stockage protégé.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use zyr_proto::net::{BasePortHorsPlage, EnginePorts};

use crate::credentials::Credentials;

const CLE_PORT: &str = "port_base";
const CLE_UTILISATEUR: &str = "utilisateur";
const CLE_MOT_DE_PASSE: &str = "mot_de_passe";

#[derive(Debug)]
pub enum ErreurRuntime {
    Absent(PathBuf),
    Illisible(io::Error),
    Malforme(String),
}

impl fmt::Display for ErreurRuntime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErreurRuntime::Absent(p) => write!(
                f,
                "aucun moteur hôte en cours d'exécution (fichier attendu : {})",
                p.display()
            ),
            ErreurRuntime::Illisible(e) => write!(f, "état du moteur illisible : {e}"),
            ErreurRuntime::Malforme(d) => write!(f, "état du moteur incohérent : {d}"),
        }
    }
}

impl std::error::Error for ErreurRuntime {}

impl From<BasePortHorsPlage> for ErreurRuntime {
    fn from(e: BasePortHorsPlage) -> Self {
        ErreurRuntime::Malforme(e.to_string())
    }
}

pub struct EngineRuntime {
    pub ports: EnginePorts,
    pub credentials: Credentials,
}

impl EngineRuntime {
    /// Emplacement standard, propre à l'utilisateur courant.
    pub fn chemin_standard() -> PathBuf {
        zyr_proto::paths::user_dir().join("host-runtime.conf")
    }

    pub fn ecrire(&self, chemin: &Path) -> io::Result<()> {
        if let Some(parent) = chemin.parent() {
            fs::create_dir_all(parent)?;
        }
        let contenu = format!(
            "{CLE_PORT}={}\n{CLE_UTILISATEUR}={}\n{CLE_MOT_DE_PASSE}={}\n",
            self.ports.base(),
            self.credentials.utilisateur,
            self.credentials.mot_de_passe
        );
        fs::write(chemin, contenu)
    }

    pub fn lire(chemin: &Path) -> Result<Self, ErreurRuntime> {
        let contenu = match fs::read_to_string(chemin) {
            Ok(c) => c,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Err(ErreurRuntime::Absent(chemin.to_path_buf()));
            }
            Err(e) => return Err(ErreurRuntime::Illisible(e)),
        };

        let champ = |cle: &str| -> Result<String, ErreurRuntime> {
            contenu
                .lines()
                .filter_map(|l| l.split_once('='))
                .find(|(k, _)| k.trim() == cle)
                .map(|(_, v)| v.trim().to_string())
                .ok_or_else(|| ErreurRuntime::Malforme(format!("champ « {cle} » absent")))
        };

        let base: u16 = champ(CLE_PORT)?
            .parse()
            .map_err(|_| ErreurRuntime::Malforme(format!("champ « {CLE_PORT} » non numérique")))?;

        Ok(Self {
            ports: EnginePorts::new(base)?,
            credentials: Credentials {
                utilisateur: champ(CLE_UTILISATEUR)?,
                mot_de_passe: champ(CLE_MOT_DE_PASSE)?,
            },
        })
    }

    pub fn supprimer(chemin: &Path) -> io::Result<()> {
        match fs::remove_file(chemin) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            autre => autre,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chemin_temporaire() -> PathBuf {
        std::env::temp_dir().join(format!(
            "zyrdesk-runtime-{}.conf",
            zyr_proto::alea::chaine_alphanumerique(12)
        ))
    }

    #[test]
    fn ecriture_puis_lecture_conservent_tout() {
        let chemin = chemin_temporaire();
        let runtime = EngineRuntime {
            ports: EnginePorts::new(42375).unwrap(),
            credentials: Credentials::aleatoires(),
        };
        runtime.ecrire(&chemin).unwrap();

        let relu = EngineRuntime::lire(&chemin).unwrap();
        assert_eq!(relu.ports, runtime.ports);
        assert_eq!(relu.credentials, runtime.credentials);

        EngineRuntime::supprimer(&chemin).unwrap();
        EngineRuntime::supprimer(&chemin).unwrap();
    }

    #[test]
    fn un_moteur_non_demarre_est_signale_clairement() {
        let chemin = chemin_temporaire();
        assert!(matches!(
            EngineRuntime::lire(&chemin),
            Err(ErreurRuntime::Absent(_))
        ));
    }

    #[test]
    fn les_fichiers_incoherents_sont_rejetes() {
        let cas = [
            "utilisateur=u\nmot_de_passe=p\n",
            "port_base=abc\nutilisateur=u\nmot_de_passe=p\n",
            "port_base=80\nutilisateur=u\nmot_de_passe=p\n",
            "port_base=42375\nmot_de_passe=p\n",
        ];
        for contenu in cas {
            let chemin = chemin_temporaire();
            fs::write(&chemin, contenu).unwrap();
            assert!(
                matches!(
                    EngineRuntime::lire(&chemin),
                    Err(ErreurRuntime::Malforme(_))
                ),
                "{contenu}"
            );
            EngineRuntime::supprimer(&chemin).unwrap();
        }
    }
}
