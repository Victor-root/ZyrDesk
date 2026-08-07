//! Ce que coûte le tunnel, en millisecondes et en paquets perdus.
//!
//! La question à laquelle ce banc répond est simple : entre deux
//! ordinateurs, le tunnel ajoute-t-il quelque chose au trajet ? Il mesure
//! donc deux fois le même trajet, avec les mêmes paquets à la même
//! cadence : une fois en UDP nu, une fois à travers le tunnel complet,
//! ports des moteurs compris. Seul l'écart entre les deux a du sens ;
//! les valeurs absolues portent aussi le bruit du système.
//!
//! Les deux ordinateurs doivent connaître l'empreinte l'un de l'autre.
//! « zyr-cli identite » l'affiche sur chaque machine. Elle ne change
//! plus une fois créée.

use std::error::Error;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::process::ExitCode;
use std::time::Duration;

use clap::{Args, Subcommand};
use tokio::net::UdpSocket;
use zyr_proto::net::{EnginePorts, device_loopback_addr};
use zyr_proto::paths;
use zyr_transport::{Chemin, Empreinte, Identite, PointTerminal, ProfilMedia};
use zyr_tunnel::Tunnel;

use crate::echec;
use crate::mesure::{Resultat, ecart, millisecondes};
use crate::sonde::{self, Cadence};

/// Port du banc, hors de la plage réservée aux moteurs.
const PORT_TUNNEL: u16 = 47010;
/// Écho joint sans tunnel, qui sert de référence.
const PORT_DIRECT: u16 = 47011;
/// Base de ports que le banc prête à ses faux moteurs.
const BASE_MOTEUR: u16 = 42900;
/// Le banc ne mesure qu'un seul chemin à la fois.
const APPAREIL: u16 = 0;
/// Le moteur hôte n'écoute que sur la machine locale.
const MOTEUR: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
/// Le banc accepte les connexions venues de n'importe quelle interface.
const TOUTES_INTERFACES: IpAddr = IpAddr::V4(Ipv4Addr::UNSPECIFIED);
/// Temps laissé au transport pour trouver la taille de paquet du chemin.
const DECOUVERTE_DU_CHEMIN: Duration = Duration::from_secs(2);

#[derive(Subcommand)]
pub enum Action {
    /// Se met en attente et répond aux mesures de l'autre ordinateur
    Hote(ArgsHote),
    /// Mesure le chemin vers un ordinateur en attente
    Client(ArgsClient),
}

#[derive(Args)]
pub struct ArgsHote {
    /// Empreinte de l'ordinateur qui va mesurer
    #[arg(long, value_name = "EMPREINTE")]
    pair: Empreinte,
    /// Débit visé, en mégabits par seconde. À régler comme sur l'autre
    /// ordinateur, sans quoi le chemin de retour est bridé.
    #[arg(long, default_value_t = DEBIT_PAR_DEFAUT, value_parser = debit_admis())]
    debit: u64,
}

#[derive(Args)]
pub struct ArgsClient {
    /// Adresse de l'ordinateur en attente
    adresse: IpAddr,
    /// Empreinte de cet ordinateur
    #[arg(long, value_name = "EMPREINTE")]
    pair: Empreinte,
    /// Débit visé, en mégabits par seconde
    #[arg(long, default_value_t = DEBIT_PAR_DEFAUT, value_parser = debit_admis())]
    debit: u64,
    /// Durée de chaque salve, en secondes
    #[arg(long, default_value_t = 10, value_parser = clap::value_parser!(u64).range(1..=3600))]
    duree: u64,
    /// Images par seconde simulées
    #[arg(long, default_value_t = 60, value_parser = clap::value_parser!(u32).range(1..=480))]
    fps: u32,
    /// Perte à provoquer sous le tunnel, pour mille paquets émis.
    /// Sert à vérifier que la perte n'étrangle pas le débit.
    #[arg(long, default_value_t = 0, value_parser = clap::value_parser!(u16).range(0..=1000))]
    perte: u16,
}

const DEBIT_PAR_DEFAUT: u64 = 50;

