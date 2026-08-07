//! Le tunnel complet, entre deux faux moteurs.
//!
//! Les vrais moteurs ne sont pas nécessaires pour vérifier ce qui est en
//! jeu ici : qu'un octet déposé d'un côté ressorte identique de l'autre,
//! sur le bon port, dans les deux sens, sans qu'aucune des deux
//! extrémités ait à savoir qu'un tunnel existe.
//!
//! Les deux côtés tournent dans le même processus, sur deux adresses
//! loopback distinctes : le moteur hôte sur 127.0.0.1, les écoutes du
//! côté client sur l'adresse dédiée à l'appareil. C'est exactement le
//! schéma d'adressage prévu pour de vrai.

use std::future::Future;
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use zyr_proto::net::{EnginePorts, device_loopback_addr};
use zyr_transport::{Identite, PointTerminal, ProfilMedia};
use zyr_tunnel::Tunnel;

/// Au-delà, c'est que rien ne passe.
const PATIENCE: Duration = Duration::from_secs(10);

/// Là où le moteur hôte écoute, comme sur une vraie machine.
const MOTEUR: IpAddr = IpAddr::V4(std::net::Ipv4Addr::LOCALHOST);

async fn avant_la_fin<T>(travail: impl Future<Output = T>) -> T {
    tokio::time::timeout(PATIENCE, travail)
        .await
        .expect("le tunnel n'a rien laissé passer")
}

/// Le tunnel monté des deux côtés, maintenu en vie le temps du test.
///
/// Tout est lâché ensemble à la fin : les pompes s'arrêtent avec lui.
struct Banc {
    _points: (PointTerminal, PointTerminal),
    _hote: Tunnel,
    client: Tunnel,
    /// Adresse à laquelle le moteur client croit joindre l'hôte.
    cote_client: IpAddr,
    ports: EnginePorts,
}

impl Banc {
    async fn monter(base: u16, appareil: u16) -> Self {
        let ports = EnginePorts::new(base).unwrap();
        let cote_client = IpAddr::V4(device_loopback_addr(appareil).unwrap());

        let id_hote = Identite::generer().unwrap();
        let id_client = Identite::generer().unwrap();
        let profil = ProfilMedia::default();
        let ephemere = SocketAddr::new(MOTEUR, 0);

        let point_hote =
            PointTerminal::hote(&id_hote, id_client.empreinte(), profil, ephemere).unwrap();
        let rendez_vous = point_hote.adresse_locale().unwrap();
        let point_client =
            PointTerminal::client(&id_client, id_hote.empreinte(), profil, ephemere).unwrap();

        let (cote_hote, cote_client_connexion) =
            tokio::join!(point_hote.accepter(), point_client.connecter(rendez_vous));

        let hote = Tunnel::hote(cote_hote.unwrap(), MOTEUR, ports)
            .await
            .unwrap();
        let client = Tunnel::client(cote_client_connexion.unwrap(), cote_client, ports)
            .await
            .unwrap();

        Self {
            _points: (point_hote, point_client),
            _hote: hote,
            client,
            cote_client,
            ports,
        }
    }

    /// Adresse d'un port du moteur, telle que le moteur client la voit.
    fn vue_du_client(&self, port: u16) -> SocketAddr {
        SocketAddr::new(self.cote_client, port)
    }
}

/// Faux moteur hôte qui renvoie ce qu'on lui écrit, en TCP.
async fn moteur_tcp(port: u16) {
    let liaison = TcpListener::bind(SocketAddr::new(MOTEUR, port))
        .await
        .unwrap();
    tokio::spawn(async move {
        while let Ok((mut flux, _)) = liaison.accept().await {
            tokio::spawn(async move {
                let (mut lecture, mut ecriture) = flux.split();
                let _ = tokio::io::copy(&mut lecture, &mut ecriture).await;
                let _ = ecriture.shutdown().await;
            });
        }
    });
}

