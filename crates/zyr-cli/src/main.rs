mod doctor;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "zyr-cli",
    version = zyr_proto::PRODUCT_VERSION,
    about = "Outil technique ZyrDesk"
)]
struct Cli {
    #[command(subcommand)]
    commande: Commande,
}

#[derive(Subcommand)]
enum Commande {
    /// Vérifie que cette machine est prête pour ZyrDesk
    Doctor,
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    match cli.commande {
        Commande::Doctor => doctor::executer(),
    }
}
