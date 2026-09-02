//! Whom a device believes when a server presents a certificate.
//!
//! First whoever any browser believes: the public roots. When they say
//! nothing, the key the person pinned, and nothing else; and when there
//! is no pin either, a refusal that carries the key presented, so the
//! window can show it and ask the person to compare. That is the whole
//! of what « self-signed » costs: one comparison, once.
//!
//! What this is not: a client that accepts everything. Signatures are
//! verified as always, only TLS 1.3 is spoken, and a public certificate
//! goes down the ordinary road.
//!
//! Here rather than beside the account link because the server checks
//! itself with it too: what an installation proves is that a device
//! knocking with this very trust is let in.

use std::fmt;
use std::sync::{Arc, Mutex};

use rustls::client::WebPkiServerVerifier;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, RootCertStore, SignatureScheme};

use crate::identity::{Fingerprint, public_key_fingerprint};

/// What the device holds of the server, besides the public roots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trust {
    /// Only what a public authority vouches for.
    PublicOnly,
    /// That key, confirmed by a person, when the public roots say
    /// nothing.
    Pinned(Fingerprint),
}

/// Why the server was not believed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Untrusted {
    /// Nobody vouches for this key, and nothing was pinned: the person
    /// may compare it and confirm it.
    Unpinned { presented: Fingerprint },
    /// The key pinned is not the one presented: the server changed key,
    /// or this is not the server.
    Changed {
        pinned: Fingerprint,
        presented: Fingerprint,
    },
    /// No key could be read from what was presented.
    Unreadable,
}

impl fmt::Display for Untrusted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Untrusted::Unpinned { presented } => write!(
                f,
                "ce serveur présente un certificat que personne ne garantit ; son empreinte est \
                 {presented}"
            ),
            Untrusted::Changed { pinned, presented } => write!(
                f,
                "ce serveur ne présente plus la clé épinglée ({pinned}) mais une autre \
                 ({presented}) : il a changé de clé, ou ce n'est pas lui"
            ),
            Untrusted::Unreadable => {
                f.write_str("ce serveur présente un certificat dont la clé ne se lit pas")
            }
        }
    }
}

impl std::error::Error for Untrusted {}

/// Believes the public roots, then the pin, then nobody; and remembers
/// why it said no, for whoever asks after the connection failed.
#[derive(Debug)]
pub struct Verifier {
    trust: Trust,
    public: Arc<WebPkiServerVerifier>,
    refused: Mutex<Option<Untrusted>>,
}

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

impl Verifier {
    pub fn new(trust: Trust) -> Self {
        let roots = RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        };
        let public = WebPkiServerVerifier::builder_with_provider(Arc::new(roots), provider())
            .build()
            .expect("les racines publiques se chargent");
        Self {
            trust,
            public,
            refused: Mutex::new(None),
        }
    }

    /// Why the last handshake was refused, if it was.
    pub fn why_refused(&self) -> Option<Untrusted> {
        self.refused.lock().expect("refus").take()
    }
}

impl ServerCertVerifier for Verifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if self
            .public
            .verify_server_cert(end_entity, intermediates, server_name, ocsp_response, now)
            .is_ok()
        {
            return Ok(ServerCertVerified::assertion());
        }
        let refusal = match (public_key_fingerprint(end_entity), self.trust) {
            (None, _) => Untrusted::Unreadable,
            (Some(presented), Trust::Pinned(pinned)) if presented == pinned => {
                return Ok(ServerCertVerified::assertion());
            }
            (Some(presented), Trust::Pinned(pinned)) => Untrusted::Changed { pinned, presented },
            (Some(presented), Trust::PublicOnly) => Untrusted::Unpinned { presented },
        };
        *self.refused.lock().expect("refus") = Some(refusal.clone());
        Err(rustls::Error::General(refusal.to_string()))
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.public.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.public.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.public.supported_verify_schemes()
    }
}

/// A TLS configuration that believes as `trust` says, and the verifier
/// behind it, to ask why after a refusal.
pub fn client_config(trust: Trust) -> (Arc<rustls::ClientConfig>, Arc<Verifier>) {
    let verifier = Arc::new(Verifier::new(trust));
    let config = rustls::ClientConfig::builder_with_provider(provider())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .expect("TLS 1.3 est connu du fournisseur")
        .dangerous()
        .with_custom_certificate_verifier(verifier.clone())
        .with_no_client_auth();
    (Arc::new(config), verifier)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn self_signed() -> CertificateDer<'static> {
        let generated =
            rcgen::generate_simple_self_signed(vec!["zyr.exemple.fr".to_string()]).unwrap();
        CertificateDer::from(generated.cert)
    }

    fn judged(verifier: &Verifier, certificate: &CertificateDer<'_>) -> Result<(), Untrusted> {
        let name = ServerName::try_from("zyr.exemple.fr").unwrap();
        match verifier.verify_server_cert(certificate, &[], &name, &[], UnixTime::now()) {
            Ok(_) => Ok(()),
            Err(_) => Err(verifier.why_refused().expect("un refus a une raison")),
        }
    }

    #[test]
    fn a_self_signed_server_is_refused_with_its_key_shown() {
        // C'est ce que la fenêtre montre à la personne pour qu'elle
        // compare avec ce que l'installation a affiché.
        let certificate = self_signed();
        let presented = public_key_fingerprint(&certificate).unwrap();
        assert_eq!(
            judged(&Verifier::new(Trust::PublicOnly), &certificate),
            Err(Untrusted::Unpinned { presented })
        );
    }

    #[test]
    fn the_pinned_key_is_believed_and_another_is_not() {
        let certificate = self_signed();
        let presented = public_key_fingerprint(&certificate).unwrap();
        assert_eq!(
            judged(&Verifier::new(Trust::Pinned(presented)), &certificate),
            Ok(())
        );

        let other = public_key_fingerprint(&self_signed()).unwrap();
        assert_eq!(
            judged(&Verifier::new(Trust::Pinned(other)), &certificate),
            Err(Untrusted::Changed {
                pinned: other,
                presented
            })
        );
        assert_eq!(
            judged(
                &Verifier::new(Trust::Pinned(other)),
                &CertificateDer::from(b"pas un certificat".to_vec())
            ),
            Err(Untrusted::Unreadable)
        );
    }
}
