//! Ce qui émet, ce qui renvoie, ce qui chronomètre.
//!
//! Les paquets partent par rafales, une par image, comme le fait un
//! encodeur vidéo : c'est ce rythme-là qui met un chemin à l'épreuve, pas
//! un flot régulier. Chacun porte son âge, si bien que l'aller-retour se
//! lit au retour sans que les deux ordinateurs aient à s'accorder sur
//! l'heure.

use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::net::UdpSocket;

use crate::mesure::{AllerRetour, Resultat};

/// Âge du paquet, inscrit en tête.
const HORODATAGE: usize = size_of::<u64>();

/// Délai laissé aux derniers paquets pour revenir.
const GRACE: Duration = Duration::from_millis(500);

/// Aucun datagramme UDP ne dépasse cette taille.
const TAMPON: usize = 65_535;

/// Cadence d'émission, calquée sur celle d'un encodeur vidéo.
#[derive(Debug, Clone, Copy)]
pub struct Cadence {
    pub taille: u16,
    pub debit_mbps: u64,
    pub images_par_seconde: u32,
    pub duree: Duration,
}

impl Cadence {
    /// Paquets à émettre par image, au moins un.
    pub fn paquets_par_image(&self) -> u32 {
        let par_seconde = self.debit_mbps * 1_000_000 / 8 / self.taille.max(1) as u64;
        (par_seconde / self.images_par_seconde.max(1) as u64).max(1) as u32
    }

    fn intervalle(&self) -> Duration {
        Duration::from_secs(1) / self.images_par_seconde.max(1)
    }
}

/// Renvoie tout ce qui arrive, sans rien y changer.
pub async fn faire_echo(socket: UdpSocket) -> io::Result<()> {
    let mut tampon = vec![0u8; TAMPON];
    loop {
        let (lus, source) = socket.recv_from(&mut tampon).await?;
        socket.send_to(&tampon[..lus], source).await?;
    }
}

/// Émet à la cadence demandée et chronomètre ce qui revient.
pub async fn sonder(
    socket: UdpSocket,
    cible: SocketAddr,
    cadence: Cadence,
) -> io::Result<Resultat> {
    if (cadence.taille as usize) < HORODATAGE {
        return Err(io::Error::other(format!(
            "un paquet de sonde fait au moins {HORODATAGE} octets"
        )));
    }

    socket.connect(cible).await?;
    let socket = Arc::new(socket);
    let depart = Instant::now();

    let receveur = {
        let socket = socket.clone();
        tokio::spawn(async move { recueillir(&socket, depart, cadence.duree).await })
    };

    let (emis, duree) = emettre(&socket, depart, cadence).await?;
    let mesures = receveur.await.map_err(io::Error::other)?;
    Ok(Resultat::depuis(
        mesures,
        emis,
        emis * cadence.taille as u64,
        duree,
    ))
}

/// Émet jusqu'à la fin du temps imparti, et dit combien de paquets sont
/// partis et en combien de temps.
async fn emettre(
    socket: &UdpSocket,
    depart: Instant,
    cadence: Cadence,
) -> io::Result<(u64, Duration)> {
    let mut paquet = vec![0u8; cadence.taille as usize];
    let mut rythme = tokio::time::interval(cadence.intervalle());
    let par_image = cadence.paquets_par_image();
    let mut emis = 0u64;

    while depart.elapsed() < cadence.duree {
        rythme.tick().await;
        for _ in 0..par_image {
            let age = depart.elapsed().as_nanos() as u64;
            paquet[..HORODATAGE].copy_from_slice(&age.to_le_bytes());
            socket.send(&paquet).await?;
            emis += 1;
        }
    }

    Ok((emis, depart.elapsed()))
}

/// Recueille les retours, jusqu'à l'échéance plus le délai de grâce.
async fn recueillir(socket: &UdpSocket, depart: Instant, duree: Duration) -> Vec<AllerRetour> {
    let mut mesures = Vec::new();
    let mut tampon = vec![0u8; TAMPON];

    let _ = tokio::time::timeout(duree + GRACE, async {
        while let Ok(lus) = socket.recv(&mut tampon).await {
            if let Some(aller_retour) = chronometrer(&tampon[..lus], depart) {
                mesures.push(aller_retour);
            }
        }
    })
    .await;

    mesures
}

