//! A device's cryptographic identity, and the pinning of its peer.
//!
//! Each end presents a certificate it signed itself. A certificate
//! authority would have nothing to validate here: there is no domain
//! name to check, only two computers that must recognise each other.
//!
//! Recognition goes by fingerprint: each end knows its peer's in
//! advance and refuses any other certificate. A third party stepping in
//! would necessarily present a different certificate, hence a different
//! fingerprint.
//!
//! Where the expected fingerprint comes from is a separate question. On
//! a local network it is exchanged at pairing time. Once the rendezvous
//! server exists, it will come from the session ticket, which changes
//! nothing about the mechanism.

use std::path::{Path, PathBuf};

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::WebPkiSupportedAlgorithms;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{DigitallySignedStruct, DistinguishedName, SignatureScheme};
use sha2::{Digest, Sha256};

/// Fingerprint of a certificate, the only identity the tunnel cares for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fingerprint([u8; 32]);

impl Fingerprint {
    pub fn of_certificate(certificate: &CertificateDer<'_>) -> Self {
        Self(Sha256::digest(certificate.as_ref()).into())
    }
}

/// Text that is not a fingerprint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidFingerprint;

impl std::fmt::Display for InvalidFingerprint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "une empreinte s'écrit avec 64 caractères hexadécimaux")
    }
}

impl std::error::Error for InvalidFingerprint {}

/// Reads a fingerprint back, exactly as it is displayed.
impl std::str::FromStr for Fingerprint {
    type Err = InvalidFingerprint;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let text = text.trim();
        if text.len() != 64 {
            return Err(InvalidFingerprint);
        }
        let mut bytes = [0u8; 32];
        for (slot, pair) in bytes.iter_mut().zip(text.as_bytes().chunks_exact(2)) {
            let pair = std::str::from_utf8(pair).map_err(|_| InvalidFingerprint)?;
            *slot = u8::from_str_radix(pair, 16).map_err(|_| InvalidFingerprint)?;
        }
        Ok(Self(bytes))
    }
}

/// Shown in hexadecimal, the way a fingerprint is shown everywhere else.
impl std::fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum IdentityError {
    Generation(String),
    File(PathBuf, std::io::Error),
    /// One of the two files is missing: making a new identity would
    /// change the machine's fingerprint and break all of its pairings.
    Incomplete(PathBuf),
}

impl std::fmt::Display for IdentityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IdentityError::Generation(e) => write!(f, "génération d'identité impossible : {e}"),
            IdentityError::File(path, e) => write!(f, "{} : {e}", path.display()),
            IdentityError::Incomplete(folder) => write!(
                f,
                "identité incomplète dans {} : effacer le dossier pour en refaire une, \
                 en sachant que les appairages existants seront perdus",
                folder.display()
            ),
        }
    }
}

impl std::error::Error for IdentityError {}

/// The device's certificate, in its identity folder.
const CERTIFICATE_FILE: &str = "device.crt";
/// The device's private key.
const KEY_FILE: &str = "device.key";

fn read(path: &Path) -> Result<Vec<u8>, IdentityError> {
    std::fs::read(path).map_err(|e| IdentityError::File(path.to_path_buf(), e))
}

fn write(path: &Path, contents: &[u8]) -> Result<(), IdentityError> {
    std::fs::write(path, contents).map_err(|e| IdentityError::File(path.to_path_buf(), e))
}

/// A device's certificate and private key.
///
/// Deliberately not clonable: the private key has no reason to exist in
/// several copies in memory.
#[derive(Debug)]
pub struct Identity {
    certificate: CertificateDer<'static>,
    key: PrivateKeyDer<'static>,
    fingerprint: Fingerprint,
}

impl Identity {
    /// Produces a brand new identity.
    pub fn generate() -> Result<Self, IdentityError> {
        // The name the certificate carries is never checked: only the
        // fingerprint counts. It is filled in all the same, since some
        // stacks refuse a certificate without one.
        let generated = rcgen::generate_simple_self_signed(vec!["zyrdesk".to_string()])
            .map_err(|e| IdentityError::Generation(e.to_string()))?;
        let certificate = CertificateDer::from(generated.cert);
        let key = PrivateKeyDer::try_from(generated.signing_key.serialize_der())
            .map_err(|e| IdentityError::Generation(e.to_string()))?;
        let fingerprint = Fingerprint::of_certificate(&certificate);
        Ok(Self {
            certificate,
            key,
            fingerprint,
        })
    }

    /// Loads this machine's identity, or creates it the first time.
    ///
    /// A device's fingerprint has to last: it is what the peer pins. So
    /// it is kept on disk and never made again while both files are
    /// there.
    ///
    /// The private key is written in the clear, under the project
    /// folder. That is the same exposure as the rest of the project on a
    /// machine its owner administers. The Windows service will put it
    /// under the system's protection, out of reach of other accounts.
    pub fn load_or_create(folder: &Path) -> Result<Self, IdentityError> {
        let certificate = folder.join(CERTIFICATE_FILE);
        let key = folder.join(KEY_FILE);

        match (certificate.is_file(), key.is_file()) {
            (true, true) => Self::load(&certificate, &key),
            (false, false) => Self::create(folder, &certificate, &key),
            _ => Err(IdentityError::Incomplete(folder.to_path_buf())),
        }
    }

