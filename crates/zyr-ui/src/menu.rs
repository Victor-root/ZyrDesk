//! Le menu du bouton flottant, dessiné par ZyrDesk.
//!
//! La carte qui s'ouvre sous le logo, dans une fenêtre à elle, faite des
//! mêmes pixels que lui : une image qui porte sa transparence et qu'on
//! remet telle quelle à Windows. Ni découpe, ni fond à effacer, ni cadre,
//! et les clics passent partout où l'image est claire.
//!
//! Tout ce qui décide de son allure vient du système de design, lu dans
//! la feuille de style à la compilation. Rien n'est écrit en dur ici : ce
//! fichier dit où les choses vont, jamais de quelle couleur elles sont.
//!
//! Les longueurs sont écrites en pixels de page, comme dans la feuille de
//! style, et `echelle` les passe en vrais pixels au moment de dessiner.
//! C'est le même partage que partout ailleurs, et c'est ce qui permet de
//! relire une mesure ici et de la retrouver là-bas.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};

use tauri::{AppHandle, Manager};

use crate::design::{self, Couleur, Palette};
use crate::floating::Act;
use crate::journal::note;
use crate::mesures::Mesures;
use crate::paint::{Cadre, Cale, Icone, Toile};
use crate::settings::{Offered, SessionMenu};
use crate::shortcuts::Doing;

/// Une ligne de la carte.
///
/// La carte se décrit et se dessine ensuite : mesurer sa largeur demande
/// de connaître toutes ses lignes avant d'en poser une seule, et une
/// carte aussi large que sa plus longue ligne est ce que la feuille de
/// style demande depuis toujours.
enum Ligne {
    /// Ce que la session coûte : quatre nombres et une phrase.
    Mesures,
    /// Un trait entre deux groupes.
    Separateur,
    /// Une ligne qu'on clique, comme la page les appelle.
    Entree(Entree),
    /// Une ligne qui porte un choix entre deux côtés.
    Bascule(Bascule),
    /// Une ligne qui porte quelques valeurs côte à côte.
    Choix(Choix),
    /// Une ligne qu'on pousse le long d'une barre.
    Curseur(Curseur),
    /// Une ligne qui ouvre une liste à elle.
    Liste(Liste),
    /// Relancer l'image avec ce qui vient d'être choisi.
    ///
    /// Elle n'est là que quand ce qui est choisi n'est pas ce qui est à
    /// l'écran, ce qui permet d'en changer plusieurs et de ne relancer
    /// qu'une fois.
    Appliquer,
}

/// Une ligne qui porte quelques valeurs sans ordre entre elles.
///
/// Des boutons et non une barre : le codec n'est pas une échelle, ce sont
/// quelques noms dont un « Automatique » qui n'est pas une valeur mais un
/// renoncement, et pousser un curseur promettrait un plus et un moins qui
/// n'existent pas.
struct Choix {
    icone: &'static Icone,
    mot: &'static str,
    quoi: Reglage,
}

/// Une ligne qu'on règle en poussant un curseur, la valeur écrite
/// au-dessus.
///
/// Une échelle : plus grand, plus rapide, et on en cherche le bon cran en
/// regardant l'image bouger. Les crans viennent du produit, et le curseur
/// va de zéro au nombre de valeurs moins une : ce sont des crans nommés
/// et non des nombres, les débits n'étant pas espacés régulièrement.
struct Curseur {
    icone: &'static Icone,
    mot: &'static str,
    quoi: Reglage,
}

/// Une ligne qui ouvre une liste à elle, à gauche de la carte.
///
/// Une liste plutôt qu'une barre pour deux raisons : ses premières
/// entrées ne sont pas des nombres mais disent lequel des deux
/// ordinateurs décide, ce qu'aucune barre ne sait dire, et il y en a
/// quinze en dessous, ce qui fait des crans qu'on ne vise plus.
struct Liste {
    icone: &'static Icone,
    mot: &'static str,
    quoi: Reglage,
}

