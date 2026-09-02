//! The server checking itself, as a device would.
//!
//! Asked on the machine after an installation, by the script and by
//! hand: it knocks on the API the way the application does, with the
//! same trust, and reads what the server says of itself. What it proves
//! is the one thing nothing else can see from the machine: that the
//! certificate written, the key generated and the port opened are the
//! ones a device will meet, and that the server answering is the one of
//! this configuration and not another one left running.

use std::fmt;
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream};
use std::time::Duration;

use rustls::pki_types::ServerName;
use zyr_broker::PROTOCOL;
use zyr_broker::rest::{ServerInfo, paths};
use zyr_transport::Fingerprint;
use zyr_transport::probe::{self, Heard};
use zyr_transport::trust::{Trust, client_config};

use crate::config::Config;
use crate::keys::{self, Tls};

/// How long the server gets to answer.
const PATIENCE: Duration = Duration::from_secs(5);

/// What the check found.
#[derive(Debug)]
pub struct Checked {
    /// Where it was knocked on.
    pub address: SocketAddr,
    /// The key a device pins, when the API serves TLS itself.
    pub fingerprint: Option<Fingerprint>,
    pub info: ServerInfo,
    /// Where the mirror said the question came from, when the server
    /// has a mirror.
    pub mirror: Option<SocketAddr>,
}

/// Why the server did not pass.
#[derive(Debug)]
pub enum Trouble {
    Keys(String),
    Unreachable(SocketAddr, io::Error),
    /// TLS was refused, with the same words a device would use.
    Tls(String),
    Answer(String),
    /// Another version of the dialect: another program answers there.
    Protocol(u32),
    /// Another signing key: another server's data answers there.
    OtherServer(SocketAddr),
    /// The server says it has a mirror, and it does not answer.
    Mirror(SocketAddr, String),
}

impl fmt::Display for Trouble {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Trouble::Keys(e) => write!(f, "{e}"),
            Trouble::Unreachable(address, e) => write!(
                f,
                "rien ne répond sur {address} : {e}\n  Le service tourne-t-il ? systemctl status \
                 zyrdesk-server"
            ),
            Trouble::Tls(e) => write!(f, "un appareil refuserait ce serveur : {e}"),
            Trouble::Answer(e) => write!(f, "le serveur a répondu quelque chose d'illisible : {e}"),
            Trouble::Protocol(version) => write!(
                f,
                "le serveur qui répond parle le dialecte {version}, ce programme le {PROTOCOL} : \
                 ce n'est pas le même programme"
            ),
            Trouble::OtherServer(address) => write!(
                f,
                "le serveur qui répond sur {address} signe avec une autre clé que celle de cette \
                 configuration : ce n'est pas celui-ci"
            ),
            Trouble::Mirror(address, e) => write!(
                f,
                "le miroir sur UDP {address} ne répond pas : {e}\n  Le port est-il pris par un \
                 autre programme, ou filtré sur la machine elle-même ?"
            ),
        }
    }
}

impl std::error::Error for Trouble {}

/// Where to knock: what the configuration says, an address open to
/// every interface brought back to this machine.
pub fn where_to_knock(listen: SocketAddr) -> SocketAddr {
    let mut probe = listen;
    if probe.ip().is_unspecified() {
        probe.set_ip(if probe.is_ipv4() {
            Ipv4Addr::LOCALHOST.into()
        } else {
            Ipv6Addr::LOCALHOST.into()
        });
    }
    probe
}

/// Knocks on the API as a device would, and reads what it says.
pub fn check(config: &Config) -> Result<Checked, Trouble> {
    let key = keys::load_or_create_signing_key(&config.keys_dir())
        .map_err(|e| Trouble::Keys(e.to_string()))?;
    let tls = config
        .api
        .tls()
        .map(|(certificate, key)| Tls::load(certificate, key))
        .transpose()
        .map_err(|e| Trouble::Keys(e.to_string()))?;
    let fingerprint = tls.as_ref().and_then(Tls::fingerprint);
    let address = where_to_knock(config.api.listen);
    let host = host_of(&config.api.public_url);

    let stream = TcpStream::connect_timeout(&address, PATIENCE)
        .map_err(|e| Trouble::Unreachable(address, e))?;
    stream
        .set_read_timeout(Some(PATIENCE))
        .and_then(|()| stream.set_write_timeout(Some(PATIENCE)))
        .map_err(|e| Trouble::Unreachable(address, e))?;
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {host}\r\nAccept: application/json\r\nConnection: close\r\n\r\n",
        paths::SERVER
    );
    // With the trust a device has: the public roots, then the key of the
    // certificate this configuration names, and the name the devices
    // type. A public certificate passes the ordinary way; a self-signed
    // one passes on its key, as it will in the application once the
    // person has confirmed it.
    let answer = match fingerprint {
        Some(pin) => {
            let (client, verifier) = client_config(Trust::Pinned(pin));
            let name =
                ServerName::try_from(host.clone()).map_err(|e| Trouble::Tls(e.to_string()))?;
            let connection = rustls::ClientConnection::new(client, name)
                .map_err(|e| Trouble::Tls(e.to_string()))?;
            let mut tls = rustls::StreamOwned::new(connection, stream);
            exchange(&mut tls, &request).map_err(|e| match verifier.why_refused() {
                Some(why) => Trouble::Tls(why.to_string()),
                None => Trouble::Tls(e.to_string()),
            })?
        }
        None => {
            let mut stream = stream;
            exchange(&mut stream, &request).map_err(|e| Trouble::Answer(e.to_string()))?
        }
    };
    let info: ServerInfo =
        serde_json::from_slice(&answer).map_err(|e| Trouble::Answer(e.to_string()))?;
    if info.protocol != PROTOCOL {
        return Err(Trouble::Protocol(info.protocol));
    }
    if info.signing_key != key.public() {
        return Err(Trouble::OtherServer(address));
    }
    let mirror = match info.udp_port {
        Some(port) => Some(knock_on_the_mirror(where_to_knock(SocketAddr::new(
            config.relay.listen.ip(),
            port,
        )))?),
        None => None,
    };
    Ok(Checked {
        address,
        fingerprint,
        info,
        mirror,
    })
}

