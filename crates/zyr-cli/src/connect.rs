//! Ouvre une session sur un ordinateur distant.
//!
//! À ce stade il n'y a ni tunnel ni serveur de mise en relation : la
//! session vise directement une adresse du réseau local, et le code
//! d'appairage doit être reporté à la main sur l'autre ordinateur. Le
//! tunnel automatise cet échange au jalon M5.

use std::process::ExitCode;

use clap::Args as ClapArgs;
use zyr_engine_client::state::identifiant_depuis_adresse;
use zyr_engine_client::{ClientEngine, DeviceState, IssueSession};
use zyr_proto::alea;
use zyr_proto::paths;
use zyr_proto::session::{Codec, ModeAffichage, SessionSettings, parse_resolution};

use crate::echec;

#[derive(ClapArgs)]
pub struct Args {
    /// Adresse de l'ordinateur distant sur le réseau local
    hote: String,

    /// Résolution demandée, par exemple 1920x1080
    #[arg(long, default_value = "1920x1080")]
    resolution: String,

    /// Images par seconde
    #[arg(long, default_value_t = 60)]
    fps: u32,

    /// Débit vidéo en kilobits par seconde
    #[arg(long, default_value_t = 20_000)]
    bitrate: u32,

    /// Codec vidéo : auto, h264, hevc ou av1
    #[arg(long, default_value = "auto")]
    codec: String,

    /// Affichage : fullscreen, borderless ou windowed
    #[arg(long, default_value = "fullscreen")]
    affichage: String,

    /// Affiche les statistiques de performance par-dessus la vidéo
    #[arg(long)]
    stats: bool,

    /// Souris relative, adaptée aux jeux, au lieu de la souris de bureau
    #[arg(long)]
    souris_relative: bool,

    /// Refait l'appairage même si cet ordinateur est déjà connu
    #[arg(long)]
    reappairer: bool,
}

pub fn executer(args: Args) -> ExitCode {
    let reglages = match construire_reglages(&args) {
        Ok(r) => r,
        Err(message) => return echec("réglages de session invalides", message),
    };

    let exe = paths::client_engine_exe();
    if !exe.is_file() {
        return echec(
            "moteur client introuvable",
            format!(
                "{}\n  Lancez « zyr-cli engines status » pour la marche à suivre.",
                exe.display()
            ),
        );
    }

    let etat = DeviceState::pour_appareil(&identifiant_depuis_adresse(&args.hote));
    if args.reappairer
        && let Err(e) = etat.oublier()
    {
        return echec("réinitialisation de l'appairage", e);
    }

    let deja_connu = etat.a_un_hote_appaire();
    let journal = paths::logs_dir().join("session.log");
    let moteur = ClientEngine::nouveau(&exe, etat).avec_journal(&journal);

    if !deja_connu && let Err(code) = appairer(&moteur, &args.hote) {
        return code;
    }

    println!(
        "Connexion à {} en {}x{} à {} images par seconde...",
        args.hote, reglages.largeur, reglages.hauteur, reglages.fps
    );
    match moteur.lancer_session(&args.hote, &reglages) {
        Ok(IssueSession::Terminee) => {
            // Le moteur signale un succès même lorsqu'il a renoncé : le
            // journal reste la seule source fiable tant que ses codes de
            // sortie ne sont pas différenciés (patch P-M5).
            println!("Session terminée.");
            println!("  Journal : {}", journal.display());
            ExitCode::SUCCESS
        }
        Ok(IssueSession::Echec { code }) => {
            let code = code
                .map(|c| c.to_string())
                .unwrap_or_else(|| "interrompu".to_string());
            echec(
                "la session s'est arrêtée sur une erreur",
                format!("code {code}\n  Journal : {}", journal.display()),
            )
        }
        Err(e) => echec("démarrage de la session", e),
    }
}

fn appairer(moteur: &ClientEngine, hote: &str) -> Result<(), ExitCode> {
    let pin = alea::pin_appairage();
    println!("Premier accès à cet ordinateur : autorisation nécessaire.\n");
    println!("  Sur {hote}, lancez maintenant :");
    println!("\n      zyr-cli host pin {pin}\n");
    println!("  En attente de l'autorisation...");

    match moteur.appairer(hote, &pin) {
        Ok(()) => {
            println!("  Autorisé.\n");
            Ok(())
        }
        Err(e) => Err(echec(
            "appairage",
            format!("{e}\n  Vérifiez que « zyr-cli host start » tourne sur {hote}."),
        )),
    }
}

fn construire_reglages(args: &Args) -> Result<SessionSettings, String> {
    let (largeur, hauteur) = parse_resolution(&args.resolution).map_err(|e| e.to_string())?;
    let codec: Codec = args.codec.parse()?;
    let mode_affichage: ModeAffichage = args.affichage.parse()?;
    if args.fps == 0 {
        return Err("le nombre d'images par seconde doit être supérieur à zéro".to_string());
    }
    if args.bitrate == 0 {
        return Err("le débit vidéo doit être supérieur à zéro".to_string());
    }
    Ok(SessionSettings {
        largeur,
        hauteur,
        fps: args.fps,
        bitrate_kbps: args.bitrate,
        codec,
        mode_affichage,
        packet_size: None,
        souris_absolue: !args.souris_relative,
        overlay_stats: args.stats,
    })
}
