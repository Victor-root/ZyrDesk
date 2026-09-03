//! The small datagrams two computers exchange beside the tunnel.
//!
//! A probe goes to an address that might reach the other computer; an
//! echo comes back from wherever the probe landed, and says where the
//! probe came from as seen from there. Both are signed with the key of
//! the device that sends them, and verified against the fingerprint a
//! ticket named: nobody else on the Internet can make a path count, nor
//! make a computer believe in an address.
//!
//! Beside them, two words for the mirror a server offers: « who am I »
//! and its answer, which carries no more than a nonce and the address
//! the question came from. Neither is signed: a wrong answer is a
//! candidate that no probe will ever confirm.
//!
//! Every one of them starts with a byte no QUIC packet can start with,
//! so the junction tells them apart before the transport sees anything.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use rustls::pki_types::CertificateDer;

use crate::identity::{Fingerprint, Identity, IdentityError, signed_by};

/// The first bytes of every datagram of ours.
///
/// QUIC keeps the second bit of its first byte set on every packet; the
/// first byte here is nought, so the two are never confused.
pub const MAGIC: [u8; 4] = [0x00, b'Z', b'Y', b'R'];

const PROBE: u8 = 1;
const ECHO: u8 = 2;
const WHO_AM_I: u8 = 3;
const SEEN_AS: u8 = 4;

/// The longest session name a probe carries.
const LONGEST_SESSION: usize = 64;

/// The nonce a question to the mirror travels with.
pub type Nonce = [u8; 8];

/// One probe, as sent towards an address that might reach a computer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probe {
    /// The session the ticket named, so an echo of another session
    /// counts for nothing.
    pub session: String,
    pub from: Fingerprint,
    pub to: Fingerprint,
    /// Counted by the sender, so an echo is matched to its probe.
    pub number: u32,
    /// The sender's clock when it left, in milliseconds, copied back by
    /// the echo: the round trip is the difference.
    pub sent: u64,
}

/// The answer to a probe, from wherever it landed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Echo {
    /// The probe answered, with `from` and `to` swapped: whoever echoes
    /// signs as itself.
    pub probe: Probe,
    /// Where the probe came from, as seen by the computer that echoes.
    pub seen: SocketAddr,
}

/// Something signed by a device, not yet believed.
#[derive(Debug, Clone)]
pub struct Sealed<T> {
    inner: T,
    certificate: Vec<u8>,
    signed: Vec<u8>,
    signature: Vec<u8>,
}

impl<T> Sealed<T> {
    /// Who claims to have sent it, before anything is verified.
    pub fn claims(&self) -> &T {
        &self.inner
    }

    /// What is inside, if the device expected did sign it.
    ///
    /// Both halves are checked: the certificate carried is the one that
    /// fingerprint names, and the signature is that certificate's over
    /// every byte before it.
    pub fn opened_by(&self, expected: Fingerprint) -> Option<&T> {
        let certificate = CertificateDer::from(self.certificate.as_slice());
        if Fingerprint::of_certificate(&certificate) != expected {
            return None;
        }
        signed_by(&certificate, &self.signed, &self.signature).then_some(&self.inner)
    }
}

/// What a datagram of ours turned out to be.
#[derive(Debug, Clone)]
pub enum Heard {
    Probe(Sealed<Probe>),
    Echo(Sealed<Echo>),
    WhoAmI(Nonce),
    SeenAs { nonce: Nonce, seen: SocketAddr },
}

/// Whether that datagram is one of ours rather than a packet of the
/// transport.
pub fn is_ours(datagram: &[u8]) -> bool {
    datagram.starts_with(&MAGIC)
}

/// A probe, sealed with this device's key.
pub fn seal_probe(identity: &Identity, probe: &Probe) -> Result<Vec<u8>, IdentityError> {
    let mut bytes = head(PROBE);
    write_probe(&mut bytes, probe);
    seal(identity, bytes)
}

/// An echo, sealed with this device's key.
pub fn seal_echo(identity: &Identity, echo: &Echo) -> Result<Vec<u8>, IdentityError> {
    let mut bytes = head(ECHO);
    write_probe(&mut bytes, &echo.probe);
    write_address(&mut bytes, echo.seen);
    seal(identity, bytes)
}

/// The question to a mirror.
pub fn who_am_i(nonce: Nonce) -> Vec<u8> {
    let mut bytes = head(WHO_AM_I);
    bytes.extend_from_slice(&nonce);
    bytes
}

/// The mirror's answer: that question came from there.
pub fn seen_as(nonce: Nonce, seen: SocketAddr) -> Vec<u8> {
    let mut bytes = head(SEEN_AS);
    bytes.extend_from_slice(&nonce);
    write_address(&mut bytes, seen);
    bytes
}