/// Relit l'âge inscrit dans un paquet et en déduit son aller-retour.
fn chronometrer(paquet: &[u8], depart: Instant) -> Option<AllerRetour> {
    let horodatage: [u8; HORODATAGE] = paquet.get(..HORODATAGE)?.try_into().ok()?;
    let age = u64::from_le_bytes(horodatage);
    let maintenant = depart.elapsed().as_nanos() as u64;
    // Un paquet plus jeune que sa date de départ n'a pas de sens : c'est
    // du remplissage étranger à la sonde.
    Some(AllerRetour(Duration::from_nanos(
        maintenant.checked_sub(age)?,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cadence(taille: u16, debit: u64, fps: u32) -> Cadence {
        Cadence {
            taille,
            debit_mbps: debit,
            images_par_seconde: fps,
            duree: Duration::from_secs(1),
        }
    }

    #[test]
    fn la_rafale_par_image_correspond_au_debit_vise() {
        // 50 Mb/s à 60 images par seconde et 1300 octets par paquet :
        // 6,25 Mo/s, soit environ 4807 paquets, soit 80 par image.
        let c = cadence(1300, 50, 60);
        assert_eq!(c.paquets_par_image(), 80);
        assert_eq!(c.intervalle(), Duration::from_nanos(16_666_666));
    }

    #[test]
    fn une_cadence_minuscule_emet_quand_meme() {
        // Sans plancher, le banc n'enverrait rien et ne mesurerait rien.
        assert_eq!(cadence(1300, 1, 240).paquets_par_image(), 1);
        assert_eq!(cadence(1300, 0, 60).paquets_par_image(), 1);
    }

    #[test]
    fn une_cadence_degeneree_ne_divise_pas_par_zero() {
        // Les valeurs sont bornées à la saisie ; ces gardes sont là pour
        // qu'un zéro venu d'ailleurs ne fasse jamais paniquer le banc.
        let degeneree = cadence(0, 50, 0);
        assert!(degeneree.paquets_par_image() >= 1);
        assert_eq!(degeneree.intervalle(), Duration::from_secs(1));
    }

    #[test]
    fn l_age_inscrit_donne_l_aller_retour() {
        let depart = Instant::now();
        let mut paquet = vec![0u8; 64];
        paquet[..HORODATAGE].copy_from_slice(&0u64.to_le_bytes());
        let mesure = chronometrer(&paquet, depart).unwrap();
        assert!(mesure.0 < Duration::from_millis(100));
    }

    #[test]
    fn un_paquet_etranger_a_la_sonde_est_ignore() {
        let depart = Instant::now();
        assert!(chronometrer(&[1, 2, 3], depart).is_none());

        // Âge situé dans le futur : ce paquet ne vient pas d'ici.
        let mut paquet = vec![0u8; 64];
        paquet[..HORODATAGE].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(chronometrer(&paquet, depart).is_none());
    }

    #[tokio::test]
    async fn l_echo_renvoie_ce_qu_il_recoit() {
        let echo = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let adresse = echo.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = faire_echo(echo).await;
        });

        let sonde = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        sonde.send_to(b"paquet", adresse).await.unwrap();
        let mut recu = [0u8; 16];
        let (lus, _) = sonde.recv_from(&mut recu).await.unwrap();
        assert_eq!(&recu[..lus], b"paquet");
    }

    #[tokio::test]
    async fn une_sonde_complete_mesure_ce_qui_revient() {
        let echo = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let cible = echo.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = faire_echo(echo).await;
        });

        let mut c = cadence(1300, 10, 60);
        c.duree = Duration::from_millis(200);
        let sonde = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let resultat = sonder(sonde, cible, c).await.unwrap();

        assert!(resultat.emis > 0);
        assert_eq!(resultat.perdus(), 0, "rien ne se perd en loopback");
        assert!(resultat.debit() > 0.0);
    }

    #[tokio::test]
    async fn un_paquet_trop_court_pour_porter_son_age_est_refuse() {
        let sonde = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let cible: SocketAddr = "127.0.0.1:9".parse().unwrap();
        assert!(sonder(sonde, cible, cadence(4, 10, 60)).await.is_err());
    }
}