/// Faux moteur hôte qui répond en UDP, en annonçant sur quel port il a
/// été joint : c'est ainsi qu'on vérifie qu'aucun canal n'en croise un autre.
async fn moteur_udp(port: u16) {
    let socket = UdpSocket::bind(SocketAddr::new(MOTEUR, port))
        .await
        .unwrap();
    tokio::spawn(async move {
        let mut tampon = [0u8; 2048];
        while let Ok((lus, source)) = socket.recv_from(&mut tampon).await {
            let reponse = format!("{port}:{}", String::from_utf8_lossy(&tampon[..lus]));
            let _ = socket.send_to(reponse.as_bytes(), source).await;
        }
    });
}

#[tokio::test]
async fn un_flux_fiable_traverse_le_tunnel_dans_les_deux_sens() {
    let banc = Banc::monter(42100, 0).await;
    moteur_tcp(banc.ports.http()).await;

    let mut flux = avant_la_fin(TcpStream::connect(banc.vue_du_client(banc.ports.http())))
        .await
        .unwrap();
    flux.write_all(b"appairage").await.unwrap();
    flux.shutdown().await.unwrap();

    let mut recu = Vec::new();
    avant_la_fin(flux.read_to_end(&mut recu)).await.unwrap();
    assert_eq!(recu, b"appairage");
}

#[tokio::test]
async fn un_datagramme_traverse_le_tunnel_dans_les_deux_sens() {
    let banc = Banc::monter(42200, 1).await;
    moteur_udp(banc.ports.video()).await;

    let client = UdpSocket::bind(SocketAddr::new(banc.cote_client, 0))
        .await
        .unwrap();
    client
        .send_to(b"ping", banc.vue_du_client(banc.ports.video()))
        .await
        .unwrap();

    let mut recu = [0u8; 64];
    let (lus, _) = avant_la_fin(client.recv_from(&mut recu)).await.unwrap();
    assert_eq!(
        &recu[..lus],
        format!("{}:ping", banc.ports.video()).as_bytes()
    );
}

#[tokio::test]
async fn chaque_canal_arrive_sur_son_propre_port_du_moteur() {
    let banc = Banc::monter(42300, 2).await;
    for port in banc.ports.udp_ports() {
        moteur_udp(port).await;
    }

    // Les trois canaux partagent une seule file de datagrammes : si
    // l'en-tête était mal relu, la vidéo atterrirait dans l'audio.
    for port in banc.ports.udp_ports() {
        let client = UdpSocket::bind(SocketAddr::new(banc.cote_client, 0))
            .await
            .unwrap();
        client
            .send_to(b"ping", banc.vue_du_client(port))
            .await
            .unwrap();

        let mut recu = [0u8; 64];
        let (lus, _) = avant_la_fin(client.recv_from(&mut recu)).await.unwrap();
        assert_eq!(&recu[..lus], format!("{port}:ping").as_bytes());
    }
}

#[tokio::test]
async fn l_interface_web_du_moteur_reste_hors_du_tunnel() {
    let banc = Banc::monter(42400, 3).await;
    moteur_tcp(banc.ports.web_ui()).await;

    // Elle tourne bel et bien côté hôte, mais rien ne l'écoute côté
    // client : elle n'est joignable que depuis la machine qui l'héberge.
    assert!(
        TcpStream::connect(banc.vue_du_client(banc.ports.web_ui()))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn les_compteurs_suivent_ce_qui_circule() {
    let banc = Banc::monter(42600, 4).await;
    moteur_udp(banc.ports.audio()).await;
    assert_eq!(banc.client.releve(), zyr_tunnel::Releve::default());

    let client = UdpSocket::bind(SocketAddr::new(banc.cote_client, 0))
        .await
        .unwrap();
    client
        .send_to(b"ping", banc.vue_du_client(banc.ports.audio()))
        .await
        .unwrap();
    let mut recu = [0u8; 64];
    avant_la_fin(client.recv_from(&mut recu)).await.unwrap();

    let releve = banc.client.releve();
    assert_eq!(releve.vers_tunnel, 1);
    assert_eq!(releve.vers_moteur, 1);
    assert_eq!(releve.trop_gros, 0);
    assert_eq!(releve.illisibles, 0);
}