/// What a mirror answers that datagram, or nothing when it is not a
/// question for one.
///
/// The whole of what a mirror does, written once: a server answers with
/// it on the port its relay listens on, and a server without a relay
/// answers with it on that port alone.
pub fn what_the_mirror_answers(datagram: &[u8], from: SocketAddr) -> Option<Vec<u8>> {
    match heard(datagram)? {
        Heard::WhoAmI(nonce) => Some(seen_as(nonce, from)),
        _ => None,
    }
}

/// Reads one of our datagrams back, or nothing when it is not one, or
/// not whole.
pub fn heard(datagram: &[u8]) -> Option<Heard> {
    let mut reader = Reader(datagram.strip_prefix(&MAGIC)?);
    let kind = reader.byte()?;
    match kind {
        PROBE => {
            let probe = read_probe(&mut reader)?;
            let sealed = read_seal(datagram, reader, probe)?;
            Some(Heard::Probe(sealed))
        }
        ECHO => {
            let probe = read_probe(&mut reader)?;
            let seen = read_address(&mut reader)?;
            let sealed = read_seal(datagram, reader, Echo { probe, seen })?;
            Some(Heard::Echo(sealed))
        }
        WHO_AM_I => Some(Heard::WhoAmI(reader.nonce()?)),
        SEEN_AS => {
            let nonce = reader.nonce()?;
            let seen = read_address(&mut reader)?;
            Some(Heard::SeenAs { nonce, seen })
        }
        _ => None,
    }
}

fn head(kind: u8) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(800);
    bytes.extend_from_slice(&MAGIC);
    bytes.push(kind);
    bytes
}

fn write_probe(bytes: &mut Vec<u8>, probe: &Probe) {
    let session = probe.session.as_bytes();
    let length = session.len().min(LONGEST_SESSION);
    bytes.push(length as u8);
    bytes.extend_from_slice(&session[..length]);
    bytes.extend_from_slice(probe.from.as_bytes());
    bytes.extend_from_slice(probe.to.as_bytes());
    bytes.extend_from_slice(&probe.number.to_be_bytes());
    bytes.extend_from_slice(&probe.sent.to_be_bytes());
}

fn write_address(bytes: &mut Vec<u8>, address: SocketAddr) {
    match address.ip() {
        IpAddr::V4(ip) => {
            bytes.push(4);
            bytes.extend_from_slice(&ip.octets());
        }
        IpAddr::V6(ip) => {
            bytes.push(6);
            bytes.extend_from_slice(&ip.octets());
        }
    }
    bytes.extend_from_slice(&address.port().to_be_bytes());
}

/// Appends the certificate and the signature over everything before it.
fn seal(identity: &Identity, mut bytes: Vec<u8>) -> Result<Vec<u8>, IdentityError> {
    let certificate = identity.certificate().as_ref();
    bytes.extend_from_slice(&(certificate.len() as u16).to_be_bytes());
    bytes.extend_from_slice(certificate);
    let signature = identity.sign(&bytes)?;
    bytes.push(signature.len() as u8);
    bytes.extend_from_slice(&signature);
    Ok(bytes)
}

fn read_probe(reader: &mut Reader<'_>) -> Option<Probe> {
    let length = usize::from(reader.byte()?);
    let session = std::str::from_utf8(reader.take(length)?).ok()?.to_string();
    let from = Fingerprint::from(reader.fingerprint()?);
    let to = Fingerprint::from(reader.fingerprint()?);
    let number = u32::from_be_bytes(reader.take(4)?.try_into().ok()?);
    let sent = u64::from_be_bytes(reader.take(8)?.try_into().ok()?);
    Some(Probe {
        session,
        from,
        to,
        number,
        sent,
    })
}

fn read_address(reader: &mut Reader<'_>) -> Option<SocketAddr> {
    let ip = match reader.byte()? {
        4 => IpAddr::V4(Ipv4Addr::from(<[u8; 4]>::try_from(reader.take(4)?).ok()?)),
        6 => IpAddr::V6(Ipv6Addr::from(<[u8; 16]>::try_from(reader.take(16)?).ok()?)),
        _ => return None,
    };
    let port = u16::from_be_bytes(reader.take(2)?.try_into().ok()?);
    Some(SocketAddr::new(ip, port))
}

/// The certificate and the signature at the end, and what they cover.
fn read_seal<T>(datagram: &[u8], mut reader: Reader<'_>, inner: T) -> Option<Sealed<T>> {
    let length = usize::from(u16::from_be_bytes(reader.take(2)?.try_into().ok()?));
    let certificate = reader.take(length)?.to_vec();
    let signed = datagram[..datagram.len() - reader.0.len()].to_vec();
    let length = usize::from(reader.byte()?);
    let signature = reader.take(length)?.to_vec();
    if !reader.0.is_empty() {
        return None;
    }
    Some(Sealed {
        inner,
        certificate,
        signed,
        signature,
    })
}

