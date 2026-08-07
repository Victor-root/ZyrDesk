//! Empreinte de cette machine.
//!
//! C'est ce que l'autre ordinateur doit connaître pour accepter une
//! connexion. Elle est créée à la première demande et ne change plus :
//! la refaire romprait tous les appairages existants.

use std::process::ExitCode;

use zyr_proto::paths;
use zyr_transport::Identite;

use crate::echec;

pub fn executer() -> ExitCode {
    let dossier = paths::identite_dir();
    match Identite::charger_ou_creer(&dossier) {
        Ok(identite) => {
            println!("{}", identite.empreinte());
            println!("\n  Conservée dans {}", dossier.display());
            ExitCode::SUCCESS
        }
        Err(e) => echec("identité de cette machine", e),
    }
}
