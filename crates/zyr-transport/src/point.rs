//! Établissement de la connexion chiffrée entre deux appareils.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use quinn::crypto::rustls::{QuicClientConfig, QuicServerConfig};
use quinn::{ClientConfig, Connection, Endpoint, ServerConfig, TransportConfig};

use crate::congestion::ProfilMedia;
use crate::identite::{Empreinte, Identite, PairEpingle};

/// Protocole annoncé à la négociation, pour ne pas répondre à autre chose.
const PROTOCOLE: &[u8] = b"zyrdesk/1";

/// Au-delà, la session est considérée perdue.
const INACTIVITE_MAXIMALE: Duration = Duration::from_secs(30);

/// Maintien de la correspondance dans les équipements réseau traversés.
const INTERVALLE_MAINTIEN: Duration = Duration::from_secs(5);

/// File d'émission volontairement courte.
///
/// Sous congestion, mieux vaut jeter une image périmée que la garder :
/// elle arriverait trop tard pour être affichée, et tout ce qui la suit
/// aurait pris son retard. La correction d'erreur du protocole vidéo est
/// faite pour combler ces trous.
const FILE_EMISSION: usize = 128 * 1024;

/// File de réception, dimensionnée pour absorber une rafale d'images.
const FILE_RECEPTION: usize = 8 * 1024 * 1024;

#[derive(Debug)]
pub enum ErreurPoint {
    Configuration(String),
    Reseau(std::io::Error),
    Connexion(String),
    Ferme,
}

impl std::fmt::Display for ErreurPoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErreurPoint::Configuration(e) => write!(f, "configuration du transport : {e}"),
            ErreurPoint::Reseau(e) => write!(f, "erreur réseau : {e}"),
            ErreurPoint::Connexion(e) => write!(f, "connexion impossible : {e}"),
            ErreurPoint::Ferme => write!(f, "le point de connexion est fermé"),
        }
    }
}

impl std::error::Error for ErreurPoint {}

impl From<std::io::Error> for ErreurPoint {
    fn from(e: std::io::Error) -> Self {
        ErreurPoint::Reseau(e)
    }
}

/// Réglages communs aux deux extrémités.
fn transport(profil: ProfilMedia) -> Arc<TransportConfig> {
    let mut config = TransportConfig::default();
    config.congestion_controller_factory(Arc::new(profil));
    config.datagram_send_buffer_size(FILE_EMISSION);
    config.datagram_receive_buffer_size(Some(FILE_RECEPTION));
    config.max_idle_timeout(Some(
        INACTIVITE_MAXIMALE
            .try_into()
            .expect("délai d'inactivité représentable"),
    ));
    config.keep_alive_interval(Some(INTERVALLE_MAINTIEN));
    Arc::new(config)
}

fn fournisseur() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// Une extrémité du tunnel.
pub struct PointTerminal {
    endpoint: Endpoint,
}

impl PointTerminal {
    /// Extrémité qui attend la connexion de l'autre appareil.
    pub fn hote(
        identite: &Identite,
        pair: Empreinte,
        profil: ProfilMedia,
        ecoute: SocketAddr,
    ) -> Result<Self, ErreurPoint> {
        let mut tls = rustls::ServerConfig::builder_with_provider(fournisseur())
            .with_protocol_versions(&[&rustls::version::TLS13])
            .map_err(|e| ErreurPoint::Configuration(e.to_string()))?
            .with_client_cert_verifier(Arc::new(PairEpingle::nouveau(pair)))
            .with_single_cert(vec![identite.certificat().clone()], identite.cle())
            .map_err(|e| ErreurPoint::Configuration(e.to_string()))?;
        tls.alpn_protocols = vec![PROTOCOLE.to_vec()];

        let quic = QuicServerConfig::try_from(tls)
            .map_err(|e| ErreurPoint::Configuration(e.to_string()))?;
        let mut config = ServerConfig::with_crypto(Arc::new(quic));
        config.transport_config(transport(profil));

        Ok(Self {
            endpoint: Endpoint::server(config, ecoute)?,
        })
    }

