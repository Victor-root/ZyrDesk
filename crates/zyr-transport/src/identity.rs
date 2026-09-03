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

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

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

    /// The fingerprint as its bytes, for whoever needs a stable number
    /// derived from it rather than its spelling.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// A fingerprint read back from its bytes, as a datagram carries it.
impl From<[u8; 32]> for Fingerprint {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
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
        let (pairs, _) = text.as_bytes().as_chunks::<2>();
        for (slot, pair) in bytes.iter_mut().zip(pairs) {
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
    /// The key could not sign, which is a key file that is not what the
    /// certificate was made with.
    Signing(String),
}

impl std::fmt::Display for IdentityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IdentityError::Generation(e) => write!(f, "génération d'identité impossible : {e}"),
            IdentityError::File(path, e) => write!(f, "{} : {e}", path.display()),
            IdentityError::Signing(e) => {
                write!(f, "signature avec la clé de l'appareil impossible : {e}")
            }
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
        Self::load_or_create_at(&folder.join(CERTIFICATE_FILE), &folder.join(KEY_FILE))
    }

    /// The same, at two files named by the caller.
    ///
    /// For whoever keeps more than one: a server holds its relay's
    /// certificate beside its signing key, under its own name.
    pub fn load_or_create_at(certificate: &Path, key: &Path) -> Result<Self, IdentityError> {
        match (certificate.is_file(), key.is_file()) {
            (true, true) => Self::load(certificate, key),
            (false, false) => Self::create(certificate, key),
            _ => Err(IdentityError::Incomplete(
                certificate.parent().unwrap_or(certificate).to_path_buf(),
            )),
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

    fn create(certificate: &Path, key: &Path) -> Result<Self, IdentityError> {
        let identity = Self::generate()?;
        if let Some(folder) = certificate.parent() {
            std::fs::create_dir_all(folder)
                .map_err(|e| IdentityError::File(folder.to_path_buf(), e))?;
        }
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

    /// Signs that message with this device's key.
    ///
    /// ECDSA over P-256 and SHA-256, in the ASN.1 form, which is what the
    /// certificate's key was made for and what `signed_by` verifies
    /// against the certificate this device presents everywhere else.
    /// What gets signed is a challenge a server hands out: the key is
    /// proven without ever leaving this machine.
    pub fn sign(&self, message: &[u8]) -> Result<Vec<u8>, IdentityError> {
        let rng = ring::rand::SystemRandom::new();
        let key = ring::signature::EcdsaKeyPair::from_pkcs8(
            &ring::signature::ECDSA_P256_SHA256_ASN1_SIGNING,
            self.key.secret_der(),
            &rng,
        )
        .map_err(|e| IdentityError::Signing(e.to_string()))?;
        let signature = key
            .sign(&rng, message)
            .map_err(|e| IdentityError::Signing(e.to_string()))?;
        Ok(signature.as_ref().to_vec())
    }
}

/// Whether the key of that certificate produced that signature over that
/// message.
///
/// The other half of `Identity::sign`, for whoever holds only the
/// certificate: a server checking that the device attaching itself holds
/// the key of the certificate it sends.
/// The fingerprint of the public key inside that certificate.
///
/// What a server's certificate is pinned by when nobody else vouches for
/// it: a certificate gets renewed, a key can stay, and a pin on the key
/// survives the renewal. The key is read straight out of the DER, which
/// is a fixed walk down the certificate's structure and nothing more.
pub fn public_key_fingerprint(certificate: &CertificateDer<'_>) -> Option<Fingerprint> {
    let key = der::subject_public_key_info(certificate.as_ref())?;
    Some(Fingerprint(Sha256::digest(key).into()))
}

/// Just enough DER to find the public key of a certificate.
mod der {
    const SEQUENCE: u8 = 0x30;
    /// The explicit `[0]` that carries the version, when there is one.
    const VERSION: u8 = 0xa0;

    /// One element: its tag, its contents, and what follows it.
    fn element(bytes: &[u8]) -> Option<(u8, &[u8], &[u8])> {
        let (&tag, after_tag) = bytes.split_first()?;
        let (&first, after_first) = after_tag.split_first()?;
        let (length, contents_on) = if first & 0x80 == 0 {
            (usize::from(first), after_first)
        } else {
            let count = usize::from(first & 0x7f);
            if count == 0 || count > 4 || after_first.len() < count {
                return None;
            }
            let length = after_first[..count]
                .iter()
                .fold(0usize, |length, &byte| (length << 8) | usize::from(byte));
            (length, &after_first[count..])
        };
        if contents_on.len() < length {
            return None;
        }
        let (contents, rest) = contents_on.split_at(length);
        Some((tag, contents, rest))
    }

    /// The `SubjectPublicKeyInfo`, tag and length included.
    pub fn subject_public_key_info(certificate: &[u8]) -> Option<&[u8]> {
        let (tag, whole, _) = element(certificate)?;
        if tag != SEQUENCE {
            return None;
        }
        let (tag, mut fields, _) = element(whole)?;
        if tag != SEQUENCE {
            return None;
        }
        let (tag, _, rest) = element(fields)?;
        if tag == VERSION {
            fields = rest;
        }
        // Serial number, signature algorithm, issuer, validity, subject:
        // the key comes right after.
        for _ in 0..5 {
            let (_, _, rest) = element(fields)?;
            fields = rest;
        }
        let (tag, _, rest) = element(fields)?;
        if tag != SEQUENCE {
            return None;
        }
        Some(&fields[..fields.len() - rest.len()])
    }
}

pub fn signed_by(certificate: &CertificateDer<'_>, message: &[u8], signature: &[u8]) -> bool {
    webpki::EndEntityCert::try_from(certificate)
        .and_then(|cert| cert.verify_signature(webpki::ring::ECDSA_P256_SHA256, message, signature))
        .is_ok()
}

/// Devices whose fingerprint this machine accepts.
///
/// A client knows exactly one host. A host serves several computers over
/// time, and authorising one more must not mean cutting the session
/// already running: the set is read at each handshake rather than frozen
/// when the endpoint opens.
#[derive(Debug, Clone, Default)]
pub struct AllowedPeers(Arc<Mutex<HashSet<Fingerprint>>>);

impl AllowedPeers {
    /// Replaces the whole set. What is already connected stays.
    pub fn replace_with(&self, peers: impl IntoIterator<Item = Fingerprint>) {
        *self.0.lock().expect("allowed peers lock") = peers.into_iter().collect();
    }

    pub fn contains(&self, peer: &Fingerprint) -> bool {
        self.0.lock().expect("allowed peers lock").contains(peer)
    }

    /// True when no device is allowed, so nothing can connect.
    pub fn is_empty(&self) -> bool {
        self.0.lock().expect("allowed peers lock").is_empty()
    }
}

impl FromIterator<Fingerprint> for AllowedPeers {
    fn from_iter<I: IntoIterator<Item = Fingerprint>>(peers: I) -> Self {
        Self(Arc::new(Mutex::new(peers.into_iter().collect())))
    }
}

/// A single expected device, which is what a client always has.
impl From<Fingerprint> for AllowedPeers {
    fn from(peer: Fingerprint) -> Self {
        std::iter::once(peer).collect()
    }
}

/// Lets through only the devices whose fingerprints are known in advance.
#[derive(Debug)]
pub struct PinnedPeer {
    allowed: AllowedPeers,
    algorithms: WebPkiSupportedAlgorithms,
}

impl PinnedPeer {
    pub fn new(allowed: impl Into<AllowedPeers>) -> Self {
        Self {
            allowed: allowed.into(),
            algorithms: rustls::crypto::ring::default_provider().signature_verification_algorithms,
        }
    }

    fn check_fingerprint(&self, presented: &CertificateDer<'_>) -> Result<(), rustls::Error> {
        let obtained = Fingerprint::of_certificate(presented);
        if self.allowed.contains(&obtained) {
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

/// Takes every certificate, and judges none.
///
/// One place needs it, and needs it to be exactly this: a relay, which
/// serves devices it has never heard of and decides on the pass they
/// present, not on the certificate they carry. The certificate is still
/// read, and its fingerprint is what the pass has to name: presenting
/// somebody else's fingerprint would mean holding their key, which is
/// what TLS proves here and what nothing else could.
#[derive(Debug)]
pub struct AnyPeer {
    algorithms: WebPkiSupportedAlgorithms,
}

impl Default for AnyPeer {
    fn default() -> Self {
        Self {
            algorithms: rustls::crypto::ring::default_provider().signature_verification_algorithms,
        }
    }
}

impl ClientCertVerifier for AnyPeer {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, rustls::Error> {
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
    fn a_device_signs_and_its_certificate_vouches_for_it() {
        // C'est ainsi qu'un appareil prouve sa clé à un serveur sans la
        // lui montrer : le serveur n'a que le certificat, et il suffit.
        let device = Identity::generate().unwrap();
        let signature = device.sign("un défi".as_bytes()).unwrap();
        assert!(signed_by(
            device.certificate(),
            "un défi".as_bytes(),
            &signature
        ));
        assert!(!signed_by(
            device.certificate(),
            "un autre défi".as_bytes(),
            &signature
        ));
        let other = Identity::generate().unwrap();
        assert!(!signed_by(
            other.certificate(),
            "un défi".as_bytes(),
            &signature
        ));
        assert!(!signed_by(
            device.certificate(),
            "un défi".as_bytes(),
            b"pas une signature"
        ));
    }

    #[test]
    fn the_public_key_is_found_in_the_certificate_and_outlives_its_renewal() {
        // Deux certificats sur la même clé ont la même empreinte de clé :
        // c'est ce qui permet de renouveler le premier sans réépingler.
        let key = rcgen::KeyPair::generate().unwrap();
        let first = rcgen::CertificateParams::new(vec!["zyr.exemple.fr".to_string()])
            .unwrap()
            .self_signed(&key)
            .unwrap();
        let second = rcgen::CertificateParams::new(vec!["autre.exemple.fr".to_string()])
            .unwrap()
            .self_signed(&key)
            .unwrap();
        let of_first = public_key_fingerprint(first.der()).unwrap();
        assert_eq!(public_key_fingerprint(second.der()).unwrap(), of_first);
        assert_ne!(Fingerprint::of_certificate(first.der()), of_first);

        let other = Identity::generate().unwrap();
        assert_ne!(
            public_key_fingerprint(other.certificate()).unwrap(),
            of_first
        );
        assert_eq!(
            public_key_fingerprint(&CertificateDer::from(b"pas un certificat".to_vec())),
            None
        );
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
    fn several_devices_are_let_in_and_the_others_are_not() {
        let first = Identity::generate().unwrap();
        let second = Identity::generate().unwrap();
        let intruder = Identity::generate().unwrap();

        let allowed: AllowedPeers = [first.fingerprint(), second.fingerprint()]
            .into_iter()
            .collect();
        let pinned = PinnedPeer::new(allowed);

        assert!(pinned.check_fingerprint(first.certificate()).is_ok());
        assert!(pinned.check_fingerprint(second.certificate()).is_ok());
        assert!(pinned.check_fingerprint(intruder.certificate()).is_err());
    }

    #[test]
    fn a_device_authorised_afterwards_gets_in_without_reopening() {
        // The host reads its list again while it runs: authorising one
        // more computer must not mean cutting the session in progress.
        let known = Identity::generate().unwrap();
        let latecomer = Identity::generate().unwrap();

        let allowed: AllowedPeers = known.fingerprint().into();
        let pinned = PinnedPeer::new(allowed.clone());
        assert!(pinned.check_fingerprint(latecomer.certificate()).is_err());

        allowed.replace_with([known.fingerprint(), latecomer.fingerprint()]);
        assert!(pinned.check_fingerprint(latecomer.certificate()).is_ok());
        assert!(pinned.check_fingerprint(known.certificate()).is_ok());
    }

    #[test]
    fn a_machine_that_allows_nobody_lets_nobody_in() {
        let anyone = Identity::generate().unwrap();
        let pinned = PinnedPeer::new(AllowedPeers::default());
        assert!(pinned.check_fingerprint(anyone.certificate()).is_err());
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
