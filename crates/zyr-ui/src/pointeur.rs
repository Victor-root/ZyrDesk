//! La forme du curseur d'en face, tenue à jour pendant une session.
//!
//! Le curseur qu'une main suit est dessiné ici, sans réseau au milieu,
//! et c'est tout l'intérêt : celui de l'ordinateur distant est sa
//! réponse à un mouvement qui a traversé deux fois. Mais un bureau dit
//! ce qu'un clic va faire par la forme du curseur et par presque rien
//! d'autre, et cette forme-là n'existe que chez lui.
//!
//! Elle est donc demandée, plusieurs fois par seconde tant qu'une
//! session est à l'écran, et écrite dans le fichier que le lecteur suit.
//! Rien n'est retenu d'une question à l'autre : une forme ne vaut rien
//! un instant plus tard.
//!
//! Une seule connexion au service pour toute la session, et non une par
//! question comme partout ailleurs dans cette fenêtre : ailleurs c'est
//! une question de temps en temps, ici c'est vingt par seconde.

// Une session n'existe que sous Windows, et cette boucle avec elle.
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use zyr_control::{Answer, Request, Service, WayId};
use zyr_proto::session::Pointer;

use crate::app::App;

/// Ce sous quoi ce module classe ses lignes du journal.
const TAG: &str = "pointer";

/// Écrit une ligne sous l'étiquette de ce module.
fn note(what: &str) {
    crate::journal::note_about(TAG, what);
}

/// Combien de fois par seconde la forme est demandée.
///
/// Vingt : une main qui entre dans un champ de texte voit la barre
/// apparaître dans les cinquante millisecondes, ce qui est en dessous de
/// ce qu'un œil sépare, et c'est vingt allers-retours par seconde sur un
/// canal déjà ouvert plutôt que soixante.
const ASK_EVERY: Duration = Duration::from_millis(50);

/// Combien de refus d'affilée avant d'abandonner.
///
/// Un refus isolé est une voie qui vient de se fermer ou un service qui
/// redémarre. Plusieurs de suite veulent dire que cet ordinateur-là ne
/// sait pas répondre, ce qui est le cas d'une machine d'en face plus
/// ancienne que celle-ci : la session continue très bien sans, avec la
/// flèche ordinaire, et il n'y a pas de raison de la harceler.
const REFUSALS_BEFORE_GIVING_UP: u32 = 20;

/// Vrai tant que la boucle tourne.
static FOLLOWING: AtomicBool = AtomicBool::new(false);

/// Suit la forme du curseur d'en face jusqu'à la fin de la session.
///
/// Appelée à chaque tour de la veille : elle ne fait rien tant qu'une
/// boucle tourne déjà, et en relance une quand la précédente s'est
/// arrêtée, ce qui arrive à la fin d'une session comme à l'ouverture
/// d'une image.
pub fn follow(app: &App) {
    if FOLLOWING.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    crate::app::spawn(async move {
        let seen = keep_it_in_step(&app).await;
        zyr_session::point_like_nothing();
        note(&format!("forme du curseur : {seen}"));
        FOLLOWING.store(false, Ordering::SeqCst);
    });
}

/// La boucle elle-même, et ce qu'elle a vu passer.
async fn keep_it_in_step(app: &App) -> Seen {
    let mut seen = Seen::default();
    let mut way = None;
    let mut talking = None;
    let mut refused = 0;
    let mut in_a_game = false;
    loop {
        tokio::time::sleep(ASK_EVERY).await;
        if !crate::floating::a_session_is_up(app) {
            seen.why = "la session est terminée";
            return seen;
        }
        // La voie est cherchée à chaque tour tant qu'elle manque, et non
        // une fois au départ. Le service ne connaît une session qu'une
        // fois le lecteur confié : cette boucle démarre bien avant, et
        // renoncer là revenait à ne rien demander pendant les six
        // secondes que met une session à être crue.
        let asking = match way {
            Some(known) => known,
            None => match crate::session::the_way_in_use().await {
                Some(found) => {
                    way = Some(found);
                    found
                }
                None => continue,
            },
        };
        // En mode jeu, le jeu dessine son propre curseur et celui d'ici
        // est caché : demander une forme que personne ne montrera serait
        // vingt allers-retours par seconde pour rien. La boucle reste en
        // vie, parce qu'on peut revenir au bureau sans fermer.
        //
        // La flèche ordinaire est posée en partant, et jamais la dernière
        // forme reçue. Le lecteur garde celle qu'on lui laisse, et l'une
        // des treize n'en est pas une : « theirs » est une forme vide,
        // pour les instants où l'ordinateur d'en face dessine lui-même le
        // sien. Laissée là, elle rend invisible tout curseur que ce
        // lecteur montrerait ensuite.
        if crate::floating::in_game_mouse(app) {
            if !in_a_game {
                in_a_game = true;
                zyr_session::point_like_nothing();
            }
            continue;
        }
        in_a_game = false;
        match asked(&mut talking, asking).await {
            Ok(shape) => {
                refused = 0;
                seen.saw(shape);
                if let Err(reason) = zyr_session::point_like(shape) {
                    seen.why = "la forme n'a pas pu être écrite pour le lecteur";
                    note(&format!("forme du curseur non écrite : {reason}"));
                    return seen;
                }
            }
            Err(reason) => {
                // La connexion est jetée, et la voie oubliée : un refus
                // vient souvent d'un service qui a redémarré ou d'une
                // image relancée, et la voie est alors une autre.
                talking = None;
                way = None;
                refused += 1;
                seen.first_refusal.get_or_insert(reason);
                if refused >= REFUSALS_BEFORE_GIVING_UP {
                    seen.why = "l'ordinateur distant ne répond pas sur la forme de son curseur";
                    return seen;
                }
            }
        }
    }
}

