//! Génération de valeurs aléatoires à usage sensible.
//!
//! Le générateur du système est utilisé partout : identifiants du moteur
//! hôte et code d'appairage doivent rester imprévisibles pour un autre
//! utilisateur local.

use rand::RngExt;
use rand::distr::{Alphanumeric, SampleString};

/// Chaîne alphanumérique tirée du générateur du système.
pub fn chaine_alphanumerique(longueur: usize) -> String {
    Alphanumeric.sample_string(&mut rand::rng(), longueur)
}

/// Code d'appairage à quatre chiffres, zéros de tête compris.
///
/// Le format est imposé par le protocole d'appairage des moteurs.
pub fn pin_appairage() -> String {
    format!("{:04}", rand::rng().random_range(0..10_000u16))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn longueur_respectee_et_caracteres_sains() {
        let s = chaine_alphanumerique(32);
        assert_eq!(s.len(), 32);
        assert!(s.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn deux_tirages_different() {
        assert_ne!(chaine_alphanumerique(32), chaine_alphanumerique(32));
    }

    #[test]
    fn pin_toujours_sur_quatre_chiffres() {
        for _ in 0..500 {
            let pin = pin_appairage();
            assert_eq!(pin.len(), 4, "{pin}");
            assert!(pin.chars().all(|c| c.is_ascii_digit()), "{pin}");
        }
    }
}
