//! Que faire quand le moteur hôte s'arrête.
//!
//! Un service qui relance aveuglément un moteur qui meurt à chaque
//! démarrage tourne en boucle et masque la panne : au lieu d'un message
//! clair, l'utilisateur voit un ordinateur qui n'aboutit jamais. Un
//! service qui ne relance rien laisse au contraire tomber une machine
//! qu'un incident passager aurait suffi à remettre debout.
//!
//! La règle retenue distingue trois cas. Le moteur qui s'arrête parce
//! que Windows s'éteint n'est pas un incident. Le moteur qui tombe après
//! avoir tenu longtemps mérite une relance immédiate. Le moteur qui
//! tombe aussitôt lancé, plusieurs fois de suite, ne sera pas sauvé par
//! une relance de plus.
//!
//! Ce module ne connaît ni Windows ni les processus : il décide, à
//! partir de ce qu'on lui rapporte.

use std::time::Duration;

/// Code que rend le moteur quand il s'arrête parce que Windows s'éteint.
///
/// C'est celui du système, `ERROR_SHUTDOWN_IN_PROGRESS`. Le moteur amont
/// s'en sert pour distinguer sa propre fin d'un incident, et le service
/// amont s'appuie dessus pour ne pas le relancer pendant l'extinction.
pub const ARRET_DE_WINDOWS: i32 = 1115;

/// Au-delà, le moteur est réputé avoir tenu : le compteur d'échecs
/// repart de zéro.
const VIE_SAINE: Duration = Duration::from_secs(60);

/// Délai avant la première relance après un échec.
const DELAI_INITIAL: Duration = Duration::from_secs(2);

/// Plafond du délai de relance. Au-delà, l'attente serait plus pénible
/// que la panne.
const DELAI_MAXIMAL: Duration = Duration::from_secs(60);

/// Échecs rapprochés admis avant de renoncer.
const ECHECS_MAX: u32 = 5;

/// Ce qu'il convient de faire après l'arrêt du moteur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Suite {
    /// Relancer après ce délai.
    Relancer(Duration),
    /// Renoncer et le signaler : le moteur ne tient pas debout.
    Renoncer,
    /// Ne rien faire : l'arrêt était voulu.
    Terminer,
}

/// Décide de la suite, en gardant la mémoire des échecs récents.
#[derive(Debug, Default)]
pub struct Politique {
    echecs: u32,
}

impl Politique {
    pub fn nouvelle() -> Self {
        Self::default()
    }

    /// Nombre d'échecs rapprochés accumulés.
    pub fn echecs(&self) -> u32 {
        self.echecs
    }

    /// Décide de la suite après un arrêt du moteur.
    ///
    /// `code` est le code de sortie, absent si le moteur a été
    /// interrompu. `duree_de_vie` est le temps qu'il a tenu.
    pub fn apres_arret(&mut self, code: Option<i32>, duree_de_vie: Duration) -> Suite {
        if code == Some(ARRET_DE_WINDOWS) {
            return Suite::Terminer;
        }

        // Un moteur qui a tenu son temps n'est pas en cause : l'incident
        // qui vient de l'abattre est isolé, et l'ardoise est effacée.
        if duree_de_vie >= VIE_SAINE {
            self.echecs = 0;
            return Suite::Relancer(Duration::ZERO);
        }

        self.echecs += 1;
        if self.echecs > ECHECS_MAX {
            return Suite::Renoncer;
        }
        Suite::Relancer(delai(self.echecs))
    }
}

/// Délai avant la relance numéro `echec`, doublant à chaque fois.
fn delai(echec: u32) -> Duration {
    let facteur = 1u32 << (echec.saturating_sub(1)).min(16);
    DELAI_INITIAL.saturating_mul(facteur).min(DELAI_MAXIMAL)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seconde(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    #[test]
    fn l_extinction_de_windows_n_est_pas_un_incident() {
        let mut p = Politique::nouvelle();
        // Même tombé aussitôt : c'est la machine qui s'éteint.
        assert_eq!(
            p.apres_arret(Some(ARRET_DE_WINDOWS), Duration::ZERO),
            Suite::Terminer
        );
        assert_eq!(p.echecs(), 0);
    }

    #[test]
    fn un_moteur_qui_a_tenu_repart_tout_de_suite() {
        let mut p = Politique::nouvelle();
        assert_eq!(
            p.apres_arret(Some(1), VIE_SAINE),
            Suite::Relancer(Duration::ZERO)
        );
    }

    #[test]
    fn les_echecs_rapproches_espacent_les_relances() {
        let mut p = Politique::nouvelle();
        let mut precedent = Duration::ZERO;
        for _ in 0..ECHECS_MAX {
            let Suite::Relancer(delai) = p.apres_arret(Some(1), seconde(1)) else {
                panic!("le moteur devrait encore avoir sa chance");
            };
            assert!(
                delai >= precedent,
                "{delai:?} après {precedent:?} : l'attente doit croître"
            );
            assert!(delai <= DELAI_MAXIMAL);
            precedent = delai;
        }
        assert!(precedent > DELAI_INITIAL, "l'attente n'a jamais grandi");
    }

    #[test]
    fn un_moteur_qui_ne_tient_jamais_finit_par_etre_laisse() {
        let mut p = Politique::nouvelle();
        for _ in 0..ECHECS_MAX {
            assert!(matches!(
                p.apres_arret(Some(1), seconde(1)),
                Suite::Relancer(_)
            ));
        }
        assert_eq!(p.apres_arret(Some(1), seconde(1)), Suite::Renoncer);
    }

    #[test]
    fn une_reussite_efface_les_echecs_precedents() {
        let mut p = Politique::nouvelle();
        for _ in 0..ECHECS_MAX {
            p.apres_arret(Some(1), seconde(1));
        }
        assert_eq!(p.echecs(), ECHECS_MAX);

        // Le moteur repart, tient son temps, puis retombe : il doit
        // retrouver toutes ses chances, sans quoi une machine allumée
        // depuis des semaines finirait par ne plus rien relancer.
        p.apres_arret(Some(1), VIE_SAINE);
        assert_eq!(p.echecs(), 0);
        assert!(matches!(
            p.apres_arret(Some(1), seconde(1)),
            Suite::Relancer(_)
        ));
    }

    #[test]
    fn un_moteur_interrompu_sans_code_est_traite_comme_un_echec() {
        let mut p = Politique::nouvelle();
        assert!(matches!(
            p.apres_arret(None, seconde(1)),
            Suite::Relancer(_)
        ));
        assert_eq!(p.echecs(), 1);
    }

    #[test]
    fn le_delai_ne_deborde_jamais() {
        // Le décalage qui double le délai ne doit pas partir en vrille
        // sur une valeur absurde.
        for echec in [0u32, 1, 5, 100, u32::MAX] {
            assert!(delai(echec) <= DELAI_MAXIMAL, "échec {echec}");
        }
    }
}