/// Une ligne qui porte un choix plutôt qu'une action.
///
/// Les deux mots sont là et celui qui est en place est allumé : la ligne
/// d'avant annonçait ce que le clic ferait et jamais où l'on en était, et
/// les deux modes ne se distinguent pas à l'oeil sur un bureau immobile.
/// Il faut que ça se voie sans lire.
///
/// La ligne elle-même ne se clique pas, seulement ses deux côtés : ni
/// main sous le pointeur ni fond allumé sur le reste, qui promettraient
/// un clic qui ne fait rien.
struct Bascule {
    icone: &'static Icone,
    mot: &'static str,
    /// Les deux côtés, dans l'ordre où ils se lisent. Le second est celui
    /// qui vaut « oui ».
    cotes: [&'static str; 2],
    /// Ce qu'on demande à la session pour passer d'un côté à l'autre.
    passe: Act,
    /// Où l'on en est : vrai pour le côté de droite.
    ou: &'static AtomicBool,
}

/// Une entrée du menu : une icône, un mot, ce qui est écrit à sa droite,
/// et ce qu'elle demande.
struct Entree {
    icone: &'static Icone,
    mot: &'static str,
    droite: Droite,
    fait: Fait,
    /// Écrite dans la couleur des choses qui ne se défont pas. Une seule
    /// ligne du menu l'est, et c'est celle qui coupe la session.
    grave: bool,
}

/// Ce qui s'écrit à droite d'une ligne.
enum Droite {
    /// Ce que la ligne fait, dit en toutes lettres.
    Mot(&'static str),
    /// La combinaison en place pour ça, ou ce mot-ci tant que personne
    /// ne lui en a donné une.
    Touche(Doing, &'static str),
}

/// Ce qu'une ligne demande quand on clique dessus.
#[derive(Clone, Copy)]
enum Fait {
    /// Ce que la session sait faire, dans sa langue.
    Session(Act),
    /// Ranger le bouton jusqu'à ce que le raccourci le rappelle.
    Ranger,
}

/// Un des réglages que la session porte.
///
/// Nommé ici comme le produit le nomme des deux côtés : c'est ce mot-là
/// qui voyage jusqu'au service, et en avoir un deuxième pour l'affichage
/// serait deux noms pour un réglage.
#[derive(Clone, Copy, PartialEq)]
enum Reglage {
    Taille,
    Ecran,
    Debit,
    Codec,
    Cadence,
}

/// Ce que la carte contient, dans l'ordre.
///
/// Les mêmes lignes que la page, dans le même ordre, avec les mêmes mots,
/// les mêmes icônes et les mêmes actions. Ce qui manque encore est dit
/// dans le journal à l'ouverture plutôt que remplacé par du vide qui
/// ressemblerait à un défaut.
const LIGNES: [Ligne; 19] = [
    Ligne::Mesures,
    Ligne::Separateur,
    Ligne::Entree(Entree {
        icone: &icones::PLEIN_ECRAN,
        mot: "Fenêtré ou plein écran",
        droite: Droite::Touche(Doing::Fullscreen, ""),
        fait: Fait::Session(Act::Fullscreen),
        grave: false,
    }),
    Ligne::Entree(Entree {
        icone: &icones::STATISTIQUES,
        mot: "Statistiques",
        droite: Droite::Mot("Ctrl+Alt+Maj+S"),
        fait: Fait::Session(Act::Stats),
        grave: false,
    }),
    Ligne::Bascule(Bascule {
        icone: &icones::SOURIS,
        mot: "Souris",
        cotes: ["Bureau", "Jeu"],
        passe: Act::MouseMode,
        ou: &EN_JEU,
    }),
    Ligne::Bascule(Bascule {
        icone: &icones::SON,
        mot: "Son",
        cotes: ["Actif", "Coupé"],
        passe: Act::Sound,
        ou: &COUPE,
    }),
    Ligne::Bascule(Bascule {
        icone: &icones::CLAVIER,
        mot: "Clavier",
        cotes: ["Partagé", "Immersif"],
        passe: Act::SystemKeys,
        ou: &IMMERSIF,
    }),
    Ligne::Entree(Entree {
        icone: &icones::CAD,
        mot: "Ctrl+Alt+Suppr",
        droite: Droite::Mot("sur l'ordinateur distant"),
        fait: Fait::Session(Act::SecureAttention),
        grave: false,
    }),
    Ligne::Entree(Entree {
        icone: &icones::VERROU,
        mot: "Verrouiller",
        droite: Droite::Mot("l'ordinateur distant"),
        fait: Fait::Session(Act::LockScreen),
        grave: false,
    }),
    Ligne::Separateur,
    Ligne::Liste(Liste {
        icone: &icones::RESOLUTION,
        mot: "Résolution",
        quoi: Reglage::Taille,
    }),
    Ligne::Liste(Liste {
        icone: &icones::ECRAN_HOTE,
        mot: "Écran de l'hôte",
        quoi: Reglage::Ecran,
    }),
    Ligne::Curseur(Curseur {
        icone: &icones::DEBIT,
        mot: "Débit",
        quoi: Reglage::Debit,
    }),
    Ligne::Choix(Choix {
        icone: &icones::CODEC,
        mot: "Codec",
        quoi: Reglage::Codec,
    }),
    Ligne::Choix(Choix {
        icone: &icones::ECRAN_EN_FACE,
        mot: "Écran d'en face",
        quoi: Reglage::Cadence,
    }),
    Ligne::Appliquer,
    Ligne::Separateur,
    Ligne::Entree(Entree {
        icone: &icones::MASQUER,
        mot: "Masquer ce bouton",
        droite: Droite::Touche(Doing::Menu, "jusqu'à la fin"),
        fait: Fait::Ranger,
        grave: false,
    }),
    Ligne::Entree(Entree {
        icone: &icones::QUITTER,
        mot: "Terminer la session",
        droite: Droite::Touche(Doing::End, "rend le bureau distant"),
        fait: Fait::Session(Act::End),
        grave: true,
    }),
];

/// Les icônes du menu, reprises trait pour trait du dessin de la page.
///
/// Recopiées et non redessinées : ce sont les mêmes icônes, et les
/// redessiner en donnerait d'autres. Le jour où la page du menu s'en va,
/// c'est ici qu'elles vivront, et il n'y en aura plus qu'un exemplaire.
///
/// Une par ligne existante, et pas une de plus : celles des lignes qui
/// sont encore dans la vue web arriveront avec elles.
///
/// Toutes dans un repère de vingt-quatre et d'un trait de un et huit
/// dixièmes, ce que la feuille de style demande à toutes sans exception.
mod icones {
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
}

/// Ce que la feuille de style dit d'une ligne, en pixels de page.
mod tenue {
    /// La hauteur qu'une ligne ne descend jamais en dessous.
    pub const LIGNE: f32 = 38.0;
    /// Le côté d'une icône, et l'espace entre elle et le mot.
    pub const ICONE: f32 = 18.0;
    /// Ce qui sépare le mot de ce qui est écrit à sa droite.
    pub const APRES_LE_MOT: f32 = 24.0;
    /// L'épaisseur d'un trait de séparation, et celle d'une bordure.
    pub const TRAIT: f32 = 1.0;
    /// La largeur que chaque mesure garde quel que soit son nombre, pour
    /// que la barre ne respire pas au rythme des chiffres.
    pub const MESURE: f32 = 78.0;
    /// Ce qui sépare deux mesures, et ce qui sépare leur mot de leur
    /// nombre.
    pub const ENTRE_MESURES: f32 = 16.0;
    pub const SOUS_LE_MOT: f32 = 2.0;
    /// La hauteur d'un interrupteur : sa légende, ce qui l'entoure
    /// au-dessus et en dessous, et sa bordure. La page l'obtient de la
    /// hauteur de ligne du navigateur, qui n'existe pas ici : elle est
    /// donc dite.
    pub const BASCULE: f32 = 24.0;
    /// La place que prend un curseur, son pouce compris.
    pub const CURSEUR: f32 = 18.0;
    /// L'épaisseur de la barre d'un curseur, et le côté de son pouce.
    pub const BARRE: f32 = 4.0;
    pub const POUCE: f32 = 14.0;
    /// Le côté d'un chevron et d'une coche : plus petits qu'une icône de
    /// ligne, parce que ce sont des marques et non des dessins.
    pub const MARQUE: f32 = 16.0;
}

/// Les deux mots de la ligne « Appliquer », qui n'est pas un réglage et
/// n'en porte donc pas le nom.
const APPLIQUER: &str = "Appliquer les changements";
const APRES_APPLIQUER: &str = "relance l'image";

/// Un des quatre chiffres de la barre : ce qu'il coûte, comment il se
/// lit, et où il se prend dans ce que le moteur écrit.
struct Chiffre {
    mot: &'static str,
    unite: &'static str,
    /// Combien de décimales : le réseau se lit en millisecondes rondes,
    /// le reste au centième.
    apres: usize,
    lu: fn(&Mesures) -> Option<f64>,
}

/// Les quatre mesures, dans l'ordre où elles se lisent : ce que coûte une
/// image ici, ce qu'elle a coûté là-bas, ce qu'il y a entre les deux, et
/// ce que le fil porte vraiment.
///
/// Les mêmes mots et les mêmes unités que la page, parce que ce sont les
/// mêmes mesures : les inventer ici en donnerait quatre autres, et deux
/// barres qui ne disent pas la même chose sur le même moteur.
const MESURES: [Chiffre; 4] = [
    Chiffre {
        mot: "Décodage",
        unite: "ms",
        apres: 2,
        lu: |dit| dit.decode_ms,
    },
    Chiffre {
        mot: "Encodage",
        unite: "ms",
        apres: 2,
        lu: |dit| dit.host_ms,
    },
    Chiffre {
        mot: "Réseau",
        unite: "ms",
        apres: 0,
        lu: |dit| dit.network_ms,
    },
    Chiffre {
        mot: "Débit",
        unite: "Mb/s",
        apres: 2,
        lu: |dit| dit.bitrate_mbps,
    },
];

/// Ce qu'une mesure montre tant qu'elle n'a rien à dire.
///
/// Le moteur ne dit rien plutôt que zéro quand il n'a rien mesuré, et
/// zéro serait un mensonge : une seconde sans image décodée n'a pas un
/// temps de décodage nul.
const RIEN: &str = "-";

/// Le rythme du moteur, qui écrit une fois par seconde. Demander plus
/// souvent relirait le même fichier pour le même nombre.
const RYTHME: std::time::Duration = std::time::Duration::from_secs(1);

/// De combien une couleur teinte le fond quand elle sert de survol : ce
/// que la feuille de style écrit `color-mix(in srgb, ... 12%,
/// transparent)`.
const VOILE: f32 = 0.12;

/// La fenêtre de la carte, et ce qu'elle sait d'elle-même.
static ITS_WINDOW: AtomicIsize = AtomicIsize::new(0);
static LARGE: AtomicU32 = AtomicU32::new(0);
static HAUTE: AtomicU32 = AtomicU32::new(0);
static OUVERT: AtomicBool = AtomicBool::new(false);
static CLAIR: AtomicBool = AtomicBool::new(false);

/// Ce qui est sous la souris, et ce sur quoi un clic a commencé.
///
/// Écrits par la réponse de la fenêtre, que le système appelle, et lus
/// par le dessin. Les deux tournent sur le fil qui possède la fenêtre,
/// donc ces verrous ne sont jamais disputés.
static SURVOL: Mutex<Option<Cible>> = Mutex::new(None);
static PRESSEE: Mutex<Option<Cible>> = Mutex::new(None);

/// Si la souris est dans cette fenêtre, pour ne demander qu'une fois à
/// être prévenu de son départ.
static DEDANS: AtomicBool = AtomicBool::new(false);

/// Ce qu'on peut cliquer, dans la carte ou dans le panneau ouvert.
#[derive(Clone, Copy, PartialEq)]
enum Cible {
    /// Une ligne qu'on clique en entier, par son rang dans `LIGNES`.
    Ligne(usize),
    /// Un côté d'un interrupteur ou d'une ligne à boutons : le rang de sa
    /// ligne, et lequel des côtés.
    Cote(usize, usize),
    /// La barre d'un curseur.
    Barre(usize),
    /// Le titre du panneau ouvert, qui le referme.
    Retour,
    /// Une valeur du panneau ouvert, par son rang dans la liste.
    Valeur(usize),
}

impl Cible {
    /// La ligne de la carte dont il s'agit, quand c'en est une.
    fn ligne(self) -> Option<usize> {
        match self {
            Cible::Ligne(rang) | Cible::Cote(rang, _) | Cible::Barre(rang) => Some(rang),
            Cible::Retour | Cible::Valeur(_) => None,
        }
    }
}

/// Ce que la barre des mesures montre : quatre nombres déjà écrits et la
/// phrase du flux.
///
/// Écrits là où ils sont lus plutôt que gardés en nombres : la mise en
/// forme se fait alors une fois par seconde et non une fois par image, et
/// le fil qui dessine n'a plus qu'à poser du texte.
#[derive(PartialEq)]
struct Barre {
    chiffres: [String; 4],
    flux: String,
}

static BARRE: Mutex<Barre> = Mutex::new(Barre::vide());

/// Le tour de veille des mesures.
///
/// Il change à chaque ouverture et à chaque fermeture, ce qui arrête le
/// tour précédent : sans ça, ouvrir et refermer vite laisserait deux
/// veilles derrière la même carte.
static TOUR: AtomicU32 = AtomicU32::new(0);

/// Où en est chacun des trois interrupteurs.
///
/// Relus à chaque ouverture de la carte plutôt que retenus : le raccourci
/// du produit bascule la souris, et le mélangeur de Windows est ouvert à
/// tout le monde. Un interrupteur qui montre ce qu'il croit plutôt que ce
/// qui est est un interrupteur qu'on ne croit pas deux fois.
static EN_JEU: AtomicBool = AtomicBool::new(false);
static COUPE: AtomicBool = AtomicBool::new(false);
static IMMERSIF: AtomicBool = AtomicBool::new(false);

/// De combien un pixel de page vaut de vrais pixels.
static ECHELLE: AtomicU32 = AtomicU32::new(100);

/// La hauteur d'une ligne de légende et d'une ligne de corps, en vrais
/// pixels.
///
/// Ce n'est pas la taille du caractère : une ligne de douze pixels en
/// occupe environ seize, l'espace au-dessus et en dessous étant celui que
/// la police demande. Empiler du texte sur sa taille plutôt que sur sa
/// hauteur serre tout ce qui est empilé, et c'est ce qui rendait la barre
/// des mesures plus tassée que celle de la page.
///
/// Mesurées une fois, quand la carte l'est : elles ne dépendent que de la
/// taille du texte et de l'agrandissement de l'écran, dont aucun ne bouge
/// pendant une session.
static HAUTE_LEGENDE: AtomicU32 = AtomicU32::new(0);
static HAUTE_CORPS: AtomicU32 = AtomicU32::new(0);

/// Vers où le menu s'ouvre, donc à quel bord de sa fenêtre la carte est
/// collée.
static VERS_LE_HAUT: AtomicBool = AtomicBool::new(false);

/// Ce que la carte prend de large, mesuré sur toutes ses lignes.
static LARGE_CARTE: AtomicU32 = AtomicU32::new(0);

/// Ce que la session propose et où elle en est, demandé à l'ouverture de
/// la carte.
///
/// Demandé d'un coup plutôt qu'une liste à la fois : la carte se mesure
/// sur ce qu'elle contient, donc elle a besoin de tout avant de poser
/// quoi que ce soit.
static REGLAGES: Mutex<Option<SessionMenu>> = Mutex::new(None);

/// Le sous-menu ouvert, ou rien.
static PANNEAU: Mutex<Option<Reglage>> = Mutex::new(None);

/// Le cran où une main tient le curseur du débit, tant qu'elle le tient.
///
/// Ce qui est choisi n'est écrit qu'au relâchement : un curseur poussé
/// d'un bout à l'autre traverse quinze crans, et chacun d'eux serait un
/// aller-retour jusqu'au service pour un débit que personne n'a voulu.
static POUSSE: Mutex<Option<usize>> = Mutex::new(None);

/// Le programme, pour les endroits que le système appelle et à qui la
/// boîte à outils ne donne rien.
static PROGRAM: Mutex<Option<AppHandle>> = Mutex::new(None);

/// Les combinaisons en place, lues à l'ouverture de la session.
///
/// Lues et non gravées : elles se choisissent dans les réglages. Lues une
/// fois, parce que la carte prend la largeur de sa plus longue ligne et
/// que cette largeur est celle de sa fenêtre, laquelle ne change pas de
/// taille d'une session à l'autre. C'est le moment que la page choisit
/// elle aussi.
static TOUCHES: Mutex<Vec<(Doing, String)>> = Mutex::new(Vec::new());

// La toile de cette fenêtre, tenue par le fil qui la possède : une
// surface de dessin et la fenêtre qu'elle habille appartiennent au fil
// qui les a faites.
thread_local! {
    static TOILE: std::cell::RefCell<Option<Toile>> = const { std::cell::RefCell::new(None) };
}

/// Une longueur rangée dans un entier partagé, en centièmes de pixel : un
/// nombre à virgule ne s'y range pas, et le centième suffit à un écran
/// agrandi de cent soixante-quinze pour cent.
fn range(ou: &AtomicU32, combien: f32) {
    ou.store((combien * 100.0).round() as u32, Ordering::Relaxed);
}

fn lue(ou: &AtomicU32) -> f32 {
    ou.load(Ordering::Relaxed) as f32 / 100.0
}

fn echelle() -> f32 {
    lue(&ECHELLE)
}

fn palette() -> Palette {
    design::palette(CLAIR.load(Ordering::Relaxed))
}

impl Barre {
    const fn vide() -> Self {
        Barre {
            chiffres: [String::new(), String::new(), String::new(), String::new()],
            flux: String::new(),
        }
    }

    /// Ce qu'une lecture du moteur donne à lire.
    fn de(dit: &Mesures) -> Self {
        Barre {
            chiffres: std::array::from_fn(|rang| {
                let quoi = &MESURES[rang];
                match (quoi.lu)(dit) {
                    Some(nombre) => format!("{nombre:.*} {}", quoi.apres, quoi.unite),
                    None => RIEN.to_string(),
                }
            }),
            flux: flux(dit),
        }
    }
}

impl Reglage {
    /// Le nom sous lequel il voyage, des deux côtés.
    fn nom(self) -> &'static str {
        match self {
            Reglage::Taille => "asked",
            Reglage::Ecran => "screen",
            Reglage::Debit => "bitrate",
            Reglage::Codec => "codec",
            Reglage::Cadence => "steady",
        }
    }

    /// Les valeurs proposées, dans l'ordre du produit.
    fn valeurs(self, menu: &SessionMenu) -> Vec<String> {
        match self {
            Reglage::Taille => menu
                .sizes
                .iter()
                .map(|taille| taille.value.clone())
                .collect(),
            Reglage::Ecran => menu.screens.iter().map(|ecran| ecran.id.clone()).collect(),
            Reglage::Debit => menu.rates.iter().map(u32::to_string).collect(),
            Reglage::Codec => menu.codecs.clone(),
            // Deux mots et non une liste : c'est un interrupteur, et ses
            // deux côtés se nomment dans la fenêtre comme ceux d'à côté.
            Reglage::Cadence => vec!["off".to_string(), "on".to_string()],
        }
    }

    /// Ce qui s'écrit pour cette valeur, là où on la choisit.
    fn dit(self, menu: &SessionMenu, valeur: &str) -> String {
        match self {
            Reglage::Taille => match valeur {
                "client" => "Résolution du client".to_string(),
                "host" => "Résolution de l'hôte".to_string(),
                _ => menu
                    .sizes
                    .iter()
                    .find(|taille| taille.value == valeur)
                    .map_or_else(|| valeur.to_string(), en_pixels),
            },
            Reglage::Ecran => menu
                .screens
                .iter()
                .find(|ecran| ecran.id == valeur)
                .map_or_else(
                    || valeur.to_string(),
                    |ecran| {
                        if ecran.main {
                            format!("{} (principal)", ecran.name)
                        } else {
                            ecran.name.clone()
                        }
                    },
                ),
            Reglage::Debit => format!(
                "{} Mb/s",
                (valeur.parse::<f64>().unwrap_or(0.0) / 1000.0).round()
            ),
            Reglage::Codec => {
                if valeur == "auto" {
                    "Automatique".to_string()
                } else {
                    valeur.to_string()
                }
            }
            Reglage::Cadence => if valeur == "on" { "Fluide" } else { "Économe" }.to_string(),
        }
    }

    /// Ce qui s'écrit à droite de la ligne du menu, quand la valeur en
    /// place ne s'y lit pas déjà.
    fn resume(self, menu: &SessionMenu) -> String {
        let ou = self.ou(menu);
        match self {
            // Ce à quoi le choix revient réellement ici : « client » ne
            // dit pas si on demande du 4K ou du 1080p, et c'est justement
            // ce qu'on veut savoir avant d'ouvrir la session.
            Reglage::Taille => {
                if ou == "host" {
                    return "hôte".to_string();
                }
                let nombres = menu
                    .sizes
                    .iter()
                    .find(|taille| taille.value == ou)
                    .map_or_else(|| ou.clone(), en_pixels);
                if ou == "client" {
                    format!("client, {nombres}")
                } else {
                    nombres
                }
            }
            // Le nom seul : « (principal) » y prendrait la place du nom
            // sans rien apprendre, la liste le disant déjà.
            Reglage::Ecran => menu
                .screens
                .iter()
                .find(|ecran| ecran.id == ou)
                .map_or_else(String::new, |ecran| ecran.name.clone()),
            _ => self.dit(menu, &ou),
        }
    }

    /// Ce qui s'écrit en colonne de droite dans la liste.
    fn aparte(self, menu: &SessionMenu, valeur: &str) -> String {
        match self {
            // Le rapport de la taille, dit comme les écrans se vendent :
            // deux nombres se comparent mal, et 21:9 à côté de 16:9 dit
            // tout de suite ce qui va être coupé. Rien pour les deux
            // premières : ce à quoi elles reviennent dépend de l'écran
            // qu'on a en face.
            Reglage::Taille if valeur != "client" && valeur != "host" => menu
                .sizes
                .iter()
                .find(|taille| taille.value == valeur)
                .filter(|taille| taille.width > 0)
                .map_or_else(String::new, |taille| rapport(taille.width, taille.height)),
            // La taille de l'écran, comme le rapport l'est pour la
            // résolution : deux écrans se distinguent d'abord par là, et
            // un nom de modèle ne dit rien à qui ne l'a pas acheté.
            Reglage::Ecran => menu
                .screens
                .iter()
                .find(|ecran| ecran.id == valeur)
                .map_or_else(String::new, |ecran| {
                    format!("{}x{}", ecran.wide, ecran.high)
                }),
            _ => String::new(),
        }
    }

    /// Où l'on en est.
    fn ou(self, menu: &SessionMenu) -> String {
        match self {
            Reglage::Taille => menu.now.asked.clone(),
            Reglage::Ecran => menu.now.screen.clone(),
            Reglage::Debit => menu.now.bitrate_kbps.to_string(),
            Reglage::Codec => menu.now.codec.clone(),
            Reglage::Cadence => if menu.now.steady { "on" } else { "off" }.to_string(),
        }
    }

    /// Ce que la machine d'en face a dit ne pas savoir faire.
    ///
    /// Rien du tout veut dire qu'elle n'a rien dit, jamais qu'elle ne sait
    /// rien faire : hors session, ou pendant que son moteur démarre, la
    /// question n'a pas de réponse, et une question sans réponse doit
    /// laisser le menu exactement comme il était.
    fn hors_de_portee(self, menu: &SessionMenu, valeur: &str) -> bool {
        self == Reglage::Codec && menu.beyond_it.iter().any(|autre| autre == valeur)
    }
}

/// Une taille, en pixels.
fn en_pixels(taille: &Offered) -> String {
    format!("{}x{}", taille.width, taille.height)
}

/// Le rapport d'une taille, réduit comme on le lit sur une fiche d'écran.
///
/// Calculé plutôt qu'écrit à côté de chaque nombre : une deuxième table
/// s'écarterait de la première le jour où une taille s'ajoute. Les deux
/// rapports que personne n'écrit sous leur forme réduite sont dits comme
/// tout le monde les dit.
fn rapport(large: u32, haut: u32) -> String {
    fn pgcd(a: u32, b: u32) -> u32 {
        if b == 0 { a } else { pgcd(b, a % b) }
    }

    let par = pgcd(large, haut).max(1);
    match (large / par, haut / par) {
        (8, 5) => "16:10".to_string(),
        (683, 384) => "16:9".to_string(),
        (x, y) => format!("{x}:{y}"),
    }
}

/// La ligne grise sous les chiffres : de quoi l'image est faite. Ce qui
/// manque ne laisse pas de trou, il ne s'écrit pas.
fn flux(dit: &Mesures) -> String {
    let mut bouts: Vec<String> = Vec::new();
    if let Some(codec) = &dit.codec {
        bouts.push(codec.clone());
    }
    if let (Some(large), Some(haute)) = (dit.width, dit.height) {
        bouts.push(format!("{large}x{haute}"));
    }
    if let Some(images) = dit.fps {
        bouts.push(format!("{images:.0} images/s"));
    }
    bouts.join(" · ")
}

impl Droite {
    /// Ce qui s'écrit, une fois les raccourcis connus.
    fn dit(&self) -> String {
        match self {
            Droite::Mot(mot) => (*mot).to_string(),
            Droite::Touche(quoi, sinon) => TOUCHES
                .lock()
                .expect("raccourcis du menu")
                .iter()
                .find(|(autre, _)| autre == quoi)
                .map_or_else(|| (*sinon).to_string(), |(_, dit)| dit.clone()),
        }
    }
}

impl Ligne {
    /// La hauteur que cette ligne prend, en vrais pixels.
    fn haute(&self, echelle: f32) -> f32 {
        match self {
            Ligne::Mesures => hauteur_des_mesures(echelle),
            Ligne::Separateur => (design::PAS_2 * 2.0 + tenue::TRAIT) * echelle,
            Ligne::Curseur(_) => hauteur_du_curseur(echelle),
            _ => tenue::LIGNE * echelle,
        }
    }

