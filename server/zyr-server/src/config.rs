//! What the server is told at its start, and what it refuses to be told.
//!
//! One TOML file, written by the installation script and read once. The
//! one rule that lives in code rather than in the documentation is the
//! rule about the clear: an API listening without TLS is accepted on a
//! loopback address and nowhere else, because a reverse proxy on the
//! same machine is the only thing that may terminate TLS for us.

use std::fmt;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use zyr_broker::rest::Registration;

/// Where the file lives unless said otherwise.
pub const DEFAULT_PATH: &str = "/etc/zyrdesk-server/server.toml";

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// What the application shows for this server.
    pub name: String,
    pub data_dir: PathBuf,
    pub api: Api,
    #[serde(default)]
    pub registration: RegistrationConfig,
    #[serde(default)]
    pub relay: Relay,
    #[serde(default)]
    pub limits: Limits,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Api {
    /// Where the API and the live channel listen.
    pub listen: SocketAddr,
    /// The certificate chain and its key, PEM. Both or neither: neither
    /// is the reverse-proxy case, and then `listen` must be loopback.
    #[serde(default)]
    pub tls_cert: Option<PathBuf>,
    #[serde(default)]
    pub tls_key: Option<PathBuf>,
    /// What the devices type to reach this server, `https://` included.
    pub public_url: String,
}

