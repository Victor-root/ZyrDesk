//! Emplacements de fichiers du produit.
//!
//! Deux racines distinctes : les données partagées entre tous les
//! utilisateurs de la machine (moteurs, état de l'hôte, journaux) et les
//! données propres à l'utilisateur courant (état d'exécution, secrets de
//! prototypage). Séparer les deux évite qu'un secret lisible par un autre
//! utilisateur local se retrouve dans un dossier commun.

use std::path::PathBuf;

/// Racine des données partagées : `%ProgramData%\ZyrDesk` sous Windows.
pub fn data_dir() -> PathBuf {
    if cfg!(windows) {
        std::env::var_os("ProgramData")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
            .join("ZyrDesk")
    } else {
        std::env::temp_dir().join("zyrdesk-dev")
    }
}

/// Racine des données de l'utilisateur courant : `%LOCALAPPDATA%\ZyrDesk`.
pub fn user_dir() -> PathBuf {
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(data_dir)
            .join("ZyrDesk")
    } else {
        std::env::temp_dir().join("zyrdesk-dev-user")
    }
}

/// Binaires des moteurs, déposés à part de notre propre exécutable.
pub fn engines_dir() -> PathBuf {
    data_dir().join("engines")
}

/// Nom de fichier d'un exécutable, avec l'extension de la plateforme.
pub fn nom_executable(base: &str) -> String {
    if cfg!(windows) {
        format!("{base}.exe")
    } else {
        base.to_string()
    }
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

pub fn logs_dir() -> PathBuf {
    data_dir().join("logs")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_racines_sont_distinctes() {
        assert_ne!(data_dir(), user_dir());
    }

    #[test]
    fn les_sous_dossiers_derivent_de_la_racine_partagee() {
        let racine = data_dir();
        for chemin in [
            engines_dir(),
            host_engine_dir(),
            client_engine_dir(),
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