    /// Si cette ligne a lieu d'être en ce moment.
    ///
    /// Trois ne sont pas toujours là. Une machine d'en face qui n'a qu'un
    /// écran, ou dont le moteur n'a pas encore dit lesquels, ne laisse
    /// rien à choisir : la ligne s'efface plutôt que d'ouvrir une liste
    /// vide. Et « Appliquer » n'apparaît que quand ce qui est choisi n'est
    /// pas ce qui est à l'écran.
    fn se_voit(&self, menu: Option<&SessionMenu>) -> bool {
        let Some(menu) = menu else {
            // Sans réponse, la carte se réduit à ce qui ne dépend pas de
            // la session : mieux vaut une carte courte qu'une carte de
            // lignes vides.
            return !matches!(
                self,
                Ligne::Choix(_) | Ligne::Curseur(_) | Ligne::Liste(_) | Ligne::Appliquer
            );
        };
        match self {
            Ligne::Liste(liste) => !liste.quoi.valeurs(menu).is_empty(),
            Ligne::Appliquer => menu.now.to_apply,
            _ => true,
        }
    }
}

impl Bascule {
    /// Ce qui s'écrit sur ses deux côtés.
    fn mots(&self) -> Vec<String> {
        self.cotes.iter().map(|mot| (*mot).to_string()).collect()
    }

    /// Lequel des deux est en place.
    fn en_place(&self) -> usize {
        usize::from(self.ou.load(Ordering::Relaxed))
    }
}

/// Ce qui s'écrit sur les côtés d'une ligne à choix.
///
/// À part de la ligne pour qu'on puisse le demander avec les réglages
/// déjà en main : les redemander à ce moment-là reprendrait un verrou
/// qu'on tient.
fn mots_de(menu: &SessionMenu, quoi: Reglage) -> Vec<String> {
    quoi.valeurs(menu)
        .iter()
        .map(|valeur| quoi.dit(menu, valeur))
        .collect()
}

impl Choix {
    /// Ce qui s'écrit sur ses côtés, tel que la session les propose.
    fn mots(&self) -> Option<Vec<String>> {
        let reglages = REGLAGES.lock().expect("réglages du menu");
        Some(mots_de(reglages.as_ref()?, self.quoi))
    }

    /// Lequel est en place, et ceux que la machine d'en face ne sait pas
    /// faire.
    fn ou(&self) -> Option<(usize, Vec<bool>)> {
        let reglages = REGLAGES.lock().expect("réglages du menu");
        let menu = reglages.as_ref()?;
        let valeurs = self.quoi.valeurs(menu);
        let ou = self.quoi.ou(menu);
        Some((
            valeurs.iter().position(|valeur| *valeur == ou)?,
            valeurs
                .iter()
                .map(|valeur| self.quoi.hors_de_portee(menu, valeur))
                .collect(),
        ))
    }
}

impl Curseur {
    /// Le cran où il en est : celui qu'une main tient, sinon celui qui est
    /// écrit.
    fn cran(&self) -> Option<(usize, usize)> {
        let reglages = REGLAGES.lock().expect("réglages du menu");
        let menu = reglages.as_ref()?;
        let valeurs = self.quoi.valeurs(menu);
        if valeurs.is_empty() {
            return None;
        }
        let ou = self.quoi.ou(menu);
        let ecrit = valeurs.iter().position(|valeur| *valeur == ou).unwrap_or(0);
        let tenu = *POUSSE.lock().expect("curseur du menu");
        Some((tenu.unwrap_or(ecrit).min(valeurs.len() - 1), valeurs.len()))
    }

    /// Ce qui s'écrit à droite de son mot : ce qu'il vaut au cran où il
    /// est, y compris pendant qu'une main le pousse.
    fn valeur(&self) -> String {
        let reglages = REGLAGES.lock().expect("réglages du menu");
        let Some(menu) = reglages.as_ref() else {
            return String::new();
        };
        let valeurs = self.quoi.valeurs(menu);
        match *POUSSE.lock().expect("curseur du menu") {
            Some(cran) if cran < valeurs.len() => self.quoi.dit(menu, &valeurs[cran]),
            _ => self.quoi.resume(menu),
        }
    }
}

/// Ouvre la fenêtre de la carte, une fois par session.
///
/// Bâtie sur le fil qui dessine, comme celle du logo : une fenêtre
/// appartient au fil qui l'a faite, et une fenêtre faite sur le fil de la
/// veille n'entendrait jamais une souris.
pub fn raise(app: &AppHandle, echelle: f32, clair: bool) {
    if ITS_WINDOW.load(Ordering::Relaxed) != 0 {
        return;
    }
    let owner = app
        .get_webview_window(crate::HOME)
        .and_then(|home| home.hwnd().ok())
        .map(|handle| handle.0 as isize)
        .unwrap_or(0);
    *PROGRAM.lock().expect("programme du menu") = Some(app.clone());
    *TOUCHES.lock().expect("raccourcis du menu") = crate::shortcuts::engraved();
    // Quatre tirets avant la première lecture, et non quatre vides : la
    // barre est là dès la première ouverture, et ce qu'elle montre alors
    // est ce que le produit montre pour une mesure qui manque.
    *BARRE.lock().expect("mesures du menu") = Barre::de(&Mesures::default());
    range(&ECHELLE, echelle);
    CLAIR.store(clair, Ordering::Relaxed);
    OUVERT.store(false, Ordering::Relaxed);
    *PANNEAU.lock().expect("panneau du menu") = None;
    let _ = app.run_on_main_thread(move || build(owner));
    // Ce que la session propose, demandé une fois : les crans ne changent
    // pas d'un clic à l'autre. La fenêtre est bâtie sans attendre, parce
    // qu'une carte fermée n'a rien à montrer et que la réponse la
    // rattrapera avant la première ouverture.
    relis_les_reglages(app);
}

/// Redemande ce que la session propose et où elle en est.
fn relis_les_reglages(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let menu = crate::settings::session_menu(app.clone()).await;
        *REGLAGES.lock().expect("réglages du menu") = Some(menu);
        redessine(&app);
    });
}

/// Referme la carte et rend sa fenêtre avec la session.
pub fn lower(app: &AppHandle) {
    let window = ITS_WINDOW.swap(0, Ordering::Relaxed);
    if window == 0 {
        return;
    }
    OUVERT.store(false, Ordering::Relaxed);
    // La veille des mesures ne se range pas d'elle-même : elle suit la
    // carte, et une carte ouverte à la fin d'une session ne se referme
    // pas, elle disparaît.
    suis_les_mesures(app, false);
    *PROGRAM.lock().expect("programme du menu") = None;
    let _ = app.run_on_main_thread(move || {
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow;

        // SAFETY: une fenêtre à nous, défaite sur le fil qui l'a faite.
        unsafe { DestroyWindow(window as HWND) };
        TOILE.with_borrow_mut(|toile| *toile = None);
    });
}

