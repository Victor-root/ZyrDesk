//! Circulation des octets entre les moteurs et le tunnel.
//!
//! Ce module ne sait pas de quel côté il se trouve. Il fait passer des
//! octets entre une extrémité locale, qui parle à un moteur en loopback,
//! et la connexion chiffrée. Les deux bouts du tunnel s'en servent avec
//! les mêmes règles ; seule l'assemblage diffère.

use std::io;
use std::net::SocketAddr;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::io::AsyncWriteExt;
use tokio::net::{TcpStream, UdpSocket};
use zyr_proto::net::EnginePorts;
use zyr_transport::{Connexion, ErreurDatagramme, FluxEnvoi, FluxReception};

use crate::canal::{CanalDatagramme, CanalFlux};
use crate::trame;

/// Aucun datagramme UDP ne peut dépasser cette taille.
///
/// Le tampon est dimensionné pour ne jamais tronquer : une troncature
/// passerait pour un paquet valide et corromprait silencieusement le flux.
const TAMPON: usize = 65_535;

/// Compteurs du tunnel, relevés par le banc de mesure.
#[derive(Debug, Default)]
pub struct Statistiques {
    vers_tunnel: AtomicU64,
    vers_moteur: AtomicU64,
    trop_gros: AtomicU64,
    sans_destinataire: AtomicU64,
    illisibles: AtomicU64,
}

/// Relevé instantané des compteurs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Releve {
    pub vers_tunnel: u64,
    pub vers_moteur: u64,
    /// Paquets refusés parce qu'ils dépassaient ce que le chemin accepte.
    /// Non nul, c'est que la taille de paquet demandée au moteur est trop
    /// grande pour le chemin.
    pub trop_gros: u64,
    /// Paquets arrivés pour un canal sur lequel le moteur local ne s'est
    /// pas encore manifesté.
    pub sans_destinataire: u64,
    /// Datagrammes dont l'en-tête ne désigne aucun canal connu.
    pub illisibles: u64,
}

impl Statistiques {
    fn incrementer(compteur: &AtomicU64) {
        compteur.fetch_add(1, Ordering::Relaxed);
    }

    pub fn releve(&self) -> Releve {
        Releve {
            vers_tunnel: self.vers_tunnel.load(Ordering::Relaxed),
            vers_moteur: self.vers_moteur.load(Ordering::Relaxed),
            trop_gros: self.trop_gros.load(Ordering::Relaxed),
            sans_destinataire: self.sans_destinataire.load(Ordering::Relaxed),
            illisibles: self.illisibles.load(Ordering::Relaxed),
        }
    }
}

/// Extrémité UDP d'un canal : d'un côté le moteur, de l'autre le tunnel.
#[derive(Debug)]
pub struct PortMoteur {
    socket: UdpSocket,
    /// Où joindre le moteur.
    moteur: Mutex<Option<SocketAddr>>,
    /// Côté client, le moteur choisit son port source et peut en changer
    /// d'une session à l'autre : l'adresse se relit à chaque paquet. Côté
    /// hôte, elle est fixée par le port d'écoute du moteur et ne bouge pas.
    suit_la_source: bool,
}

impl PortMoteur {
    /// Extrémité côté hôte : le moteur écoute à une adresse connue.
    pub async fn vers_moteur(moteur: SocketAddr) -> io::Result<Self> {
        let socket = UdpSocket::bind(SocketAddr::new(moteur.ip(), 0)).await?;
        Ok(Self {
            socket,
            moteur: Mutex::new(Some(moteur)),
            suit_la_source: false,
        })
    }

    /// Extrémité côté client : le moteur vient à nous, sur le port qu'il
    /// croit être celui de l'hôte distant.
    pub async fn depuis_moteur(ecoute: SocketAddr) -> io::Result<Self> {
        let socket = UdpSocket::bind(ecoute).await?;
        Ok(Self {
            socket,
            moteur: Mutex::new(None),
            suit_la_source: true,
        })
    }

    pub fn adresse_locale(&self) -> io::Result<SocketAddr> {
        self.socket.local_addr()
    }

    fn destination(&self) -> Option<SocketAddr> {
        *self.moteur.lock().expect("verrou d'adresse du moteur")
    }

