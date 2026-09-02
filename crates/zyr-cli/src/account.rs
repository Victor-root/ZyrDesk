//! The account this computer is attached to, asked of the service.
//!
//! For diagnosis without a window: what the link says, attaching and
//! detaching, the devices of the account. The service holds all of it
//! and does all of it; this only asks, the way the window does.

use std::io::{self, BufRead, Write};
use std::process::ExitCode;

use clap::{Args as ClapArgs, Subcommand};
use zyr_control::{Answer, Attach, Registering, Request, Service};
use zyr_transport::Fingerprint;

use crate::failure;

#[derive(Subcommand)]
pub enum Action {
    /// Says whether this computer is attached to an account, and how the
    /// link stands
    Status,
    /// Attaches this computer to an account on a server
    Attach(AttachArgs),
    /// Takes this computer off its account, and revokes it at the server
    Detach,
    /// Lists the devices of the account
    Devices,
    /// Renames a device of the account, by its identifier
    Rename { device: String, name: String },
    /// Revokes a device of the account, by its identifier
    Revoke { device: String },
}

#[derive(ClapArgs)]
pub struct AttachArgs {
    /// Address of the server, as "zyr.exemple.fr" or "zyr.exemple.fr:8443".
    /// Only https is ever spoken; "http://" is refused
    server: String,

    /// Username of the account
    #[arg(long)]
    user: String,

    /// Reads the password from standard input instead of asking for it
    #[arg(long)]
    password_stdin: bool,

    /// Creates the account first
    #[arg(long)]
    register: bool,

    /// E-mail address of the account, when creating it
    #[arg(long, requires = "register")]
    email: Option<String>,

    /// Invitation code, when the server asks for one to create an account
    #[arg(long, requires = "register")]
    invitation: Option<String>,

    /// What this computer is called at the server. The name Windows gives
    /// it otherwise
    #[arg(long)]
    name: Option<String>,

    /// The key of a server nobody vouches for, as its installation showed
    /// it: "attach" without it says which key the server presents
    #[arg(long, value_name = "FINGERPRINT")]
    trust: Option<Fingerprint>,
}

pub fn run(action: Action) -> ExitCode {
    match action {
        Action::Status => status(),
        Action::Attach(args) => attach(args),
        Action::Detach => done_or_not(
            "détachement",
            &Request::Detach,
            "Cet ordinateur est détaché de son compte.",
        ),
        Action::Devices => devices(),
        Action::Rename { device, name } => done_or_not(
            "renommage",
            &Request::RenameDevice { device, name },
            "Appareil renommé.",
        ),
        Action::Revoke { device } => done_or_not(
            "révocation",
            &Request::RevokeDevice { device },
            "Appareil révoqué : il ne parle plus au nom du compte.",
        ),
    }
}

fn status() -> ExitCode {
    let account = match ask(&Request::Account) {
        Ok(Answer::Account(account)) => account,
        Ok(other) => return failure("état du compte", unexpected(other)),
        Err(e) => return failure("état du compte", e),
    };
    let Some(account) = account else {
        println!("Aucun compte : cet ordinateur ne connaît aucun serveur.");
        println!("  Pour l'y rattacher : zyr-cli account attach <serveur> --user <nom>");
        return ExitCode::SUCCESS;
    };
    println!(
        "Compte : {} sur {}{}",
        account.username,
        if account.name.is_empty() {
            account.server.clone()
        } else {
            account.name.clone()
        },
        if account.name.is_empty() {
            String::new()
        } else {
            format!(" ({})", account.server)
        }
    );
    println!("  Cet ordinateur : appareil {}", account.device);
    if account.connected {
        println!("  Canal vivant : relié");
    } else {
        println!(
            "  Canal vivant : injoignable{}",
            account.trouble.map_or_else(String::new, |why| format!(
                "\n    {}",
                why.replace('\n', "\n    ")
            ))
        );
    }
    ExitCode::SUCCESS
}

