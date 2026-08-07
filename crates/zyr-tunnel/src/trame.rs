//! Repérage du canal en tête de chaque datagramme.
//!
//! Un seul octet suffit : le budget de taille de paquet en tient compte,
//! et chaque octet pris ici est un octet de moins pour la vidéo.

use crate::canal::{CanalDatagramme, CanalInconnu};

/// Datagramme mal formé.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErreurTrame {
    Vide,
    Canal(CanalInconnu),
}

impl std::fmt::Display for ErreurTrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErreurTrame::Vide => write!(f, "datagramme vide"),
            ErreurTrame::Canal(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ErreurTrame {}

/// Préfixe la charge utile de son canal.
pub fn encoder(canal: CanalDatagramme, charge: &[u8]) -> Vec<u8> {
    let mut trame = Vec::with_capacity(1 + charge.len());
    trame.push(canal.identifiant());
    trame.extend_from_slice(charge);
    trame
}

/// Sépare le canal de la charge utile.
pub fn decoder(trame: &[u8]) -> Result<(CanalDatagramme, &[u8]), ErreurTrame> {
    let (tete, charge) = trame.split_first().ok_or(ErreurTrame::Vide)?;
    let canal = CanalDatagramme::depuis_identifiant(*tete).map_err(ErreurTrame::Canal)?;
    Ok((canal, charge))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn une_charge_fait_l_aller_retour_intacte() {
        for canal in CanalDatagramme::TOUS {
            let charge = b"paquet video quelconque";
            let trame = encoder(canal, charge);
            let (relu, contenu) = decoder(&trame).unwrap();
            assert_eq!(relu, canal);
            assert_eq!(contenu, charge);
        }
    }

    #[test]
    fn le_surcout_correspond_a_ce_que_le_budget_retranche() {
        // Si l'en-tête grossissait sans que le budget suive, les paquets
        // dépasseraient la taille annoncée et se fragmenteraient.
        let charge = vec![0u8; 1300];
        let trame = encoder(CanalDatagramme::Video, &charge);
        let surcout = u16::try_from(trame.len() - charge.len()).unwrap();
        assert_eq!(surcout, zyr_transport::mtu::SURCOUT_MUX);
    }

    #[test]
    fn une_charge_vide_reste_transportable() {
        let trame = encoder(CanalDatagramme::Controle, &[]);
        let (canal, contenu) = decoder(&trame).unwrap();
        assert_eq!(canal, CanalDatagramme::Controle);
        assert!(contenu.is_empty());
    }

    #[test]
    fn les_trames_invalides_sont_refusees() {
        assert_eq!(decoder(&[]), Err(ErreurTrame::Vide));
        assert!(matches!(decoder(&[0, 1, 2]), Err(ErreurTrame::Canal(_))));
        assert!(matches!(decoder(&[99]), Err(ErreurTrame::Canal(_))));
    }
}