/// Montre la carte, ou la range.
pub fn montre(ouvert: bool) {
    if ITS_WINDOW.load(Ordering::Relaxed) == 0 || OUVERT.swap(ouvert, Ordering::Relaxed) == ouvert {
        return;
    }
    // Une carte rangée ne garde rien de la main qui la lisait : rouverte,
    // elle montrerait une ligne allumée sous une souris posée ailleurs.
    *SURVOL.lock().expect("survol du menu") = None;
    *PRESSEE.lock().expect("appui du menu") = None;
    DEDANS.store(false, Ordering::Relaxed);
    let Some(app) = PROGRAM.lock().expect("programme du menu").clone() else {
        return;
    };
    // Un menu qu'on rouvre s'ouvre sur lui-même : rester dans une liste
    // choisie il y a deux sessions serait un menu qui a l'air d'un autre.
    *PANNEAU.lock().expect("panneau du menu") = None;
    *POUSSE.lock().expect("curseur du menu") = None;
    // Ce qui vit dans la carte ne vit que pendant qu'on la regarde. Les
    // interrupteurs et les réglages se relisent à chaque ouverture parce
    // qu'ils peuvent avoir bougé sans elle.
    suis_les_mesures(&app, ouvert);
    if ouvert {
        let asked = app.clone();
        tauri::async_runtime::spawn(async move { relis_les_bascules(&asked).await });
        relis_les_reglages(&app);
    }
    let _ = app.run_on_main_thread(move || {
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::UI::WindowsAndMessaging::{SW_HIDE, SW_SHOWNOACTIVATE, ShowWindow};

        let window = ITS_WINDOW.load(Ordering::Relaxed) as HWND;
        if window.is_null() {
            return;
        }
        if ouvert {
            repaint(window);
        }
        // SAFETY: une fenêtre à nous, montrée sans prendre le premier
        // plan.
        unsafe {
            ShowWindow(window, if ouvert { SW_SHOWNOACTIVATE } else { SW_HIDE });
        }
    });
}

/// Dit si la carte est ouverte, pour qui a besoin de la basculer.
pub fn ouvert() -> bool {
    OUVERT.load(Ordering::Relaxed)
}

/// Pose la carte sous le logo, ou au-dessus quand le menu s'ouvre vers le
/// haut.
///
/// La même ancre que le logo, dans le même geste : les deux fenêtres ne
/// peuvent donc pas être en désaccord sur l'endroit où se trouve le
/// bouton.
pub fn lay(anchor: (i32, i32), upward: bool, logo: i32) {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
    };

    let window = ITS_WINDOW.load(Ordering::Relaxed);
    if window == 0 {
        return;
    }
    let (large, haute) = (
        LARGE.load(Ordering::Relaxed) as i32,
        HAUTE.load(Ordering::Relaxed) as i32,
    );
    // La fenêtre est plus grande que la carte, de tout ce que l'ombre
    // déborde : c'est donc la **carte** qu'on pose, et la fenêtre autour
    // d'elle. Posée comme si les deux ne faisaient qu'une, la carte
    // tombait vingt pixels trop bas et vingt trop à gauche, ce qui se
    // voit au premier coup d'oeil à côté de l'ancien menu.
    let echelle = echelle();
    VERS_LE_HAUT.store(upward, Ordering::Relaxed);
    let debord = debord_de_l_ombre(echelle).round() as i32;
    let carte_haute = haute - debord * 2;
    // Collée au même bord droit que le logo, et séparée de lui de
    // l'espace que la feuille de style met entre les deux.
    let entre = (design::PAS_2 * echelle).round() as i32;
    let haut = if upward {
        anchor.1 - logo - entre - carte_haute
    } else {
        anchor.1 + logo + entre
    } - debord;
    // SAFETY: une fenêtre à nous, posée sans être activée ni
    // redimensionnée.
    unsafe {
        SetWindowPos(
            window as HWND,
            std::ptr::null_mut(),
            anchor.0 - large + debord,
            haut,
            0,
            0,
            SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER,
        )
    };
}

/// Bâtit la fenêtre, à la taille que ses lignes demandent.
fn build(owner: isize) {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CS_HREDRAW, CS_VREDRAW, CreateWindowExW, IDC_ARROW, LoadCursorW, RegisterClassW, WNDCLASSW,
        WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
    };

    /// Le nom de la classe, dans les caractères que Windows compte, fini
    /// par le zéro qu'il cherche.
    const CLASS: [u16; 13] = [
        b'Z' as u16,
        b'y' as u16,
        b'r' as u16,
        b'D' as u16,
        b'e' as u16,
        b's' as u16,
        b'k' as u16,
        b'M' as u16,
        b'e' as u16,
        b'n' as u16,
        b'u' as u16,
        0,
        0,
    ];

    // La taille se mesure avant que la fenêtre existe : elle dépend du
    // texte, et mesurer du texte demande de quoi le dessiner.
    let Some(mesure) = Toile::neuve(1, 1) else {
        note("bouton flottant : le menu n'a pas pu être mesuré");
        return;
    };
    let echelle = echelle();
    // La hauteur d'une ligne de texte, demandée à la police une fois pour
    // toutes : tout ce qui est empilé dans cette carte s'appuie dessus.
    range(
        &HAUTE_LEGENDE,
        mesure.haute(design::LEGENDE * echelle, false),
    );
    range(&HAUTE_CORPS, mesure.haute(design::CORPS * echelle, false));
    mesure_la_carte(&mesure, echelle);
    let (large, haute) = taille(&mesure);
    LARGE.store(large as u32, Ordering::Relaxed);
    HAUTE.store(haute as u32, Ordering::Relaxed);
    drop(mesure);

    // SAFETY: une classe déclarée une fois et une fenêtre bâtie dessus,
    // sur le fil qui pompera ses messages. Une classe déclarée deux fois
    // est refusée sans autre effet, d'où la réponse non lue : la deuxième
    // session retrouve celle de la première.
    let window = unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(answer),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: std::ptr::null_mut(),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: CLASS.as_ptr(),
        };
        RegisterClassW(&class);
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
            CLASS.as_ptr(),
            std::ptr::null(),
            WS_POPUP,
            0,
            0,
            large,
            haute,
            owner as HWND,
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        )
    };
    if window.is_null() {
        note("bouton flottant : la fenêtre du menu n'a pas pu s'ouvrir");
        return;
    }
    ITS_WINDOW.store(window as isize, Ordering::Relaxed);
    note(&format!(
        "bouton flottant : menu dessiné par ZyrDesk, {large}x{haute} px au \
         départ ; la fenêtre suit ensuite ce que la carte demande. Il ne \
         reste dans la vue web que la ligne rouge qui porte un refus, \
         lequel n'est donc dit ici que dans ce journal"
    ));
}

/// Ce que la carte prend, en vrais pixels.
///
/// Aussi large que sa ligne la plus longue, ce que la feuille de style
/// demande depuis toujours et qu'aucun nombre écrit à la main ne saurait
/// tenir : un libellé rallongé couperait son raccourci.
fn taille(toile: &Toile) -> (i32, i32) {
    let echelle = echelle();
    let debord = debord_de_l_ombre(echelle);
    let panneau = largeur_des_panneaux(toile, echelle);
    let large = largeur_de_la_carte(echelle)
        + if panneau > 0.0 {
            panneau + design::PAS_2 * echelle
        } else {
            0.0
        };
    let haute = contenu(echelle).max(hauteur_des_panneaux(echelle));
    (
        (large + debord * 2.0).ceil() as i32,
        (haute + debord * 2.0).ceil() as i32,
    )
}

/// Ce que la carte prend de large : sa ligne la plus longue.
///
/// Ce que la feuille de style demande depuis toujours et qu'aucun nombre
/// écrit à la main ne saurait tenir : un libellé rallongé couperait son
/// raccourci. Mesurée sur **toutes** ses lignes, y compris celles qui ne
/// se voient pas en ce moment : une carte qui rétrécit quand une ligne
/// s'en va est une carte qui change de largeur sous la main.
fn largeur_de_la_carte(echelle: f32) -> f32 {
    lue(&LARGE_CARTE).max(design::PAS_2 * 2.0 * echelle)
}

/// La même, mesurée. Rangée ensuite, parce que la mise en page la
/// redemande à chaque image et que mesurer du texte coûte.
fn mesure_la_carte(toile: &Toile, echelle: f32) {
    let reglages = REGLAGES.lock().expect("réglages du menu");
    let mut large: f32 = 0.0;
    for ligne in &LIGNES {
        large = large.max(match ligne {
            Ligne::Mesures => {
                (tenue::MESURE * 4.0 + tenue::ENTRE_MESURES * 3.0 + design::PAS_2 * 2.0) * echelle
            }
            Ligne::Separateur => 0.0,
            Ligne::Entree(entree) => {
                let droite = toile.largeur(&entree.droite.dit(), design::LEGENDE * echelle, false);
                autour(toile, entree.mot, droite, echelle)
            }
            Ligne::Bascule(bascule) => autour(
                toile,
                bascule.mot,
                cotes_larges(toile, &bascule.mots(), echelle),
                echelle,
            ),
            // Ses mots sont demandés avec les réglages déjà en main : les
            // redemander à la ligne reprendrait le verrou qu'on tient, ce
            // qui arrête le fil qui dessine pour de bon.
            Ligne::Choix(choix) => match reglages.as_ref() {
                Some(menu) => autour(
                    toile,
                    choix.mot,
                    cotes_larges(toile, &mots_de(menu, choix.quoi), echelle),
                    echelle,
                ),
                None => 0.0,
            },
            // Sa barre prend toute la largeur, donc elle n'en demande
            // aucune : c'est sa tête qui décide, comme pour les autres.
            Ligne::Curseur(curseur) => {
                let valeur = reglages
                    .as_ref()
                    .map_or_else(String::new, |menu| curseur.quoi.resume(menu));
                let droite = toile.largeur(&valeur, design::CORPS * echelle, false);
                autour(toile, curseur.mot, droite, echelle)
            }
            Ligne::Liste(liste) => {
                let valeur = reglages
                    .as_ref()
                    .map_or_else(String::new, |menu| liste.quoi.resume(menu));
                let droite = toile.largeur(&valeur, design::LEGENDE * echelle, false)
                    + (design::PAS_2 + tenue::MARQUE) * echelle;
                autour(toile, liste.mot, droite, echelle)
            }
            Ligne::Appliquer => {
                let droite = toile.largeur(APRES_APPLIQUER, design::LEGENDE * echelle, false);
                autour(toile, APPLIQUER, droite, echelle)
            }
        });
    }
    drop(reglages);
    range(&LARGE_CARTE, large);
}

/// Ce qu'une ligne prend de large : son icône, son mot, ce qui vient à
/// droite, et tout ce qui les entoure.
///
/// La même mesure pour toutes les sortes de lignes, parce que c'est la
/// même mise en page : ce qui change est ce qu'il y a à droite.
fn autour(toile: &Toile, mot: &str, droite: f32, echelle: f32) -> f32 {
    toile.largeur(mot, design::CORPS * echelle, false)
        + droite
        + (design::PAS_2 * 2.0 + tenue::ICONE + design::PAS_3 + tenue::APRES_LE_MOT) * echelle
}

/// Ce que les côtés d'une ligne à choix prennent de large, ensemble.
fn cotes_larges(toile: &Toile, mots: &[String], echelle: f32) -> f32 {
    mots.iter().map(|mot| cote_large(toile, mot, echelle)).sum()
}

/// Et ce qu'un seul côté prend : son mot et ce qui l'entoure.
fn cote_large(toile: &Toile, mot: &str, echelle: f32) -> f32 {
    toile.largeur(mot, design::LEGENDE * echelle, false) + design::PAS_3 * 2.0 * echelle
}