/// Bornes du débit. Un débit nul n'émettrait rien, et une cadence nulle
/// concentrerait toute la salve en un instant.
fn debit_admis() -> impl clap::builder::TypedValueParser<Value = u64> {
    clap::value_parser!(u64).range(1..=1000)
}

pub fn executer(action: Action) -> ExitCode {
    let execution = match tokio::runtime::Runtime::new() {
        Ok(e) => e,
        Err(e) => return echec("démarrage du banc", e),
    };

    let resultat = match action {
        Action::Hote(args) => execution.block_on(tenir(args)),
        Action::Client(args) => execution.block_on(mesurer(args)),
    };

    match resultat {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => echec("le banc n'a pas pu aller au bout", e),
    }
}

fn profil(debit_mbps: u64, images_par_seconde: u32) -> ProfilMedia {
    ProfilMedia {
        debit_bits_par_seconde: debit_mbps * 1_000_000,
        images_par_seconde,
    }
}

/// Côté mesuré : deux échos, un par chemin, et le tunnel qui les sert.
async fn tenir(args: ArgsHote) -> Result<(), Box<dyn Error>> {
    let identite = Identite::charger_ou_creer(&paths::identite_dir())?;
    let ports = EnginePorts::new(BASE_MOTEUR)?;

    // Référence : le même écho, joint sans passer par le tunnel.
    let direct = UdpSocket::bind(SocketAddr::new(TOUTES_INTERFACES, PORT_DIRECT)).await?;
    tokio::spawn(async move {
        let _ = sonde::faire_echo(direct).await;
    });

    // Faux moteur : le tunnel lui remet ce qu'il reçoit sur le canal
    // vidéo, il le renvoie tel quel.
    let moteur = UdpSocket::bind(SocketAddr::new(MOTEUR, ports.video())).await?;
    tokio::spawn(async move {
        let _ = sonde::faire_echo(moteur).await;
    });

    let point = PointTerminal::hote(
        &identite,
        args.pair,
        profil(args.debit, 60),
        SocketAddr::new(TOUTES_INTERFACES, PORT_TUNNEL),
    )?;

    println!("Banc en attente sur le port {PORT_TUNNEL}.");
    println!("  Empreinte de cet ordinateur : {}", identite.empreinte());
    println!("  Débit servi : {} Mb/s", args.debit);
    println!("\nCtrl+C pour arrêter.\n");

    loop {
        let connexion = match point.accepter().await {
            Ok(c) => c,
            // Un appareil refusé ne doit pas fermer le banc.
            Err(e) => {
                println!("Connexion écartée : {e}");
                continue;
            }
        };

        println!("Mesure en cours...");
        // Chaque mesure a sa tâche : le banc doit rester prêt à accepter
        // la suivante, sinon la connexion d'après expire en attendant.
        tokio::spawn(async move {
            match Tunnel::hote(connexion, MOTEUR, ports).await {
                Ok(mut tunnel) => {
                    if let Err(e) = tunnel.attendre().await {
                        println!("Fin de la mesure : {e}");
                    }
                }
                Err(e) => println!("Tunnel impossible : {e}"),
            }
            println!("Mesure terminée.\n");
        });
    }
}

