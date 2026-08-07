//! Emplacements de fichiers du produit.
//!
//! Tout vit sous un dossier `data` unique, placé à la racine du projet.
//! Rien n'est éparpillé ailleurs sur la machine : le contenu se lit, se
//! sauvegarde et s'efface d'un seul geste.
//!
//! La variable d'environnement `ZYRDESK_DATA` permet de le déplacer.
//! L'emplacement système d'un produit installé viendra avec le service
//! du jalon M3.

use std::ffi::OsString;
use std::path::PathBuf;

const VAR_DATA: &str = "ZYRDESK_DATA";

/// Racine de toutes les données du produit.
pub fn data_dir() -> PathBuf {
    resoudre_data_dir(std::env::var_os(VAR_DATA), racine_projet)
}

/// Règle de résolution, isolée de l'environnement pour être vérifiable.
fn resoudre_data_dir(surcharge: Option<OsString>, racine: impl FnOnce() -> PathBuf) -> PathBuf {
    match surcharge {
        Some(chemin) if !chemin.is_empty() => PathBuf::from(chemin),
        _ => racine().join("data"),
    }
}

/// Racine du projet : premier dossier ancêtre de l'exécutable contenant
/// un `Cargo.toml`. L'exécutable vivant sous `target/<profil>/`, la
/// remontée aboutit à la racine du dépôt, quel que soit le profil de
/// compilation. À défaut, le dossier de l'exécutable lui-même.
fn racine_projet() -> PathBuf {
    let Ok(exe) = std::env::current_exe() else {
        return PathBuf::from(".");
    };
    let mut candidat = exe.parent();
    while let Some(dossier) = candidat {
        if dossier.join("Cargo.toml").is_file() {
            return dossier.to_path_buf();
        }
        candidat = dossier.parent();
    }
    exe.parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Nom de fichier d'un exécutable, avec l'extension de la plateforme.
pub fn nom_executable(base: &str) -> String {
    if cfg!(windows) {
        format!("{base}.exe")
    } else {
        base.to_string()
    }
}

/// Binaires des moteurs.
pub fn engines_dir() -> PathBuf {
    data_dir().join("engines")
}

/// Moteur hôte (dérivé de Sunshine).
pub fn host_engine_dir() -> PathBuf {
    engines_dir().join("host")
}

/// Exécutable attendu du moteur hôte.
///
/// Le nom est celui du produit, jamais celui du projet amont : c'est ce
/// que voit l'utilisateur dans le gestionnaire des tâches.
pub fn host_engine_exe() -> PathBuf {
    host_engine_dir().join(nom_executable("zyrdesk-host-engine"))
}

/// Moteur client (dérivé de Moonlight).
pub fn client_engine_dir() -> PathBuf {
    engines_dir().join("client")
}

/// Exécutable attendu du moteur client.
pub fn client_engine_exe() -> PathBuf {
    client_engine_dir().join(nom_executable("zyrdesk-session"))
}

/// Configuration et état générés pour le moteur hôte.
pub fn host_state_dir() -> PathBuf {
    data_dir().join("host")
}

/// État isolé du moteur client pour un appareil distant donné.
pub fn device_state_dir(device_id: &str) -> PathBuf {
    data_dir().join("devices").join(device_id)
}

/// Journaux de tous les composants.
pub fn logs_dir() -> PathBuf {
    data_dir().join("logs")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tout_vit_sous_la_racine_unique() {
        let racine = data_dir();
        for chemin in [
            engines_dir(),
            host_engine_dir(),
            host_engine_exe(),
            client_engine_dir(),
            client_engine_exe(),
            host_state_dir(),
            device_state_dir("pc-bureau"),
            logs_dir(),
        ] {
            assert!(
                chemin.starts_with(&racine),
                "{} hors racine",
                chemin.display()
            );
        }
    }

    #[test]
    fn la_surcharge_prime_sur_la_racine_du_projet() {
        let projet = || PathBuf::from("/le/projet");
        assert_eq!(
            resoudre_data_dir(Some(OsString::from("/ailleurs")), projet),
            PathBuf::from("/ailleurs")
        );
    }

    #[test]
    fn sans_surcharge_les_donnees_sont_dans_le_projet() {
        let projet = || PathBuf::from("/le/projet");
        assert_eq!(
            resoudre_data_dir(None, projet),
            PathBuf::from("/le/projet/data")
        );
        // Une variable vide vaut absence de surcharge.
        assert_eq!(
            resoudre_data_dir(Some(OsString::new()), projet),
            PathBuf::from("/le/projet/data")
        );
    }

    #[test]
    fn chaque_appareil_a_son_dossier() {
        assert_ne!(device_state_dir("a"), device_state_dir("b"));
    }

    #[test]
    fn les_executables_portent_le_nom_du_produit() {
        for exe in [host_engine_exe(), client_engine_exe()] {
            let nom = exe.file_name().unwrap().to_string_lossy().to_lowercase();
            assert!(nom.starts_with("zyrdesk"), "{nom}");
            assert!(
                !nom.contains("sunshine") && !nom.contains("moonlight"),
                "{nom}"
            );
        }
    }

    #[test]
    fn extension_d_executable_selon_la_plateforme() {
        let attendu = if cfg!(windows) { "outil.exe" } else { "outil" };
        assert_eq!(nom_executable("outil"), attendu);
    }
}