/// Où tombent les côtés d'une ligne à choix, poussés au bord droit et
/// collés les uns aux autres.
///
/// Ils forment un seul objet, avec une bordure autour de tous et rien
/// entre eux.
fn cotes_de(toile: &Toile, ou: Cadre, mots: &[String], echelle: f32) -> Vec<Cadre> {
    let larges: Vec<f32> = mots
        .iter()
        .map(|mot| cote_large(toile, mot, echelle))
        .collect();
    let haute = tenue::BASCULE * echelle;
    let haut = ou.haut + (ou.bas - ou.haut - haute) / 2.0;
    let mut gauche = ou.droite - design::PAS_2 * echelle - larges.iter().sum::<f32>();
    larges
        .iter()
        .map(|large| {
            let place = Cadre::pose(gauche, haut, *large, haute);
            gauche += large;
            place
        })
        .collect()
}

/// La barre d'un curseur, sous la tête de sa ligne.
fn barre_du_curseur(ou: Cadre, echelle: f32) -> Cadre {
    let bord = design::PAS_2 * echelle;
    let haut = ou.haut
        + bord
        + lue(&HAUTE_CORPS)
        + tenue::SOUS_LE_MOT * echelle
        + (tenue::CURSEUR - tenue::BARRE) * echelle / 2.0;
    Cadre::pose(
        ou.gauche + bord,
        haut,
        ou.droite - ou.gauche - bord * 2.0,
        tenue::BARRE * echelle,
    )
}

/// Les réglages qui ouvrent une liste, dans l'ordre de la carte.
///
/// Lus dans les lignes plutôt qu'écrits une seconde fois : ajouter une
/// liste au menu suffit alors à lui donner son panneau.
fn a_panneau() -> impl Iterator<Item = Reglage> {
    LIGNES.iter().filter_map(|ligne| match ligne {
        Ligne::Liste(liste) => Some(liste.quoi),
        _ => None,
    })
}

/// Ce que le plus large des panneaux prend, ou rien quand aucun n'a de
/// quoi s'ouvrir.
///
/// Le plus large et non celui qui est ouvert : la fenêtre ne peut pas
/// changer de largeur au moment où l'on ouvre une liste sans que le
/// dessin qu'elle porte change de place au même instant.
fn largeur_des_panneaux(toile: &Toile, echelle: f32) -> f32 {
    let reglages = REGLAGES.lock().expect("réglages du menu");
    let Some(menu) = reglages.as_ref() else {
        return 0.0;
    };
    a_panneau()
        .map(|quoi| largeur_du_panneau(toile, menu, quoi, echelle))
        .fold(0.0, f32::max)
}

/// Ce qu'un panneau prend de large : son titre ou sa plus longue valeur.
fn largeur_du_panneau(toile: &Toile, menu: &SessionMenu, quoi: Reglage, echelle: f32) -> f32 {
    let valeurs = quoi.valeurs(menu);
    if valeurs.is_empty() {
        return 0.0;
    }
    let titre = autour(toile, mot_du_panneau(quoi), 0.0, echelle);
    valeurs
        .iter()
        .map(|valeur| {
            let aparte =
                toile.largeur(&quoi.aparte(menu, valeur), design::LEGENDE * echelle, false);
            autour(toile, &quoi.dit(menu, valeur), aparte, echelle)
        })
        .fold(titre, f32::max)
}

/// La hauteur du plus haut des panneaux, pour la même raison.
fn hauteur_des_panneaux(echelle: f32) -> f32 {
    let reglages = REGLAGES.lock().expect("réglages du menu");
    let Some(menu) = reglages.as_ref() else {
        return 0.0;
    };
    a_panneau()
        .map(|quoi| hauteur_du_panneau(menu, quoi, echelle))
        .fold(0.0, f32::max)
}

/// Ce qu'un panneau prend de haut : son titre, son trait, et ses valeurs.
fn hauteur_du_panneau(menu: &SessionMenu, quoi: Reglage, echelle: f32) -> f32 {
    let combien = quoi.valeurs(menu).len();
    if combien == 0 {
        return 0.0;
    }
    (design::PAS_2 * 2.0
        + tenue::LIGNE
        + design::PAS_2 * 2.0
        + tenue::TRAIT
        + tenue::LIGNE * combien as f32)
        * echelle
}

/// Le mot qu'un panneau porte en tête, qui est celui de la ligne qui
/// l'ouvre.
fn mot_du_panneau(quoi: Reglage) -> &'static str {
    LIGNES
        .iter()
        .find_map(|ligne| match ligne {
            Ligne::Liste(liste) if liste.quoi == quoi => Some(liste.mot),
            _ => None,
        })
        .unwrap_or_default()
}

/// Le panneau ouvert dans sa fenêtre, à gauche de la carte.
fn panneau(toile: &Toile, quoi: Reglage, echelle: f32) -> Option<Cadre> {
    // La carte d'abord, et le verrou des réglages ensuite : la mesurer
    // demande ce même verrou, et un verrou repris pendant qu'on le tient
    // arrête le fil qui dessine pour de bon.
    let carte = carte(echelle);
    let reglages = REGLAGES.lock().expect("réglages du menu");
    let menu = reglages.as_ref()?;
    let haute = hauteur_du_panneau(menu, quoi, echelle);
    if haute <= 0.0 {
        return None;
    }
    let large = largeur_du_panneau(toile, menu, quoi, echelle);
    // Aligné par le bord d'où le menu s'ouvre : les deux cartes n'ont pas
    // la même hauteur, et c'est ce bord-là qui doit rester commun.
    let haut = if VERS_LE_HAUT.load(Ordering::Relaxed) {
        carte.bas - haute
    } else {
        carte.haut
    };
    Some(Cadre::pose(
        carte.gauche - design::PAS_2 * echelle - large,
        haut,
        large,
        haute,
    ))
}

/// Le titre du panneau ouvert et la place de chacune de ses valeurs.
fn parcours_du_panneau(toile: &Toile, quoi: Reglage, echelle: f32) -> (Cadre, Vec<Cadre>) {
    let Some(panneau) = panneau(toile, quoi, echelle) else {
        return (Cadre::pose(0.0, 0.0, 0.0, 0.0), Vec::new());
    };
    let bord = design::PAS_2 * echelle;
    let dedans = |haut: f32, haute: f32| {
        Cadre::pose(
            panneau.gauche + bord,
            haut,
            panneau.droite - panneau.gauche - bord * 2.0,
            haute,
        )
    };
    let titre = dedans(panneau.haut + bord, tenue::LIGNE * echelle);
    let mut haut = titre.bas + (design::PAS_2 * 2.0 + tenue::TRAIT) * echelle;
    let combien = REGLAGES
        .lock()
        .expect("réglages du menu")
        .as_ref()
        .map_or(0, |menu| quoi.valeurs(menu).len());
    let valeurs = (0..combien)
        .map(|_| {
            let place = dedans(haut, tenue::LIGNE * echelle);
            haut = place.bas;
            place
        })
        .collect();
    (titre, valeurs)
}

/// De combien l'ombre sort de la carte, de chaque côté.
fn debord_de_l_ombre(echelle: f32) -> f32 {
    let ombre = palette().ombre_2;
    (ombre.soft + ombre.down.abs().max(ombre.across.abs())) * echelle
}

/// La hauteur de la barre des mesures.
fn hauteur_des_mesures(echelle: f32) -> f32 {
    design::PAS_2 * echelle
        + lue(&HAUTE_LEGENDE)
        + tenue::SOUS_LE_MOT * echelle
        + lue(&HAUTE_CORPS)
        + design::PAS_1 * echelle
        + lue(&HAUTE_LEGENDE)
        + design::PAS_1 * echelle
}

/// La hauteur d'une ligne à curseur : sa tête, puis la barre en dessous.
fn hauteur_du_curseur(echelle: f32) -> f32 {
    (design::PAS_2 + tenue::SOUS_LE_MOT + tenue::CURSEUR + design::PAS_3) * echelle
        + lue(&HAUTE_CORPS)
}

/// La carte dans sa fenêtre.
///
/// Aussi haute que ce qu'elle montre, et pas plus. Des lignes vont et
/// viennent selon la session, et la fenêtre est bâtie une fois pour la
/// plus grande des cartes possibles : celle-ci est donc collée au bord
/// d'où le menu s'ouvre, qui est le seul que personne ne doit voir bouger.
fn carte(echelle: f32) -> Cadre {
    let (large, haute) = (
        LARGE.load(Ordering::Relaxed) as f32,
        HAUTE.load(Ordering::Relaxed) as f32,
    );
    let debord = debord_de_l_ombre(echelle);
    let dedans = haute - debord * 2.0;
    let montre = contenu(echelle).min(dedans);
    let haut = if VERS_LE_HAUT.load(Ordering::Relaxed) {
        debord + dedans - montre
    } else {
        debord
    };
    Cadre::pose(
        large - debord - largeur_de_la_carte(echelle),
        haut,
        largeur_de_la_carte(echelle),
        montre,
    )
}

/// La hauteur de ce que la carte montre en ce moment.
fn contenu(echelle: f32) -> f32 {
    let reglages = REGLAGES.lock().expect("réglages du menu");
    design::PAS_2 * echelle * 2.0
        + LIGNES
            .iter()
            .filter(|ligne| ligne.se_voit(reglages.as_ref()))
            .map(|ligne| ligne.haute(echelle))
            .sum::<f32>()
}

/// Chaque ligne visible et la place qu'elle prend, du haut de la carte
/// vers le bas.
///
/// Lue par le dessin et par la souris, une seule fois écrite : une carte
/// dont les lignes sont dessinées à un endroit et cliquées à un autre est
/// une carte qui rend le mauvais menu.
fn parcours(echelle: f32) -> Vec<(usize, &'static Ligne, Cadre)> {
    let carte = carte(echelle);
    let bord = design::PAS_2 * echelle;
    let reglages = REGLAGES.lock().expect("réglages du menu");
    let mut haut = carte.haut + bord;
    let mut pose = Vec::with_capacity(LIGNES.len());
    for (rang, ligne) in LIGNES.iter().enumerate() {
        if !ligne.se_voit(reglages.as_ref()) {
            continue;
        }
        let haute = ligne.haute(echelle);
        pose.push((
            rang,
            ligne,
            Cadre::pose(
                carte.gauche + bord,
                haut,
                carte.droite - carte.gauche - bord * 2.0,
                haute,
            ),
        ));
        haut += haute;
    }
    pose
}

/// Ce qui est sous ce point de la fenêtre, quand c'est quelque chose
/// qu'on clique.
///
/// Ce qui est en morceaux, les côtés d'un interrupteur et les valeurs
/// d'un panneau, demande de savoir où ils tombent, donc de quoi mesurer
/// du texte : la toile de la fenêtre, celle-là même sur laquelle ils ont
/// été dessinés. Une souris qui viserait d'après une autre mesure que le
/// dessin viserait à côté.
fn sous(ou: (i32, i32)) -> Option<Cible> {
    let (x, y) = (ou.0 as f32, ou.1 as f32);
    let echelle = echelle();
    let dedans =
        |place: &Cadre| x >= place.gauche && x < place.droite && y >= place.haut && y < place.bas;

    if let Some(quoi) = *PANNEAU.lock().expect("panneau du menu") {
        let dans_le_panneau = TOILE.with_borrow(|toile| {
            let toile = toile.as_ref()?;
            let (titre, valeurs) = parcours_du_panneau(toile, quoi, echelle);
            if dedans(&titre) {
                return Some(Cible::Retour);
            }
            valeurs.iter().position(dedans).map(Cible::Valeur)
        });
        if dans_le_panneau.is_some() {
            return dans_le_panneau;
        }
    }

    let (rang, ligne, place) = parcours(echelle)
        .into_iter()
        .find(|(_, _, place)| dedans(place))?;
    match ligne {
        Ligne::Entree(_) | Ligne::Appliquer | Ligne::Liste(_) => Some(Cible::Ligne(rang)),
        Ligne::Bascule(bascule) => TOILE.with_borrow(|toile| {
            cotes_de(toile.as_ref()?, place, &bascule.mots(), echelle)
                .iter()
                .position(dedans)
                .map(|cote| Cible::Cote(rang, cote))
        }),
        Ligne::Choix(choix) => TOILE.with_borrow(|toile| {
            let toile = toile.as_ref()?;
            cotes_de(toile, place, &choix.mots()?, echelle)
                .iter()
                .position(dedans)
                .map(|cote| Cible::Cote(rang, cote))
        }),
        Ligne::Curseur(_) => dedans(&barre_du_curseur(place, echelle).elargi(
            // La barre fait quatre pixels de haut : viser quatre pixels
            // avec une souris est un travail, et personne n'a demandé un
            // travail. Ce qu'on attrape est la hauteur du pouce.
            (tenue::POUCE - tenue::BARRE) * echelle / 2.0,
        ))
        .then_some(Cible::Barre(rang)),
        Ligne::Mesures | Ligne::Separateur => None,
    }
}

