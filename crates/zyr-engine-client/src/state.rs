//! État du moteur client, cloisonné par appareil distant.
//!
//! Le moteur bascule en mode portable dès qu'un fichier `portable.dat`
//! est présent dans son répertoire de travail : tout son état (réglages,
//! identité, hôtes appairés) reste alors dans ce répertoire au lieu de la
//! base de registre.
//!
//! Un répertoire par appareil distant apporte trois choses : aucune
//! écriture concurrente entre deux sessions sortantes simultanées, une
//! identité stable dans le temps pour chaque relation, et une remise à
//! zéro d'une relation qui se réduit à supprimer un dossier.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const MARQUEUR_PORTABLE: &str = "portable.dat";

/// Identifiant de dossier dérivé de l'adresse d'un hôte.
///
/// Tant qu'il n'y a ni compte ni registre d'appareils, l'adresse tient
/// lieu d'identité. Elle est réduite à des caractères sûrs pour un nom de
/// dossier, sur toutes les plateformes.
pub fn identifiant_depuis_adresse(hote: &str) -> String {
    let nettoye: String = hote
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let nettoye = nettoye.trim_matches('-').to_ascii_lowercase();
    if nettoye.is_empty() {
        "appareil".to_string()
    } else {
        nettoye
    }
}

pub struct DeviceState {
    dossier: PathBuf,
}

impl DeviceState {
    /// État de l'appareil dans l'emplacement standard du produit.
    pub fn pour_appareil(device_id: &str) -> Self {
        Self::dans(zyr_proto::paths::device_state_dir(device_id))
    }

    pub fn dans(dossier: impl Into<PathBuf>) -> Self {
        Self {
            dossier: dossier.into(),
        }
    }

    pub fn dossier(&self) -> &Path {
        &self.dossier
    }

    /// Crée le répertoire et y pose le marqueur de mode portable.
    pub fn preparer(&self) -> io::Result<()> {
        fs::create_dir_all(&self.dossier)?;
        let marqueur = self.dossier.join(MARQUEUR_PORTABLE);
        if !marqueur.exists() {
            fs::write(&marqueur, b"")?;
        }
        Ok(())
    }

    pub fn est_prepare(&self) -> bool {
        self.dossier.join(MARQUEUR_PORTABLE).is_file()
    }

    /// Fichiers de réglages écrits par le moteur sous ce répertoire.
    ///
    /// Leur emplacement exact dépend de la manière dont le moteur nomme
    /// son arborescence : la recherche est récursive plutôt que devinée.
    pub fn fichiers_reglages(&self) -> Vec<PathBuf> {
        let mut trouves = Vec::new();
        collecter_ini(&self.dossier, &mut trouves);
        trouves.sort();
        trouves
    }

    /// Vrai si le moteur a déjà enregistré un hôte appairé.
    ///
    /// Sert à décider s'il faut lancer un appairage avant la session.
    pub fn a_un_hote_appaire(&self) -> bool {
        self.fichiers_reglages().iter().any(|f| {
            fs::read_to_string(f)
                .map(|contenu| contenu.contains("hosts"))
                .unwrap_or(false)
        })
    }

    /// Supprime tout l'état de la relation avec cet appareil.
    pub fn oublier(&self) -> io::Result<()> {
        match fs::remove_dir_all(&self.dossier) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            autre => autre,
        }
    }
}

fn collecter_ini(dossier: &Path, sortie: &mut Vec<PathBuf>) {
    let Ok(entrees) = fs::read_dir(dossier) else {
        return;
    };
    for entree in entrees.flatten() {
        let chemin = entree.path();
        if chemin.is_dir() {
            collecter_ini(&chemin, sortie);
        } else if chemin
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("ini"))
        {
            sortie.push(chemin);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dossier_temporaire() -> PathBuf {
        let chemin = std::env::temp_dir().join(format!(
            "zyrdesk-state-{}",
            zyr_proto::alea::chaine_alphanumerique(12)
        ));
        fs::create_dir_all(&chemin).unwrap();
        chemin
    }

    #[test]
    fn preparer_pose_le_marqueur_de_mode_portable() {
        let base = dossier_temporaire();
        let etat = DeviceState::dans(base.join("pc-bureau"));
        assert!(!etat.est_prepare());
        etat.preparer().unwrap();
        assert!(etat.est_prepare());
        // Une seconde préparation ne doit rien casser.
        etat.preparer().unwrap();
        assert!(etat.est_prepare());
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn les_reglages_sont_trouves_quelle_que_soit_l_arborescence() {
        let base = dossier_temporaire();
        let etat = DeviceState::dans(&base);
        etat.preparer().unwrap();
        assert!(etat.fichiers_reglages().is_empty());
        assert!(!etat.a_un_hote_appaire());

        let imbrique = base.join("Un Editeur").join("Un Produit");
        fs::create_dir_all(&imbrique).unwrap();
        fs::write(imbrique.join("reglages.ini"), "[General]\nhosts\\size=1\n").unwrap();

        assert_eq!(etat.fichiers_reglages().len(), 1);
        assert!(etat.a_un_hote_appaire());
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn oublier_efface_tout_et_reste_sans_effet_si_absent() {
        let base = dossier_temporaire();
        let etat = DeviceState::dans(base.join("pc"));
        etat.preparer().unwrap();
        assert!(etat.dossier().exists());
        etat.oublier().unwrap();
        assert!(!etat.dossier().exists());
        etat.oublier().unwrap();
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn chaque_appareil_a_son_propre_etat() {
        let a = DeviceState::pour_appareil("pc-bureau");
        let b = DeviceState::pour_appareil("pc-portable");
        assert_ne!(a.dossier(), b.dossier());
    }

    #[test]
    fn les_adresses_deviennent_des_noms_de_dossier_surs() {
        assert_eq!(identifiant_depuis_adresse("192.168.1.10"), "192-168-1-10");
        assert_eq!(identifiant_depuis_adresse("PC-Bureau"), "pc-bureau");
        assert_eq!(identifiant_depuis_adresse("fe80::1%eth0"), "fe80--1-eth0");
        assert_eq!(identifiant_depuis_adresse("..."), "appareil");
        assert_eq!(identifiant_depuis_adresse(""), "appareil");
    }

    #[test]
    fn les_identifiants_ne_contiennent_aucun_caractere_de_chemin() {
        for entree in ["../evasion", "a/b\\c", "C:\\Windows", "hôte étrange"] {
            let id = identifiant_depuis_adresse(entree);
            assert!(
                id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
                "{entree} donne {id}"
            );
        }
    }
}
