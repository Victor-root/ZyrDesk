//! `zyrdesk-server`: the server, and the administrator's hand on it.
//!
//! Without a command it runs; with one it does the gesture and returns.
//! Everything an administrator does happens on the machine, against the
//! same file the server reads, and never over the network.

use std::io::{IsTerminal, Read};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use zyr_broker::now;
use zyr_server::config::{self, Config};
use zyr_server::keys::{self, Tls};
use zyr_server::store::Store;

#[derive(Parser)]
#[command(name = "zyrdesk-server", version = zyr_proto::PRODUCT_VERSION, about = "Serveur ZyrDesk : comptes, mise en relation et relais")]
struct Cli {
    /// Le fichier de configuration.
    #[arg(long, default_value = config::DEFAULT_PATH, global = true)]
    config: PathBuf,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Sert, jusqu'à ce qu'on l'arrête.
    Run,
    /// Où en est le serveur.
    Status,
    /// Les comptes.
    User {
        #[command(subcommand)]
        action: UserAction,
    },
    /// Les codes d'invitation.
    Invite {
        #[command(subcommand)]
        action: InviteAction,
    },
    /// L'empreinte à comparer dans l'application.
    Fingerprint,
    /// Une copie cohérente de la base, de la configuration et des clés.
    Backup { folder: PathBuf },
}

#[derive(Subcommand)]
enum UserAction {
    /// Crée un compte, quelle que soit la politique d'inscription.
    Create {
        username: String,
        #[arg(long)]
        email: Option<String>,
        /// Lit le mot de passe sur l'entrée standard, une ligne.
        #[arg(long)]
        password_stdin: bool,
    },
    List,
    /// Remet un mot de passe, ce qui déconnecte le compte partout.
    ResetPassword {
        username: String,
        #[arg(long)]
        password_stdin: bool,
    },
    /// Supprime un compte et tout ce qui était à lui.
    Delete {
        username: String,
    },
}