/// Dessine la carte et la remet à la fenêtre.
///
/// La fenêtre suit ce que la carte demande. Elle peut changer de taille
/// sans que rien ne clignote : l'image et la taille sont remises à Windows
/// dans le même geste, donc il n'existe pas d'instant où la fenêtre soit
/// grande sans être peinte. C'est ce qu'une vue web ne sait pas faire, et
/// c'est ce qui permet ici de mesurer la carte sur ce qu'elle contient
/// vraiment plutôt que sur ce qu'elle pourrait contenir un jour.
fn repaint(window: windows_sys::Win32::Foundation::HWND) {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect;

    let echelle = echelle();
    let couleurs = palette();
    let rayon = design::RAYON * echelle;
    let survol = *SURVOL.lock().expect("survol du menu");
    let ouvert = *PANNEAU.lock().expect("panneau du menu");

    TOILE.with_borrow_mut(|toile| {
        // Ce qu'il faut de place, mesuré sur la toile qui est là : mesurer
        // du texte ne demande pas la bonne taille de toile, seulement une
        // toile.
        if toile.is_none() {
            *toile = Toile::neuve(1, 1);
        }
        let Some(mesure) = toile.as_ref() else {
            return;
        };
        mesure_la_carte(mesure, echelle);
        let (large, haute) = taille(mesure);
        if large <= 0 || haute <= 0 {
            return;
        }
        LARGE.store(large as u32, Ordering::Relaxed);
        HAUTE.store(haute as u32, Ordering::Relaxed);
        // Refaite dès qu'elle n'est plus à la bonne taille, ce qui est
        // aussi le cas de celle d'un pixel qui vient de servir à mesurer.
        if mesure.taille() != (large, haute) {
            *toile = Toile::neuve(large, haute);
        }
        let Some(toile) = toile.as_ref() else {
            return;
        };

        let carte = carte(echelle);
        toile.commence();
        toile.ombre(carte, rayon, couleurs.ombre_2, echelle);
        toile.remplis(carte, rayon, couleurs.surface_1);
        toile.trace_dedans(carte, rayon, tenue::TRAIT * echelle, couleurs.trait_fort);

        let pinceau = Pinceau {
            toile,
            echelle,
            couleurs,
        };
        for (rang, ligne, ou) in parcours(echelle) {
            let sous_la_main = survol.filter(|cible| cible.ligne() == Some(rang));
            let cote = match sous_la_main {
                Some(Cible::Cote(_, cote)) => Some(cote),
                _ => None,
            };
            match ligne {
                Ligne::Mesures => pinceau.mesures(ou),
                Ligne::Separateur => pinceau.separateur(ou),
                Ligne::Entree(entree) => pinceau.entree(ou, entree, sous_la_main.is_some()),
                Ligne::Bascule(bascule) => pinceau.cotes(
                    ou,
                    &Cotes {
                        icone: bascule.icone,
                        mot: bascule.mot,
                        mots: &bascule.mots(),
                        en_place: bascule.en_place(),
                        barres: &[],
                    },
                    cote,
                ),
                Ligne::Choix(choix) => {
                    if let (Some(mots), Some((en_place, barres))) = (choix.mots(), choix.ou()) {
                        pinceau.cotes(
                            ou,
                            &Cotes {
                                icone: choix.icone,
                                mot: choix.mot,
                                mots: &mots,
                                en_place,
                                barres: &barres,
                            },
                            cote,
                        );
                    }
                }
                Ligne::Curseur(curseur) => pinceau.curseur(ou, curseur),
                Ligne::Liste(liste) => pinceau.liste(ou, liste, sous_la_main.is_some()),
                Ligne::Appliquer => pinceau.appliquer(ou, sous_la_main.is_some()),
            }
        }

        if let Some(quoi) = ouvert {
            pinceau.panneau(quoi, survol);
        }
        if !toile.finit() {
            return;
        }

        let mut place = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        // SAFETY: une fenêtre à nous, dont le rectangle est lu dans le
        // nôtre.
        if unsafe { GetWindowRect(window, &mut place) } == 0 {
            return;
        }
        // Accrochée par le bord droit, et par celui d'où le menu s'ouvre :
        // ce sont les deux seuls que personne ne doit voir bouger quand la
        // fenêtre change de taille. Ce sont aussi ceux que `lay` calcule,
        // donc les deux tombent d'accord d'eux-mêmes.
        let x = place.right - large;
        let y = if VERS_LE_HAUT.load(Ordering::Relaxed) {
            place.bottom - haute
        } else {
            place.top
        };
        toile.pose(window as isize, x, y);
    });
}

/// Ce qui ne change pas pendant qu'une carte se dessine : de quoi
/// dessiner, de combien un pixel de page compte, et le thème.
///
/// Porté ensemble plutôt que passé trois fois à chaque ligne, et le menu
/// en a maintenant sept sortes.
struct Pinceau<'a> {
    toile: &'a Toile,
    echelle: f32,
    couleurs: Palette,
}

/// Ce qu'une ligne à côtés montre : sa tête, ses mots, celui qui est en
/// place, et ceux que la machine d'en face ne sait pas faire.
///
/// Porté ensemble parce que ça se dessine ensemble, et qu'un interrupteur
/// et une ligne à boutons ne s'en décrivent pas autrement.
struct Cotes<'a> {
    icone: &'a Icone,
    mot: &'a str,
    mots: &'a [String],
    en_place: usize,
    barres: &'a [bool],
}

