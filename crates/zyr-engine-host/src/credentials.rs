//! Identifiants de l'API locale du moteur hôte.
//!
//! L'interface web du moteur ne peut pas être désactivée : elle porte
//! l'API d'appairage. Elle est donc restreinte au PC local et protégée
//! par des identifiants aléatoires, régénérés à chaque démarrage et
//! jamais montrés à l'utilisateur.

use zyr_proto::alea;

const LONGUEUR: usize = 32;

#[derive(Clone, PartialEq, Eq)]
pub struct Credentials {
    pub utilisateur: String,
    pub mot_de_passe: String,
}

impl Credentials {
    pub fn aleatoires() -> Self {
        Self {
            utilisateur: alea::chaine_alphanumerique(LONGUEUR),
            mot_de_passe: alea::chaine_alphanumerique(LONGUEUR),
        }
    }

    /// Valeur d'en-tête `Authorization` pour l'authentification Basic.
    pub fn en_tete_autorisation(&self) -> String {
        use base64::Engine;
        let brut = format!("{}:{}", self.utilisateur, self.mot_de_passe);
        let encode = base64::engine::general_purpose::STANDARD.encode(brut);
        format!("Basic {encode}")
    }
}

/// Masque les identifiants dans les journaux et les messages d'erreur.
impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credentials")
            .field("utilisateur", &"[masqué]")
            .field("mot_de_passe", &"[masqué]")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiants_longs_et_uniques() {
        let a = Credentials::aleatoires();
        let b = Credentials::aleatoires();
        assert_eq!(a.utilisateur.len(), LONGUEUR);
        assert_eq!(a.mot_de_passe.len(), LONGUEUR);
        assert_ne!(a.utilisateur, b.utilisateur);
        assert_ne!(a.mot_de_passe, b.mot_de_passe);
        assert_ne!(a.utilisateur, a.mot_de_passe);
    }

    #[test]
    fn en_tete_conforme_a_l_authentification_basic() {
        let creds = Credentials {
            utilisateur: "aladdin".to_string(),
            mot_de_passe: "opensesame".to_string(),
        };
        assert_eq!(
            creds.en_tete_autorisation(),
            "Basic YWxhZGRpbjpvcGVuc2VzYW1l"
        );
    }

    #[test]
    fn les_identifiants_ne_fuient_pas_dans_les_journaux() {
        let creds = Credentials {
            utilisateur: "utilisateur-secret".to_string(),
            mot_de_passe: "mot-de-passe-secret".to_string(),
        };
        let trace = format!("{creds:?}");
        assert!(!trace.contains("secret"), "{trace}");
    }
}
