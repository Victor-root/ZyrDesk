//! The link itself, as it is kept on disk.
//!
//! A few lines of `key = value`, the way every other note of the product
//! is kept: readable by eye, replaced whole. The token in it is the one
//! secret, under the same limit as the device's key beside it.

use std::fmt;
use std::io;
use std::path::Path;

use zyr_broker::ServerPublicKey;
use zyr_transport::Fingerprint;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// `https://host:port`, as `address::normalized` writes it.
    pub server: String,
    /// What the server calls itself, for the window.
    pub name: String,
    pub username: String,
    /// This device's identifier at the server.
    pub device: String,
    pub token: String,
    /// The server's key, pinned by a person, when no public authority
    /// vouches for its certificate.
    pub pin: Option<Fingerprint>,
    /// The key its tickets are signed with, learned at the attachment
    /// over a verified channel, and required ever after.
    pub signing_key: ServerPublicKey,
}

/// A file that is there but cannot be read as a link.
#[derive(Debug)]
pub struct Unreadable(pub String);

impl fmt::Display for Unreadable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "le lien de compte ne se lit pas : {}", self.0)
    }
}

impl Link {
    /// Reads the link, if the file is there.
    pub fn read(path: &Path) -> io::Result<Option<Link>> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        Self::parse(&text)
            .map(Some)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
    }

    fn parse(text: &str) -> Result<Link, Unreadable> {
        let mut server = None;
        let mut name = None;
        let mut username = None;
        let mut device = None;
        let mut token = None;
        let mut pin = None;
        let mut signing_key = None;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "server" => server = Some(value.to_string()),
                "name" => name = Some(value.to_string()),
                "username" => username = Some(value.to_string()),
                "device" => device = Some(value.to_string()),
                "token" => token = Some(value.to_string()),
                "pin" => {
                    pin = Some(
                        value
                            .parse()
                            .map_err(|_| Unreadable(format!("pin : {value}")))?,
                    )
                }
                "signing_key" => {
                    signing_key = Some(
                        value
                            .parse()
                            .map_err(|_| Unreadable(format!("signing_key : {value}")))?,
                    )
                }
                _ => {}
            }
        }
        let missing = |what: &str| Unreadable(format!("{what} manque"));
        Ok(Link {
            server: server.ok_or_else(|| missing("server"))?,
            name: name.unwrap_or_default(),
            username: username.ok_or_else(|| missing("username"))?,
            device: device.ok_or_else(|| missing("device"))?,
            token: token.ok_or_else(|| missing("token"))?,
            pin,
            signing_key: signing_key.ok_or_else(|| missing("signing_key"))?,
        })
    }

    fn render(&self) -> String {
        let mut lines = vec![
            "# Le lien de cet appareil à un compte ZyrDesk. Le jeton est un secret :".to_string(),
            "# effacer ce fichier détache l'appareil.".to_string(),
            format!("server = {}", self.server),
            format!("name = {}", self.name),
            format!("username = {}", self.username),
            format!("device = {}", self.device),
            format!("token = {}", self.token),
            format!("signing_key = {}", self.signing_key),
        ];
        if let Some(pin) = self.pin {
            lines.push(format!("pin = {pin}"));
        }
        lines.push(String::new());
        lines.join("\n")
    }

    /// Writes the link, whole or not at all.
    pub fn write(&self, path: &Path) -> io::Result<()> {
        zyr_proto::files::replace(path, &self.render())
    }

    /// Detaches: the file goes, and with it the token.
    pub fn remove(path: &Path) -> io::Result<()> {
        match std::fs::remove_file(path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zyr_broker::ServerKey;
    use zyr_transport::Identity;

    fn fresh_file(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "zyrdesk-link-{name}-{}.conf",
            zyr_proto::random::alphanumeric_string(8)
        ))
    }

    #[test]
    fn a_link_survives_the_disk_with_or_without_a_pin() {
        let path = fresh_file("aller-retour");
        assert_eq!(Link::read(&path).unwrap(), None);
        let mut link = Link {
            server: "https://zyr.exemple.fr:443".into(),
            name: "Maison".into(),
            username: "victor".into(),
            device: "d1".into(),
            token: "un jeton".into(),
            pin: Some(Identity::generate().unwrap().fingerprint()),
            signing_key: ServerKey::generate().public(),
        };
        link.write(&path).unwrap();
        assert_eq!(Link::read(&path).unwrap(), Some(link.clone()));
        link.pin = None;
        link.write(&path).unwrap();
        assert_eq!(Link::read(&path).unwrap(), Some(link));
        Link::remove(&path).unwrap();
        assert_eq!(Link::read(&path).unwrap(), None);
        Link::remove(&path).unwrap();
    }

    #[test]
    fn a_file_that_is_not_a_link_is_said_rather_than_taken_for_none() {
        // Un lien illisible n'est pas « pas de lien » : le service doit le
        // dire, sinon un jeton perdu passerait pour un choix.
        let path = fresh_file("illisible");
        std::fs::write(&path, "server = https://x:443\nusername = v\n").unwrap();
        let refusal = Link::read(&path).unwrap_err();
        assert_eq!(refusal.kind(), io::ErrorKind::InvalidData);
        assert!(refusal.to_string().contains("device"), "{refusal}");
        std::fs::remove_file(&path).unwrap();
    }
}