impl Pinceau<'_> {
    /// Le début d'une ligne, qui est le même pour toutes : son icône à sa
    /// place, et son mot après.
    fn tete(&self, ou: Cadre, icone: &Icone, mot: &str, encre: Couleur) {
        let (toile, echelle) = (self.toile, self.echelle);
        let cote = tenue::ICONE * echelle;
        toile.icone(
            icone,
            Cadre::pose(
                ou.gauche + design::PAS_2 * echelle,
                ou.haut + (ou.bas - ou.haut - cote) / 2.0,
                cote,
                cote,
            ),
            encre,
        );
        toile.ecris(
            mot,
            design::CORPS * echelle,
            false,
            encre,
            Cadre {
                gauche: ou.gauche + (design::PAS_2 + tenue::ICONE + design::PAS_3) * echelle,
                ..ou
            },
            Cale::Gauche,
        );
    }

    /// Le fond qu'une ligne prend sous la main.
    fn survol(&self, ou: Cadre, teinte: Option<Couleur>) {
        if let Some(teinte) = teinte {
            self.toile
                .remplis(ou, design::RAYON_PETIT * self.echelle, teinte);
        }
    }

    /// Ce qui s'écrit à droite d'une ligne, dans la couleur des choses
    /// qu'on lit sans les chercher.
    fn a_droite(&self, ou: Cadre, mot: &str, taille: f32, encre: Couleur) {
        if mot.is_empty() {
            return;
        }
        self.toile.ecris(
            mot,
            taille,
            false,
            encre,
            Cadre {
                droite: ou.droite - design::PAS_2 * self.echelle,
                ..ou
            },
            Cale::Droite,
        );
    }

    /// Une entrée : son icône, son mot, ce qui est écrit à sa droite, et
    /// le fond que le survol lui met.
    fn entree(&self, ou: Cadre, entree: &Entree, sous_la_main: bool) {
        let couleurs = self.couleurs;
        let encre = if entree.grave {
            couleurs.erreur
        } else {
            couleurs.texte
        };
        // La ligne qui coupe la session s'allume de sa propre couleur
        // plutôt que du gris des autres : ce n'est pas un survol de plus,
        // c'est celui dont il faut se méfier.
        self.survol(
            ou,
            sous_la_main.then(|| {
                if entree.grave {
                    couleurs.erreur.voile(VOILE)
                } else {
                    couleurs.surface_3
                }
            }),
        );
        self.tete(ou, entree.icone, entree.mot, encre);
        self.a_droite(
            ou,
            &entree.droite.dit(),
            design::LEGENDE * self.echelle,
            couleurs.texte_faible,
        );
    }

    /// La ligne qui relance l'image avec ce qui vient d'être choisi.
    fn appliquer(&self, ou: Cadre, sous_la_main: bool) {
        let couleurs = self.couleurs;
        self.survol(ou, sous_la_main.then(|| couleurs.accent_vif.voile(VOILE)));
        self.tete(ou, &icones::APPLIQUER, APPLIQUER, couleurs.accent_vif);
        self.a_droite(
            ou,
            APRES_APPLIQUER,
            design::LEGENDE * self.echelle,
            couleurs.texte_faible,
        );
    }

    /// Une ligne qui ouvre une liste : sa valeur en place, puis le chevron
    /// qui dit qu'elle mène ailleurs.
    fn liste(&self, ou: Cadre, liste: &Liste, sous_la_main: bool) {
        let (toile, echelle, couleurs) = (self.toile, self.echelle, self.couleurs);
        self.survol(ou, sous_la_main.then_some(couleurs.surface_3));
        self.tete(ou, liste.icone, liste.mot, couleurs.texte);

        let marque = tenue::MARQUE * echelle;
        let bord = design::PAS_2 * echelle;
        let ouverte = *PANNEAU.lock().expect("panneau du menu") == Some(liste.quoi);
        toile.icone(
            // Le chevron dit dans quel sens la liste s'ouvre, donc il se
            // retourne quand elle est ouverte : elle paraît à gauche, il
            // pointe vers elle.
            if ouverte {
                &icones::RETOUR
            } else {
                &icones::CHEVRON
            },
            Cadre::pose(
                ou.droite - bord - marque,
                ou.haut + (ou.bas - ou.haut - marque) / 2.0,
                marque,
                marque,
            ),
            couleurs.texte_faible,
        );
        let valeur = REGLAGES
            .lock()
            .expect("réglages du menu")
            .as_ref()
            .map_or_else(String::new, |menu| liste.quoi.resume(menu));
        self.a_droite(
            Cadre {
                droite: ou.droite - marque - bord,
                ..ou
            },
            &valeur,
            design::LEGENDE * echelle,
            couleurs.texte_faible,
        );
    }

    /// Une ligne à côtés : un interrupteur ou une suite de boutons, dont
    /// un seul est plein.
    ///
    /// Les deux se dessinent ici parce qu'ils se dessinent pareil. Ce qui
    /// les sépare est ce qu'ils font, pas ce qu'ils montrent : l'un
    /// bascule la session tout de suite, l'autre range un choix que le
    /// moteur ne lira qu'à son prochain démarrage.
    fn cotes(&self, ou: Cadre, quoi: &Cotes, sous_la_main: Option<usize>) {
        let (toile, echelle, couleurs) = (self.toile, self.echelle, self.couleurs);
        let mots = quoi.mots;
        self.tete(ou, quoi.icone, quoi.mot, couleurs.texte);

        let cotes = cotes_de(toile, ou, mots, echelle);
        let Some(entier) = cotes.first().map(|premier| Cadre {
            gauche: premier.gauche,
            ..*cotes.last().unwrap_or(premier)
        }) else {
            return;
        };
        let rayon = design::RAYON_PETIT * echelle;
        for (rang, place) in cotes.iter().enumerate() {
            let barre = quoi.barres.get(rang).copied().unwrap_or(false);
            let (fond, encre) = if rang == quoi.en_place {
                (Some(couleurs.accent_vif), couleurs.sur_accent)
            } else if barre {
                (None, couleurs.texte_faible)
            } else if sous_la_main == Some(rang) {
                (Some(couleurs.surface_3), couleurs.texte)
            } else {
                (None, couleurs.texte_faible)
            };
            if let Some(fond) = fond {
                // Le fond de l'objet entier, vu au travers de ce côté-là :
                // les côtés n'en forment qu'un, arrondi par dehors et droit
                // là où ils se touchent, ce qu'aucun rectangle arrondi ne
                // sait être à lui seul.
                toile.serre(*place, || toile.remplis(entier, rayon, fond));
            }
            toile.ecris(
                &mots[rang],
                design::LEGENDE * echelle,
                false,
                encre,
                *place,
                Cale::Centre,
            );
            if barre {
                // Ce que la machine d'en face ne sait pas faire garde sa
                // place : une possibilité qui disparaît d'un ordinateur à
                // l'autre laisse croire à un menu qui change d'avis, là où
                // c'est la machine regardée qui n'a pas la même carte
                // graphique. Barré, donc, et non effacé.
                let milieu = (place.haut + place.bas) / 2.0;
                let mi_mot = toile.largeur(&mots[rang], design::LEGENDE * echelle, false) / 2.0;
                let au_centre = (place.gauche + place.droite) / 2.0;
                toile.remplis(
                    Cadre::pose(
                        au_centre - mi_mot,
                        milieu,
                        mi_mot * 2.0,
                        tenue::TRAIT * echelle,
                    ),
                    0.0,
                    couleurs.texte_faible,
                );
            }
        }
        toile.trace_dedans(entier, rayon, tenue::TRAIT * echelle, couleurs.r#trait);
    }

    /// Une ligne à curseur : sa tête, sa valeur, et la barre en dessous.
    fn curseur(&self, ou: Cadre, curseur: &Curseur) {
        let (toile, echelle, couleurs) = (self.toile, self.echelle, self.couleurs);
        // Sa tête tient sur la hauteur d'une ligne de corps, la barre
        // prenant le reste.
        let tete = Cadre {
            bas: ou.haut + design::PAS_2 * echelle * 2.0 + lue(&HAUTE_CORPS),
            ..ou
        };
        self.tete(tete, curseur.icone, curseur.mot, couleurs.texte);
        // La valeur d'un réglage se lit là où se lisent les raccourcis,
        // mais elle n'en est pas un : c'est ce que la ligne vaut, donc elle
        // se lit comme le reste de la ligne et non en retrait.
        self.a_droite(
            tete,
            &curseur.valeur(),
            design::CORPS * echelle,
            couleurs.texte,
        );

        let Some((cran, combien)) = curseur.cran() else {
            return;
        };
        let barre = barre_du_curseur(ou, echelle);
        let rayon = tenue::BARRE * echelle / 2.0;
        toile.remplis(barre, rayon, couleurs.r#trait);
        let part = if combien > 1 {
            cran as f32 / (combien - 1) as f32
        } else {
            0.0
        };
        let pouce = tenue::POUCE * echelle;
        // Le pouce reste entier dans la barre à ses deux bouts : posé sur
        // sa seule part, il déborderait de la moitié de lui-même.
        let au = barre.gauche + pouce / 2.0 + (barre.droite - barre.gauche - pouce) * part;
        let milieu = (barre.haut + barre.bas) / 2.0;
        toile.remplis(
            Cadre::pose(barre.gauche, barre.haut, au - barre.gauche, rayon * 2.0),
            rayon,
            couleurs.accent_vif,
        );
        toile.remplis(
            Cadre::pose(au - pouce / 2.0, milieu - pouce / 2.0, pouce, pouce),
            pouce / 2.0,
            couleurs.accent_vif,
        );
    }

    /// Le trait entre deux groupes, au milieu de la place qu'il prend.
    ///
    /// Rentré d'un pas de chaque côté, comme la feuille de style le
    /// demande : un trait qui va d'un bord à l'autre coupe la carte en
    /// deux au lieu de séparer deux groupes de lignes.
    fn separateur(&self, ou: Cadre) {
        let bord = design::PAS_2 * self.echelle;
        self.toile.remplis(
            Cadre::pose(
                ou.gauche + bord,
                ou.haut + bord,
                ou.droite - ou.gauche - bord * 2.0,
                tenue::TRAIT * self.echelle,
            ),
            0.0,
            self.couleurs.r#trait,
        );
    }

    /// La barre des quatre mesures : un mot par-dessus un nombre, quatre
    /// fois, et la phrase du flux en dessous.
    fn mesures(&self, ou: Cadre) {
        let (toile, echelle, couleurs) = (self.toile, self.echelle, self.couleurs);
        let bord = design::PAS_2 * echelle;
        let haut = ou.haut + bord;
        let barre = BARRE.lock().expect("mesures du menu");
        for (rang, quoi) in MESURES.iter().enumerate() {
            let gauche =
                ou.gauche + bord + rang as f32 * (tenue::MESURE + tenue::ENTRE_MESURES) * echelle;
            let colonne = tenue::MESURE * echelle;
            toile.ecris(
                quoi.mot,
                design::LEGENDE * echelle,
                false,
                couleurs.texte_faible,
                Cadre::pose(gauche, haut, colonne, lue(&HAUTE_LEGENDE)),
                Cale::Gauche,
            );
            toile.ecris(
                &barre.chiffres[rang],
                design::CORPS * echelle,
                false,
                couleurs.texte,
                Cadre::pose(
                    gauche,
                    haut + lue(&HAUTE_LEGENDE) + tenue::SOUS_LE_MOT * echelle,
                    colonne,
                    lue(&HAUTE_CORPS),
                ),
                Cale::Gauche,
            );
        }
        if !barre.flux.is_empty() {
            toile.ecris(
                &barre.flux,
                design::LEGENDE * echelle,
                false,
                couleurs.texte_faible,
                Cadre::pose(
                    ou.gauche + bord,
                    haut + lue(&HAUTE_LEGENDE)
                        + tenue::SOUS_LE_MOT * echelle
                        + lue(&HAUTE_CORPS)
                        + design::PAS_1 * echelle,
                    ou.droite - ou.gauche - bord * 2.0,
                    lue(&HAUTE_LEGENDE),
                ),
                Cale::Gauche,
            );
        }
    }

    /// Le panneau d'un réglage, à gauche de la carte : son titre, un
    /// trait, et ses valeurs dont une porte la marque.
    fn panneau(&self, quoi: Reglage, survol: Option<Cible>) {
        let (toile, echelle, couleurs) = (self.toile, self.echelle, self.couleurs);
        let Some(place) = panneau(toile, quoi, echelle) else {
            return;
        };
        let rayon = design::RAYON * echelle;
        toile.ombre(place, rayon, couleurs.ombre_2, echelle);
        toile.remplis(place, rayon, couleurs.surface_1);
        toile.trace_dedans(place, rayon, tenue::TRAIT * echelle, couleurs.trait_fort);

        let (titre, valeurs) = parcours_du_panneau(toile, quoi, echelle);
        self.survol(
            titre,
            (survol == Some(Cible::Retour)).then_some(couleurs.surface_3),
        );
        let cote = tenue::MARQUE * echelle;
        toile.icone(
            &icones::RETOUR,
            Cadre::pose(
                titre.gauche + design::PAS_2 * echelle,
                titre.haut + (titre.bas - titre.haut - cote) / 2.0,
                cote,
                cote,
            ),
            couleurs.texte,
        );
        toile.ecris(
            mot_du_panneau(quoi),
            design::CORPS * echelle,
            true,
            couleurs.texte,
            Cadre {
                gauche: titre.gauche + (design::PAS_2 + tenue::ICONE + design::PAS_3) * echelle,
                ..titre
            },
            Cale::Gauche,
        );
        self.separateur(Cadre {
            haut: titre.bas,
            bas: titre.bas + (design::PAS_2 * 2.0 + tenue::TRAIT) * echelle,
            ..titre
        });

        let reglages = REGLAGES.lock().expect("réglages du menu");
        let Some(menu) = reglages.as_ref() else {
            return;
        };
        let choisies = quoi.valeurs(menu);
        let ou = quoi.ou(menu);
        for (rang, place) in valeurs.iter().enumerate() {
            let Some(valeur) = choisies.get(rang) else {
                break;
            };
            self.survol(
                *place,
                (survol == Some(Cible::Valeur(rang))).then_some(couleurs.surface_3),
            );
            if *valeur == ou {
                toile.icone(
                    &icones::COCHE,
                    Cadre::pose(
                        place.gauche + design::PAS_2 * echelle,
                        place.haut + (place.bas - place.haut - cote) / 2.0,
                        cote,
                        cote,
                    ),
                    couleurs.accent_vif,
                );
            }
            toile.ecris(
                &quoi.dit(menu, valeur),
                design::CORPS * echelle,
                false,
                couleurs.texte,
                Cadre {
                    gauche: place.gauche + (design::PAS_2 + tenue::ICONE + design::PAS_3) * echelle,
                    ..*place
                },
                Cale::Gauche,
            );
            self.a_droite(
                *place,
                &quoi.aparte(menu, valeur),
                design::LEGENDE * echelle,
                couleurs.texte_faible,
            );
        }
    }
}

/// Ce que la fenêtre répond quand le système lui parle.
///
/// SAFETY: appelée par le système sur le fil qui a fait cette fenêtre,
/// avec les arguments qu'il documente.
unsafe extern "system" fn answer(
    window: windows_sys::Win32::Foundation::HWND,
    message: u32,
    holding: windows_sys::Win32::Foundation::WPARAM,
    with: windows_sys::Win32::Foundation::LPARAM,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::UI::Controls::WM_MOUSELEAVE;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DefWindowProcW, HTCLIENT, IDC_ARROW, IDC_HAND, LoadCursorW, SetCursor, WM_LBUTTONDOWN,
        WM_LBUTTONUP, WM_MOUSEMOVE, WM_SETCURSOR,
    };

    match message {
        WM_MOUSEMOVE => {
            if !DEDANS.swap(true, Ordering::Relaxed) {
                // Demandé dès qu'une main arrive : sans ça rien ne dit
                // jamais qu'elle est repartie, et la dernière ligne
                // survolée resterait allumée sous une souris qui n'est
                // plus là.
                let mut veille = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: window,
                    dwHoverTime: 0,
                };
                // SAFETY: une fenêtre à nous, et la demande est à nous.
                unsafe { TrackMouseEvent(&mut veille) };
            }
            if pousse(window, point(with)) {
                return 0;
            }
            survole(window, sous(point(with)));
            0
        }
        WM_MOUSELEAVE => {
            DEDANS.store(false, Ordering::Relaxed);
            survole(window, None);
            0
        }
        WM_SETCURSOR if (with as u32 & 0xFFFF) == HTCLIENT => {
            // La ligne est demandée au système plutôt que reprise du
            // dernier survol : le curseur se décide avant que le
            // mouvement soit annoncé, et la main serait alors en retard
            // d'un geste.
            let forme = if sous_la_souris(window).is_some() {
                IDC_HAND
            } else {
                IDC_ARROW
            };
            // SAFETY: un curseur du système, demandé par son nom.
            unsafe { SetCursor(LoadCursorW(std::ptr::null_mut(), forme)) };
            1
        }
        WM_LBUTTONDOWN => {
            let cible = sous(point(with));
            *PRESSEE.lock().expect("appui du menu") = cible;
            // Un curseur se prend et se pousse : le geste commence ici et
            // ne finit qu'au relâchement, où seul le cran d'arrivée est
            // écrit.
            if matches!(cible, Some(Cible::Barre(_))) {
                pousse(window, point(with));
            }
            0
        }
        // Au relâchement, et là où l'appui a commencé : c'est ce qu'un
        // clic veut dire, et c'est ce qui laisse repartir d'un bouton
        // qu'on n'aurait pas dû viser.
        WM_LBUTTONUP => {
            let pressee = PRESSEE.lock().expect("appui du menu").take();
            if let Some(Cible::Barre(rang)) = pressee {
                lache(window, rang);
                return 0;
            }
            if let Some(cible) = sous(point(with))
                && Some(cible) == pressee
            {
                agit(cible);
            }
            0
        }
        // SAFETY: la réponse du système à tout ce à quoi on ne répond pas
        // ici.
        _ => unsafe { DefWindowProcW(window, message, holding, with) },
    }
}

/// Où la souris est dans la fenêtre, tel que le système l'écrit dans un
/// message : deux nombres signés dans les deux moitiés d'un seul.
fn point(with: windows_sys::Win32::Foundation::LPARAM) -> (i32, i32) {
    (
        i32::from((with & 0xFFFF) as i16),
        i32::from(((with >> 16) & 0xFFFF) as i16),
    )
}

