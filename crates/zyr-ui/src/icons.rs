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

use crate::paint::{Icon, Stroke};

/// Le repère et le trait communs, écrits une fois.
const fn icon_of(strokes: &'static [Stroke]) -> Icon {
    Icon {
        grid: 24.0,
        thickness: 1.8,
        strokes,
    }
}

pub const FULL_SCREEN: Icon = icon_of(&[
    Stroke::SvgPath("M8 3H5a2 2 0 0 0-2 2v3M16 3h3a2 2 0 0 1 2 2v3"),
    Stroke::SvgPath("M8 21H5a2 2 0 0 1-2-2v-3M16 21h3a2 2 0 0 0 2-2v-3"),
]);

pub const STATISTICS: Icon = icon_of(&[Stroke::SvgPath("M3 20V10M9 20V4M15 20v-7M21 20V8")]);

pub const MOUSE: Icon = icon_of(&[
    Stroke::RoundRect(7.0, 2.5, 10.0, 19.0, 5.0),
    Stroke::SvgPath("M12 7v3"),
]);

pub const SOUND: Icon = icon_of(&[
    Stroke::SvgPath("M11 5 6.5 9H3v6h3.5L11 19z"),
    Stroke::SvgPath("M15.5 8.5a5 5 0 0 1 0 7M18.5 5.5a9 9 0 0 1 0 13"),
]);

pub const KEYBOARD: Icon = icon_of(&[
    Stroke::RoundRect(2.0, 5.0, 20.0, 14.0, 2.0),
    Stroke::SvgPath("M6 9h1M9.5 9h1M13 9h1M16.5 9h1M6 13h1M9.5 13h5M17 13h1"),
]);

/// Le presse-papiers : la planche, la pince qui la tient par le haut, et
/// les deux lignes de ce qui y est posé.
pub const CLIPBOARD: Icon = icon_of(&[
    Stroke::RoundRect(4.0, 4.5, 16.0, 17.5, 2.0),
    Stroke::RoundRect(8.5, 2.0, 7.0, 4.5, 1.5),
    Stroke::SvgPath("M8 12h8M8 16h5"),
]);

pub const CAD: Icon = icon_of(&[
    Stroke::RoundRect(2.5, 6.0, 19.0, 12.0, 2.0),
    Stroke::SvgPath("M6 10h1M9.5 10h1M13 10h1M16.5 10h1M6 14h12"),
]);

pub const LOCK: Icon = icon_of(&[
    Stroke::RoundRect(4.0, 10.5, 16.0, 10.5, 2.0),
    Stroke::SvgPath("M8 10.5V7a4 4 0 0 1 8 0v3.5M12 14.5v2.5"),
]);

pub const HIDE: Icon = icon_of(&[
    Stroke::SvgPath(
        "M10.6 6.2A9.9 9.9 0 0 1 12 6c5 0 9 4.5 10 6a15 15 0 0 1-3 3.6M6.1 8.3C4.4 9.5 3.3 11 3 12c1 1.5 5 6 9 6a9.6 9.6 0 0 0 3.6-.7",
    ),
    Stroke::SvgPath("M9.9 9.9a3 3 0 0 0 4.2 4.2M3 3l18 18"),
]);

pub const QUIT: Icon = icon_of(&[
    Stroke::SvgPath("M12 3v9"),
    Stroke::SvgPath("M18.4 6.6a9 9 0 1 1-12.8 0"),
]);

pub const RESOLUTION: Icon = icon_of(&[
    Stroke::RoundRect(2.5, 4.0, 19.0, 13.0, 2.0),
    Stroke::SvgPath("M9 21h6M12 17v4"),
]);

/// Les deux ordinateurs d'une session, comme le logo du produit les
/// dessine : celui d'en face derrière, celui qui regarde devant.
pub const HOST_SCREEN: Icon = icon_of(&[OVER_THERE, HERE]);

/// Chacun des deux seul.
///
/// De quoi allumer l'un d'eux par-dessus la paire sans redessiner la
/// paire, ce qui est comment un voyant dit lequel des deux ordinateurs
/// coince. Écrits à partir des mêmes deux traits que la paire : recopiés,
/// ils s'en écarteraient au premier pixel changé.
///
/// Celui d'en face ne se dessine que par-dessus la paire, jamais seul :
/// son contour s'arrête là où l'autre commence, et seule la paire remet
/// ce qui lui manque.
pub const SCREEN_OVER_THERE: Icon = icon_of(&[OVER_THERE]);
pub const SCREEN_HERE: Icon = icon_of(&[HERE]);

