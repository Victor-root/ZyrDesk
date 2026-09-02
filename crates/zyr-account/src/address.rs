//! The address of a server, as a person types it and as it is kept.
//!
//! Typed as `zyr.exemple.fr`, `zyr.exemple.fr:8443` or with `https://`
//! in front; kept as `https://host:port`, always. `http://` is refused
//! here, before anything is tried: a server is never spoken to in the
//! clear.

use std::fmt;

/// The port a server answers on unless another is written.
const DEFAULT_PORT: u16 = 443;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BadAddress {
    Empty,
    /// `http://` was written: the clear is refused before being tried.
    NotHttps,
    /// A port that is not a number, or a host with a slash in it.
    Malformed(String),
}

impl fmt::Display for BadAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BadAddress::Empty => f.write_str("aucune adresse de serveur"),
            BadAddress::NotHttps => f.write_str(
                "un serveur ZyrDesk ne se joint qu'en https:// : l'adresse en http:// est refusée",
            ),
            BadAddress::Malformed(text) => write!(f, "adresse de serveur illisible : {text}"),
        }
    }
}

impl std::error::Error for BadAddress {}

/// `https://host:port`, from whatever was typed.
pub fn normalized(typed: &str) -> Result<String, BadAddress> {
    let text = typed.trim().trim_end_matches('/');
    if text.is_empty() {
        return Err(BadAddress::Empty);
    }
    if text.starts_with("http://") {
        return Err(BadAddress::NotHttps);
    }
    let text = text.strip_prefix("https://").unwrap_or(text);
    if text.is_empty() || text.contains('/') || text.contains(' ') {
        return Err(BadAddress::Malformed(typed.trim().to_string()));
    }
    let (host, port) = host_and_port(text)?;
    Ok(format!("https://{host}:{port}"))
}

/// The host and the port of a normalized address, or of what was typed.
pub fn host_and_port(address: &str) -> Result<(String, u16), BadAddress> {
    let text = address.strip_prefix("https://").unwrap_or(address);
    // An IPv6 address is written between brackets, and its colons are
    // not a port.
    if let Some(rest) = text.strip_prefix('[') {
        let (host, after) = rest
            .split_once(']')
            .ok_or_else(|| BadAddress::Malformed(address.to_string()))?;
        let port = match after.strip_prefix(':') {
            Some(port) => port
                .parse()
                .map_err(|_| BadAddress::Malformed(address.to_string()))?,
            None if after.is_empty() => DEFAULT_PORT,
            None => return Err(BadAddress::Malformed(address.to_string())),
        };
        return Ok((format!("[{host}]"), port));
    }
    match text.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') => {
            let port = port
                .parse()
                .map_err(|_| BadAddress::Malformed(address.to_string()))?;
            if host.is_empty() {
                return Err(BadAddress::Malformed(address.to_string()));
            }
            Ok((host.to_string(), port))
        }
        _ => Ok((text.to_string(), DEFAULT_PORT)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_typed_becomes_one_shape() {
        for (typed, kept) in [
            ("zyr.exemple.fr", "https://zyr.exemple.fr:443"),
            ("  https://zyr.exemple.fr/ ", "https://zyr.exemple.fr:443"),
            ("zyr.exemple.fr:8443", "https://zyr.exemple.fr:8443"),
            ("192.168.1.40", "https://192.168.1.40:443"),
            ("[fd00::1]:8443", "https://[fd00::1]:8443"),
            ("[fd00::1]", "https://[fd00::1]:443"),
        ] {
            assert_eq!(normalized(typed).unwrap(), kept, "{typed}");
        }
        assert_eq!(
            host_and_port("https://zyr.exemple.fr:8443").unwrap(),
            ("zyr.exemple.fr".to_string(), 8443)
        );
    }

    #[test]
    fn the_clear_and_the_unreadable_are_refused() {
        // Le clair se refuse avant d'être essayé : c'est la règle du
        // produit, et elle vit ici.
        assert_eq!(
            normalized("http://zyr.exemple.fr"),
            Err(BadAddress::NotHttps)
        );
        assert_eq!(normalized("   "), Err(BadAddress::Empty));
        assert!(matches!(
            normalized("zyr.exemple.fr/api"),
            Err(BadAddress::Malformed(_))
        ));
        assert!(matches!(
            normalized("zyr.exemple.fr:port"),
            Err(BadAddress::Malformed(_))
        ));
        assert!(matches!(normalized(":443"), Err(BadAddress::Malformed(_))));
    }
}
