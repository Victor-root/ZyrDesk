//! Temps processeur consommé par ce programme.
//!
//! Le banc doit dire ce que le tunnel coûte en calcul. Relever ce chiffre
//! à la main dans le gestionnaire de tâches est pénible et peu fiable :
//! il échantillonne, il arrondit, et il faut le lire pendant que la
//! mesure tourne. Le programme sait le faire lui-même, exactement sur la
//! fenêtre de temps qui l'intéresse.

use std::time::{Duration, Instant};

/// Relevé de départ, à comparer une fois le travail fait.
#[derive(Debug, Clone, Copy)]
pub struct Chronometre {
    processeur: Duration,
    horloge: Instant,
}

impl Chronometre {
    /// Prend le relevé de départ. Rend `None` si la plateforme ne sait
    /// pas répondre, auquel cas le banc se tait plutôt que d'inventer.
    pub fn demarrer() -> Option<Self> {
        Some(Self {
            processeur: temps_processeur()?,
            horloge: Instant::now(),
        })
    }

    /// Part d'un coeur occupée depuis le départ, en pourcentage.
    ///
    /// Cent pour cent vaut un coeur saturé. Deux coeurs pleins donnent
    /// deux cents.
    ///
    /// Le compte porte sur tout le programme, fils compris : deux
    /// travaux menés en même temps par le même programme ne se
    /// distinguent pas. Le banc les enchaîne pour cette raison.
    pub fn charge(&self) -> Option<f64> {
        let consomme = temps_processeur()?.checked_sub(self.processeur)?;
        part_d_un_coeur(consomme, self.horloge.elapsed())
    }
}

/// Part d'un coeur qu'occupe un temps de calcul sur une durée donnée.
fn part_d_un_coeur(consomme: Duration, ecoule: Duration) -> Option<f64> {
    let ecoule = ecoule.as_secs_f64();
    if ecoule <= 0.0 {
        return None;
    }
    Some(consomme.as_secs_f64() * 100.0 / ecoule)
}

/// Nombre de coeurs, pour situer la charge relevée.
pub fn coeurs() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

/// Temps processeur cumulé du processus, tous fils confondus.
#[cfg(windows)]
fn temps_processeur() -> Option<Duration> {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

    let mut creation = FILETIME::default();
    let mut fin = FILETIME::default();
    let mut noyau = FILETIME::default();
    let mut utilisateur = FILETIME::default();

    // Sans risque : les quatre écritures visent des variables locales
    // dont la durée de vie couvre l'appel, et le processus courant est
    // toujours valide.
    let obtenu = unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut creation,
            &mut fin,
            &mut noyau,
            &mut utilisateur,
        )
    };
    if obtenu == 0 {
        return None;
    }

    // Le système compte par tranches de cent nanosecondes, réparties sur
    // deux moitiés de trente-deux bits.
    fn centaines_de_nanosecondes(t: FILETIME) -> u64 {
        (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime)
    }
    let tranches = centaines_de_nanosecondes(noyau) + centaines_de_nanosecondes(utilisateur);
    Some(Duration::from_nanos(tranches.saturating_mul(100)))
}

/// Même chose sous Linux, pour la machine de développement.
#[cfg(target_os = "linux")]
fn temps_processeur() -> Option<Duration> {
    // Le noyau compte en tops d'horloge, dont l'unité exposée aux
    // programmes vaut cent par seconde quelle que soit la machine.
    const TOPS_PAR_SECONDE: u64 = 100;

    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    // Le nom du programme peut contenir des espaces, mais il est entre
    // parenthèses : les champs numériques commencent après la dernière.
    let apres_nom = &stat[stat.rfind(')')? + 1..];
    let champs: Vec<&str> = apres_nom.split_whitespace().collect();
    // Après le nom vient l'état, puis les champs 3 à 13 ; le temps passé
    // en espace utilisateur est le douzième, celui en espace noyau le
    // treizième.
    let utilisateur: u64 = champs.get(11)?.parse().ok()?;
    let noyau: u64 = champs.get(12)?.parse().ok()?;
    Some(Duration::from_nanos(
        (utilisateur + noyau).saturating_mul(1_000_000_000 / TOPS_PAR_SECONDE),
    ))
}

#[cfg(not(any(windows, target_os = "linux")))]
fn temps_processeur() -> Option<Duration> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_temps_processeur_est_lisible_et_ne_recule_pas() {
        let premier = temps_processeur().expect("plateforme sans temps processeur");
        let mut somme = 0u64;
        for i in 0..3_000_000u64 {
            somme = somme.wrapping_add(i * i);
        }
        assert_ne!(somme, u64::MAX);
        let second = temps_processeur().unwrap();
        assert!(second >= premier, "{second:?} après {premier:?}");
    }

    #[test]
    fn un_travail_soutenu_se_voit_dans_la_charge() {
        let chrono = Chronometre::demarrer().unwrap();
        let debut = Instant::now();
        let mut somme = 0u64;
        // Assez long pour dépasser la granularité du compteur système,
        // qui avance par tranches de quelques millisecondes.
        while debut.elapsed() < Duration::from_millis(120) {
            for i in 0..100_000u64 {
                somme = somme.wrapping_add(i * i);
            }
        }
        assert_ne!(somme, u64::MAX);

        let charge = chrono.charge().unwrap();
        assert!(
            charge > 50.0,
            "{charge:.1} % pour une boucle qui occupe un coeur entier"
        );
    }

    #[test]
    fn la_part_d_un_coeur_se_lit_comme_un_pourcentage() {
        // Une seconde de calcul en une seconde vaut un coeur saturé.
        let plein = part_d_un_coeur(Duration::from_secs(1), Duration::from_secs(1)).unwrap();
        assert!((plein - 100.0).abs() < 0.001, "{plein}");

        // Un quart de seconde en une seconde vaut le quart d'un coeur.
        let quart = part_d_un_coeur(Duration::from_millis(250), Duration::from_secs(1)).unwrap();
        assert!((quart - 25.0).abs() < 0.001, "{quart}");

        // Deux coeurs pleins pendant une seconde donnent deux cents.
        let double = part_d_un_coeur(Duration::from_secs(2), Duration::from_secs(1)).unwrap();
        assert!((double - 200.0).abs() < 0.001, "{double}");

        // Sans temps écoulé, il n'y a rien à rapporter.
        assert!(part_d_un_coeur(Duration::from_secs(1), Duration::ZERO).is_none());
    }

    #[test]
    fn le_nombre_de_coeurs_est_plausible() {
        assert!(coeurs() >= 1);
    }
}
