//! Contrôle de congestion adapté à un flux vidéo temps réel.
//!
//! Les contrôleurs de congestion habituels réduisent leur débit dès
//! qu'ils constatent une perte : ils supposent que la perte signale une
//! saturation et qu'il faut ralentir. Pour un transfert de fichier, c'est
//! juste. Pour de la vidéo interactive, c'est ruineux : à 1 % de perte et
//! 25 ms d'aller-retour, un tel contrôleur converge vers environ 5 Mb/s,
//! alors qu'une session confortable en demande quarante. Le flux se
//! retrouve étranglé, ou sa file d'attente gonfle en secondes de latence.
//!
//! Ce contrôleur maintient au contraire une fenêtre calée sur ce que la
//! session a réellement besoin de garder en vol : le produit du débit par
//! le temps de trajet, doublé pour la marge, plus de quoi absorber une
//! image entière émise d'un bloc.
//!
//! Ne pas réagir aux pertes serait déraisonnable pour un flux capable de
//! saturer un lien. Ce n'est pas le cas ici : le débit est fixé par
//! l'encodeur et ne dépasse jamais sa consigne. La fenêtre ne sert donc
//! pas à émettre davantage, seulement à ne pas bloquer ce que l'encodeur
//! produit déjà. Les pertes restent traitées par la correction d'erreur
//! du protocole vidéo, qui est faite pour ça.
//!
//! Effet de bord recherché : une fenêtre large désamorce aussi le
//! lissage d'émission. Chaque image part en rafale de plusieurs dizaines
//! de paquets ; un lisseur les étalerait, ajoutant une gigue régulière
//! que la régulation d'affichage du client devrait ensuite absorber.

use std::any::Any;
use std::sync::Arc;
use std::time::{Duration, Instant};

use quinn::congestion::{Controller, ControllerFactory};
use quinn_proto::RttEstimator;

/// Fenêtre plancher, quels que soient débit et temps de trajet.
const FENETRE_MINIMALE: u64 = 64 * 1024;

/// Temps de trajet maximal retenu pour le calcul.
///
/// Une mesure aberrante, relevée pendant un blocage, produirait sinon
/// une fenêtre absurde qui mettrait longtemps à redescendre.
const RTT_MAXIMAL: Duration = Duration::from_millis(500);

/// Temps de trajet supposé avant la première mesure.
const RTT_INITIAL: Duration = Duration::from_millis(25);

/// Caractéristiques du flux à transporter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProfilMedia {
    pub debit_bits_par_seconde: u64,
    pub images_par_seconde: u32,
}

impl Default for ProfilMedia {
    fn default() -> Self {
        Self {
            debit_bits_par_seconde: 20_000_000,
            images_par_seconde: 60,
        }
    }
}

impl ProfilMedia {
    /// Fenêtre nécessaire pour ce profil au temps de trajet donné.
    ///
    /// Deux fois le produit débit-délai, plus une image entière : le
    /// premier terme couvre ce qui est en vol, le second la rafale de
    /// paquets qu'une image représente.
    pub fn fenetre(&self, rtt: Duration) -> u64 {
        let rtt = rtt.min(RTT_MAXIMAL);
        let octets_par_seconde = self.debit_bits_par_seconde / 8;
        let en_vol = (octets_par_seconde as f64 * rtt.as_secs_f64()) as u64;
        let image = octets_par_seconde / self.images_par_seconde.max(1) as u64;
        en_vol
            .saturating_mul(2)
            .saturating_add(image)
            .max(FENETRE_MINIMALE)
    }
}

/// Contrôleur maintenant la fenêtre nécessaire au flux.
#[derive(Debug, Clone)]
pub struct ControleurMedia {
    profil: ProfilMedia,
    rtt: Duration,
}

impl ControleurMedia {
    pub fn nouveau(profil: ProfilMedia) -> Self {
        Self {
            profil,
            rtt: RTT_INITIAL,
        }
    }
}