    /// Attend un paquet du moteur.
    pub async fn recevoir(&self, tampon: &mut [u8]) -> io::Result<usize> {
        let (lus, source) = self.socket.recv_from(tampon).await?;
        if self.suit_la_source {
            *self.moteur.lock().expect("verrou d'adresse du moteur") = Some(source);
        }
        Ok(lus)
    }

    /// Transmet au moteur ce qui sort du tunnel.
    ///
    /// Retourne `false` si le moteur ne s'est pas encore manifesté sur ce
    /// canal : il n'y a alors personne à qui remettre le paquet.
    pub async fn envoyer(&self, charge: &[u8]) -> io::Result<bool> {
        let Some(destination) = self.destination() else {
            return Ok(false);
        };
        self.socket.send_to(charge, destination).await?;
        Ok(true)
    }
}

/// Les trois extrémités UDP d'un côté du tunnel.
///
/// Construites ensemble pour que chaque canal tombe forcément sur le bon
/// port du moteur.
#[derive(Debug)]
pub struct PortsDatagramme([PortMoteur; CanalDatagramme::TOUS.len()]);

impl PortsDatagramme {
    /// Côté hôte : chaque canal parle au port correspondant du moteur.
    pub async fn vers_moteur(moteur: std::net::IpAddr, ports: EnginePorts) -> io::Result<Self> {
        Self::monter(ports, |port| async move {
            PortMoteur::vers_moteur(SocketAddr::new(moteur, port)).await
        })
        .await
    }

    /// Côté client : chaque canal écoute là où le moteur croit joindre l'hôte.
    pub async fn depuis_moteur(ecoute: std::net::IpAddr, ports: EnginePorts) -> io::Result<Self> {
        Self::monter(ports, |port| async move {
            PortMoteur::depuis_moteur(SocketAddr::new(ecoute, port)).await
        })
        .await
    }

    async fn monter<F, T>(ports: EnginePorts, mut ouvrir: F) -> io::Result<Self>
    where
        F: FnMut(u16) -> T,
        T: Future<Output = io::Result<PortMoteur>>,
    {
        let mut montes = Vec::with_capacity(CanalDatagramme::TOUS.len());
        for canal in CanalDatagramme::TOUS {
            montes.push(ouvrir(canal.port(ports)).await?);
        }
        Ok(Self(
            montes.try_into().expect("un port monté par canal connu"),
        ))
    }

    pub fn port(&self, canal: CanalDatagramme) -> &PortMoteur {
        &self.0[canal.rang()]
    }

    pub fn tous(&self) -> impl Iterator<Item = (CanalDatagramme, &PortMoteur)> {
        CanalDatagramme::TOUS.into_iter().zip(self.0.iter())
    }
}

/// Annonce le canal en tête d'un flux fiable.
pub async fn annoncer(envoi: &mut FluxEnvoi, canal: CanalFlux) -> io::Result<()> {
    envoi.write_all(&[canal.identifiant()]).await?;
    Ok(())
}

/// Lit l'annonce de canal en tête d'un flux fiable.
pub async fn lire_annonce(reception: &mut FluxReception) -> io::Result<CanalFlux> {
    let mut tete = [0u8; 1];
    reception
        .read_exact(&mut tete)
        .await
        .map_err(io::Error::other)?;
    CanalFlux::depuis_identifiant(tete[0]).map_err(io::Error::other)
}

/// Fait circuler les octets entre une connexion locale et un flux du tunnel.
///
/// Chaque sens s'arrête à sa propre fin de flux, sans couper l'autre : un
/// moteur qui a fini de parler attend encore la réponse.
pub async fn relayer_flux(
    mut local: TcpStream,
    mut envoi: FluxEnvoi,
    mut reception: FluxReception,
) -> io::Result<()> {
    let (mut lecture_locale, mut ecriture_locale) = local.split();

    let montant = async {
        tokio::io::copy(&mut lecture_locale, &mut envoi).await?;
        envoi.shutdown().await
    };
    let descendant = async {
        tokio::io::copy(&mut reception, &mut ecriture_locale).await?;
        ecriture_locale.shutdown().await
    };

    tokio::try_join!(montant, descendant)?;
    Ok(())
}