/// Celui d'en face : le même rectangle que l'autre, mais ouvert aux deux
/// endroits où celui de devant le recouvre.
///
/// Ouvert et non entier, parce que deux contours entiers se traversent :
/// quatre traits se croisaient dans un carré de deux unités de haut, et à
/// la taille d'un voyant ça fait une tache au lieu de deux ordinateurs.
/// S'arrêter là où l'autre commence est comment un dessin dit « derrière »
/// sans avoir besoin d'être rempli, donc sans avoir à connaître la couleur
/// de ce qu'il y a dessous.
const OVER_THERE: Stroke = Stroke::SvgPath(
    "M15 11V5.5A1.5 1.5 0 0 0 13.5 4H3.5A1.5 1.5 0 0 0 2 5.5V11.5A1.5 1.5 0 0 0 3.5 13H9",
);
const HERE: Stroke = Stroke::RoundRect(9.0, 11.0, 13.0, 9.0, 1.5);

pub const BITRATE: Icon = icon_of(&[Stroke::SvgPath("M3 12h3l3-7 4 14 3-7h5")]);

pub const CODEC: Icon = icon_of(&[
    Stroke::RoundRect(5.0, 5.0, 14.0, 14.0, 2.0),
    Stroke::SvgPath("M9 2v3M15 2v3M9 19v3M15 19v3M2 9h3M2 15h3M19 9h3M19 15h3"),
]);

pub const FAR_SCREEN: Icon = icon_of(&[
    Stroke::RoundRect(2.5, 4.0, 19.0, 13.0, 2.0),
    Stroke::SvgPath("M9 21h6M12 17v4M7 10.5h3l1.5-3 2 6 1.5-3h2"),
]);

/// Le lien entre les deux ordinateurs : trois arcs et un point, le dessin
/// que tout le monde lit comme « réseau » sans qu'on ait à l'écrire.
///
/// Les trois arcs et le point tournent autour d'un seul et même centre, à
/// douze et trente centièmes, ouverts du même angle et espacés du même
/// écart : c'est ce qui laisse le même jour de deux unités et quatre
/// dixièmes partout. Les arcs d'avant avaient chacun leur centre, donc
/// des jours de un et demi puis trois et demi puis quatre, et le point se
/// trouvait collé sous le plus petit.
pub const LINK: Icon = icon_of(&[
    Stroke::SvgPath("M2.62 8.59A13.5 13.5 0 0 1 21.38 8.59"),
    Stroke::SvgPath("M5.54 11.61A9.3 9.3 0 0 1 18.46 11.61"),
    Stroke::SvgPath("M8.46 14.63A5.1 5.1 0 0 1 15.54 14.63"),
    Stroke::RoundRect(11.1, 17.4, 1.8, 1.8, 0.9),
]);

pub const CHEVRON: Icon = icon_of(&[Stroke::SvgPath("M9 5l7 7-7 7")]);

pub const BACK: Icon = icon_of(&[Stroke::SvgPath("M15 5l-7 7 7 7")]);

/// La marque de ce qui est choisi dans une liste. Plus épaisse que les
/// autres, comme dans la page : c'est une coche et non un dessin.
pub const TICK: Icon = Icon {
    grid: 24.0,
    thickness: 2.2,
    strokes: &[Stroke::SvgPath("M4 12.5l5.5 5.5L20 6")],
};
/* ---- L'accueil ------------------------------------------------------- */

pub const JOURNAL: Icon = icon_of(&[
    Stroke::SvgPath("M5 3h11l3 3v15H5z"),
    Stroke::SvgPath("M9 9h6M9 13h6M9 17h4"),
]);

/// Une maison : joindre un ordinateur sans sortir d'ici.
///
/// Le dessin dit le lieu et non le fil, parce que c'est le lieu qui fait
/// la différence : la session ne quitte pas la maison, et rien de ce qui
/// est dehors ne peut la faire tomber.
pub const LOCAL_NETWORK: Icon = icon_of(&[
    Stroke::SvgPath("M3 10.5 12 3.5l9 7V19a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"),
    Stroke::SvgPath("M9.5 21v-6h5v6"),
]);

pub const SETTINGS: Icon = icon_of(&[
    Stroke::SvgPath("M21 4h-7M10 4H3M21 12h-9M8 12H3M21 20h-5M12 20H3"),
    Stroke::SvgPath("M14 2v4M8 10v4M16 18v4"),
]);

pub const CROSS: Icon = icon_of(&[Stroke::SvgPath("M18 6 6 18M6 6l12 12")]);

pub const PLUS: Icon = icon_of(&[Stroke::SvgPath("M12 5v14M5 12h14")]);

/// Le chevron du repli « Avancé », ouvert vers le bas.
pub const CHEVRON_DOWN: Icon = icon_of(&[Stroke::SvgPath("M5 9l7 7 7-7")]);

/// Le dessin de l'écran vide : un ordinateur, dans son propre repère.
///
/// Le sien parce qu'il n'est pas carré, et que c'est ce qui lui donne sa
/// forme d'écran posé sur son pied.
pub const NO_COMPUTER: Icon = Icon {
    grid: 64.0,
    thickness: 2.0,
    strokes: &[
        Stroke::RoundRect(1.5, 1.5, 45.0, 32.0, 4.0),
        Stroke::SvgPath("M17 41h27M24 33.5v7"),
    ],
};
