//! Budget de taille de paquet vidéo.
//!
//! Un paquet vidéo trop gros se fragmente en route, et la fragmentation
//! coûte de la latence : un seul fragment perdu détruit le paquet entier.
//! La taille demandée au moteur hôte doit donc tenir dans ce que le
//! tunnel peut transporter d'un bloc.
//!
//! Le calcul part de la taille réellement annoncée par le transport
//! plutôt que d'une estimation du surcoût QUIC. Les en-têtes varient avec
//! la longueur des identifiants de connexion et l'état du chemin : les
//! deviner reviendrait à refaire, moins bien, un calcul que le transport
//! tient déjà à jour.

/// En-tête ZyrDesk devant chaque datagramme : l'identifiant de canal.
pub const SURCOUT_MUX: u16 = 1;

/// En-têtes ajoutés par le protocole des moteurs à chaque paquet vidéo.
///
/// Estimation en attendant la mesure réelle par capture réseau. La
/// vérification V5 du jalon M1 doit la confirmer ; toute erreur ici se
/// paie en fragmentation.
pub const EN_TETE_MOTEUR_ESTIME: u16 = 28;

/// Marge conservée tant que l'en-tête réel n'est pas mesuré.
///
/// Se réduira à quelques octets une fois la vérification V5 rendue.
pub const MARGE: u16 = 32;

/// Plafond : la valeur qu'emploie le moteur client en réseau local.
///
/// Aller au-delà n'apporte rien et rapproche du seuil de fragmentation.
pub const TAILLE_NOMINALE: u16 = 1392;

/// Plancher imposé par le moteur client.
///
/// En dessous, il refuse la valeur. Sa propre valeur pour un réseau
/// distant est 1024 : rester au-dessus garde le mode « local », c'est-à-
/// dire sa détection de réseau distant désactivée, puisque c'est nous qui
/// gérons le chemin.
pub const TAILLE_MINIMALE: u16 = 1025;

/// Le chemin ne peut pas porter un paquet vidéo exploitable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheminTropEtroit {
    pub datagramme_disponible: u16,
    pub datagramme_requis: u16,
}

impl std::fmt::Display for CheminTropEtroit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "chemin trop étroit : {} octets utilisables, {} nécessaires",
            self.datagramme_disponible, self.datagramme_requis
        )
    }
}

impl std::error::Error for CheminTropEtroit {}

/// Taille de paquet à demander au moteur hôte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaillePaquet {
    pub octets: u16,
    /// Vrai si le chemin a imposé une taille sous la valeur nominale.
    ///
    /// Sans conséquence sur le fonctionnement, mais mérite d'être
    /// journalisé : le débit de paquets augmente à mesure qu'elle baisse.
    pub reduite_par_le_chemin: bool,
}

/// Surcoût total retranché à la place utilisable du datagramme.
const SURCOUT_TOTAL: u16 = SURCOUT_MUX + EN_TETE_MOTEUR_ESTIME + MARGE;

/// Calcule la taille de paquet tenant dans le datagramme annoncé.
///
/// `datagramme_utilisable` est la charge utile qu'accepte le transport
/// sans fragmenter, telle qu'il la rapporte pour le chemin en cours.
pub fn taille_paquet(datagramme_utilisable: u16) -> Result<TaillePaquet, CheminTropEtroit> {
    let disponible = datagramme_utilisable.saturating_sub(SURCOUT_TOTAL);
    if disponible < TAILLE_MINIMALE {
        return Err(CheminTropEtroit {
            datagramme_disponible: datagramme_utilisable,
            datagramme_requis: TAILLE_MINIMALE + SURCOUT_TOTAL,
        });
    }
    Ok(TaillePaquet {
        octets: disponible.min(TAILLE_NOMINALE),
        reduite_par_le_chemin: disponible < TAILLE_NOMINALE,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_chemin_large_donne_la_taille_nominale() {
        let t = taille_paquet(TAILLE_NOMINALE + SURCOUT_TOTAL).unwrap();
        assert_eq!(t.octets, TAILLE_NOMINALE);
        assert!(!t.reduite_par_le_chemin);

        // Encore plus large : la valeur reste plafonnée.
        let t = taille_paquet(4000).unwrap();
        assert_eq!(t.octets, TAILLE_NOMINALE);
        assert!(!t.reduite_par_le_chemin);
    }

    #[test]
    fn un_chemin_ordinaire_reste_confortable() {
        // Chemin Ethernet courant : le transport annonce environ 1400
        // octets utilisables une fois son propre surcoût retranché.
        let t = taille_paquet(1400).unwrap();
        assert!(t.octets >= 1300, "{} octets seulement", t.octets);
        assert!(t.reduite_par_le_chemin);
    }

    #[test]
    fn un_chemin_etroit_reduit_sans_descendre_sous_le_plancher() {
        let juste = TAILLE_MINIMALE + SURCOUT_TOTAL;
        let t = taille_paquet(juste).unwrap();
        assert_eq!(t.octets, TAILLE_MINIMALE);
        assert!(t.reduite_par_le_chemin);
    }

    #[test]
    fn un_chemin_trop_etroit_est_refuse_plutot_que_rabote() {
        let trop_juste = TAILLE_MINIMALE + SURCOUT_TOTAL - 1;
        let e = taille_paquet(trop_juste).unwrap_err();
        assert_eq!(e.datagramme_disponible, trop_juste);
        assert!(e.datagramme_requis > trop_juste);

        assert!(taille_paquet(0).is_err());
        assert!(taille_paquet(500).is_err());
    }

    #[test]
    fn la_taille_rendue_tient_toujours_dans_le_datagramme() {
        for datagramme in (TAILLE_MINIMALE + SURCOUT_TOTAL)..=2000 {
            let t = taille_paquet(datagramme).unwrap();
            let occupe = t.octets + SURCOUT_TOTAL;
            assert!(
                occupe <= datagramme,
                "{} octets occupés pour {} disponibles",
                occupe,
                datagramme
            );
            assert!(t.octets >= TAILLE_MINIMALE);
        }
    }
}
