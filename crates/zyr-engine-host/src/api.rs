//! Dialogue avec l'API locale du moteur hôte.
//!
//! Deux usages seulement : vérifier que le moteur répond, et lui
//! transmettre un code d'appairage. Tout passe par 127.0.0.1.
//!
//! Le certificat du moteur est auto-signé et régénéré par lui : sa
//! vérification est désactivée. C'est sans conséquence ici car la
//! connexion ne quitte jamais la machine, et l'authentification repose
//! sur des identifiants aléatoires connus de nous seuls.

use std::fmt;
use std::time::{Duration, Instant};

use zyr_proto::net::EnginePorts;

use crate::credentials::Credentials;

const DELAI_REQUETE: Duration = Duration::from_secs(5);
const INTERVALLE_ATTENTE: Duration = Duration::from_millis(250);

#[derive(Debug)]
pub enum ErreurApi {
    Transport(String),
    StatutHttp(u16),
    ReponseIllisible(String),
    PinRefuse,
    Delai,
}

impl fmt::Display for ErreurApi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErreurApi::Transport(e) => write!(f, "moteur injoignable : {e}"),
            ErreurApi::StatutHttp(c) => write!(f, "le moteur a répondu {c}"),
            ErreurApi::ReponseIllisible(e) => write!(f, "réponse du moteur illisible : {e}"),
            ErreurApi::PinRefuse => write!(f, "code d'appairage refusé par le moteur"),
            ErreurApi::Delai => write!(f, "le moteur n'a pas répondu dans le délai imparti"),
        }
    }
}

impl std::error::Error for ErreurApi {}

/// Adresse de la sonde de santé, servie en clair sur le port de base.
pub fn url_sante(ports: EnginePorts) -> String {
    format!("http://127.0.0.1:{}/serverinfo", ports.http())
}

/// Requête d'appairage, construite sans effet de bord pour être vérifiable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequetePin {
    pub url: String,
    pub autorisation: String,
    pub corps: String,
}

/// Construit la soumission d'un code d'appairage.
///
/// L'absence d'en-tête `Origin` est délibérée : le moteur réserve sa
/// protection contre les requêtes inter-sites aux clients de type
/// navigateur, et l'exemption vaut pour les appels programmatiques.
pub fn requete_pin(
    ports: EnginePorts,
    creds: &Credentials,
    pin: &str,
    nom_appareil: &str,
) -> RequetePin {
    RequetePin {
        url: format!("https://127.0.0.1:{}/api/pin", ports.web_ui()),
        autorisation: creds.en_tete_autorisation(),
        corps: serde_json::json!({ "pin": pin, "name": nom_appareil }).to_string(),
    }
}

/// Interprète la réponse d'une soumission de code d'appairage.
pub fn interpreter_reponse_pin(corps: &str) -> Result<(), ErreurApi> {
    let valeur: serde_json::Value =
        serde_json::from_str(corps).map_err(|e| ErreurApi::ReponseIllisible(e.to_string()))?;
    match valeur.get("status").and_then(|s| s.as_bool()) {
        Some(true) => Ok(()),
        Some(false) => Err(ErreurApi::PinRefuse),
        None => Err(ErreurApi::ReponseIllisible(
            "champ « status » absent ou non booléen".to_string(),
        )),
    }
}

pub struct EngineApi {
    ports: EnginePorts,
    creds: Credentials,
    agent: ureq::Agent,
}

impl EngineApi {
    pub fn nouvelle(ports: EnginePorts, creds: Credentials) -> Self {
        let tls = ureq::tls::TlsConfig::builder()
            .disable_verification(true)
            .build();
        let agent = ureq::Agent::config_builder()
            .tls_config(tls)
            .timeout_global(Some(DELAI_REQUETE))
            .build()
            .new_agent();
        Self {
            ports,
            creds,
            agent,
        }
    }

    /// Vrai si le moteur répond sur sa sonde de santé.
    pub fn sante(&self) -> Result<(), ErreurApi> {
        let reponse = self
            .agent
            .get(url_sante(self.ports))
            .call()
            .map_err(|e| ErreurApi::Transport(e.to_string()))?;
        let statut = reponse.status().as_u16();
        if (200..300).contains(&statut) {
            Ok(())
        } else {
            Err(ErreurApi::StatutHttp(statut))
        }
    }

    /// Attend que le moteur réponde, jusqu'au délai indiqué.
    pub fn attendre_disponible(&self, delai: Duration) -> Result<(), ErreurApi> {
        let echeance = Instant::now() + delai;
        loop {
            match self.sante() {
                Ok(()) => return Ok(()),
                Err(_) if Instant::now() < echeance => std::thread::sleep(INTERVALLE_ATTENTE),
                Err(_) => return Err(ErreurApi::Delai),
            }
        }
    }

    /// Transmet un code d'appairage au moteur.
    pub fn soumettre_pin(&self, pin: &str, nom_appareil: &str) -> Result<(), ErreurApi> {
        let requete = requete_pin(self.ports, &self.creds, pin, nom_appareil);
        let mut reponse = self
            .agent
            .post(&requete.url)
            .header("Authorization", &requete.autorisation)
            .header("Content-Type", "application/json")
            .send(&requete.corps)
            .map_err(|e| ErreurApi::Transport(e.to_string()))?;
        let statut = reponse.status().as_u16();
        if !(200..300).contains(&statut) {
            return Err(ErreurApi::StatutHttp(statut));
        }
        let corps = reponse
            .body_mut()
            .read_to_string()
            .map_err(|e| ErreurApi::ReponseIllisible(e.to_string()))?;
        interpreter_reponse_pin(&corps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ports() -> EnginePorts {
        EnginePorts::new(42100).unwrap()
    }

    fn creds() -> Credentials {
        Credentials {
            utilisateur: "u".to_string(),
            mot_de_passe: "p".to_string(),
        }
    }

    #[test]
    fn la_sonde_de_sante_vise_le_port_de_base_en_clair() {
        assert_eq!(url_sante(ports()), "http://127.0.0.1:42100/serverinfo");
    }

    #[test]
    fn la_requete_d_appairage_vise_l_interface_locale_en_https() {
        let r = requete_pin(ports(), &creds(), "1234", "PC-PORTABLE");
        assert_eq!(r.url, "https://127.0.0.1:42101/api/pin");
        assert_eq!(r.autorisation, "Basic dTpw");
    }

    #[test]
    fn le_corps_est_du_json_valide_et_echappe() {
        let r = requete_pin(ports(), &creds(), "0042", "PC \"guillemets\"");
        let v: serde_json::Value = serde_json::from_str(&r.corps).unwrap();
        assert_eq!(v["pin"], "0042");
        assert_eq!(v["name"], "PC \"guillemets\"");
    }

    #[test]
    fn les_reponses_d_appairage_sont_interpretees() {
        assert!(interpreter_reponse_pin(r#"{"status": true}"#).is_ok());
        assert!(matches!(
            interpreter_reponse_pin(r#"{"status": false}"#),
            Err(ErreurApi::PinRefuse)
        ));
        assert!(matches!(
            interpreter_reponse_pin(r#"{"autre": 1}"#),
            Err(ErreurApi::ReponseIllisible(_))
        ));
        assert!(matches!(
            interpreter_reponse_pin("pas du json"),
            Err(ErreurApi::ReponseIllisible(_))
        ));
    }
}