/// Asks the mirror where this question comes from, as a device would.
fn knock_on_the_mirror(mirror: SocketAddr) -> Result<SocketAddr, Trouble> {
    let trouble = |e: &dyn fmt::Display| Trouble::Mirror(mirror, e.to_string());
    let anywhere: SocketAddr = if mirror.is_ipv6() {
        "[::]:0".parse().expect("une adresse écrite en dur")
    } else {
        "0.0.0.0:0".parse().expect("une adresse écrite en dur")
    };
    let socket = std::net::UdpSocket::bind(anywhere).map_err(|e| trouble(&e))?;
    socket
        .set_read_timeout(Some(PATIENCE))
        .map_err(|e| trouble(&e))?;
    let nonce: probe::Nonce = rand::random();
    socket
        .send_to(&probe::who_am_i(nonce), mirror)
        .map_err(|e| trouble(&e))?;
    let mut buf = [0u8; 1500];
    let (count, _) = socket.recv_from(&mut buf).map_err(|e| trouble(&e))?;
    match probe::heard(&buf[..count]) {
        Some(Heard::SeenAs {
            nonce: answered,
            seen,
        }) if answered == nonce => Ok(seen),
        _ => Err(trouble(&"la réponse n'est pas celle d'un miroir ZyrDesk")),
    }
}

/// The host the devices type, out of the public address.
fn host_of(public_url: &str) -> String {
    let text = public_url
        .strip_prefix("https://")
        .unwrap_or(public_url)
        .split('/')
        .next()
        .unwrap_or_default();
    if let Some(rest) = text.strip_prefix('[') {
        return rest.split(']').next().unwrap_or_default().to_string();
    }
    match text.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') && port.parse::<u16>().is_ok() => {
            host.to_string()
        }
        _ => text.to_string(),
    }
}

