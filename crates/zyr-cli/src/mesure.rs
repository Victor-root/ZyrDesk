//! Ce qu'on retient d'une salve de paquets.
//!
//! La moyenne ne dit rien d'utile pour une session interactive : un seul
//! paquet en retard sur mille se voit à l'écran, et disparaît dans une
//! moyenne. Ce sont les queues de distribution qui comptent, d'où les
//! centiles.

use std::time::Duration;

/// Un aller-retour mesuré.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct AllerRetour(pub Duration);

/// Ce qu'une salve a donné.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resultat {
    pub emis: u64,
    pub revenus: u64,
    pub octets_emis: u64,
    pub duree: Duration,
    pub median: Duration,
    pub centile_95: Duration,
    pub centile_99: Duration,
    pub pire: Duration,
}

impl Resultat {
    /// Calcule le bilan d'une salve. Les mesures sont consommées : elles
    /// doivent être triées, et rien d'autre n'en a besoin ensuite.
    pub fn depuis(
        mut mesures: Vec<AllerRetour>,
        emis: u64,
        octets_emis: u64,
        duree: Duration,
    ) -> Self {
        mesures.sort_unstable();
        let revenus = mesures.len() as u64;
        Self {
            emis,
            revenus,
            octets_emis,
            duree,
            median: centile(&mesures, 50),
            centile_95: centile(&mesures, 95),
            centile_99: centile(&mesures, 99),
            pire: mesures.last().map(|m| m.0).unwrap_or_default(),
        }
    }

    pub fn perdus(&self) -> u64 {
        self.emis.saturating_sub(self.revenus)
    }

    /// Part de paquets jamais revenus, en pourcentage.
    pub fn perte(&self) -> f64 {
        if self.emis == 0 {
            return 0.0;
        }
        self.perdus() as f64 * 100.0 / self.emis as f64
    }

    /// Débit réellement soutenu, en mégabits par seconde.
    pub fn debit(&self) -> f64 {
        let secondes = self.duree.as_secs_f64();
        if secondes <= 0.0 {
            return 0.0;
        }
        self.octets_emis as f64 * 8.0 / secondes / 1_000_000.0
    }
}

/// Valeur en dessous de laquelle tombe le centile demandé.
///
/// Les mesures doivent être triées.
fn centile(mesures: &[AllerRetour], rang: u32) -> Duration {
    if mesures.is_empty() {
        return Duration::ZERO;
    }
    // Le centile 100 doit désigner la dernière mesure, pas au-delà.
    let place = (mesures.len() as u64 * rang as u64 / 100) as usize;
    mesures[place.min(mesures.len() - 1)].0
}

/// Millisecondes, à deux décimales : en dessous, le bruit de mesure
/// dépasse la précision affichée.
pub fn millisecondes(duree: Duration) -> String {
    format!("{:.2} ms", duree.as_secs_f64() * 1000.0)
}

/// Écart entre deux durées, signé, tel qu'on le lit dans un rapport.
pub fn ecart(reference: Duration, mesure: Duration) -> String {
    if mesure >= reference {
        format!("+{}", millisecondes(mesure - reference))
    } else {
        format!("-{}", millisecondes(reference - mesure))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mesures(millis: &[u64]) -> Vec<AllerRetour> {
        millis
            .iter()
            .map(|m| AllerRetour(Duration::from_millis(*m)))
            .collect()
    }

    #[test]
    fn les_centiles_designent_les_bonnes_mesures() {
        let cent: Vec<u64> = (1..=100).collect();
        let r = Resultat::depuis(mesures(&cent), 100, 0, Duration::from_secs(1));
        assert_eq!(r.median, Duration::from_millis(51));
        assert_eq!(r.centile_95, Duration::from_millis(96));
        assert_eq!(r.centile_99, Duration::from_millis(100));
        assert_eq!(r.pire, Duration::from_millis(100));
    }

    #[test]
    fn les_mesures_arrivees_en_desordre_sont_remises_en_ordre() {
        // Ordonnées : 1, 2, 4, 30, 50.
        let r = Resultat::depuis(mesures(&[50, 1, 30, 2, 4]), 5, 0, Duration::from_secs(1));
        assert_eq!(r.median, Duration::from_millis(4));
        assert_eq!(r.pire, Duration::from_millis(50));
    }

    #[test]
    fn les_paquets_jamais_revenus_sont_comptes() {
        let r = Resultat::depuis(mesures(&[1, 2, 3]), 4, 0, Duration::from_secs(1));
        assert_eq!(r.perdus(), 1);
        assert_eq!(r.perte(), 25.0);
    }

    #[test]
    fn une_salve_sans_retour_ne_fait_pas_tout_exploser() {
        let r = Resultat::depuis(Vec::new(), 0, 0, Duration::ZERO);
        assert_eq!(r.median, Duration::ZERO);
        assert_eq!(r.pire, Duration::ZERO);
        assert_eq!(r.perte(), 0.0);
        assert_eq!(r.debit(), 0.0);
    }

    #[test]
    fn le_debit_correspond_a_ce_qui_est_parti() {
        // 1 250 000 octets en une seconde, soit 10 Mb/s.
        let r = Resultat::depuis(Vec::new(), 0, 1_250_000, Duration::from_secs(1));
        assert!((r.debit() - 10.0).abs() < 0.001, "{}", r.debit());
    }

    #[test]
    fn l_ecart_se_lit_dans_les_deux_sens() {
        let court = Duration::from_micros(1500);
        let long = Duration::from_micros(2300);
        assert_eq!(ecart(court, long), "+0.80 ms");
        assert_eq!(ecart(long, court), "-0.80 ms");
        assert_eq!(ecart(court, court), "+0.00 ms");
    }
}
