//! Le système de design de ZyrDesk, du côté de ce qui est dessiné.
//!
//! Les couleurs, les espacements, les rayons, les ombres et les tailles
//! de texte sont écrits une seule fois, dans `design.css`, et lus d'ici à
//! la compilation. Rien n'est recopié : deux copies d'une palette, ce
//! sont deux palettes, et la première couleur changée dans l'une est le
//! jour où le produit cesse de se ressembler.
//!
//! Plus aucun navigateur ne lit ce fichier. Il garde sa notation parce
//! qu'elle écrit deux thèmes côte à côte, et parce que le lire à la
//! compilation est ce qui vérifie que les deux disent bien les mêmes
//! rôles.
//!
//! Tous les rôles sont extraits, y compris ceux que rien ne dessine
//! encore : le système de design est une palette, pas une liste de
//! courses. En extraire seulement ce qui sert aujourd'hui reviendrait à
//! le rouvrir à chaque écran repris, ce qui est la porte ouverte à une
//! deuxième palette écrite à la main en attendant.
#![allow(dead_code)]

/// Une couleur, en quatre nombres entre zéro et un, qui est la façon dont
/// tout ce qui dessine les veut.
#[derive(Clone, Copy)]
pub struct Couleur {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
    pub alpha: f32,
}

impl Couleur {
    /// Rien du tout.
    ///
    /// Ce sur quoi une fenêtre à calque commence son dessin : là où la
    /// toile reste de cette couleur, on voit ce qu'il y a derrière la
    /// fenêtre, et les clics y passent.
    pub const RIEN: Couleur = Couleur {
        red: 0.0,
        green: 0.0,
        blue: 0.0,
        alpha: 0.0,
    };

    /// Le noir plein.
    ///
    /// Dont seule la part employée sert : c'est ce qu'une boîte de
    /// dialogue pose sur ce qu'elle recouvre, et le seul endroit du
    /// produit où une couleur n'est pas un rôle.
    pub const NOIR: Couleur = Couleur {
        red: 0.0,
        green: 0.0,
        blue: 0.0,
        alpha: 1.0,
    };

    /// Celle-ci mêlée à celle-là, dans cette proportion.
    ///
    /// Ce que la feuille de style écrit `color-mix(in srgb, ... 8%,
    /// ...)` : la teinte d'un rôle passée sur un fond, là où poser une
    /// deuxième couleur pleine donnerait une couleur de plus à tenir.
    pub fn melee(self, fond: Couleur, part: f32) -> Couleur {
        let entre = |mien: f32, sien: f32| sien + (mien - sien) * part;
        Couleur {
            red: entre(self.red, fond.red),
            green: entre(self.green, fond.green),
            blue: entre(self.blue, fond.blue),
            alpha: entre(self.alpha, fond.alpha),
        }
    }

    /// La même, posée en voile.
    ///
    /// Ce que la feuille de style écrit `color-mix(in srgb, ... 12%,
    /// transparent)` : la teinte d'un rôle employée comme fond, là où
    /// repeindre avec une deuxième couleur donnerait une couleur de plus
    /// à tenir.
    pub fn voile(self, part: f32) -> Couleur {
        Couleur {
            alpha: self.alpha * part,
            ..self
        }
    }
}

/// Une ombre portée : de combien elle est décalée, de combien elle est
/// floue, et de quelle couleur.
#[derive(Clone, Copy)]
pub struct Ombre {
    pub across: f32,
    pub down: f32,
    pub soft: f32,
    pub tint: Couleur,
}

include!(concat!(env!("OUT_DIR"), "/design.rs"));

/// La palette du thème que la fenêtre porte.
///
/// Demandée au système plutôt que gardée ici : c'est le même thème que
/// l'accueil, et l'accueil le tient déjà de sa fenêtre.
pub fn palette(clair: bool) -> Palette {
    if clair { CLAIR } else { SOMBRE }
}
