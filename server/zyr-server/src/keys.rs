//! The server's own keys, and the certificate it serves TLS with.
//!
//! The signing key is the server's identity: made once, at the first
//! start, and kept in the data folder beside the database. Losing it is
//! losing every device's trust, which the installation script says in
//! its summary. The TLS certificate is whatever the configuration names,
//! read in PEM, and its public key is what a device pins when nobody
//! else vouches for it.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use zyr_broker::ServerKey;
use zyr_transport::Fingerprint;

/// The signing key's file, inside the keys folder.
const SIGNING_KEY_FILE: &str = "signing.key";

#[derive(Debug)]
pub enum KeyError {
    File(PathBuf, std::io::Error),
    /// Not thirty-two bytes.
    Malformed(PathBuf),
    Pem(PathBuf, String),
    Tls(String),
}

impl fmt::Display for KeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeyError::File(path, e) => write!(f, "{} : {e}", path.display()),
            KeyError::Malformed(path) => {
                write!(f, "{} : ce n'est pas une clé de signature", path.display())
            }
            KeyError::Pem(path, e) => write!(f, "{} : {e}", path.display()),
            KeyError::Tls(e) => write!(f, "configuration TLS : {e}"),
        }
    }
}

impl std::error::Error for KeyError {}

/// Loads the signing key, or makes it the first time.
///
/// The file is written readable by its owner alone: it is the one
/// secret the server has.
pub fn load_or_create_signing_key(keys_dir: &Path) -> Result<ServerKey, KeyError> {
    let path = keys_dir.join(SIGNING_KEY_FILE);
    if path.is_file() {
        let bytes = std::fs::read(&path).map_err(|e| KeyError::File(path.clone(), e))?;
        let secret: [u8; 32] = bytes
            .try_into()
            .map_err(|_| KeyError::Malformed(path.clone()))?;
        return Ok(ServerKey::from_bytes(&secret));
    }
    std::fs::create_dir_all(keys_dir).map_err(|e| KeyError::File(keys_dir.to_path_buf(), e))?;
    let key = ServerKey::generate();
    write_private(&path, &key.to_bytes())?;
    Ok(key)
}

#[cfg(unix)]
fn write_private(path: &Path, bytes: &[u8]) -> Result<(), KeyError> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| KeyError::File(path.to_path_buf(), e))?;
    file.write_all(bytes)
        .map_err(|e| KeyError::File(path.to_path_buf(), e))
}

#[cfg(not(unix))]
fn write_private(path: &Path, bytes: &[u8]) -> Result<(), KeyError> {
    std::fs::write(path, bytes).map_err(|e| KeyError::File(path.to_path_buf(), e))
}

/// The certificate chain and key the API serves TLS with.
pub struct Tls {
    pub chain: Vec<CertificateDer<'static>>,
    pub key: PrivateKeyDer<'static>,
}

impl Tls {
    pub fn load(certificate: &Path, key: &Path) -> Result<Self, KeyError> {
        let chain = CertificateDer::pem_file_iter(certificate)
            .map_err(|e| KeyError::Pem(certificate.to_path_buf(), e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| KeyError::Pem(certificate.to_path_buf(), e.to_string()))?;
        if chain.is_empty() {
            return Err(KeyError::Pem(
                certificate.to_path_buf(),
                "aucun certificat dans ce fichier".to_string(),
            ));
        }
        let key = PrivateKeyDer::from_pem_file(key)
            .map_err(|e| KeyError::Pem(key.to_path_buf(), e.to_string()))?;
        Ok(Self { chain, key })
    }

    /// What the API's own certificate is pinned by: its public key.
    pub fn fingerprint(&self) -> Option<Fingerprint> {
        zyr_transport::identity::public_key_fingerprint(&self.chain[0])
    }

    /// TLS 1.3 only, serving the chain, for the API and its WebSocket.
    pub fn server_config(&self) -> Result<Arc<rustls::ServerConfig>, KeyError> {
        let mut config = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|e| KeyError::Tls(e.to_string()))?
        .with_no_client_auth()
        .with_single_cert(self.chain.clone(), self.key.clone_key())
        .map_err(|e| KeyError::Tls(e.to_string()))?;
        config.alpn_protocols = vec![b"http/1.1".to_vec()];
        Ok(Arc::new(config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "zyrdesk-server-{name}-{}",
            zyr_proto::random::alphanumeric_string(8)
        ));
        let _ = std::fs::remove_dir_all(&folder);
        folder
    }

    #[test]
    fn the_signing_key_is_made_once_and_kept() {
        let folder = fresh_folder("cles");
        let first = load_or_create_signing_key(&folder).unwrap();
        let second = load_or_create_signing_key(&folder).unwrap();
        assert_eq!(first.public(), second.public());
        std::fs::write(folder.join(SIGNING_KEY_FILE), b"court").unwrap();
        assert!(matches!(
            load_or_create_signing_key(&folder).unwrap_err(),
            KeyError::Malformed(_)
        ));
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_pem_certificate_and_its_key_are_read_and_pinned_by_public_key() {
        let folder = fresh_folder("tls");
        std::fs::create_dir_all(&folder).unwrap();
        let generated =
            rcgen::generate_simple_self_signed(vec!["zyr.exemple.fr".to_string()]).unwrap();
        let certificate = folder.join("server.crt");
        let key = folder.join("server.key");
        std::fs::write(&certificate, generated.cert.pem()).unwrap();
        std::fs::write(&key, generated.signing_key.serialize_pem()).unwrap();

        let tls = Tls::load(&certificate, &key).unwrap();
        assert_eq!(tls.chain.len(), 1);
        assert!(tls.fingerprint().is_some());
        assert!(tls.server_config().is_ok());

        std::fs::write(&certificate, "pas du PEM").unwrap();
        assert!(matches!(
            Tls::load(&certificate, &key),
            Err(KeyError::Pem(..))
        ));
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
