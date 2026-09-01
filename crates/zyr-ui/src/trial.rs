//! Deux façons de poser la fenêtre du bouton, une par session, pour dire
//! si le blanc est bien parti.
//!
//! Ce qui a été trouvé, après six essais qui ont tous répondu non.
//!
//! Le blanc n'est ni la découpe, ni le redessin qu'on demande après elle,
//! ni le calque, ni le fond de cette fenêtre, ni un pixel que la page ne
//! peint pas : chacune de ces cinq pistes a été éteinte tour à tour et le
//! blanc est resté. Il vient d'ailleurs, et de deux fichiers qu'on peut
//! lire.
//!
//! La boîte à outils redemande à la vue web ses limites **à chaque fois
//! que la fenêtre reçoit un message de redimensionnement**, et Windows en
//! envoie un à chaque appel qui pose la fenêtre sans dire que la taille
//! n'a pas changé. Or la fenêtre du bouton est reposée **cent vingt fois
//! par seconde** pendant qu'une main la déplace, toujours à la même
//! taille. Cent vingt fois par seconde, la vue web recrée donc sa surface,
//! et une vue web qui recrée sa surface montre son fond à elle, qui est
//! blanc, le temps que son dessin revienne.
//!
//! D'où tout le reste : le blanc au clic et au déplacement et nulle part
//! ailleurs, puisque ce sont les deux seuls moments où cette fenêtre
//! bouge ou change de taille ; rien au repos, où Windows n'envoie rien du
//! tout ; et un liseré plutôt qu'une plaque, parce que la découpe collée
//! au dessin n'en laisse voir que la marge.
//!
//! La correction est de dire à Windows ce qui est vrai : la taille n'a pas
//! changé. Elle est dans le produit, et cet essai remet l'ancien geste
//! pour que la différence se voie sur la même machine, à la suite.

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::journal::note;

/// Comment la fenêtre est posée pour un essai.
#[derive(Clone, Copy)]
pub struct Trial {
    /// Redire sa taille à la fenêtre à chaque pas, même quand elle n'a
    /// pas changé, ce que le produit faisait avant.
    pub resizes: bool,
}

/// Les deux, dans l'ordre où ils sont joués.
const TRIALS: [Trial; 2] = [Trial { resizes: false }, Trial { resizes: true }];

/// Combien ont été lancés, ce qui est aussi le numéro de celui qui tourne.
static STARTED: AtomicUsize = AtomicUsize::new(0);

/// Prend l'essai suivant et dit lequel c'est.
///
/// Appelé là où la fenêtre du bouton est bâtie, ce qui arrive une fois
/// par session : la fenêtre est fermée à la fin de la session, donc
/// fermer la session et en ouvrir une autre est ce qui fait avancer.
pub fn starts() -> Trial {
    let which = STARTED.load(Ordering::Relaxed) % TRIALS.len();
    STARTED.store(which + 1, Ordering::Relaxed);
    let trial = TRIALS[which];
    note(&format!(
        "essai du bord {}/{} : {}. Clique sur le bouton et déplace-le, \
         puis ferme la session et rouvre-la pour passer au suivant.",
        which + 1,
        TRIALS.len(),
        if trial.resizes {
            "la fenêtre retaillée à chaque pas, comme avant"
        } else {
            "le bouton corrigé"
        }
    ));
    trial
}

/// L'essai en cours, là où la fenêtre est posée, ce que seul Windows
/// fait ici.
#[cfg(windows)]
pub fn now() -> Trial {
    let started = STARTED.load(Ordering::Relaxed);
    // Rien de lancé, c'est le produit corrigé, qui est le premier essai.
    TRIALS[started.saturating_sub(1) % TRIALS.len()]
}
