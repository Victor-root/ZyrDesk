//! The account this computer is attached to, asked of the service.
//!
//! For diagnosis without a window: what the link says, attaching and
//! detaching, the devices of the account. The service holds all of it
//! and does all of it; this only asks, the way the window does.

use std::io::{self, BufRead, Write};
use std::process::ExitCode;

use clap::{Args as ClapArgs, Subcommand};
use zyr_control::{Answer, Attach, Registering, Request, Service};
use zyr_proto::fingerprint::Fingerprint;

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
            "detaching",
            &Request::Detach,
            "This computer is detached from its account.",
        ),
        Action::Devices => devices(),
        Action::Rename { device, name } => done_or_not(
            "renaming",
            &Request::RenameDevice { device, name },
            "Device renamed.",
        ),
        Action::Revoke { device } => done_or_not(
            "revoking",
            &Request::RevokeDevice { device },
            "Device revoked: it no longer speaks for the account.",
        ),
    }
}

fn status() -> ExitCode {
    let account = match ask(&Request::Account) {
        Ok(Answer::Account(account)) => account,
        Ok(other) => return failure("account status", unexpected(other)),
        Err(e) => return failure("account status", e),
    };
    let Some(account) = account else {
        println!("No account: this computer knows no server.");
        println!("  To attach it to one: zyr-cli account attach <server> --user <name>");
        return ExitCode::SUCCESS;
    };
    println!(
        "Account: {} on {}{}",
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
    println!("  This computer: device {}", account.device);
    if account.connected {
        println!("  Live channel: connected");
    } else {
        println!(
            "  Live channel: unreachable{}",
            account.trouble.map_or_else(String::new, |why| format!(
                "\n    {}",
                zyr_i18n::fact(&why).replace('\n', "\n    ")
            ))
        );
    }
    ExitCode::SUCCESS
}

fn attach(args: AttachArgs) -> ExitCode {
    let password = match password(args.password_stdin) {
        Ok(password) => password,
        Err(e) => return failure("reading the password", e),
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
            println!("This computer is attached to the account.");
            println!("  What it knows of it: zyr-cli account status");
            ExitCode::SUCCESS
        }
        // Neither done nor refused: the person is asked to compare, and
        // to come back with the key pinned if it is the right one.
        Ok(Answer::Unpinned { presented }) => {
            println!("This server presents a certificate nobody vouches for.");
            println!("  Fingerprint of its key: {presented}");
            println!("  If it is indeed the one the server's installation showed, run again with:");
            println!("    --trust {presented}");
            ExitCode::FAILURE
        }
        Ok(Answer::Refused(reason)) => failure("attaching to the account", zyr_i18n::fact(&reason)),
        Ok(other) => failure("attaching to the account", unexpected(other)),
        Err(e) => failure("attaching to the account", e),
    }
}

/// The password, from standard input or from a question.
///
/// Asked in the clear when asked: this is a technical tool, and a hidden
/// prompt would be a library for one line.
fn password(from_stdin: bool) -> Result<String, String> {
    if !from_stdin {
        print!("Password: ");
        io::stdout().flush().map_err(|e| e.to_string())?;
    }
    let mut line = String::new();
    io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    let password = line.trim_end_matches(['\r', '\n']).to_string();
    if password.is_empty() {
        return Err("no password".to_string());
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
        Err(e) => return failure("the account's devices", e),
    };
    if devices.is_empty() {
        println!("No device: this computer is attached to no account, or the server");
        println!("  has not answered yet. Run « zyr-cli account status ».");
        return ExitCode::SUCCESS;
    }
    println!("Devices of the account:\n");
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
            if device.this { "  (this computer)" } else { "" },
            width = widest
        );
    }
    ExitCode::SUCCESS
}

/// Where a device stands, in one phrase.
fn presence(online: bool, access: zyr_broker::rest::Access, last_seen: Option<u64>) -> String {
    if online {
        return format!("online, {}", zyr_i18n::fact(&access.fact()));
    }
    match last_seen {
        Some(seen) => format!(
            "offline, seen {}",
            ago(zyr_broker::now().saturating_sub(seen))
        ),
        None => "offline".to_string(),
    }
}

/// How long ago, in words.
fn ago(seconds: u64) -> String {
    match seconds {
        0..60 => "less than a minute ago".to_string(),
        60..3600 => format!("{} min ago", seconds / 60),
        3600..86_400 => format!("{} h ago", seconds / 3600),
        _ => format!("{} d ago", seconds / 86_400),
    }
}

/// One request that is done or refused, and nothing else.
fn done_or_not(context: &str, request: &Request, said: &str) -> ExitCode {
    match ask(request) {
        Ok(Answer::Done) => {
            println!("{said}");
            ExitCode::SUCCESS
        }
        Ok(Answer::Refused(reason)) => failure(context, zyr_i18n::fact(&reason)),
        Ok(other) => failure(context, unexpected(other)),
        Err(e) => failure(context, e),
    }
}

/// Asks the service one thing.
fn ask(request: &Request) -> Result<Answer, String> {
    runtime()?.block_on(async {
        let mut service = Service::join()
            .await
            .map_err(|e| zyr_i18n::fact(&e.fact()))?;
        service
            .ask(request)
            .await
            .map_err(|e| zyr_i18n::fact(&e.fact()))
    })
}

/// Asks the service for a list.
fn list(request: &Request) -> Result<Vec<Answer>, String> {
    runtime()?.block_on(async {
        let mut service = Service::join()
            .await
            .map_err(|e| zyr_i18n::fact(&e.fact()))?;
        service
            .ask_for_a_list(request)
            .await
            .map_err(|e| zyr_i18n::fact(&e.fact()))
    })
}

fn runtime() -> Result<tokio::runtime::Runtime, String> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())
}

fn unexpected(answer: Answer) -> String {
    zyr_i18n::fact(&answer.unexpected())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn how_long_ago_reads_in_words() {
        assert_eq!(ago(12), "less than a minute ago");
        assert_eq!(ago(200), "3 min ago");
        assert_eq!(ago(7_300), "2 h ago");
        assert_eq!(ago(200_000), "2 d ago");
    }
}
