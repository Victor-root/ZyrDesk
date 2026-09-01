//! La dernière chose qui bouge quand le blanc paraît : quatre façons de
//! **déplacer** la fenêtre du bouton, une par session.
//!
//! Ce qui a été éteint, tour à tour, chaque fois dans une session à
//! elle, et chaque fois le blanc est resté : la découpe, refaite à chaque
//! image puis figée pour toute la session ; le redessin qu'on demande
//! après elle, sans effacement, sans la vue web, puis pas demandé du
//! tout ; le calque ; le fond de la fenêtre ; sa transparence ; et le
//! message de redimensionnement qu'une pose de fenêtre envoyait pour
//! rien. Sept pistes, sept fois non.
//!
//! Ce qui a été éteint aussi, mais mesuré ici et non chez Victor : la
//! page. Le dessin a été rendu dans un navigateur à 125 et 175 %, sur
//! fond noir et sur fond blanc, au repos **et arrêté net à sept endroits
//! de son animation**. Le pixel le plus clair des quatre pixels qui
//! entourent le dessin vaut `0,0,0` dans tous les cas. La page ne peint
//! rien autour du logo, jamais.
//!
//! Il ne reste donc qu'une chose que le produit fait pendant qu'un
//! bouton est cliqué ou traîné et qu'il ne fait pas au repos : **la
//! fenêtre est déplacée**, cent vingt fois par seconde tant qu'une main
//! la tient. Ces quatre essais sont les quatre façons de le faire
//! autrement, la dernière étant de ne pas le faire du tout.
//!
//! Un essai par session, dans l'ordre, et le journal dit lequel tourne.
//! C'est un instrument, pas une fonctionnalité : il part le jour où il a
//! répondu.

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::journal::note;

/// Comment la fenêtre est déplacée pour un essai.
#[derive(Clone, Copy, PartialEq)]
pub enum Move {
    /// Ce que fait le produit.
    AsToday,
    /// Sans laisser Windows recopier les pixels d'avant à la nouvelle
    /// place.
    NoCopy,
    /// Sans rien redessiner du tout au passage.
    NoRedraw,
    /// Pas déplacée. Le bouton ne suit plus la main, ce qui est le prix
    /// de la question.
    Still,
}

/// Les quatre, dans l'ordre où ils sont joués.
const TRIALS: [Move; 4] = [Move::AsToday, Move::NoCopy, Move::NoRedraw, Move::Still];

/// Combien ont été lancés, ce qui est aussi le numéro de celui qui tourne.
static STARTED: AtomicUsize = AtomicUsize::new(0);

/// Prend l'essai suivant et dit lequel c'est.
///
/// Appelé là où la fenêtre du bouton est bâtie, ce qui arrive une fois
/// par session : la fenêtre est fermée à la fin de la session, donc
/// fermer la session et en ouvrir une autre est ce qui fait avancer.
pub fn starts() -> Move {
    let which = STARTED.load(Ordering::Relaxed) % TRIALS.len();
    STARTED.store(which + 1, Ordering::Relaxed);
    let trial = TRIALS[which];
    note(&format!(
        "essai du bord {}/{} : {}. Clique sur le bouton et déplace-le, \
         puis ferme la session et rouvre-la pour passer au suivant.",
        which + 1,
        TRIALS.len(),
        match trial {
            Move::AsToday => "le bouton tel qu'il est",
            Move::NoCopy => "sans recopier les pixels au déplacement",
            Move::NoRedraw => "sans redessiner au déplacement",
            Move::Still => "la fenêtre qui ne se déplace pas (le bouton ne suivra pas la main)",
        }
    ));
    trial
}

/// L'essai en cours, là où la fenêtre est posée, ce que seul Windows
/// fait ici.
#[cfg(windows)]
pub fn now() -> Move {
    let started = STARTED.load(Ordering::Relaxed);
    // Rien de lancé, c'est le produit tel quel, qui est le premier essai.
    TRIALS[started.saturating_sub(1) % TRIALS.len()]
}