/// Côté qui mesure : le même trajet deux fois, puis le rapport.
async fn mesurer(args: ArgsClient) -> Result<(), Box<dyn Error>> {
    let identite = Identite::charger_ou_creer(&paths::identite_dir())?;
    let ports = EnginePorts::new(BASE_MOTEUR)?;
    let ecoute = IpAddr::V4(
        device_loopback_addr(APPAREIL).ok_or("aucune adresse locale disponible pour le banc")?,
    );

    println!("Empreinte de cet ordinateur : {}", identite.empreinte());
    println!("Connexion à {}...", args.adresse);

    // La référence directe n'est jamais dégradée : elle doit rester le
    // même trajet pour les deux mesures, sinon la comparaison ne dit rien.
    let chemin = match args.perte {
        0 => Chemin::Direct,
        perte_pour_mille => Chemin::Degrade { perte_pour_mille },
    };
    let point = PointTerminal::client_sur_chemin(
        &identite,
        args.pair,
        profil(args.debit, args.fps),
        SocketAddr::new(TOUTES_INTERFACES, 0),
        chemin,
    )?;
    let connexion = point
        .connecter(SocketAddr::new(args.adresse, PORT_TUNNEL))
        .await?;

    let utilisable = connexion
        .datagramme_utilisable_stabilise(DECOUVERTE_DU_CHEMIN)
        .await
        .ok_or("le chemin n'accepte aucun datagramme")?;
    let taille = zyr_transport::taille_paquet(utilisable)?;

    let cadence = Cadence {
        taille: taille.octets,
        debit_mbps: args.debit,
        images_par_seconde: args.fps,
        duree: Duration::from_secs(args.duree),
    };

    println!(
        "\n{} paquets de {} octets par seconde, pendant {} s, deux fois.",
        cadence.paquets_par_image() as u64 * cadence.images_par_seconde as u64,
        cadence.taille,
        args.duree
    );
    if args.perte > 0 {
        println!(
            "Perte provoquée sous le tunnel : {:.1} % des paquets émis.",
            args.perte as f64 / 10.0
        );
    }

    println!("\nMesure directe...");
    let direct = sonde::sonder(
        UdpSocket::bind(SocketAddr::new(TOUTES_INTERFACES, 0)).await?,
        SocketAddr::new(args.adresse, PORT_DIRECT),
        cadence,
    )
    .await?;

    println!("Mesure à travers le tunnel...");
    let tunnel = Tunnel::client(connexion.clone(), ecoute, ports).await?;
    let par_tunnel = sonde::sonder(
        UdpSocket::bind(SocketAddr::new(ecoute, 0)).await?,
        SocketAddr::new(ecoute, ports.video()),
        cadence,
    )
    .await?;

    rapporter(&direct, &par_tunnel, &taille, &connexion, &tunnel);

    // Refermer proprement libère l'autre banc tout de suite, au lieu de
    // le laisser attendre l'expiration de la connexion.
    drop(tunnel);
    point.fermer().await;
    Ok(())
}

fn rapporter(
    direct: &Resultat,
    par_tunnel: &Resultat,
    taille: &zyr_transport::TaillePaquet,
    connexion: &zyr_transport::Connexion,
    tunnel: &Tunnel,
) {
    println!("\n--- Sans tunnel (référence) ---");
    detailler(direct);

    println!("\n--- À travers le tunnel ---");
    detailler(par_tunnel);
    println!("  taille de paquet   {} octets", taille.octets);
    if taille.reduite_par_le_chemin {
        println!("                     réduite par le chemin");
    }
    println!(
        "  aller-retour vu par le transport   {}",
        millisecondes(connexion.aller_retour())
    );

    let releve = tunnel.releve();
    if releve.trop_gros > 0 {
        println!(
            "  {} paquets trop gros pour le chemin : la taille demandée au \
             moteur devra baisser",
            releve.trop_gros
        );
    }
    if releve.illisibles > 0 {
        println!("  {} datagrammes illisibles", releve.illisibles);
    }
    if releve.sans_destinataire > 0 {
        println!(
            "  {} datagrammes arrivés sur un canal muet côté local",
            releve.sans_destinataire
        );
    }

    println!("\n--- Ce que coûte le tunnel ---");
    println!(
        "  médiane            {}",
        ecart(direct.median, par_tunnel.median)
    );
    println!(
        "  centile 95         {}",
        ecart(direct.centile_95, par_tunnel.centile_95)
    );
    println!(
        "  centile 99         {}",
        ecart(direct.centile_99, par_tunnel.centile_99)
    );
    println!(
        "  perte              {:+.2} point(s)",
        par_tunnel.perte() - direct.perte()
    );
    println!(
        "  débit tenu         {:.1} Mb/s contre {:.1} Mb/s",
        par_tunnel.debit(),
        direct.debit()
    );
}

fn detailler(r: &Resultat) {
    println!(
        "  aller-retour       médiane {}   c95 {}   c99 {}   pire {}",
        millisecondes(r.median),
        millisecondes(r.centile_95),
        millisecondes(r.centile_99),
        millisecondes(r.pire)
    );
    println!(
        "  perte              {} sur {} ({:.2} %)",
        r.perdus(),
        r.emis,
        r.perte()
    );
    println!("  débit tenu         {:.1} Mb/s", r.debit());
}