/// Sends the request and reads the body of the answer, whole.
///
/// Just enough HTTP for one small answer: the status line, the length
/// the server announces, and the body that follows. A refusal is the
/// status, said as it came.
fn exchange(stream: &mut (impl Read + Write), request: &str) -> io::Result<Vec<u8>> {
    stream.write_all(request.as_bytes())?;
    let mut read = Vec::new();
    let mut piece = [0u8; 4096];
    let head_end = loop {
        if let Some(at) = read.windows(4).position(|window| window == b"\r\n\r\n") {
            break at + 4;
        }
        let count = stream.read(&mut piece)?;
        if count == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "le serveur a fermé avant de répondre",
            ));
        }
        read.extend_from_slice(&piece[..count]);
    };
    let head = String::from_utf8_lossy(&read[..head_end]).into_owned();
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(|| io::Error::other("réponse sans code HTTP"))?;
    if status != 200 {
        return Err(io::Error::other(format!("le serveur a répondu {status}")));
    }
    let length = head
        .lines()
        .find_map(|line| {
            line.split_once(':')
                .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        })
        .and_then(|(_, value)| value.trim().parse::<usize>().ok());
    let mut body = read[head_end..].to_vec();
    match length {
        Some(length) => {
            while body.len() < length {
                let count = stream.read(&mut piece)?;
                if count == 0 {
                    break;
                }
                body.extend_from_slice(&piece[..count]);
            }
            body.truncate(length);
        }
        // Without a length the body goes to the close, which this side
        // asked for.
        None => {
            while let Ok(count) = stream.read(&mut piece) {
                if count == 0 {
                    break;
                }
                body.extend_from_slice(&piece[..count]);
            }
        }
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zyr_transport::identity::public_key_fingerprint;

    /// A configuration in a folder of its own, with a self-signed
    /// certificate when asked.
    fn configured(what: &str, tls: bool) -> (Config, std::path::PathBuf, Option<Fingerprint>) {
        let folder = std::env::temp_dir().join(format!(
            "zyrdesk-server-check-{}-{what}",
            zyr_proto::random::alphanumeric_string(8)
        ));
        std::fs::create_dir_all(&folder).unwrap();
        let (files, fingerprint) = if tls {
            let generated =
                rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
            let certificate = folder.join("server.crt");
            let key = folder.join("server.key");
            std::fs::write(&certificate, generated.cert.pem()).unwrap();
            std::fs::write(&key, generated.signing_key.serialize_pem()).unwrap();
            (
                format!(
                    "tls_cert = '{}'\ntls_key = '{}'\n",
                    certificate.display(),
                    key.display()
                ),
                public_key_fingerprint(generated.cert.der()),
            )
        } else {
            (String::new(), None)
        };
        let config = Config::parse(&format!(
            "name = \"Essai\"\ndata_dir = '{}'\n\n[api]\nlisten = \"127.0.0.1:0\"\n{files}\
             public_url = \"https://localhost\"\n\n[relay]\nlisten = \"127.0.0.1:0\"\n",
            folder.display()
        ))
        .unwrap();
        (config, folder, fingerprint)
    }

    /// The same configuration, pointed at the port the server took.
    fn at(config: &Config, address: SocketAddr) -> Config {
        Config {
            api: crate::config::Api {
                listen: address,
                ..config.api.clone()
            },
            ..config.clone()
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_server_passes_its_own_check_with_and_without_tls() {
        for tls in [true, false] {
            let (config, folder, fingerprint) = configured("passe", tls);
            let running = crate::start(config.clone()).await.unwrap();
            let probed = at(&config, running.address);
            let checked = tokio::task::spawn_blocking(move || check(&probed))
                .await
                .unwrap()
                .unwrap();
            assert_eq!(checked.address, running.address);
            assert_eq!(checked.fingerprint, fingerprint);
            assert_eq!(checked.info.name, "Essai");
            assert_eq!(checked.info.protocol, PROTOCOL);
            // Le miroir a répondu, et a vu la question venir d'ici.
            let seen = checked.mirror.expect("le miroir n'a pas été joint");
            assert!(seen.ip().is_loopback(), "{seen}");
            assert_eq!(checked.info.udp_port, running.app.udp_port);
            running.stop().await;
            let _ = std::fs::remove_dir_all(&folder);
        }
    }

    #[test]
    fn a_mirror_that_does_not_answer_is_told_apart_from_one_that_does() {
        let silent = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let refused = knock_on_the_mirror(silent.local_addr().unwrap()).unwrap_err();
        assert!(matches!(refused, Trouble::Mirror(..)), "{refused}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn another_server_on_the_port_is_told_apart() {
        // Deux configurations, deux clés : celle qui répond n'est pas
        // celle qu'on vérifie, et c'est dit plutôt que pris pour bon.
        let (theirs, their_folder, _) = configured("autre", true);
        let running = crate::start(theirs.clone()).await.unwrap();
        let (ours, our_folder, _) = configured("notre", true);
        // Notre configuration, mais avec le certificat et le port de
        // l'autre : la clé de signature est la seule chose qui diffère.
        let probed = Config {
            api: crate::config::Api {
                listen: running.address,
                ..theirs.api.clone()
            },
            ..ours.clone()
        };
        let refused = tokio::task::spawn_blocking(move || check(&probed))
            .await
            .unwrap()
            .unwrap_err();
        assert!(matches!(refused, Trouble::OtherServer(_)), "{refused}");
        running.stop().await;

        // Et rien qui réponde se dit avec l'adresse.
        let (silent, silent_folder, _) = configured("muet", false);
        let refused = tokio::task::spawn_blocking(move || {
            check(&at(&silent, "127.0.0.1:9".parse().unwrap()))
        })
        .await
        .unwrap()
        .unwrap_err();
        assert!(matches!(refused, Trouble::Unreachable(..)), "{refused}");
        for folder in [their_folder, our_folder, silent_folder] {
            let _ = std::fs::remove_dir_all(&folder);
        }
    }

    #[test]
    fn the_host_the_devices_type_is_read_out_of_the_public_address() {
        assert_eq!(host_of("https://zyr.exemple.fr"), "zyr.exemple.fr");
        assert_eq!(host_of("https://zyr.exemple.fr:8443/"), "zyr.exemple.fr");
        assert_eq!(host_of("https://192.168.1.40:8443"), "192.168.1.40");
        assert_eq!(host_of("https://[fd00::1]:8443"), "fd00::1");
        assert_eq!(
            where_to_knock("0.0.0.0:443".parse().unwrap()),
            "127.0.0.1:443".parse().unwrap()
        );
        assert_eq!(
            where_to_knock("[::]:443".parse().unwrap()),
            "[::1]:443".parse().unwrap()
        );
        assert_eq!(
            where_to_knock("192.168.1.40:443".parse().unwrap()),
            "192.168.1.40:443".parse().unwrap()
        );
    }
}
