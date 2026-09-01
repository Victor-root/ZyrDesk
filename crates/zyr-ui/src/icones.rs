//! Les icônes du produit, reprises trait pour trait des pages qui les
//! portaient.
//!
//! Recopiées et non redessinées : ce sont les mêmes icônes, et les
//! redessiner en donnerait d'autres.
//!
//! Toutes ensemble et non chacune chez son écran : le menu de la session
//! et l'accueil en partagent, et une icône dessinée à deux endroits est
//! le jour où l'un des deux change.
//!
//! Toutes dans un repère de vingt-quatre et d'un trait de un et huit
//! dixièmes, ce que la feuille de style demandait à toutes sans
//! exception. Celles qui ont un autre repère le disent.

use crate::paint::{Icone, Trait};

/// Le repère et le trait communs, écrits une fois.
const fn dessin(traits: &'static [Trait]) -> Icone {
    Icone {
        repere: 24.0,
        epaisseur: 1.8,
        traits,
    }
}

pub const PLEIN_ECRAN: Icone = dessin(&[
    Trait::Chemin("M8 3H5a2 2 0 0 0-2 2v3M16 3h3a2 2 0 0 1 2 2v3"),
    Trait::Chemin("M8 21H5a2 2 0 0 1-2-2v-3M16 21h3a2 2 0 0 0 2-2v-3"),
]);

pub const STATISTIQUES: Icone = dessin(&[Trait::Chemin("M3 20V10M9 20V4M15 20v-7M21 20V8")]);

pub const SOURIS: Icone = dessin(&[
    Trait::Rond(7.0, 2.5, 10.0, 19.0, 5.0),
    Trait::Chemin("M12 7v3"),
]);

pub const SON: Icone = dessin(&[
    Trait::Chemin("M11 5 6.5 9H3v6h3.5L11 19z"),
    Trait::Chemin("M15.5 8.5a5 5 0 0 1 0 7M18.5 5.5a9 9 0 0 1 0 13"),
]);

pub const CLAVIER: Icone = dessin(&[
    Trait::Rond(2.0, 5.0, 20.0, 14.0, 2.0),
    Trait::Chemin("M6 9h1M9.5 9h1M13 9h1M16.5 9h1M6 13h1M9.5 13h5M17 13h1"),
]);

pub const CAD: Icone = dessin(&[
    Trait::Rond(2.5, 6.0, 19.0, 12.0, 2.0),
    Trait::Chemin("M6 10h1M9.5 10h1M13 10h1M16.5 10h1M6 14h12"),
]);

pub const VERROU: Icone = dessin(&[
    Trait::Rond(4.0, 10.5, 16.0, 10.5, 2.0),
    Trait::Chemin("M8 10.5V7a4 4 0 0 1 8 0v3.5M12 14.5v2.5"),
]);

pub const MASQUER: Icone = dessin(&[
    Trait::Chemin(
        "M10.6 6.2A9.9 9.9 0 0 1 12 6c5 0 9 4.5 10 6a15 15 0 0 1-3 3.6M6.1 8.3C4.4 9.5 3.3 11 3 12c1 1.5 5 6 9 6a9.6 9.6 0 0 0 3.6-.7",
    ),
    Trait::Chemin("M9.9 9.9a3 3 0 0 0 4.2 4.2M3 3l18 18"),
]);

pub const QUITTER: Icone = dessin(&[
    Trait::Chemin("M12 3v9"),
    Trait::Chemin("M18.4 6.6a9 9 0 1 1-12.8 0"),
]);

pub const RESOLUTION: Icone = dessin(&[
    Trait::Rond(2.5, 4.0, 19.0, 13.0, 2.0),
    Trait::Chemin("M9 21h6M12 17v4"),
]);

pub const ECRAN_HOTE: Icone = dessin(&[
    Trait::Rond(2.0, 4.0, 13.0, 9.0, 1.5),
    Trait::Rond(9.0, 11.0, 13.0, 9.0, 1.5),
]);

pub const DEBIT: Icone = dessin(&[Trait::Chemin("M3 12h3l3-7 4 14 3-7h5")]);

pub const CODEC: Icone = dessin(&[
    Trait::Rond(5.0, 5.0, 14.0, 14.0, 2.0),
    Trait::Chemin("M9 2v3M15 2v3M9 19v3M15 19v3M2 9h3M2 15h3M19 9h3M19 15h3"),
]);

pub const ECRAN_EN_FACE: Icone = dessin(&[
    Trait::Rond(2.5, 4.0, 19.0, 13.0, 2.0),
    Trait::Chemin("M9 21h6M12 17v4M7 10.5h3l1.5-3 2 6 1.5-3h2"),
]);

pub const APPLIQUER: Icone = dessin(&[
    Trait::Chemin("M20 11A8 8 0 0 0 6.3 6.3L3 9.5"),
    Trait::Chemin("M4 13a8 8 0 0 0 13.7 4.7L21 14.5"),
    Trait::Chemin("M3 4.5v5h5M21 19.5v-5h-5"),
]);

pub const CHEVRON: Icone = dessin(&[Trait::Chemin("M9 5l7 7-7 7")]);

pub const RETOUR: Icone = dessin(&[Trait::Chemin("M15 5l-7 7 7 7")]);

/// La marque de ce qui est choisi dans une liste. Plus épaisse que les
/// autres, comme dans la page : c'est une coche et non un dessin.
pub const COCHE: Icone = Icone {
    repere: 24.0,
    epaisseur: 2.2,
    traits: &[Trait::Chemin("M4 12.5l5.5 5.5L20 6")],
};
/* ---- L'accueil ------------------------------------------------------- */

pub const JOURNAL: Icone = dessin(&[
    Trait::Chemin("M5 3h11l3 3v15H5z"),
    Trait::Chemin("M9 9h6M9 13h6M9 17h4"),
]);

pub const REGLAGES: Icone = dessin(&[
    Trait::Chemin("M21 4h-7M10 4H3M21 12h-9M8 12H3M21 20h-5M12 20H3"),
    Trait::Chemin("M14 2v4M8 10v4M16 18v4"),
]);

pub const CROIX: Icone = dessin(&[Trait::Chemin("M18 6 6 18M6 6l12 12")]);

pub const PLUS: Icone = dessin(&[Trait::Chemin("M12 5v14M5 12h14")]);

/// Le chevron du repli « Avancé », ouvert vers le bas.
pub const CHEVRON_BAS: Icone = dessin(&[Trait::Chemin("M5 9l7 7 7-7")]);

/// Le dessin de l'écran vide : un ordinateur, dans son propre repère.
///
/// Le sien parce qu'il n'est pas carré, et que c'est ce qui lui donne sa
/// forme d'écran posé sur son pied.
pub const AUCUN_ORDINATEUR: Icone = Icone {
    repere: 64.0,
    epaisseur: 2.0,
    traits: &[
        Trait::Rond(1.5, 1.5, 45.0, 32.0, 4.0),
        Trait::Chemin("M17 41h27M24 33.5v7"),
    ],
};
