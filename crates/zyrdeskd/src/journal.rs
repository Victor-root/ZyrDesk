//! Journal du service.
//!
//! Un service n'a pas de console. Sans trace écrite, un démarrage qui
//! échoue avant l'ouverture de session ne laisse rien à examiner : ni
//! message, ni fenêtre, ni personne pour le lire. Tout ce que fait le
//! service passe donc par ici.
//!
//! Les horodatages sont en temps universel, sans exception. Un journal
//! qui suit l'heure locale recule d'une heure une fois par an, et les
//! lignes se retrouvent dans le désordre au moment précis où l'on
//! cherche à comprendre un incident nocturne.

// Hors de Windows, rien n'appelle ce module : le service n'y existe
// pas. Il reste compilé et testé partout, la logique n'ayant rien de
// propre à un système, mais sans appelant il passerait pour du code
// mort. L'exception s'arrête aux plateformes sans service : sous
// Windows, un vrai code mort reste signalé.
#![cfg_attr(not(windows), allow(dead_code))]

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::Mutex;

use time::OffsetDateTime;
use time::format_description::BorrowedFormatItem;
use time::macros::format_description;

const HORODATAGE: &[BorrowedFormatItem<'static>] =
    format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");

/// Journal ouvert en ajout, partagé par tout le service.
#[derive(Debug)]
pub struct Journal {
    fichier: Mutex<File>,
}

impl Journal {
    /// Ouvre le journal, en créant le dossier au besoin.
    pub fn ouvrir(chemin: &Path) -> io::Result<Self> {
        if let Some(dossier) = chemin.parent() {
            std::fs::create_dir_all(dossier)?;
        }
        let fichier = OpenOptions::new().create(true).append(true).open(chemin)?;
        Ok(Self {
            fichier: Mutex::new(fichier),
        })
    }

    /// Écrit une ligne horodatée.
    ///
    /// N'échoue jamais : un journal qui refuse d'écrire ne doit pas
    /// arrêter le service qu'il observe.
    pub fn ecrire(&self, message: &str) {
        let Ok(mut fichier) = self.fichier.lock() else {
            return;
        };
        let _ = writeln!(fichier, "{} {message}", maintenant());
        let _ = fichier.flush();
    }
}

/// Horodatage universel, ou une marque explicite si l'heure est illisible.
fn maintenant() -> String {
    OffsetDateTime::now_utc()
        .format(HORODATAGE)
        .unwrap_or_else(|_| "date indisponible".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chemin_neuf(nom: &str) -> std::path::PathBuf {
        let chemin = std::env::temp_dir()
            .join(format!("zyrdeskd-{}-{nom}", std::process::id()))
            .join("service.log");
        let _ = std::fs::remove_dir_all(chemin.parent().unwrap());
        chemin
    }

    #[test]
    fn le_journal_cree_son_dossier_et_ajoute_ses_lignes() {
        let chemin = chemin_neuf("journal");
        {
            let journal = Journal::ouvrir(&chemin).unwrap();
            journal.ecrire("premier");
            journal.ecrire("second");
        }
        // Une seconde ouverture ne doit pas effacer la première : un
        // service relancé par Windows perdrait sinon la trace de ce qui
        // l'a fait tomber.
        {
            let journal = Journal::ouvrir(&chemin).unwrap();
            journal.ecrire("après relance");
        }

        let contenu = std::fs::read_to_string(&chemin).unwrap();
        let lignes: Vec<&str> = contenu.lines().collect();
        assert_eq!(lignes.len(), 3);
        assert!(lignes[0].ends_with("premier"), "{}", lignes[0]);
        assert!(lignes[2].ends_with("après relance"), "{}", lignes[2]);

        std::fs::remove_dir_all(chemin.parent().unwrap()).unwrap();
    }

    #[test]
    fn chaque_ligne_porte_une_date_lisible() {
        let horodatage = maintenant();
        assert_eq!(horodatage.len(), 19, "{horodatage}");
        assert!(horodatage.contains('-') && horodatage.contains(':'));
    }
}
