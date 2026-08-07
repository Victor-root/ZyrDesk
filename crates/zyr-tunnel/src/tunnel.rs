//! Le tunnel en marche, d'un côté ou de l'autre.
//!
//! Les deux côtés font le même travail en miroir. Côté client, le moteur
//! croit joindre l'ordinateur distant : il trouve en fait des ports
//! locaux qui versent tout dans la connexion chiffrée. Côté hôte, ce qui
//! en ressort est remis au moteur en loopback, comme s'il venait du
//! réseau. Aucun des deux moteurs ne sait qu'il y a un tunnel.

use std::io;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use quinn::{RecvStream, SendStream};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinSet;
use zyr_proto::net::EnginePorts;
use zyr_transport::Connexion;

use crate::canal::{CanalDatagramme, CanalFlux};
use crate::pompe::{self, PortsDatagramme, Releve, Statistiques};

/// Un côté du tunnel, pompes en marche.
///
/// Tout s'arrête quand il est lâché : les pompes n'ont aucune raison de
/// survivre à la session qu'elles servent.
pub struct Tunnel {
    taches: JoinSet<io::Result<()>>,
    stats: Arc<Statistiques>,
}

impl Tunnel {
    /// Côté hôte : ce qui sort du tunnel est remis au moteur local.
    pub async fn hote(
        connexion: Connexion,
        moteur: IpAddr,
        ports: EnginePorts,
    ) -> io::Result<Self> {
        let datagrammes = Arc::new(PortsDatagramme::vers_moteur(moteur, ports).await?);
        let stats = Arc::new(Statistiques::default());
        let mut taches = pompes_datagramme(&connexion, &datagrammes, &stats);

        let vers_moteur = connexion.clone();
        taches.spawn(async move { servir_les_flux(&vers_moteur, moteur, ports).await });

        Ok(Self { taches, stats })
    }

    /// Côté client : ce que le moteur local émet part dans le tunnel.
    pub async fn client(
        connexion: Connexion,
        ecoute: IpAddr,
        ports: EnginePorts,
    ) -> io::Result<Self> {
        // Les écoutes sont ouvertes avant de rendre la main : le moteur
        // peut se présenter dès l'instant où la session lui est annoncée.
        let mut ecoutes = Vec::new();
        for canal in CanalFlux::TOUS {
            let Some(port) = canal.port(ports) else {
                continue;
            };
            let liaison = TcpListener::bind(SocketAddr::new(ecoute, port)).await?;
            ecoutes.push((canal, liaison));
        }

        let datagrammes = Arc::new(PortsDatagramme::depuis_moteur(ecoute, ports).await?);
        let stats = Arc::new(Statistiques::default());
        let mut taches = pompes_datagramme(&connexion, &datagrammes, &stats);

        for (canal, liaison) in ecoutes {
            let vers_tunnel = connexion.clone();
            taches.spawn(async move { porter_les_flux(canal, liaison, vers_tunnel).await });
        }

        Ok(Self { taches, stats })
    }

    pub fn releve(&self) -> Releve {
        self.stats.releve()
    }

    /// Attend l'arrêt du tunnel, et dit pourquoi il s'est arrêté.
    ///
    /// Les pompes tournent tant que la connexion tient : la première qui
    /// rend la main signale la fin de la session.
    pub async fn attendre(&mut self) -> io::Result<()> {
        match self.taches.join_next().await {
            Some(resultat) => resultat.map_err(io::Error::other)?,
            None => Ok(()),
        }
    }
}

/// Les pompes UDP, identiques des deux côtés.
fn pompes_datagramme(
    connexion: &Connexion,
    datagrammes: &Arc<PortsDatagramme>,
    stats: &Arc<Statistiques>,
) -> JoinSet<io::Result<()>> {
    let mut taches = JoinSet::new();

    // Un seul lecteur pour les trois canaux : les datagrammes d'une
    // connexion arrivent par une file unique.
    let connexion_lecture = connexion.clone();
    let ports_lecture = datagrammes.clone();
    let stats_lecture = stats.clone();
    taches.spawn(async move {
        pompe::distribuer_datagrammes(&connexion_lecture, &ports_lecture, &stats_lecture).await
    });

    for canal in CanalDatagramme::TOUS {
        let connexion = connexion.clone();
        let ports = datagrammes.clone();
        let stats = stats.clone();
        taches.spawn(async move {
            pompe::collecter_datagrammes(canal, ports.port(canal), &connexion, &stats).await
        });
    }

    taches
}

/// Remet au moteur les flux fiables qui arrivent du tunnel.
async fn servir_les_flux(
    connexion: &Connexion,
    moteur: IpAddr,
    ports: EnginePorts,
) -> io::Result<()> {
    let mut sessions = JoinSet::new();
    loop {
        let (envoi, reception) = connexion
            .interne()
            .accept_bi()
            .await
            .map_err(io::Error::other)?;

        // L'échec d'un flux reste sur ce flux : un appairage raté ne doit
        // pas emporter la session en cours.
        sessions.spawn(async move {
            let _ = remettre_au_moteur(envoi, reception, moteur, ports).await;
        });
        while sessions.try_join_next().is_some() {}
    }
}

async fn remettre_au_moteur(
    envoi: SendStream,
    mut reception: RecvStream,
    moteur: IpAddr,
    ports: EnginePorts,
) -> io::Result<()> {
    let canal = pompe::lire_annonce(&mut reception).await?;
    let port = canal
        .port(ports)
        .ok_or_else(|| io::Error::other(format!("le canal {canal:?} ne vise aucun moteur")))?;

    let local = TcpStream::connect(SocketAddr::new(moteur, port)).await?;
    local.set_nodelay(true)?;
    pompe::relayer_flux(local, envoi, reception).await
}

/// Porte dans le tunnel les connexions que le moteur local ouvre.
async fn porter_les_flux(
    canal: CanalFlux,
    ecoute: TcpListener,
    connexion: Connexion,
) -> io::Result<()> {
    let mut sessions = JoinSet::new();
    loop {
        let (local, _) = ecoute.accept().await?;
        local.set_nodelay(true)?;

        let connexion = connexion.clone();
        sessions.spawn(async move {
            let _ = porter_au_tunnel(canal, local, connexion).await;
        });
        while sessions.try_join_next().is_some() {}
    }
}

async fn porter_au_tunnel(
    canal: CanalFlux,
    local: TcpStream,
    connexion: Connexion,
) -> io::Result<()> {
    let (mut envoi, reception) = connexion
        .interne()
        .open_bi()
        .await
        .map_err(io::Error::other)?;
    pompe::annoncer(&mut envoi, canal).await?;
    pompe::relayer_flux(local, envoi, reception).await
}