    /// Extrémité qui va vers l'autre appareil.
    pub fn client(
        identite: &Identite,
        pair: Empreinte,
        profil: ProfilMedia,
        ecoute: SocketAddr,
    ) -> Result<Self, ErreurPoint> {
        let mut tls = rustls::ClientConfig::builder_with_provider(fournisseur())
            .with_protocol_versions(&[&rustls::version::TLS13])
            .map_err(|e| ErreurPoint::Configuration(e.to_string()))?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(PairEpingle::nouveau(pair)))
            .with_client_auth_cert(vec![identite.certificat().clone()], identite.cle())
            .map_err(|e| ErreurPoint::Configuration(e.to_string()))?;
        tls.alpn_protocols = vec![PROTOCOLE.to_vec()];

        let quic = QuicClientConfig::try_from(tls)
            .map_err(|e| ErreurPoint::Configuration(e.to_string()))?;
        let mut config = ClientConfig::new(Arc::new(quic));
        config.transport_config(transport(profil));

        let mut endpoint = Endpoint::client(ecoute)?;
        endpoint.set_default_client_config(config);
        Ok(Self { endpoint })
    }

    pub fn adresse_locale(&self) -> Result<SocketAddr, ErreurPoint> {
        Ok(self.endpoint.local_addr()?)
    }

    /// Va vers l'appareil distant.
    ///
    /// Le nom demandé n'est pas vérifié : c'est l'empreinte qui fait foi.
    ///
    /// Le retour n'annonce que la moitié de l'autorisation : celle de
    /// l'hôte par le client. Le certificat du client part en dernier et
    /// n'est jugé qu'ensuite ; un client refusé voit donc sa connexion
    /// réussir puis se rompre aussitôt. Il ne faut annoncer la session
    /// établie qu'après le premier échange réussi.
    pub async fn connecter(&self, distant: SocketAddr) -> Result<Connexion, ErreurPoint> {
        let connexion = self
            .endpoint
            .connect(distant, "zyrdesk")
            .map_err(|e| ErreurPoint::Connexion(e.to_string()))?
            .await
            .map_err(|e| ErreurPoint::Connexion(e.to_string()))?;
        Ok(Connexion::nouvelle(connexion))
    }

    /// Attend la connexion de l'appareil distant.
    pub async fn accepter(&self) -> Result<Connexion, ErreurPoint> {
        let entrante = self.endpoint.accept().await.ok_or(ErreurPoint::Ferme)?;
        let connexion = entrante
            .await
            .map_err(|e| ErreurPoint::Connexion(e.to_string()))?;
        Ok(Connexion::nouvelle(connexion))
    }

    /// Attend la fin des connexions en cours.
    pub async fn fermer(&self) {
        self.endpoint.close(0u32.into(), b"fin de session");
        self.endpoint.wait_idle().await;
    }
}

/// Connexion établie avec l'appareil distant.
#[derive(Clone)]
pub struct Connexion {
    interne: Connection,
}

impl Connexion {
    fn nouvelle(interne: Connection) -> Self {
        Self { interne }
    }

    /// Charge utile transportable en un datagramme, pour le chemin actuel.
    ///
    /// Évolue avec la découverte de MTU : c'est elle qui décide de la
    /// taille de paquet demandée au moteur.
    pub fn datagramme_utilisable(&self) -> Option<u16> {
        self.interne
            .max_datagram_size()
            .map(|t| u16::try_from(t).unwrap_or(u16::MAX))
    }

    pub fn aller_retour(&self) -> Duration {
        self.interne.rtt()
    }