    fn load(certificate: &Path, key: &Path) -> Result<Self, IdentityError> {
        let certificate_der = read(certificate)?;
        let key_der = read(key)?;
        let certificate = CertificateDer::from(certificate_der);
        let fingerprint = Fingerprint::of_certificate(&certificate);
        Ok(Self {
            certificate,
            key: PrivateKeyDer::try_from(key_der)
                .map_err(|e| IdentityError::Generation(e.to_string()))?,
            fingerprint,
        })
    }

    fn create(folder: &Path, certificate: &Path, key: &Path) -> Result<Self, IdentityError> {
        let identity = Self::generate()?;
        std::fs::create_dir_all(folder)
            .map_err(|e| IdentityError::File(folder.to_path_buf(), e))?;
        write(certificate, identity.certificate.as_ref())?;
        write(key, identity.key.secret_der())?;
        Ok(identity)
    }

    pub fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }

    pub fn certificate(&self) -> &CertificateDer<'static> {
        &self.certificate
    }

    pub fn key(&self) -> PrivateKeyDer<'static> {
        self.key.clone_key()
    }
}

/// Lets through only the peer whose fingerprint is known in advance.
#[derive(Debug)]
pub struct PinnedPeer {
    expected: Fingerprint,
    algorithms: WebPkiSupportedAlgorithms,
}

impl PinnedPeer {
    pub fn new(expected: Fingerprint) -> Self {
        Self {
            expected,
            algorithms: rustls::crypto::ring::default_provider().signature_verification_algorithms,
        }
    }

    fn check_fingerprint(&self, presented: &CertificateDer<'_>) -> Result<(), rustls::Error> {
        let obtained = Fingerprint::of_certificate(presented);
        if obtained == self.expected {
            Ok(())
        } else {
            Err(rustls::Error::General(format!(
                "empreinte du pair inattendue : {obtained}"
            )))
        }
    }
}

impl ServerCertVerifier for PinnedPeer {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        self.check_fingerprint(end_entity)?;
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

impl ClientCertVerifier for PinnedPeer {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, rustls::Error> {
        self.check_fingerprint(end_entity)?;
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_identities_differ() {
        let first = Identity::generate().unwrap();
        let second = Identity::generate().unwrap();
        assert_ne!(first.fingerprint(), second.fingerprint());
    }

    #[test]
    fn the_fingerprint_is_stable_and_readable() {
        let identity = Identity::generate().unwrap();
        let fingerprint = identity.fingerprint();
        assert_eq!(
            fingerprint,
            Fingerprint::of_certificate(identity.certificate())
        );
        let text = fingerprint.to_string();
        assert_eq!(text.len(), 64);
        assert!(text.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn the_right_certificate_is_accepted() {
        let identity = Identity::generate().unwrap();
        let pinned = PinnedPeer::new(identity.fingerprint());
        assert!(pinned.check_fingerprint(identity.certificate()).is_ok());
    }

    #[test]
    fn any_other_certificate_is_refused() {
        let expected = Identity::generate().unwrap();
        let intruder = Identity::generate().unwrap();
        let pinned = PinnedPeer::new(expected.fingerprint());
        assert!(pinned.check_fingerprint(intruder.certificate()).is_err());
    }

    #[test]
    fn a_displayed_fingerprint_reads_back() {
        let identity = Identity::generate().unwrap();
        let fingerprint = identity.fingerprint();
        assert_eq!(
            fingerprint.to_string().parse::<Fingerprint>().unwrap(),
            fingerprint
        );
        // Copied out of a terminal, it often drags whitespace along.
        assert_eq!(
            format!("  {fingerprint}\n").parse::<Fingerprint>().unwrap(),
            fingerprint
        );
    }

    #[test]
    fn text_that_is_not_a_fingerprint_is_refused() {
        for text in ["", "abc", &"z".repeat(64), &"ab".repeat(31)] {
            assert!(text.parse::<Fingerprint>().is_err(), "{text}");
        }
    }

    /// Clean working folder, one per test.
    fn fresh_folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("zyrdesk-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        folder
    }

    #[test]
    fn the_machine_identity_does_not_change_between_runs() {
        let folder = fresh_folder("identity-stable");
        let first = Identity::load_or_create(&folder).unwrap();
        let second = Identity::load_or_create(&folder).unwrap();
        assert_eq!(first.fingerprint(), second.fingerprint());
        assert_eq!(second.certificate(), first.certificate());
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_half_erased_identity_is_not_silently_remade() {
        // Remaking it would change the machine's fingerprint and break
        // its pairings without a word.
        let folder = fresh_folder("identity-maimed");
        let original = Identity::load_or_create(&folder).unwrap();
        std::fs::remove_file(folder.join(KEY_FILE)).unwrap();

        assert!(matches!(
            Identity::load_or_create(&folder),
            Err(IdentityError::Incomplete(_))
        ));
        assert_eq!(
            Fingerprint::of_certificate(&CertificateDer::from(
                std::fs::read(folder.join(CERTIFICATE_FILE)).unwrap()
            )),
            original.fingerprint()
        );
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
