//! Génération de la configuration du moteur hôte (Sunshine officiel, non modifié).
//!
//! Le moteur est piloté exclusivement par ses interfaces officielles :
//! fichier de configuration, ligne de commande et API REST locale.
//! Les valeurs produites ici appliquent la politique décrite dans
//! docs/engines/STRATEGY.md : liaison loopback stricte, interface web
//! verrouillée, icône de zone de notification désactivée, chemins d'état
//! sous le répertoire de données ZyrDesk.

pub mod config;

pub use config::{ChiffrementInterne, SunshineConfig};