    pub fn interne(&self) -> &Connection {
        &self.interne
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Au-delà, une connexion qui devait être rompue ne l'a pas été.
    const PATIENCE: Duration = Duration::from_secs(5);

    fn locale() -> SocketAddr {
        "127.0.0.1:0".parse().unwrap()
    }

    /// Les deux extrémités et leur connexion, maintenues en vie ensemble.
    struct Duo {
        _hote: PointTerminal,
        _client: PointTerminal,
        cote_hote: Connexion,
        cote_client: Connexion,
    }

    /// Monte les deux extrémités et les met en relation.
    async fn paire() -> Duo {
        let id_hote = Identite::generer().unwrap();
        let id_client = Identite::generer().unwrap();
        let profil = ProfilMedia::default();

        let hote = PointTerminal::hote(&id_hote, id_client.empreinte(), profil, locale()).unwrap();
        let adresse = hote.adresse_locale().unwrap();
        let client =
            PointTerminal::client(&id_client, id_hote.empreinte(), profil, locale()).unwrap();

        let (cote_hote, cote_client) = tokio::join!(hote.accepter(), client.connecter(adresse));
        Duo {
            _hote: hote,
            _client: client,
            cote_hote: cote_hote.unwrap(),
            cote_client: cote_client.unwrap(),
        }
    }

    #[tokio::test]
    async fn deux_appareils_qui_se_connaissent_se_connectent() {
        let duo = paire().await;
        assert!(duo.cote_client.datagramme_utilisable().is_some());
        assert!(duo.cote_hote.datagramme_utilisable().is_some());
    }

    #[tokio::test]
    async fn le_datagramme_utilisable_permet_un_paquet_video() {
        let duo = paire().await;
        let utilisable = duo.cote_client.datagramme_utilisable().unwrap();
        let taille = crate::mtu::taille_paquet(utilisable)
            .expect("un chemin local doit permettre un paquet vidéo");
        assert!(taille.octets >= crate::mtu::TAILLE_MINIMALE);
    }

    #[tokio::test]
    async fn un_datagramme_traverse_le_tunnel() {
        let duo = paire().await;
        duo.cote_client
            .interne()
            .send_datagram(b"image".to_vec().into())
            .unwrap();
        let recu = duo.cote_hote.interne().read_datagram().await.unwrap();
        assert_eq!(&recu[..], b"image");
    }

    #[tokio::test]
    async fn un_flux_fiable_traverse_le_tunnel() {
        let duo = paire().await;
        let recepteur = duo.cote_hote.clone();
        let attente = tokio::spawn(async move {
            let (_, mut lecture) = recepteur.interne().accept_bi().await.unwrap();
            lecture.read_to_end(64).await.unwrap()
        });

        let (mut ecriture, _) = duo.cote_client.interne().open_bi().await.unwrap();
        ecriture.write_all(b"negociation").await.unwrap();
        ecriture.finish().unwrap();

        assert_eq!(attente.await.unwrap(), b"negociation");
    }

    #[tokio::test]
    async fn un_appareil_inconnu_est_refuse() {
        let id_hote = Identite::generer().unwrap();
        let id_attendu = Identite::generer().unwrap();
        let id_intrus = Identite::generer().unwrap();
        let profil = ProfilMedia::default();

        // L'hôte n'attend qu'un appareil précis.
        let hote = PointTerminal::hote(&id_hote, id_attendu.empreinte(), profil, locale()).unwrap();
        let adresse = hote.adresse_locale().unwrap();
        let intrus =
            PointTerminal::client(&id_intrus, id_hote.empreinte(), profil, locale()).unwrap();

        let (cote_hote, tentative) = tokio::join!(hote.accepter(), intrus.connecter(adresse));

        assert!(cote_hote.is_err(), "l'hôte a accepté un appareil inconnu");

        // L'intrus a pu croire un instant sa connexion établie : il
        // présente son certificat en dernier et n'apprend le refus qu'à
        // la rupture. Rien n'y aura circulé.
        if let Ok(connexion) = tentative {
            let rompue = tokio::time::timeout(PATIENCE, connexion.interne().closed()).await;
            assert!(rompue.is_ok(), "l'intrus a conservé sa connexion");
        }
    }

    #[tokio::test]
    async fn un_hote_usurpe_est_refuse() {
        let id_hote = Identite::generer().unwrap();
        let id_client = Identite::generer().unwrap();
        let autre = Identite::generer().unwrap();
        let profil = ProfilMedia::default();

        let hote = PointTerminal::hote(&id_hote, id_client.empreinte(), profil, locale()).unwrap();
        let adresse = hote.adresse_locale().unwrap();
        // Le client attend une empreinte que l'hôte ne peut pas présenter.
        let client =
            PointTerminal::client(&id_client, autre.empreinte(), profil, locale()).unwrap();

        let (_, tentative) = tokio::join!(hote.accepter(), client.connecter(adresse));
        assert!(tentative.is_err(), "le client a accepté un hôte usurpé");
    }
}