/// Porte vers le tunnel tout ce que le moteur émet sur un canal.
pub async fn collecter_datagrammes(
    canal: CanalDatagramme,
    port: &PortMoteur,
    connexion: &Connexion,
    stats: &Statistiques,
) -> io::Result<()> {
    let mut tampon = vec![0u8; TAMPON];
    loop {
        let lus = port.recevoir(&mut tampon).await?;
        let trame = trame::encoder(canal, &tampon[..lus]);
        match connexion.envoyer_datagramme(trame.into()) {
            Ok(()) => Statistiques::incrementer(&stats.vers_tunnel),
            // Le chemin s'est resserré depuis que la taille de paquet a
            // été demandée au moteur. Jeter vaut mieux que fragmenter :
            // la correction d'erreur du protocole vidéo est faite pour ça.
            Err(ErreurDatagramme::TropGros) => Statistiques::incrementer(&stats.trop_gros),
            Err(e) => return Err(io::Error::other(e)),
        }
    }
}

/// Remet aux moteurs les datagrammes qui sortent du tunnel.
///
/// Un seul lecteur pour les trois canaux : les datagrammes d'une
/// connexion arrivent par une file unique, l'en-tête dit à qui les rendre.
pub async fn distribuer_datagrammes(
    connexion: &Connexion,
    ports: &PortsDatagramme,
    stats: &Statistiques,
) -> io::Result<()> {
    loop {
        let recu = connexion
            .lire_datagramme()
            .await
            .map_err(io::Error::other)?;
        let Ok((canal, charge)) = trame::decoder(&recu) else {
            Statistiques::incrementer(&stats.illisibles);
            continue;
        };
        match ports.port(canal).envoyer(charge).await? {
            true => Statistiques::incrementer(&stats.vers_moteur),
            false => Statistiques::incrementer(&stats.sans_destinataire),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn locale(port: u16) -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], port))
    }

    #[tokio::test]
    async fn un_port_cote_hote_vise_le_moteur_sans_attendre() {
        let moteur = UdpSocket::bind(locale(0)).await.unwrap();
        let adresse = moteur.local_addr().unwrap();

        let port = PortMoteur::vers_moteur(adresse).await.unwrap();
        assert!(port.envoyer(b"ping").await.unwrap());

        let mut recu = [0u8; 16];
        let (lus, _) = moteur.recv_from(&mut recu).await.unwrap();
        assert_eq!(&recu[..lus], b"ping");
    }

    #[tokio::test]
    async fn un_port_cote_client_attend_que_le_moteur_se_manifeste() {
        let port = PortMoteur::depuis_moteur(locale(0)).await.unwrap();
        let adresse = port.adresse_locale().unwrap();

        // Rien n'est encore arrivé : il n'y a personne à qui répondre.
        assert!(!port.envoyer(b"image").await.unwrap());

        let moteur = UdpSocket::bind(locale(0)).await.unwrap();
        moteur.send_to(b"ping", adresse).await.unwrap();

        let mut recu = [0u8; 16];
        assert_eq!(port.recevoir(&mut recu).await.unwrap(), 4);
        assert!(port.envoyer(b"image").await.unwrap());

        let (lus, _) = moteur.recv_from(&mut recu).await.unwrap();
        assert_eq!(&recu[..lus], b"image");
    }

    #[tokio::test]
    async fn un_port_cote_client_suit_le_moteur_qui_change_de_source() {
        let port = PortMoteur::depuis_moteur(locale(0)).await.unwrap();
        let adresse = port.adresse_locale().unwrap();
        let mut recu = [0u8; 16];

        let premier = UdpSocket::bind(locale(0)).await.unwrap();
        premier.send_to(b"a", adresse).await.unwrap();
        port.recevoir(&mut recu).await.unwrap();

        // Nouvelle session du moteur : nouveau port source. Les réponses
        // doivent suivre, sinon elles partent vers un port mort.
        let second = UdpSocket::bind(locale(0)).await.unwrap();
        second.send_to(b"b", adresse).await.unwrap();
        port.recevoir(&mut recu).await.unwrap();

        port.envoyer(b"image").await.unwrap();
        let (lus, _) = second.recv_from(&mut recu).await.unwrap();
        assert_eq!(&recu[..lus], b"image");
    }

    #[tokio::test]
    async fn chaque_canal_tombe_sur_le_port_attendu_du_moteur() {
        let ports = EnginePorts::new(42500).unwrap();
        let montes = PortsDatagramme::depuis_moteur([127, 0, 0, 1].into(), ports)
            .await
            .unwrap();
        for (canal, port) in montes.tous() {
            assert_eq!(port.adresse_locale().unwrap().port(), canal.port(ports));
        }
    }
}