impl Api {
    /// Whether this API terminates TLS itself.
    pub fn tls(&self) -> Option<(&Path, &Path)> {
        Some((self.tls_cert.as_deref()?, self.tls_key.as_deref()?))
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RegistrationConfig {
    pub policy: Registration,
}

impl Default for RegistrationConfig {
    fn default() -> Self {
        Self {
            policy: Registration::Invitation,
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Relay {
    pub enabled: bool,
    /// Where the relay and the mirror listen, UDP.
    pub listen: SocketAddr,
    pub max_sessions: u32,
    pub max_kbps_per_session: u32,
}

impl Default for Relay {
    fn default() -> Self {
        Self {
            enabled: true,
            listen: "0.0.0.0:443".parse().expect("une adresse écrite en dur"),
            max_sessions: 10,
            max_kbps_per_session: 60_000,
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    /// Login attempts one address may make in a minute before it waits.
    pub login_attempts_per_minute: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            login_attempts_per_minute: 10,
        }
    }
}

#[derive(Debug)]
pub enum ConfigError {
    Read(PathBuf, std::io::Error),
    Parse(PathBuf, String),
    /// Listening in the clear anywhere but on this machine.
    ClearOffLoopback(SocketAddr),
    /// One of the two TLS files without the other.
    HalfTls,
    /// A public address that is not `https://`.
    NotHttps(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Read(path, e) => write!(f, "{} : {e}", path.display()),
            ConfigError::Parse(path, e) => write!(f, "{} : {e}", path.display()),
            ConfigError::ClearOffLoopback(listen) => write!(
                f,
                "api.listen = {listen} sans certificat TLS : le serveur ne parle jamais en clair \
                 ailleurs que sur 127.0.0.1, derrière un mandataire inverse qui termine TLS sur \
                 cette machine. Donner tls_cert et tls_key, ou écouter sur une adresse de boucle \
                 locale"
            ),
            ConfigError::HalfTls => f.write_str(
                "api.tls_cert et api.tls_key vont ensemble : l'un sans l'autre ne dit rien",
            ),
            ConfigError::NotHttps(url) => write!(
                f,
                "api.public_url = {url} : les appareils ne joignent un serveur qu'en https://"
            ),
        }
    }
}

impl std::error::Error for ConfigError {}

impl Config {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text =
            std::fs::read_to_string(path).map_err(|e| ConfigError::Read(path.to_path_buf(), e))?;
        Self::parse(&text).map_err(|e| match e {
            ConfigError::Parse(_, why) => ConfigError::Parse(path.to_path_buf(), why),
            other => other,
        })
    }

    /// Reads the file's text, and refuses what must be refused.
    pub fn parse(text: &str) -> Result<Self, ConfigError> {
        let config: Config = toml::from_str(text)
            .map_err(|e| ConfigError::Parse(PathBuf::from(DEFAULT_PATH), e.to_string()))?;
        config.checked()
    }

    fn checked(self) -> Result<Self, ConfigError> {
        match (&self.api.tls_cert, &self.api.tls_key) {
            (Some(_), Some(_)) => {}
            (None, None) => {
                if !is_loopback(self.api.listen.ip()) {
                    return Err(ConfigError::ClearOffLoopback(self.api.listen));
                }
            }
            _ => return Err(ConfigError::HalfTls),
        }
        if !self.api.public_url.starts_with("https://") {
            return Err(ConfigError::NotHttps(self.api.public_url.clone()));
        }
        Ok(self)
    }

    /// The SQLite file.
    pub fn database(&self) -> PathBuf {
        self.data_dir.join("zyrdesk.db")
    }

    /// Where the server's own keys live.
    pub fn keys_dir(&self) -> PathBuf {
        self.data_dir.join("keys")
    }
}

fn is_loopback(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => v6.is_loopback(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BEHIND_A_PROXY: &str = r#"
name = "Maison"
data_dir = "/var/lib/zyrdesk-server"

[api]
listen = "127.0.0.1:8443"
public_url = "https://zyr.exemple.fr"
"#;

    #[test]
    fn a_proxy_in_front_is_the_one_case_of_the_clear() {
        let config = Config::parse(BEHIND_A_PROXY).unwrap();
        assert_eq!(config.api.tls(), None);
        assert_eq!(config.registration.policy, Registration::Invitation);
        assert!(config.relay.enabled);
        assert_eq!(
            config.database(),
            PathBuf::from("/var/lib/zyrdesk-server/zyrdesk.db")
        );
    }

    #[test]
    fn the_clear_off_the_machine_is_refused() {
        // C'est la règle qui vit dans le code et non dans la
        // documentation : sans TLS, seule la boucle locale.
        let text = BEHIND_A_PROXY.replace("127.0.0.1:8443", "0.0.0.0:8080");
        assert!(matches!(
            Config::parse(&text).unwrap_err(),
            ConfigError::ClearOffLoopback(_)
        ));
        let text = BEHIND_A_PROXY.replace("127.0.0.1:8443", "[::1]:8443");
        assert!(Config::parse(&text).is_ok());
    }

    #[test]
    fn tls_files_come_in_pairs_and_the_public_address_is_https() {
        let text = BEHIND_A_PROXY.replace(
            "public_url",
            "tls_cert = \"/etc/zyrdesk-server/tls/server.crt\"\npublic_url",
        );
        assert!(matches!(
            Config::parse(&text).unwrap_err(),
            ConfigError::HalfTls
        ));

        let text = BEHIND_A_PROXY.replace("https://", "http://");
        assert!(matches!(
            Config::parse(&text).unwrap_err(),
            ConfigError::NotHttps(_)
        ));
    }

    #[test]
    fn everything_can_be_said() {
        let text = r#"
name = "Maison"
data_dir = "/var/lib/zyrdesk-server"

[api]
listen = "0.0.0.0:443"
tls_cert = "/etc/zyrdesk-server/tls/server.crt"
tls_key = "/etc/zyrdesk-server/tls/server.key"
public_url = "https://zyr.exemple.fr"

[registration]
policy = "open"

[relay]
enabled = false
listen = "0.0.0.0:4443"
max_sessions = 3
max_kbps_per_session = 20000

[limits]
login_attempts_per_minute = 3
"#;
        let config = Config::parse(text).unwrap();
        assert!(config.api.tls().is_some());
        assert_eq!(config.registration.policy, Registration::Open);
        assert!(!config.relay.enabled);
        assert_eq!(config.relay.max_sessions, 3);
        assert_eq!(config.limits.login_attempts_per_minute, 3);
    }

    #[test]
    fn a_key_nobody_knows_is_refused_rather_than_ignored() {
        // Une faute de frappe dans le fichier serait sinon un réglage
        // silencieusement laissé à son défaut.
        let text = BEHIND_A_PROXY.replace("name =", "nom =");
        assert!(matches!(
            Config::parse(&text).unwrap_err(),
            ConfigError::Parse(..)
        ));
    }
}
