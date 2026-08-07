//! Identité cryptographique d'un appareil et épinglage du pair.
//!
//! Chaque extrémité présente un certificat qu'elle a signé elle-même. Une
//! autorité de certification n'aurait rien à valider ici : il n'y a pas
//! de nom de domaine à vérifier, seulement deux ordinateurs qui doivent
//! se reconnaître.
//!
//! La reconnaissance se fait par empreinte : chaque extrémité connaît
//! d'avance celle du pair et refuse tout autre certificat. Un tiers qui
//! s'interposerait présenterait forcément un certificat différent, donc
//! une empreinte différente.
//!
//! Reste à savoir d'où vient l'empreinte attendue. Sur un réseau local,
//! elle est échangée à l'appairage. Une fois le serveur de mise en
//! relation en place, elle viendra du ticket de session, ce qui ne
//! change rien au mécanisme.

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::WebPkiSupportedAlgorithms;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{DigitallySignedStruct, DistinguishedName, SignatureScheme};
use sha2::{Digest, Sha256};

/// Empreinte d'un certificat, seule identité qui compte pour le tunnel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Empreinte([u8; 32]);

impl Empreinte {
    pub fn du_certificat(certificat: &CertificateDer<'_>) -> Self {
        Self(Sha256::digest(certificat.as_ref()).into())
    }

    pub fn octets(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Affichée en hexadécimal, comme partout ailleurs pour une empreinte.
impl std::fmt::Display for Empreinte {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for octet in &self.0 {
            write!(f, "{octet:02x}")?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum ErreurIdentite {
    Generation(String),
}

impl std::fmt::Display for ErreurIdentite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErreurIdentite::Generation(e) => write!(f, "génération d'identité impossible : {e}"),
        }
    }
}

impl std::error::Error for ErreurIdentite {}

/// Certificat et clé privée d'un appareil.
///
/// Non clonable à dessein : la clé privée n'a aucune raison d'exister en
/// plusieurs exemplaires en mémoire.
#[derive(Debug)]
pub struct Identite {
    certificat: CertificateDer<'static>,
    cle: PrivateKeyDer<'static>,
    empreinte: Empreinte,
}

impl Identite {
    /// Produit une identité neuve.
    pub fn generer() -> Result<Self, ErreurIdentite> {
        // Le nom porté par le certificat n'est jamais vérifié : seule
        // l'empreinte fait foi. Il reste néanmoins renseigné, un
        // certificat sans nom étant refusé par certaines piles.
        let genere = rcgen::generate_simple_self_signed(vec!["zyrdesk".to_string()])
            .map_err(|e| ErreurIdentite::Generation(e.to_string()))?;
        let certificat = CertificateDer::from(genere.cert);
        let cle = PrivateKeyDer::try_from(genere.signing_key.serialize_der())
            .map_err(|e| ErreurIdentite::Generation(e.to_string()))?;
        let empreinte = Empreinte::du_certificat(&certificat);
        Ok(Self {
            certificat,
            cle,
            empreinte,
        })
    }

    pub fn empreinte(&self) -> Empreinte {
        self.empreinte
    }

    pub fn certificat(&self) -> &CertificateDer<'static> {
        &self.certificat
    }

    pub fn cle(&self) -> PrivateKeyDer<'static> {
        self.cle.clone_key()
    }
}

/// Ne laisse passer que le pair dont l'empreinte est connue d'avance.
#[derive(Debug)]
pub struct PairEpingle {
    attendue: Empreinte,
    algorithmes: WebPkiSupportedAlgorithms,
}

impl PairEpingle {
    pub fn nouveau(attendue: Empreinte) -> Self {
        Self {
            attendue,
            algorithmes: rustls::crypto::ring::default_provider().signature_verification_algorithms,
        }
    }

    fn verifier_empreinte(&self, presente: &CertificateDer<'_>) -> Result<(), rustls::Error> {
        let obtenue = Empreinte::du_certificat(presente);
        if obtenue == self.attendue {
            Ok(())
        } else {
            Err(rustls::Error::General(format!(
                "empreinte du pair inattendue : {obtenue}"
            )))
        }
    }
}

impl ServerCertVerifier for PairEpingle {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        self.verifier_empreinte(end_entity)?;
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.algorithmes)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algorithmes)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithmes.supported_schemes()
    }
}

impl ClientCertVerifier for PairEpingle {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, rustls::Error> {
        self.verifier_empreinte(end_entity)?;
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.algorithmes)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algorithmes)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithmes.supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deux_identites_different() {
        let a = Identite::generer().unwrap();
        let b = Identite::generer().unwrap();
        assert_ne!(a.empreinte(), b.empreinte());
    }

    #[test]
    fn l_empreinte_est_stable_et_lisible() {
        let identite = Identite::generer().unwrap();
        let e = identite.empreinte();
        assert_eq!(e, Empreinte::du_certificat(identite.certificat()));
        let texte = e.to_string();
        assert_eq!(texte.len(), 64);
        assert!(texte.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn le_bon_certificat_est_accepte() {
        let identite = Identite::generer().unwrap();
        let epingle = PairEpingle::nouveau(identite.empreinte());
        assert!(epingle.verifier_empreinte(identite.certificat()).is_ok());
    }

    #[test]
    fn un_autre_certificat_est_refuse() {
        let attendu = Identite::generer().unwrap();
        let intrus = Identite::generer().unwrap();
        let epingle = PairEpingle::nouveau(attendu.empreinte());
        assert!(epingle.verifier_empreinte(intrus.certificat()).is_err());
    }
}