/// Une question, sur la connexion tenue, rouverte quand elle a lâché.
async fn asked(talking: &mut Option<Service>, way: WayId) -> Result<Pointer, String> {
    if talking.is_none() {
        *talking = Some(Service::join().await.map_err(|e| e.to_string())?);
    }
    let service = talking.as_mut().expect("une connexion au service");
    match service
        .ask(&Request::FarPointer { way })
        .await
        .map_err(|e| e.to_string())?
    {
        Answer::Pointer(shape) => Ok(shape),
        Answer::Refused(reason) => Err(reason),
        other => Err(crate::service::unexpected(other)),
    }
}

/// Ce que la boucle a vu, pour le journal.
///
/// Une ligne par session, et elle répond aux deux questions qu'un
/// curseur resté en flèche pose : est-ce qu'on a reçu quoi que ce soit,
/// et est-ce qu'autre chose qu'une flèche est passé. Une ligne par forme
/// reçue en ferait vingt par seconde et ne répondrait à ni l'une ni
/// l'autre.
#[derive(Default)]
struct Seen {
    answers: u64,
    shapes: Vec<Pointer>,
    first_refusal: Option<String>,
    why: &'static str,
}

impl Seen {
    fn saw(&mut self, shape: Pointer) {
        self.answers += 1;
        if !self.shapes.contains(&shape) {
            self.shapes.push(shape);
        }
    }
}

impl std::fmt::Display for Seen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} reçues", self.answers)?;
        if self.shapes.is_empty() {
            f.write_str(", aucune forme")?;
        } else {
            f.write_str(", formes vues :")?;
            for shape in &self.shapes {
                write!(f, " {shape}")?;
            }
        }
        if let Some(refusal) = &self.first_refusal {
            write!(f, " ; premier refus : {}", refusal.replace('\n', " "))?;
        }
        if !self.why.is_empty() {
            write!(f, " ; arrêt : {}", self.why)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_ligne_du_journal_dit_ce_qui_manque() {
        // Elle est là pour un curseur resté en flèche : elle doit
        // distinguer « rien n'est arrivé » de « tout est arrivé et
        // c'était des flèches ».
        let rien = Seen {
            why: "la session est terminée",
            ..Default::default()
        };
        let dit = rien.to_string();
        assert!(dit.contains("0 reçues"), "{dit}");
        assert!(dit.contains("aucune forme"), "{dit}");

        let mut vues = Seen::default();
        vues.saw(Pointer::Arrow);
        vues.saw(Pointer::Text);
        vues.saw(Pointer::Arrow);
        let dit = vues.to_string();
        assert!(dit.contains("3 reçues"), "{dit}");
        assert!(dit.contains("arrow text"), "{dit}");

        // Et un refus se dit sur une seule ligne : le journal aligne ses
        // lignes, et une raison repliée casserait la colonne.
        let refusee = Seen {
            first_refusal: Some("la voie 3\n  n'existe plus".to_string()),
            ..Default::default()
        };
        let dit = refusee.to_string();
        assert!(
            dit.contains("premier refus : la voie 3   n'existe plus"),
            "{dit}"
        );
        assert_eq!(dit.lines().count(), 1, "{dit}");
    }
}
