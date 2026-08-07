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

use std::path::{Path, PathBuf};

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
}

/// Texte qui n'est pas une empreinte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmpreinteInvalide;

impl std::fmt::Display for EmpreinteInvalide {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "une empreinte s'écrit avec 64 caractères hexadécimaux")
    }
}

impl std::error::Error for EmpreinteInvalide {}

/// Relit une empreinte telle qu'elle s'affiche.
impl std::str::FromStr for Empreinte {
    type Err = EmpreinteInvalide;

    fn from_str(texte: &str) -> Result<Self, Self::Err> {
        let texte = texte.trim();
        if texte.len() != 64 {
            return Err(EmpreinteInvalide);
        }
        let mut octets = [0u8; 32];
        for (place, paire) in octets.iter_mut().zip(texte.as_bytes().chunks_exact(2)) {
            let paire = std::str::from_utf8(paire).map_err(|_| EmpreinteInvalide)?;
            *place = u8::from_str_radix(paire, 16).map_err(|_| EmpreinteInvalide)?;
        }
        Ok(Self(octets))
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
    Fichier(PathBuf, std::io::Error),
    /// Un des deux fichiers manque : refaire une identité changerait
    /// l'empreinte de la machine et casserait tous ses appairages.
    Incomplete(PathBuf),
}

impl std::fmt::Display for ErreurIdentite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErreurIdentite::Generation(e) => write!(f, "génération d'identité impossible : {e}"),
            ErreurIdentite::Fichier(chemin, e) => write!(f, "{} : {e}", chemin.display()),
            ErreurIdentite::Incomplete(dossier) => write!(
                f,
                "identité incomplète dans {} : effacer le dossier pour en refaire une, \
                 en sachant que les appairages existants seront perdus",
                dossier.display()
            ),
        }
    }
}

impl std::error::Error for ErreurIdentite {}

/// Certificat de l'appareil, dans le dossier de son identité.
const FICHIER_CERTIFICAT: &str = "appareil.crt";
/// Clé privée de l'appareil.
const FICHIER_CLE: &str = "appareil.key";

fn lire(chemin: &Path) -> Result<Vec<u8>, ErreurIdentite> {
    std::fs::read(chemin).map_err(|e| ErreurIdentite::Fichier(chemin.to_path_buf(), e))
}

fn ecrire(chemin: &Path, contenu: &[u8]) -> Result<(), ErreurIdentite> {
    std::fs::write(chemin, contenu).map_err(|e| ErreurIdentite::Fichier(chemin.to_path_buf(), e))
}

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

    /// Charge l'identité de cette machine, ou la crée la première fois.
    ///
    /// L'empreinte d'un appareil doit durer : c'est elle que le pair
    /// épingle. Elle est donc gardée sur disque et jamais refaite tant
    /// que les deux fichiers sont là.
    ///
    /// La clé privée est écrite en clair, sous le dossier du projet.
    /// C'est la même exposition que le reste du projet sur une machine
    /// que son propriétaire administre. Le service du jalon M3 la mettra
    /// sous la protection du système, hors de portée des autres comptes.
    pub fn charger_ou_creer(dossier: &Path) -> Result<Self, ErreurIdentite> {
        let certificat = dossier.join(FICHIER_CERTIFICAT);
        let cle = dossier.join(FICHIER_CLE);

        match (certificat.is_file(), cle.is_file()) {
            (true, true) => Self::charger(&certificat, &cle),
            (false, false) => Self::creer(dossier, &certificat, &cle),
            _ => Err(ErreurIdentite::Incomplete(dossier.to_path_buf())),
        }
    }

    fn charger(certificat: &Path, cle: &Path) -> Result<Self, ErreurIdentite> {
        let der_certificat = lire(certificat)?;
        let der_cle = lire(cle)?;
        let certificat = CertificateDer::from(der_certificat);
        let empreinte = Empreinte::du_certificat(&certificat);
        Ok(Self {
            certificat,
            cle: PrivateKeyDer::try_from(der_cle)
                .map_err(|e| ErreurIdentite::Generation(e.to_string()))?,
            empreinte,
        })
    }

    fn creer(dossier: &Path, certificat: &Path, cle: &Path) -> Result<Self, ErreurIdentite> {
        let identite = Self::generer()?;
        std::fs::create_dir_all(dossier)
            .map_err(|e| ErreurIdentite::Fichier(dossier.to_path_buf(), e))?;
        ecrire(certificat, identite.certificat.as_ref())?;
        ecrire(cle, identite.cle.secret_der())?;
        Ok(identite)
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

    #[test]
    fn une_empreinte_affichee_se_relit() {
        let identite = Identite::generer().unwrap();
        let e = identite.empreinte();
        assert_eq!(e.to_string().parse::<Empreinte>().unwrap(), e);
        // Recopiée d'un terminal, elle traîne souvent des espaces.
        assert_eq!(format!("  {e}\n").parse::<Empreinte>().unwrap(), e);
    }

    #[test]
    fn un_texte_qui_n_est_pas_une_empreinte_est_refuse() {
        for texte in ["", "abc", &"z".repeat(64), &"ab".repeat(31)] {
            assert!(texte.parse::<Empreinte>().is_err(), "{texte}");
        }
    }

    /// Dossier de travail propre, distinct pour chaque test.
    fn dossier_neuf(nom: &str) -> PathBuf {
        let dossier = std::env::temp_dir().join(format!("zyrdesk-{}-{nom}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dossier);
        dossier
    }

    #[test]
    fn l_identite_de_la_machine_ne_change_pas_d_une_fois_sur_l_autre() {
        let dossier = dossier_neuf("identite-stable");
        let premiere = Identite::charger_ou_creer(&dossier).unwrap();
        let seconde = Identite::charger_ou_creer(&dossier).unwrap();
        assert_eq!(premiere.empreinte(), seconde.empreinte());
        assert_eq!(seconde.certificat(), premiere.certificat());
        std::fs::remove_dir_all(&dossier).unwrap();
    }

    #[test]
    fn une_identite_a_moitie_effacee_ne_se_refait_pas_en_silence() {
        // Refaire l'identité changerait l'empreinte de la machine et
        // casserait ses appairages sans rien dire.
        let dossier = dossier_neuf("identite-mutilee");
        let originale = Identite::charger_ou_creer(&dossier).unwrap();
        std::fs::remove_file(dossier.join(FICHIER_CLE)).unwrap();

        assert!(matches!(
            Identite::charger_ou_creer(&dossier),
            Err(ErreurIdentite::Incomplete(_))
        ));
        assert_eq!(
            Empreinte::du_certificat(&CertificateDer::from(
                std::fs::read(dossier.join(FICHIER_CERTIFICAT)).unwrap()
            )),
            originale.empreinte()
        );
        std::fs::remove_dir_all(&dossier).unwrap();
    }
}
