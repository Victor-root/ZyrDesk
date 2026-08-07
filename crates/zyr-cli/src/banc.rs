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
use zyr_proto::net::{EnginePorts, device_loopback_addr};
use zyr_proto::paths;
use zyr_transport::{Chemin, Empreinte, Identite, PointTerminal, ProfilMedia};
use zyr_tunnel::Tunnel;
use zyr_tunnel::pompe::ouvrir_socket;

use crate::echec;
use crate::mesure::{Resultat, ecart, millisecondes};
use crate::processeur::{self, Chronometre};
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
    let direct = ouvrir_socket(SocketAddr::new(TOUTES_INTERFACES, PORT_DIRECT))?;
    tokio::spawn(async move {
        let _ = sonde::faire_echo(direct).await;
    });

    // Faux moteur : le tunnel lui remet ce qu'il reçoit sur le canal
    // vidéo, il le renvoie tel quel.
    let moteur = ouvrir_socket(SocketAddr::new(MOTEUR, ports.video()))?;
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
            let observee = connexion.clone();
            match Tunnel::hote(connexion, MOTEUR, ports).await {
                Ok(mut tunnel) => {
                    let (sans, avec) = servir_et_mesurer(&mut tunnel).await;
                    // Le trajet retour n'est visible que d'ici : l'autre
                    // banc ne connaît que ce qu'il a lui-même émis.
                    println!("  {}", ventilation(&tunnel, &observee, "au retour"));
                    rapporter_calcul(args.debit, sans, avec);
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

    // Le processeur est relevé sur chacune des deux salves : c'est
    // l'écart entre les deux qui dit ce que le tunnel coûte en calcul,
    // la sonde elle-même consommant déjà quelque chose.
    println!("\nMesure directe...");
    let calcul_direct = Chronometre::demarrer();
    let direct = sonde::sonder(
        ouvrir_socket(SocketAddr::new(TOUTES_INTERFACES, 0))?,
        SocketAddr::new(args.adresse, PORT_DIRECT),
        cadence,
    )
    .await?;
    let charge_directe = calcul_direct.and_then(|c| c.charge());

    println!("Mesure à travers le tunnel...");
    let tunnel = Tunnel::client(connexion.clone(), ecoute, ports).await?;
    let calcul_tunnel = Chronometre::demarrer();
    let par_tunnel = sonde::sonder(
        ouvrir_socket(SocketAddr::new(ecoute, 0))?,
        SocketAddr::new(ecoute, ports.video()),
        cadence,
    )
    .await?;
    let charge_tunnel = calcul_tunnel.and_then(|c| c.charge());

    rapporter(&direct, &par_tunnel, &taille, &connexion, &tunnel);
    rapporter_calcul(args.debit, charge_directe, charge_tunnel);

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

    println!(
        "  ce que ce banc voit   {}",
        ventilation(tunnel, connexion, "à l'aller")
    );
    println!("                        le retour est compté par l'autre banc");

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

/// Rythme auquel le banc hôte guette le début du transport.
const PAS_DE_GUET: Duration = Duration::from_millis(200);

/// Sert le tunnel jusqu'à sa fin, en séparant les deux phases de la
/// mesure.
///
/// L'autre banc mesure d'abord le chemin nu, puis le tunnel. Vu d'ici,
/// la bascule est le premier datagramme qui traverse : avant, ce banc ne
/// fait que répondre à l'écho direct ; après, il fait tourner le tunnel
/// en plus. L'écart entre les deux dit ce que le tunnel lui coûte, comme
/// de l'autre côté. Sans cette séparation, la phase inactive diluerait
/// la mesure de moitié.
async fn servir_et_mesurer(tunnel: &mut Tunnel) -> (Option<f64>, Option<f64>) {
    let compteurs = tunnel.compteurs();
    let mut sans_tunnel = Chronometre::demarrer();
    let mut avec_tunnel: Option<Chronometre> = None;
    let mut charge_sans = None;

    loop {
        tokio::select! {
            resultat = tunnel.attendre() => {
                if let Err(e) = resultat {
                    println!("Fin de la mesure : {e}");
                }
                return (charge_sans, avec_tunnel.and_then(|c| c.charge()));
            }
            _ = tokio::time::sleep(PAS_DE_GUET) => {
                if avec_tunnel.is_none() && compteurs.releve().vers_moteur > 0 {
                    charge_sans = sans_tunnel.take().and_then(|c| c.charge());
                    avec_tunnel = Chronometre::demarrer();
                }
            }
        }
    }
}

/// Ce que le tunnel coûte en calcul, exprimé en part d'un coeur.
///
/// Le chiffre brut n'est pas comparable au seuil du projet : le banc
/// émet et reçoit à la fois, donc chaque extrémité voit passer deux fois
/// le débit demandé, là où une session réelle n'en fait qu'un sens. Le
/// second chiffre ramène le coût à ce qu'une session paierait, en
/// supposant que le calcul suit le nombre de paquets traités.
fn rapporter_calcul(debit_mbps: u64, directe: Option<f64>, tunnel: Option<f64>) {
    let (Some(directe), Some(tunnel)) = (directe, tunnel) else {
        println!("\n  Charge processeur : non mesurable sur cette plateforme.");
        return;
    };

    let cout = tunnel - directe;
    println!("\n--- Processeur de ce banc ---");
    println!("  sans tunnel        {directe:.1} % d'un coeur");
    println!("  avec tunnel        {tunnel:.1} % d'un coeur");
    println!(
        "  coût du tunnel     {cout:+.1} point(s) pour {} Mb/s traversés",
        debit_mbps * 2
    );
    println!(
        "  soit               {:.1} point(s) pour une session à {debit_mbps} Mb/s, \
         qui n'en fait qu'un sens",
        cout / 2.0
    );
    println!("  machine à {} coeurs", processeur::coeurs());
}

/// D'où vient ce qui manque, du point de vue d'un seul des deux bancs.
///
/// Chaque extrémité ne connaît que ce qu'elle a émis : le transport ne
/// détecte les pertes que par les acquittements qui lui reviennent. Les
/// deux moitiés du trajet demandent donc les deux terminaux.
fn ventilation(tunnel: &Tunnel, connexion: &zyr_transport::Connexion, sens: &str) -> String {
    let jetes = tunnel
        .releve()
        .vers_tunnel
        .saturating_sub(connexion.datagrammes_partis());
    format!(
        "{jetes} datagramme(s) jeté(s) faute de place, {} paquet(s) perdu(s) {sens}",
        connexion.paquets_perdus()
    )
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