fn attach(args: AttachArgs) -> ExitCode {
    let password = match password(args.password_stdin) {
        Ok(password) => password,
        Err(e) => return failure("lecture du mot de passe", e),
    };
    let request = Request::Attach(Attach {
        server: args.server.clone(),
        username: args.user,
        password,
        register: args.register.then_some(Registering {
            email: args.email,
            invitation: args.invitation,
        }),
        name: args.name.unwrap_or_default(),
        pin: args.trust,
    });
    match ask(&request) {
        Ok(Answer::Done) => {
            println!("Cet ordinateur est rattaché au compte.");
            println!("  Ce qu'il en sait : zyr-cli account status");
            ExitCode::SUCCESS
        }
        // Neither done nor refused: the person is asked to compare, and
        // to come back with the key pinned if it is the right one.
        Ok(Answer::Unpinned { presented }) => {
            println!("Ce serveur présente un certificat que personne ne garantit.");
            println!("  Empreinte de sa clé : {presented}");
            println!(
                "  Si c'est bien celle que l'installation du serveur a affichée, relancez avec :"
            );
            println!("    --trust {presented}");
            ExitCode::FAILURE
        }
        Ok(Answer::Refused(reason)) => failure("rattachement au compte", reason),
        Ok(other) => failure("rattachement au compte", unexpected(other)),
        Err(e) => failure("rattachement au compte", e),
    }
}

/// The password, from standard input or from a question.
///
/// Asked in the clear when asked: this is a technical tool, and a hidden
/// prompt would be a library for one line.
fn password(from_stdin: bool) -> Result<String, String> {
    if !from_stdin {
        print!("Mot de passe : ");
        io::stdout().flush().map_err(|e| e.to_string())?;
    }
    let mut line = String::new();
    io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    let password = line.trim_end_matches(['\r', '\n']).to_string();
    if password.is_empty() {
        return Err("aucun mot de passe".to_string());
    }
    Ok(password)
}

fn devices() -> ExitCode {
    let devices = match list(&Request::Devices) {
        Ok(answers) => answers
            .into_iter()
            .filter_map(|answer| match answer {
                Answer::Device(device) => Some(device),
                _ => None,
            })
            .collect::<Vec<_>>(),
        Err(e) => return failure("appareils du compte", e),
    };
    if devices.is_empty() {
        println!("Aucun appareil : cet ordinateur n'est rattaché à aucun compte, ou le serveur");
        println!("  n'a pas encore répondu. Lancez « zyr-cli account status ».");
        return ExitCode::SUCCESS;
    }
    println!("Appareils du compte :\n");
    let widest = devices
        .iter()
        .map(|device| device.name.chars().count())
        .max()
        .unwrap_or(0);
    for device in devices {
        println!(
            "  {:<8} {:<width$}  {}{}",
            device.id,
            device.name,
            presence(device.online, device.access, device.last_seen),
            if device.this {
                "  (cet ordinateur)"
            } else {
                ""
            },
            width = widest
        );
    }
    ExitCode::SUCCESS
}

/// Where a device stands, in one phrase.
fn presence(online: bool, access: zyr_broker::rest::Access, last_seen: Option<u64>) -> String {
    if online {
        return format!("en ligne, {}", access.explanation());
    }
    match last_seen {
        Some(seen) => format!(
            "hors ligne, vu {}",
            ago(zyr_broker::now().saturating_sub(seen))
        ),
        None => "hors ligne".to_string(),
    }
}

/// How long ago, in words.
fn ago(seconds: u64) -> String {
    match seconds {
        0..60 => "il y a moins d'une minute".to_string(),
        60..3600 => format!("il y a {} min", seconds / 60),
        3600..86_400 => format!("il y a {} h", seconds / 3600),
        _ => format!("il y a {} j", seconds / 86_400),
    }
}

/// One request that is done or refused, and nothing else.
fn done_or_not(context: &str, request: &Request, said: &str) -> ExitCode {
    match ask(request) {
        Ok(Answer::Done) => {
            println!("{said}");
            ExitCode::SUCCESS
        }
        Ok(Answer::Refused(reason)) => failure(context, reason),
        Ok(other) => failure(context, unexpected(other)),
        Err(e) => failure(context, e),
    }
}

/// Asks the service one thing.
fn ask(request: &Request) -> Result<Answer, String> {
    runtime()?.block_on(async {
        let mut service = Service::join().await.map_err(|e| e.to_string())?;
        service.ask(request).await.map_err(|e| e.to_string())
    })
}

/// Asks the service for a list.
fn list(request: &Request) -> Result<Vec<Answer>, String> {
    runtime()?.block_on(async {
        let mut service = Service::join().await.map_err(|e| e.to_string())?;
        service
            .ask_for_a_list(request)
            .await
            .map_err(|e| e.to_string())
    })
}

fn runtime() -> Result<tokio::runtime::Runtime, String> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())
}

fn unexpected(answer: Answer) -> String {
    format!("réponse inattendue du service : {answer}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn how_long_ago_reads_in_words() {
        assert_eq!(ago(12), "il y a moins d'une minute");
        assert_eq!(ago(200), "il y a 3 min");
        assert_eq!(ago(7_300), "il y a 2 h");
        assert_eq!(ago(200_000), "il y a 2 j");
    }
}
