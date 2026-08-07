//! This machine's fingerprint.
//!
//! It is what the other computer has to know before it accepts a
//! connection. It is created on first request and never changes: making
//! it again would break every existing pairing.

use std::process::ExitCode;

use zyr_proto::paths;
use zyr_transport::Identity;

use crate::failure;

pub fn run() -> ExitCode {
    let folder = paths::identity_dir();
    match Identity::load_or_create(&folder) {
        Ok(identity) => {
            println!("{}", identity.fingerprint());
            println!("\n  Conservée dans {}", folder.display());
            ExitCode::SUCCESS
        }
        Err(e) => failure("identité de cette machine", e),
    }
}
