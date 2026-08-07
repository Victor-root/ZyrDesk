//! Construction des lignes de commande du moteur client.
//!
//! Toutes les options passent par la ligne de commande plutôt que par le
//! fichier de réglages : réécrire le fichier pendant qu'une session peut
//! tourner exposerait à des écritures concurrentes.

use zyr_proto::session::SessionSettings;

/// Nom de l'application exposée par le moteur hôte.
///
/// La liste d'applications générée pour l'hôte n'en contient qu'une.
pub const APPLICATION: &str = "Desktop";

/// Arguments d'appairage avec un hôte.
pub fn arguments_appairage(hote: &str, pin: &str) -> Vec<String> {
    vec![
        "pair".to_string(),
        hote.to_string(),
        "--pin".to_string(),
        pin.to_string(),
    ]
}

/// Arguments de démarrage d'une session.
///
/// Le décodage matériel est imposé : un repli logiciel silencieux
/// donnerait une session qui « a l'air » de fonctionner tout en ratant
/// l'objectif de performance. Mieux vaut un échec visible.
pub fn arguments_session(hote: &str, reglages: &SessionSettings) -> Vec<String> {
    let mut args = vec![
        "stream".to_string(),
        hote.to_string(),
        APPLICATION.to_string(),
        "--resolution".to_string(),
        format!("{}x{}", reglages.largeur, reglages.hauteur),
        "--fps".to_string(),
        reglages.fps.to_string(),
        "--bitrate".to_string(),
        reglages.bitrate_kbps.to_string(),
        "--display-mode".to_string(),
        reglages.mode_affichage.valeur_moteur().to_string(),
        "--video-codec".to_string(),
        reglages.codec.valeur_moteur().to_string(),
        "--video-decoder".to_string(),
        "hardware".to_string(),
        "--frame-pacing".to_string(),
    ];
    if let Some(taille) = reglages.packet_size {
        args.push("--packet-size".to_string());
        args.push(taille.to_string());
    }
    if reglages.souris_absolue {
        args.push("--absolute-mouse".to_string());
    }
    if reglages.overlay_stats {
        args.push("--performance-overlay".to_string());
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;
    use zyr_proto::session::{Codec, ModeAffichage};

    fn valeur_de<'a>(args: &'a [String], drapeau: &str) -> Option<&'a str> {
        let index = args.iter().position(|a| a == drapeau)?;
        args.get(index + 1).map(String::as_str)
    }

    #[test]
    fn appairage_non_interactif() {
        assert_eq!(
            arguments_appairage("127.0.0.1", "0421"),
            ["pair", "127.0.0.1", "--pin", "0421"]
        );
    }

    #[test]
    fn la_session_vise_l_unique_application_de_l_hote() {
        let args = arguments_session("192.168.1.10", &SessionSettings::default());
        assert_eq!(args[0], "stream");
        assert_eq!(args[1], "192.168.1.10");
        assert_eq!(args[2], APPLICATION);
    }

    #[test]
    fn les_reglages_sont_traduits_en_options() {
        let reglages = SessionSettings {
            largeur: 2560,
            hauteur: 1440,
            fps: 120,
            bitrate_kbps: 80_000,
            codec: Codec::Hevc,
            mode_affichage: ModeAffichage::SansBordure,
            ..SessionSettings::default()
        };
        let args = arguments_session("hote", &reglages);
        assert_eq!(valeur_de(&args, "--resolution"), Some("2560x1440"));
        assert_eq!(valeur_de(&args, "--fps"), Some("120"));
        assert_eq!(valeur_de(&args, "--bitrate"), Some("80000"));
        assert_eq!(valeur_de(&args, "--video-codec"), Some("HEVC"));
        assert_eq!(valeur_de(&args, "--display-mode"), Some("borderless"));
    }

    #[test]
    fn le_decodage_materiel_et_le_frame_pacing_sont_toujours_imposes() {
        let args = arguments_session("hote", &SessionSettings::default());
        assert_eq!(valeur_de(&args, "--video-decoder"), Some("hardware"));
        assert!(args.iter().any(|a| a == "--frame-pacing"));
    }

    #[test]
    fn la_taille_de_paquet_reste_au_moteur_tant_qu_elle_n_est_pas_imposee() {
        let args = arguments_session("hote", &SessionSettings::default());
        assert!(!args.iter().any(|a| a == "--packet-size"));

        let impose = SessionSettings {
            packet_size: Some(1264),
            ..SessionSettings::default()
        };
        let args = arguments_session("hote", &impose);
        assert_eq!(valeur_de(&args, "--packet-size"), Some("1264"));
    }

    #[test]
    fn les_options_facultatives_suivent_les_reglages() {
        let sans = SessionSettings {
            souris_absolue: false,
            overlay_stats: false,
            ..SessionSettings::default()
        };
        let args = arguments_session("hote", &sans);
        assert!(!args.iter().any(|a| a == "--absolute-mouse"));
        assert!(!args.iter().any(|a| a == "--performance-overlay"));

        let avec = SessionSettings {
            souris_absolue: true,
            overlay_stats: true,
            ..SessionSettings::default()
        };
        let args = arguments_session("hote", &avec);
        assert!(args.iter().any(|a| a == "--absolute-mouse"));
        assert!(args.iter().any(|a| a == "--performance-overlay"));
    }
}
