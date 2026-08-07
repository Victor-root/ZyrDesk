//! Chemin volontairement dégradé, pour éprouver le transport.
//!
//! Un réseau de laboratoire ne perd rien. Or la propriété qui décide de
//! toute l'architecture réseau est justement celle qui ne se voit qu'en
//! présence de perte : un contrôle de congestion ordinaire prend la
//! perte pour un ordre de ralentir et étrangle la vidéo, alors que le
//! contrôleur média doit tenir son débit.
//!
//! Cette enveloppe jette une fraction des paquets sortants sous le
//! transport, là où un lien saturé les perdrait. Le transport voit donc
//! de vraies pertes, avec ses vrais mécanismes de détection.
//!
//! Elle ne sert qu'à mesurer. Rien du produit ne l'emprunte.

use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll};

use quinn::udp::{RecvMeta, Transmit};
use quinn::{AsyncUdpSocket, UdpPoller};

/// Base dans laquelle s'exprime le taux de perte.
pub const POUR_MILLE: u64 = 1000;

/// Qualité du chemin sous le transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chemin {
    /// Le chemin réel, tel qu'il est.
    Direct,
    /// Chemin dégradé : la fraction indiquée des paquets sortants est
    /// jetée, exprimée pour mille.
    Degrade { perte_pour_mille: u16 },
}

/// Socket qui perd une fraction de ce qu'on lui confie.
#[derive(Debug)]
pub struct CheminDegrade {
    interne: Arc<dyn AsyncUdpSocket>,
    perte_pour_mille: u64,
    emis: AtomicU64,
}

impl CheminDegrade {
    pub fn nouveau(interne: Arc<dyn AsyncUdpSocket>, perte_pour_mille: u16) -> Self {
        Self {
            interne,
            perte_pour_mille: u64::from(perte_pour_mille).min(POUR_MILLE),
            emis: AtomicU64::new(0),
        }
    }

    /// Décide du sort du prochain paquet.
    ///
    /// Le tirage part d'un compteur brassé plutôt que d'un état partagé :
    /// deux tâches qui émettent en même temps ne peuvent pas se marcher
    /// dessus, et la même exécution rejoue les mêmes pertes.
    fn doit_jeter(&self) -> bool {
        let rang = self.emis.fetch_add(1, Ordering::Relaxed);
        brasser(rang) % POUR_MILLE < self.perte_pour_mille
    }
}

/// Brassage d'un compteur, pour que les pertes ne tombent pas en cadence.
///
/// Une perte régulière, un paquet sur cent pile, ne ressemble à aucun
/// réseau et laisserait passer des erreurs de calcul de fenêtre.
fn brasser(rang: u64) -> u64 {
    let mut z = rang.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

impl AsyncUdpSocket for CheminDegrade {
    fn create_io_poller(self: Arc<Self>) -> Pin<Box<dyn UdpPoller>> {
        self.interne.clone().create_io_poller()
    }

    fn try_send(&self, transmit: &Transmit) -> io::Result<()> {
        if self.doit_jeter() {
            // Le paquet est réputé parti : c'est le réseau qui l'a perdu,
            // pas l'émetteur qui a renoncé.
            return Ok(());
        }
        self.interne.try_send(transmit)
    }

    fn poll_recv(
        &self,
        cx: &mut Context,
        tampons: &mut [io::IoSliceMut<'_>],
        entetes: &mut [RecvMeta],
    ) -> Poll<io::Result<usize>> {
        self.interne.poll_recv(cx, tampons, entetes)
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.interne.local_addr()
    }

    /// Un paquet par envoi, sans regroupement.
    ///
    /// Le regroupement ferait porter plusieurs paquets à un seul envoi :
    /// en jeter un reviendrait à en perdre toute une rafale, et le taux
    /// de perte demandé ne voudrait plus rien dire.
    fn max_transmit_segments(&self) -> usize {
        1
    }

    fn max_receive_segments(&self) -> usize {
        self.interne.max_receive_segments()
    }

    fn may_fragment(&self) -> bool {
        self.interne.may_fragment()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Socket qui ne sert qu'à compter ce qu'on lui remet.
    #[derive(Debug)]
    struct Compteur(AtomicU64);

    impl AsyncUdpSocket for Compteur {
        fn create_io_poller(self: Arc<Self>) -> Pin<Box<dyn UdpPoller>> {
            unimplemented!("le compteur ne s'attend pas à être interrogé")
        }

        fn try_send(&self, _transmit: &Transmit) -> io::Result<()> {
            self.0.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }

        fn poll_recv(
            &self,
            _cx: &mut Context,
            _tampons: &mut [io::IoSliceMut<'_>],
            _entetes: &mut [RecvMeta],
        ) -> Poll<io::Result<usize>> {
            Poll::Pending
        }

        fn local_addr(&self) -> io::Result<SocketAddr> {
            Ok("127.0.0.1:0".parse().unwrap())
        }
    }

    /// Fait passer le nombre de paquets voulu et rend ce qui est arrivé.
    fn emettre(perte_pour_mille: u16, paquets: u64) -> u64 {
        let arrives = Arc::new(Compteur(AtomicU64::new(0)));
        let chemin = CheminDegrade::nouveau(arrives.clone(), perte_pour_mille);
        let charge = [0u8; 64];

        for _ in 0..paquets {
            let transmit = Transmit {
                destination: "127.0.0.1:1".parse().unwrap(),
                ecn: None,
                contents: &charge,
                segment_size: None,
                src_ip: None,
            };
            chemin.try_send(&transmit).unwrap();
        }
        arrives.0.load(Ordering::Relaxed)
    }

    #[test]
    fn un_chemin_direct_ne_perd_rien() {
        assert_eq!(emettre(0, 10_000), 10_000);
    }

    #[test]
    fn le_taux_demande_est_tenu() {
        for pour_mille in [10u16, 20, 50] {
            let arrives = emettre(pour_mille, 100_000);
            let perdus = 100_000 - arrives;
            let attendu = pour_mille as u64 * 100;
            let ecart = perdus.abs_diff(attendu);
            assert!(
                ecart * 10 < attendu,
                "{pour_mille} pour mille demandés, {perdus} perdus au lieu de {attendu}"
            );
        }
    }

    #[test]
    fn un_chemin_totalement_coupe_ne_laisse_rien_passer() {
        assert_eq!(emettre(1000, 5_000), 0);
        // Au-delà de la base, le taux est ramené à la coupure totale.
        assert_eq!(emettre(u16::MAX, 5_000), 0);
    }

    #[test]
    fn les_pertes_ne_tombent_pas_en_cadence() {
        // Une perte tous les cent paquets pile épouserait n'importe quel
        // rythme d'émission et masquerait les erreurs de fenêtre.
        let chemin = CheminDegrade::nouveau(Arc::new(Compteur(AtomicU64::new(0))), 100);
        let jetes: Vec<bool> = (0..2000).map(|_| chemin.doit_jeter()).collect();
        let ecarts: Vec<usize> = jetes
            .iter()
            .enumerate()
            .filter(|(_, jete)| **jete)
            .map(|(rang, _)| rang)
            .collect::<Vec<_>>()
            .windows(2)
            .map(|paire| paire[1] - paire[0])
            .collect();

        assert!(ecarts.len() > 100, "pas assez de pertes pour conclure");
        let distincts: std::collections::HashSet<_> = ecarts.iter().collect();
        assert!(
            distincts.len() > 5,
            "les pertes tombent toujours au même écart : {distincts:?}"
        );
    }
}