/// Ce qui est sous le pointeur, demandé au système.
fn sous_la_souris(window: windows_sys::Win32::Foundation::HWND) -> Option<Cible> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

    let mut ou = POINT { x: 0, y: 0 };
    // SAFETY: un point à nous, et une fenêtre à nous dans laquelle il est
    // ramené.
    let lu = unsafe { GetCursorPos(&mut ou) != 0 && ScreenToClient(window, &mut ou) != 0 };
    if !lu {
        return None;
    }
    sous((ou.x, ou.y))
}

/// Allume ce qui est sous la souris, et redessine quand ce n'est plus la
/// même chose.
fn survole(window: windows_sys::Win32::Foundation::HWND, cible: Option<Cible>) {
    let mut survol = SURVOL.lock().expect("survol du menu");
    if *survol == cible {
        return;
    }
    *survol = cible;
    drop(survol);
    repaint(window);
}

/// Fait ce que ce qui vient d'être cliqué demande.
///
/// Un refus ne va qu'au journal tant que le menu de la vue web est encore
/// là : c'est lui qui porte la ligne rouge qui le dit, et en dessiner une
/// deuxième ici ferait deux endroits à tenir pour la même phrase.
///
/// Dit avant de partir, et pas seulement quand ça refuse. Ce menu est
/// derrière l'image et ses lignes sont rares : sans cette ligne, une
/// entrée qui semble ne rien faire ne se distingue pas d'un clic qui n'est
/// jamais arrivé, et les deux se réparent ailleurs.
fn agit(cible: Cible) {
    let Some(app) = PROGRAM.lock().expect("programme du menu").clone() else {
        return;
    };
    match (cible, cible.ligne().and_then(|rang| LIGNES.get(rang))) {
        (Cible::Ligne(_), Some(Ligne::Entree(entree))) => {
            dit_le_clic(entree.mot);
            let fait = entree.fait;
            // Refermée avant que ce soit parti, comme la page le fait : ce
            // qui suit prend le temps qu'il prend, et une carte laissée
            // ouverte par-dessus serait une nappe posée sur l'image.
            montre(false);
            tauri::async_runtime::spawn(async move {
                let refus = match fait {
                    Fait::Session(acte) => crate::floating::ask(&app, acte).await,
                    Fait::Ranger => crate::floating::hide(&app),
                };
                dit_le_refus(refus);
            });
        }
        (Cible::Ligne(_), Some(Ligne::Appliquer)) => {
            dit_le_clic(APPLIQUER);
            // Refermée avant de partir : l'image s'en va et revient, ce
            // qui prend des secondes et met un écran de chargement à sa
            // place, et un menu resté ouvert serait une nappe posée dessus.
            montre(false);
            tauri::async_runtime::spawn(async move {
                dit_le_refus(crate::session::apply_session(app).await);
            });
        }
        (Cible::Ligne(_), Some(Ligne::Liste(liste))) => {
            // La même ligne ouvre et referme : une liste ouverte à côté du
            // menu se referme là où on l'a ouverte, et pas seulement par
            // son titre.
            let mut panneau = PANNEAU.lock().expect("panneau du menu");
            *panneau = (*panneau != Some(liste.quoi)).then_some(liste.quoi);
            drop(panneau);
            redessine(&app);
        }
        (Cible::Cote(_, cote), Some(Ligne::Bascule(bascule))) => {
            // Pousser un interrupteur du côté où il est déjà ne fait rien,
            // comme tout interrupteur.
            if bascule.en_place() == cote {
                return;
            }
            note(&format!(
                "menu du bouton flottant : « {} » mis sur « {} »",
                bascule.mot, bascule.cotes[cote]
            ));
            // La carte reste ouverte : on regarde l'image après avoir
            // basculé, et la rouvrir pour la ligne d'à côté ferait deux
            // gestes pour un réglage.
            let passe = bascule.passe;
            tauri::async_runtime::spawn(async move {
                match crate::floating::ask(&app, passe).await {
                    // Relu plutôt que supposé : c'est la seule façon de
                    // montrer où l'on en est vraiment, et le son se lit
                    // dans le mélangeur de Windows et non ici.
                    Ok(()) => relis_les_bascules(&app).await,
                    Err(refus) => dit_le_refus(Err(refus)),
                }
            });
        }
        (Cible::Cote(_, cote), Some(Ligne::Choix(choix))) => {
            let Some(valeur) = valeur_de(choix.quoi, cote) else {
                return;
            };
            // Ce que la machine d'en face ne sait pas faire n'est pas un
            // choix : le proposer barré dit pourquoi, le laisser cliquer
            // dirait le contraire.
            let refuse = REGLAGES
                .lock()
                .expect("réglages du menu")
                .as_ref()
                .is_some_and(|menu| choix.quoi.hors_de_portee(menu, &valeur));
            if refuse {
                return;
            }
            choisis(&app, choix.quoi, valeur);
        }
        (Cible::Retour, _) => {
            *PANNEAU.lock().expect("panneau du menu") = None;
            redessine(&app);
        }
        (Cible::Valeur(rang), _) => {
            let Some(quoi) = *PANNEAU.lock().expect("panneau du menu") else {
                return;
            };
            let Some(valeur) = valeur_de(quoi, rang) else {
                return;
            };
            // La liste se referme sur le choix : rester dedans après avoir
            // choisi laisserait croire qu'il reste quelque chose à y faire.
            *PANNEAU.lock().expect("panneau du menu") = None;
            choisis(&app, quoi, valeur);
        }
        _ => {}
    }
}

/// La valeur d'un réglage à ce rang-là.
fn valeur_de(quoi: Reglage, rang: usize) -> Option<String> {
    REGLAGES
        .lock()
        .expect("réglages du menu")
        .as_ref()
        .and_then(|menu| quoi.valeurs(menu).get(rang).cloned())
}

/// Écrit ce choix, et relit ce que la session en dit.
///
/// Relu et non supposé : choisir une taille change ce que « client » vaut,
/// et choisir quoi que ce soit peut faire apparaître la ligne qui relance
/// l'image. La réponse porte les deux.
fn choisis(app: &AppHandle, quoi: Reglage, valeur: String) {
    note(&format!(
        "menu du bouton flottant : {} mis sur « {valeur} »",
        quoi.nom()
    ));
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match crate::settings::choose_session(app.clone(), quoi.nom().to_string(), valeur).await {
            Ok(choix) => {
                if let Some(menu) = REGLAGES.lock().expect("réglages du menu").as_mut() {
                    menu.now = choix;
                }
                redessine(&app);
            }
            Err(refus) => dit_le_refus(Err(refus)),
        }
    });
}

/// Dit qu'une ligne a été cliquée.
///
/// Dit avant que ce soit parti, et pas seulement quand ça refuse. Ce menu
/// est derrière l'image et ses lignes sont rares : sans cette ligne, une
/// entrée qui semble ne rien faire ne se distingue pas d'un clic qui n'est
/// jamais arrivé, et les deux se réparent ailleurs.
fn dit_le_clic(mot: &str) {
    note(&format!("menu du bouton flottant : « {mot} » cliqué"));
}

/// Et dit un refus, s'il y en a un.
///
/// Un refus ne va qu'au journal tant que le menu de la vue web est encore
/// là : c'est lui qui porte la ligne rouge qui le dit, et en dessiner une
/// deuxième ici ferait deux endroits à tenir pour la même phrase.
fn dit_le_refus(refus: Result<(), String>) {
    if let Err(refus) = refus {
        note(&format!("menu du bouton flottant : {refus}"));
    }
}

/// Pousse le curseur là où la main est, et dit si elle en tenait un.
///
/// Rien n'est écrit tant qu'elle le tient : un curseur poussé d'un bout à
/// l'autre traverse quinze crans, et chacun serait un aller-retour jusqu'au
/// service pour un débit que personne n'a voulu.
fn pousse(window: windows_sys::Win32::Foundation::HWND, ou: (i32, i32)) -> bool {
    let Some(Cible::Barre(rang)) = *PRESSEE.lock().expect("appui du menu") else {
        return false;
    };
    let Some(Ligne::Curseur(curseur)) = LIGNES.get(rang) else {
        return false;
    };
    let Some((_, combien)) = curseur.cran() else {
        return false;
    };
    let echelle = echelle();
    let Some((_, _, place)) = parcours(echelle)
        .into_iter()
        .find(|(autre, _, _)| *autre == rang)
    else {
        return false;
    };
    let barre = barre_du_curseur(place, echelle);
    let pouce = tenue::POUCE * echelle;
    // Le pouce ne va pas d'un bord à l'autre mais d'un centre à l'autre :
    // compté sur la barre entière, les deux crans du bout ne se
    // laisseraient pas atteindre.
    let course = (barre.droite - barre.gauche - pouce).max(1.0);
    let part = ((ou.0 as f32 - barre.gauche - pouce / 2.0) / course).clamp(0.0, 1.0);
    let cran = (part * (combien.max(1) - 1) as f32).round() as usize;
    let mut tenu = POUSSE.lock().expect("curseur du menu");
    if *tenu != Some(cran) {
        *tenu = Some(cran);
        drop(tenu);
        repaint(window);
    }
    true
}

/// Lâche le curseur, et écrit le cran où il a été laissé.
fn lache(window: windows_sys::Win32::Foundation::HWND, rang: usize) {
    let Some(cran) = POUSSE.lock().expect("curseur du menu").take() else {
        return;
    };
    repaint(window);
    let Some(Ligne::Curseur(curseur)) = LIGNES.get(rang) else {
        return;
    };
    let Some(valeur) = valeur_de(curseur.quoi, cran) else {
        return;
    };
    let deja = REGLAGES
        .lock()
        .expect("réglages du menu")
        .as_ref()
        .is_some_and(|menu| curseur.quoi.ou(menu) == valeur);
    if deja {
        return;
    }
    let Some(app) = PROGRAM.lock().expect("programme du menu").clone() else {
        return;
    };
    choisis(&app, curseur.quoi, valeur);
}

/// Relit où en sont les trois interrupteurs, et redessine si ça a bougé.
///
/// Les deux premiers sont ce que ce programme croit, parce que c'est lui
/// qui les bascule et que le moteur ne dit jamais où il en est ; le son se
/// demande au mélangeur de Windows, qui le sait et qui est ouvert à tout
/// le monde.
async fn relis_les_bascules(app: &AppHandle) {
    /// Pose où en est un interrupteur, et dit si ça a bougé.
    fn pose(ou: &AtomicBool, vrai: bool) -> bool {
        ou.swap(vrai, Ordering::Relaxed) != vrai
    }

    let mut change = pose(&EN_JEU, crate::floating::in_game_mouse(app));
    change |= pose(&IMMERSIF, crate::floating::keys_to_the_session(app));
    // Sans session le mélangeur n'a rien à dire, et la carte ne s'ouvre
    // pas sans session : un refus se laisse donc tel quel plutôt que
    // d'éteindre l'interrupteur.
    if let Ok(coupe) = crate::floating::hushed(app).await {
        change |= pose(&COUPE, coupe);
    }
    if change {
        redessine(app);
    }
}

/// Suit ce que la session coûte tant que la carte est ouverte, et pas une
/// seconde de plus : des chiffres que personne ne regarde ne valent ni le
/// fichier ni le réveil.
fn suis_les_mesures(app: &AppHandle, ouvert: bool) {
    // Le tour change à chaque appel, ce qui arrête celui d'avant : sans
    // ça, ouvrir et refermer vite laisserait deux veilles derrière la
    // même carte.
    let tour = TOUR.fetch_add(1, Ordering::Relaxed) + 1;
    if !ouvert {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        while TOUR.load(Ordering::Relaxed) == tour {
            let lue = Barre::de(&crate::mesures::session_measures());
            // Le verrou est rendu avant l'attente : un verrou tenu à
            // travers une attente est un verrou tenu une seconde.
            let change = {
                let mut barre = BARRE.lock().expect("mesures du menu");
                let change = *barre != lue;
                *barre = lue;
                change
            };
            if change {
                redessine(&app);
            }
            tokio::time::sleep(RYTHME).await;
        }
    });
}

/// Redessine la carte depuis un fil qui n'est pas celui qui la dessine.
fn redessine(app: &AppHandle) {
    let _ = app.run_on_main_thread(|| {
        use windows_sys::Win32::Foundation::HWND;

        let window = ITS_WINDOW.load(Ordering::Relaxed) as HWND;
        if !window.is_null() {
            repaint(window);
        }
    });
}
