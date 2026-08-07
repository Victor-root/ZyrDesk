//! Credentials of the host engine's local API.
//!
//! The engine's web interface cannot be turned off: it carries the
//! pairing API. So it is restricted to the local machine and protected
//! by random credentials, regenerated at every start and never shown to
//! the user.

use zyr_proto::random;

const LENGTH: usize = 32;

#[derive(Clone, PartialEq, Eq)]
pub struct Credentials {
    pub user: String,
    pub password: String,
}

impl Credentials {
    pub fn random() -> Self {
        Self {
            user: random::alphanumeric_string(LENGTH),
            password: random::alphanumeric_string(LENGTH),
        }
    }

    /// `Authorization` header value for Basic authentication.
    pub fn authorization_header(&self) -> String {
        use base64::Engine;
        let raw = format!("{}:{}", self.user, self.password);
        let encoded = base64::engine::general_purpose::STANDARD.encode(raw);
        format!("Basic {encoded}")
    }
}

/// Hides the credentials from logs and error messages.
impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credentials")
            .field("user", &"[hidden]")
            .field("password", &"[hidden]")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_are_long_and_unique() {
        let first = Credentials::random();
        let second = Credentials::random();
        assert_eq!(first.user.len(), LENGTH);
        assert_eq!(first.password.len(), LENGTH);
        assert_ne!(first.user, second.user);
        assert_ne!(first.password, second.password);
        assert_ne!(first.user, first.password);
    }

    #[test]
    fn the_header_follows_basic_authentication() {
        let credentials = Credentials {
            user: "aladdin".to_string(),
            password: "opensesame".to_string(),
        };
        assert_eq!(
            credentials.authorization_header(),
            "Basic YWxhZGRpbjpvcGVuc2VzYW1l"
        );
    }

    #[test]
    fn the_credentials_never_leak_into_the_logs() {
        let credentials = Credentials {
            user: "secret-user".to_string(),
            password: "secret-password".to_string(),
        };
        let printed = format!("{credentials:?}");
        assert!(!printed.contains("secret"), "{printed}");
    }
}