#[derive(Subcommand)]
enum InviteAction {
    New,
    List,
    Revoke { code: String },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let outcome = match cli.command.unwrap_or(Command::Run) {
        Command::Run => run(&cli.config),
        Command::Status => status(&cli.config),
        Command::User { action } => user(&cli.config, action),
        Command::Invite { action } => invite(&cli.config, action),
        Command::Fingerprint => fingerprint(&cli.config),
        Command::Backup { folder } => backup(&cli.config, &folder),
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn load(path: &Path) -> Result<Config, String> {
    Config::load(path).map_err(|e| e.to_string())
}

fn open_store(config: &Config) -> Result<Store, String> {
    Store::open(&config.database()).map_err(|e| e.to_string())
}

fn run(path: &Path) -> Result<(), String> {
    let config = load(path)?;
    let runtime = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    runtime.block_on(async {
        zyr_server::journal::say(zyr_proto::version_line());
        let running = zyr_server::start(config).await.map_err(|e| e.to_string())?;
        wait_for_a_stop().await;
        zyr_server::journal::say("stopping");
        running.stop().await;
        Ok(())
    })
}

#[cfg(unix)]
async fn wait_for_a_stop() {
    use tokio::signal::unix::{SignalKind, signal};
    let mut terminate = match signal(SignalKind::terminate()) {
        Ok(terminate) => terminate,
        Err(_) => {
            let _ = tokio::signal::ctrl_c().await;
            return;
        }
    };
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = terminate.recv() => {}
    }
}

#[cfg(not(unix))]
async fn wait_for_a_stop() {
    let _ = tokio::signal::ctrl_c().await;
}

fn status(path: &Path) -> Result<(), String> {
    let config = load(path)?;
    let store = open_store(&config)?;
    let counts = store.counts().map_err(|e| e.to_string())?;
    println!("{}", zyr_proto::version_line());
    println!(
        "Serveur        : {} ({})",
        config.name, config.api.public_url
    );
    let listening = {
        let mut probe = config.api.listen;
        if probe.ip().is_unspecified() {
            probe.set_ip(if probe.is_ipv4() {
                std::net::Ipv4Addr::LOCALHOST.into()
            } else {
                std::net::Ipv6Addr::LOCALHOST.into()
            });
        }
        std::net::TcpStream::connect_timeout(&probe, std::time::Duration::from_secs(1)).is_ok()
    };
    println!(
        "État           : {}",
        if listening {
            "en marche, l'API répond"
        } else {
            "arrêté, ou l'API ne répond pas"
        }
    );
    println!("Comptes        : {}", counts.accounts);
    println!("Appareils      : {}", counts.devices);
    println!("Contacts       : {}", counts.contacts);
    println!("Partages       : {}", counts.shares);
    println!(
        "Inscriptions   : {}",
        match config.registration.policy {
            zyr_broker::rest::Registration::Open => "ouvertes",
            zyr_broker::rest::Registration::Invitation => "sur invitation",
            zyr_broker::rest::Registration::Closed => "fermées",
        }
    );
    println!("Données        : {}", config.data_dir.display());
    Ok(())
}

/// One line of the standard input, which is how a password reaches a
/// command without appearing in the list of processes.
fn password_from_stdin(asked: bool) -> Result<String, String> {
    if !asked {
        return Err(
            "le mot de passe se lit sur l'entrée standard : ajouter --password-stdin et \
             l'écrire sur une ligne"
                .to_string(),
        );
    }
    let mut read = String::new();
    if std::io::stdin().is_terminal() {
        eprintln!("Mot de passe (douze caractères au moins), puis Entrée :");
    }
    std::io::stdin()
        .read_to_string(&mut read)
        .map_err(|e| e.to_string())?;
    let password = read.lines().next().unwrap_or_default().to_string();
    Ok(password)
}

fn user(path: &Path, action: UserAction) -> Result<(), String> {
    let config = load(path)?;
    let store = open_store(&config)?;
    match action {
        UserAction::Create {
            username,
            email,
            password_stdin,
        } => {
            let password = password_from_stdin(password_stdin)?;
            let account = store
                .create_account(
                    &username,
                    &password,
                    email.as_deref(),
                    None,
                    zyr_broker::rest::Registration::Open,
                    now(),
                )
                .map_err(refused)?;
            println!("Compte {} créé.", account.username);
        }
        UserAction::List => {
            for account in store.accounts().map_err(|e| e.to_string())? {
                let devices = store
                    .devices_of(&account.id)
                    .map_err(|e| e.to_string())?
                    .len();
                println!(
                    "{:<32} {} appareil{}{}",
                    account.username,
                    devices,
                    if devices == 1 { "" } else { "s" },
                    account
                        .email
                        .map(|email| format!("  {email}"))
                        .unwrap_or_default()
                );
            }
        }
        UserAction::ResetPassword {
            username,
            password_stdin,
        } => {
            let password = password_from_stdin(password_stdin)?;
            store
                .reset_password(&username, &password)
                .map_err(refused)?;
            println!("Mot de passe de {username} remis. Le compte est déconnecté partout.");
        }
        UserAction::Delete { username } => {
            store.delete_account(&username).map_err(refused)?;
            println!("Compte {username} supprimé, avec ses appareils, contacts et partages.");
        }
    }
    Ok(())
}

fn invite(path: &Path, action: InviteAction) -> Result<(), String> {
    let config = load(path)?;
    let store = open_store(&config)?;
    match action {
        InviteAction::New => {
            let code = store.new_invitation(now()).map_err(|e| e.to_string())?;
            println!("{code}");
        }
        InviteAction::List => {
            for invitation in store.invitations().map_err(|e| e.to_string())? {
                println!(
                    "{}  {}",
                    invitation.code,
                    match invitation.used {
                        Some(_) => "employé",
                        None => "libre",
                    }
                );
            }
        }
        InviteAction::Revoke { code } => {
            store.revoke_invitation(&code).map_err(refused)?;
            println!("Code {code} retiré.");
        }
    }
    Ok(())
}

fn refused(fault: zyr_server::store::Fault) -> String {
    match fault {
        zyr_server::store::Fault::Refused(code) => code.explanation().to_string(),
        other => other.to_string(),
    }
}

/// Eight groups of eight, which is how a person compares two of them.
fn grouped(fingerprint: &zyr_transport::Fingerprint) -> String {
    let text = fingerprint.to_string();
    text.as_bytes()
        .chunks(8)
        .map(|chunk| std::str::from_utf8(chunk).unwrap_or_default())
        .collect::<Vec<_>>()
        .join(" ")
}

fn fingerprint(path: &Path) -> Result<(), String> {
    let config = load(path)?;
    let key = keys::load_or_create_signing_key(&config.keys_dir()).map_err(|e| e.to_string())?;
    match config.api.tls() {
        Some((certificate, key_file)) => {
            let tls = Tls::load(certificate, key_file).map_err(|e| e.to_string())?;
            match tls.fingerprint() {
                Some(fingerprint) => {
                    println!("Empreinte du serveur, à comparer dans l'application :");
                    println!("  {}", grouped(&fingerprint));
                }
                None => println!("Le certificat ne se lit pas : pas de clé publique trouvée."),
            }
        }
        None => println!(
            "Derrière un mandataire inverse, le certificat est le sien : rien à comparer dans \
             l'application."
        ),
    }
    println!("Clé de signature : {}", key.public());
    Ok(())
}

fn backup(path: &Path, folder: &Path) -> Result<(), String> {
    let config = load(path)?;
    let store = open_store(&config)?;
    std::fs::create_dir_all(folder).map_err(|e| format!("{} : {e}", folder.display()))?;
    let database = folder.join("zyrdesk.db");
    if database.exists() {
        std::fs::remove_file(&database).map_err(|e| format!("{} : {e}", database.display()))?;
    }
    store.copy_to(&database).map_err(|e| e.to_string())?;
    std::fs::copy(path, folder.join("server.toml"))
        .map_err(|e| format!("{} : {e}", path.display()))?;
    let keys = folder.join("keys");
    std::fs::create_dir_all(&keys).map_err(|e| format!("{} : {e}", keys.display()))?;
    for entry in std::fs::read_dir(config.keys_dir())
        .map_err(|e| format!("{} : {e}", config.keys_dir().display()))?
    {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.path().is_file() {
            std::fs::copy(entry.path(), keys.join(entry.file_name()))
                .map_err(|e| format!("{} : {e}", entry.path().display()))?;
        }
    }
    println!("Sauvegarde écrite dans {}.", folder.display());
    Ok(())
}
