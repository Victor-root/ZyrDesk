//! Réglages d'une session distante.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Codec {
    #[default]
    Auto,
    H264,
    Hevc,
    Av1,
}

impl Codec {
    /// Valeur attendue par la ligne de commande du moteur client.
    pub fn valeur_moteur(self) -> &'static str {
        match self {
            Codec::Auto => "auto",
            Codec::H264 => "H.264",
            Codec::Hevc => "HEVC",
            Codec::Av1 => "AV1",
        }
    }
}

impl std::str::FromStr for Codec {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().replace(['.', '-'], "").as_str() {
            "auto" => Ok(Codec::Auto),
            "h264" => Ok(Codec::H264),
            "hevc" | "h265" => Ok(Codec::Hevc),
            "av1" => Ok(Codec::Av1),
            _ => Err(format!("codec inconnu : {s}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModeAffichage {
    #[default]
    PleinEcran,
    SansBordure,
    Fenetre,
}

impl ModeAffichage {
    pub fn valeur_moteur(self) -> &'static str {
        match self {
            ModeAffichage::PleinEcran => "fullscreen",
            ModeAffichage::SansBordure => "borderless",
            ModeAffichage::Fenetre => "windowed",
        }
    }
}

impl std::str::FromStr for ModeAffichage {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "fullscreen" | "plein-ecran" => Ok(ModeAffichage::PleinEcran),
            "borderless" | "sans-bordure" => Ok(ModeAffichage::SansBordure),
            "windowed" | "fenetre" => Ok(ModeAffichage::Fenetre),
            _ => Err(format!("mode d'affichage inconnu : {s}")),
        }
    }
}

/// Réglages d'une session.
///
/// Les valeurs par défaut reprennent celles du moteur client pour du
/// 1080p60, condition nécessaire à la comparaison contre les moteurs non
/// pilotés exigée au jalon M1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionSettings {
    pub largeur: u32,
    pub hauteur: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
    pub codec: Codec,
    pub mode_affichage: ModeAffichage,
    /// Taille de paquet imposée. `None` laisse le moteur décider, ce qui
    /// est le comportement attendu tant qu'il n'y a pas de tunnel : la
    /// valeur calculée pour le tunnel arrive au jalon M2.
    pub packet_size: Option<u32>,
    /// Souris absolue : adapté au bureau, pas aux jeux à visée relative.
    pub souris_absolue: bool,
    pub overlay_stats: bool,
}

impl Default for SessionSettings {
    fn default() -> Self {
        Self {
            largeur: 1920,
            hauteur: 1080,
            fps: 60,
            bitrate_kbps: 20_000,
            codec: Codec::Auto,
            mode_affichage: ModeAffichage::PleinEcran,
            packet_size: None,
            souris_absolue: true,
            overlay_stats: false,
        }
    }
}

/// Résolution mal formée.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolutionInvalide(pub String);

impl fmt::Display for ResolutionInvalide {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "résolution attendue sous la forme LARGEURxHAUTEUR : {}",
            self.0
        )
    }
}

impl std::error::Error for ResolutionInvalide {}

/// Analyse une résolution du type `1920x1080`.
pub fn parse_resolution(valeur: &str) -> Result<(u32, u32), ResolutionInvalide> {
    let invalide = || ResolutionInvalide(valeur.to_string());
    let (l, h) = valeur.split_once(['x', 'X']).ok_or_else(invalide)?;
    let largeur: u32 = l.trim().parse().map_err(|_| invalide())?;
    let hauteur: u32 = h.trim().parse().map_err(|_| invalide())?;
    if largeur == 0 || hauteur == 0 {
        return Err(invalide());
    }
    Ok((largeur, hauteur))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valeurs_moteur_des_codecs() {
        assert_eq!(Codec::Auto.valeur_moteur(), "auto");
        assert_eq!(Codec::H264.valeur_moteur(), "H.264");
        assert_eq!(Codec::Hevc.valeur_moteur(), "HEVC");
        assert_eq!(Codec::Av1.valeur_moteur(), "AV1");
    }

    #[test]
    fn codecs_analyses_avec_tolerance_de_forme() {
        for (entree, attendu) in [
            ("auto", Codec::Auto),
            ("h264", Codec::H264),
            ("H.264", Codec::H264),
            ("hevc", Codec::Hevc),
            ("h265", Codec::Hevc),
            ("AV1", Codec::Av1),
        ] {
            assert_eq!(entree.parse::<Codec>().unwrap(), attendu, "{entree}");
        }
        assert!("vp9".parse::<Codec>().is_err());
    }

    #[test]
    fn resolutions_valides_et_invalides() {
        assert_eq!(parse_resolution("1920x1080").unwrap(), (1920, 1080));
        assert_eq!(parse_resolution("2560X1440").unwrap(), (2560, 1440));
        for mauvais in ["1920", "1920x", "x1080", "0x1080", "axb", ""] {
            assert!(parse_resolution(mauvais).is_err(), "{mauvais}");
        }
    }

    #[test]
    fn reglages_par_defaut_en_1080p60_sans_taille_imposee() {
        let d = SessionSettings::default();
        assert_eq!((d.largeur, d.hauteur, d.fps), (1920, 1080, 60));
        assert_eq!(d.packet_size, None);
    }
}
