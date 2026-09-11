//! Ce que le bouton flottant montre des fichiers qui arrivent.
//!
//! Coller sur cet ordinateur des fichiers copiés sur l'autre, c'est
//! attendre : les octets traversent au moment du coller et non à celui du
//! copier, et ils traversent à côté de l'image sans jamais lui passer
//! devant. Ce qui manquait à cette attente, c'est de se voir.
//!
//! Elle se voit dans la marque elle-même : la vitre de l'écran de devant
//! se remplit comme une barre de chargement. Ni fenêtre de plus, ni
//! second dessin posé à côté du bouton.
//!
//! Ce qui est montré est ce qui arrive **ici**. Quand c'est l'ordinateur
//! d'en face qui colle, l'attente se voit sur son bureau, dans la fenêtre
//! de copie que Windows ouvre lui-même, et ce bureau-là est justement
//! l'image qu'on regarde.

// Hors de Windows il n'y a pas de bouton à dessiner, mais ce qui se lit
// se compile partout.
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use zyr_proto::clipboard::HowFar;

/// Combien de fois par seconde la barre est relue.
///
/// Le service ne réécrit ce qu'il a écrit que quand le centième change,
/// donc relire plus souvent ne montrerait rien de plus ; relire moins
/// souvent ferait une barre qui avance par sauts.
const LOOK_EVERY: Duration = Duration::from_millis(200);

/// Ce que porte le compteur quand rien n'arrive.
///
/// Au-delà de cent, ce qu'aucun centième n'est jamais.
const NOTHING: u32 = u32::MAX;

/// Où en est ce qui arrive, en centièmes.
static COMING: AtomicU32 = AtomicU32::new(NOTHING);

/// Si la veille tourne déjà, une seule suffisant par session.
static WATCHING: AtomicBool = AtomicBool::new(false);

/// Ce que la barre du bouton doit montrer, de zéro à un, ou rien.
pub fn how_far() -> Option<f32> {
    match COMING.load(Ordering::Relaxed) {
        NOTHING => None,
        hundredths => Some(hundredths as f32 / 100.0),
    }
}

/// Relit l'avancement tant qu'une session dure, et tient la barre à jour.
///
/// Relancée à chaque tour de la veille du bouton, comme les pastilles :
/// celle-ci tourne une fois par seconde, et ce qu'on regarde avancer se
/// regarde cinq fois plus souvent.
pub fn watch(app: &crate::app::App) {
    if WATCHING.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    crate::app::spawn(async move {
        keep_up(&app).await;
        // La session s'en va : la barre s'en va avec elle, et le bouton
        // l'apprend avant de disparaître.
        say(NOTHING);
        WATCHING.store(false, Ordering::SeqCst);
    });
}

/// La boucle elle-même.
async fn keep_up(app: &crate::app::App) {
    loop {
        tokio::time::sleep(LOOK_EVERY).await;
        if !crate::floating::a_session_is_up(app) {
            return;
        }
        say(coming_in().map_or(NOTHING, |far| far.hundredths()));
    }
}

/// Pose ce chiffre-là, et ne redessine que s'il a bougé.
fn say(hundredths: u32) {
    if COMING.swap(hundredths, Ordering::Relaxed) != hundredths {
        #[cfg(windows)]
        crate::logo::the_bar_moved();
    }
}

/// Ce que le service a écrit des fichiers qui arrivent, s'il en arrive.
///
/// Pas de fichier veut dire rien en route : c'est le service qui l'enlève
/// quand tout est là, et c'est ce qui fait disparaître la barre sans que
/// personne ait à dire que c'est fini.
fn coming_in() -> Option<HowFar> {
    let said = std::fs::read_to_string(zyr_proto::paths::files_coming()).ok()?;
    HowFar::read(&said).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sans_rien_qui_arrive_la_barre_ne_se_dessine_pas() {
        // C'est ce qui la fait disparaître d'elle-même : le service
        // enlève le fichier quand tout est là, et il n'y a rien à dire de
        // plus.
        say(NOTHING);
        assert_eq!(how_far(), None);
    }

    #[test]
    fn un_avancement_se_lit_en_part_du_tout() {
        say(0);
        assert_eq!(how_far(), Some(0.0));
        say(50);
        assert_eq!(how_far(), Some(0.5));
        say(100);
        assert_eq!(how_far(), Some(1.0));
        say(NOTHING);
    }
}