/// Walks a datagram from the front, refusing to read past its end.
struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        if self.0.len() < count {
            return None;
        }
        let (taken, rest) = self.0.split_at(count);
        self.0 = rest;
        Some(taken)
    }

    fn byte(&mut self) -> Option<u8> {
        self.take(1).map(|taken| taken[0])
    }

    fn nonce(&mut self) -> Option<Nonce> {
        self.take(8)?.try_into().ok()
    }

    fn fingerprint(&mut self) -> Option<[u8; 32]> {
        self.take(32)?.try_into().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(from: &Identity, to: &Identity) -> Probe {
        Probe {
            session: "s1".into(),
            from: from.fingerprint(),
            to: to.fingerprint(),
            number: 7,
            sent: 123_456,
        }
    }

    #[test]
    fn a_probe_reads_back_and_is_believed_from_its_signer_only() {
        let sender = Identity::generate().unwrap();
        let receiver = Identity::generate().unwrap();
        let sent = probe(&sender, &receiver);
        let bytes = seal_probe(&sender, &sent).unwrap();
        assert!(is_ours(&bytes));
        // Le premier octet ne peut pas être celui d'un paquet QUIC, dont
        // le bit fixe est toujours levé.
        assert_eq!(bytes[0] & 0x40, 0);
        assert!(bytes.len() < 1200, "{} octets", bytes.len());

        let Some(Heard::Probe(sealed)) = heard(&bytes) else {
            panic!("pas lu comme une sonde");
        };
        assert_eq!(sealed.claims(), &sent);
        assert_eq!(sealed.opened_by(sender.fingerprint()), Some(&sent));
        // Signée par un autre : rien.
        assert!(sealed.opened_by(receiver.fingerprint()).is_none());
    }

    #[test]
    fn an_echo_carries_where_the_probe_was_seen_from() {
        let sender = Identity::generate().unwrap();
        let receiver = Identity::generate().unwrap();
        let echo = Echo {
            probe: probe(&receiver, &sender),
            seen: "[2001:db8::1]:47000".parse().unwrap(),
        };
        let bytes = seal_echo(&receiver, &echo).unwrap();
        let Some(Heard::Echo(sealed)) = heard(&bytes) else {
            panic!("pas lu comme un écho");
        };
        assert_eq!(sealed.opened_by(receiver.fingerprint()), Some(&echo));
    }

    #[test]
    fn a_tampered_datagram_is_not_believed() {
        let sender = Identity::generate().unwrap();
        let receiver = Identity::generate().unwrap();
        let mut bytes = seal_probe(&sender, &probe(&sender, &receiver)).unwrap();
        // Le numéro, juste après les deux empreintes et la session.
        let at = MAGIC.len() + 1 + 1 + 2 + 32 + 32;
        bytes[at] ^= 0x01;
        let Some(Heard::Probe(sealed)) = heard(&bytes) else {
            panic!("pas lu comme une sonde");
        };
        assert!(sealed.opened_by(sender.fingerprint()).is_none());
        // Et un datagramme tronqué ne se lit pas du tout.
        assert!(heard(&bytes[..bytes.len() - 3]).is_none());
        assert!(heard(b"\x00ZYR\x09").is_none());
        assert!(heard(b"pas a nous").is_none());
    }

    #[test]
    fn the_mirror_words_read_back() {
        let nonce = [1, 2, 3, 4, 5, 6, 7, 8];
        let Some(Heard::WhoAmI(read)) = heard(&who_am_i(nonce)) else {
            panic!("pas lu comme une question au miroir");
        };
        assert_eq!(read, nonce);

        // Et c'est tout ce que fait un miroir : il répond à la question,
        // et à rien d'autre.
        let asking: SocketAddr = "82.64.12.7:53211".parse().unwrap();
        let answered = what_the_mirror_answers(&who_am_i(nonce), asking).unwrap();
        assert_eq!(
            heard(&answered)
                .map(|read| matches!(read, Heard::SeenAs { seen, .. } if seen == asking)),
            Some(true)
        );
        assert!(what_the_mirror_answers(b"bonjour", asking).is_none());
        let sender = Identity::generate().unwrap();
        let sonde = seal_probe(&sender, &probe(&sender, &sender)).unwrap();
        assert!(what_the_mirror_answers(&sonde, asking).is_none());

        let seen: SocketAddr = "82.64.12.7:47000".parse().unwrap();
        let Some(Heard::SeenAs {
            nonce: read,
            seen: read_seen,
        }) = heard(&seen_as(nonce, seen))
        else {
            panic!("pas lu comme une réponse du miroir");
        };
        assert_eq!(read, nonce);
        assert_eq!(read_seen, seen);
    }

    #[test]
    fn a_session_name_too_long_is_cut_rather_than_refused() {
        let sender = Identity::generate().unwrap();
        let mut long = probe(&sender, &sender);
        long.session = "x".repeat(200);
        let bytes = seal_probe(&sender, &long).unwrap();
        let Some(Heard::Probe(sealed)) = heard(&bytes) else {
            panic!("pas lu comme une sonde");
        };
        assert_eq!(sealed.claims().session.len(), LONGEST_SESSION);
    }
}