impl Controller for ControleurMedia {
    fn on_ack(
        &mut self,
        _now: Instant,
        _sent: Instant,
        _bytes: u64,
        _app_limited: bool,
        rtt: &RttEstimator,
    ) {
        self.rtt = rtt.get();
    }

    /// Les pertes ne réduisent pas la fenêtre.
    ///
    /// Ralentir n'accélérerait pas la vidéo : elle est produite à un
    /// débit fixe, et ses pertes sont réparées par la correction d'erreur
    /// du protocole. Réduire ne ferait qu'ajouter du retard.
    fn on_congestion_event(
        &mut self,
        _now: Instant,
        _sent: Instant,
        _is_persistent_congestion: bool,
        _lost_bytes: u64,
    ) {
    }

    fn on_mtu_update(&mut self, _new_mtu: u16) {}

    fn window(&self) -> u64 {
        self.profil.fenetre(self.rtt)
    }

    fn clone_box(&self) -> Box<dyn Controller> {
        Box::new(self.clone())
    }

    fn initial_window(&self) -> u64 {
        self.profil.fenetre(RTT_INITIAL)
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

impl ControllerFactory for ProfilMedia {
    fn build(self: Arc<Self>, _now: Instant, _current_mtu: u16) -> Box<dyn Controller> {
        Box::new(ControleurMedia::nouveau(*self))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profil(mbps: u64) -> ProfilMedia {
        ProfilMedia {
            debit_bits_par_seconde: mbps * 1_000_000,
            images_par_seconde: 60,
        }
    }

    #[test]
    fn la_fenetre_couvre_ce_qui_est_en_vol() {
        // À 40 Mb/s et 25 ms, il y a 125 000 octets en vol à tout
        // instant : une fenêtre plus courte bloquerait l'encodeur.
        let f = profil(40).fenetre(Duration::from_millis(25));
        assert!(f >= 250_000, "{f} octets, moins que deux fois le vol");
    }

    #[test]
    fn la_fenetre_absorbe_une_image_entiere() {
        // Même à temps de trajet négligeable, une image part d'un bloc.
        let p = profil(40);
        let image = p.debit_bits_par_seconde / 8 / 60;
        assert!(p.fenetre(Duration::ZERO) >= image);
    }

    #[test]
    fn la_fenetre_suit_le_debit_et_le_trajet() {
        let court = profil(40).fenetre(Duration::from_millis(5));
        let long = profil(40).fenetre(Duration::from_millis(50));
        assert!(long > court);

        let faible = profil(10).fenetre(Duration::from_millis(25));
        let fort = profil(80).fenetre(Duration::from_millis(25));
        assert!(fort > faible);
    }

    #[test]
    fn un_trajet_aberrant_ne_fait_pas_exploser_la_fenetre() {
        let p = profil(40);
        let plafonnee = p.fenetre(RTT_MAXIMAL);
        assert_eq!(p.fenetre(Duration::from_secs(30)), plafonnee);
    }

    #[test]
    fn un_profil_degenere_reste_exploitable() {
        let p = ProfilMedia {
            debit_bits_par_seconde: 0,
            images_par_seconde: 0,
        };
        assert_eq!(p.fenetre(Duration::from_millis(25)), FENETRE_MINIMALE);
    }

    #[test]
    fn les_pertes_ne_reduisent_pas_la_fenetre() {
        let mut c = ControleurMedia::nouveau(profil(40));
        let avant = c.window();
        let maintenant = Instant::now();
        for _ in 0..100 {
            c.on_congestion_event(maintenant, maintenant, true, 100_000);
        }
        assert_eq!(c.window(), avant);
    }

    #[test]
    fn la_fenetre_initiale_est_deja_utilisable() {
        let c = ControleurMedia::nouveau(profil(40));
        assert_eq!(c.initial_window(), c.window());
        assert!(c.initial_window() >= FENETRE_MINIMALE);
    }
}
