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

/// Ce que la carte contient, dans l'ordre.
///
/// Les mêmes lignes que la page, dans le même ordre, avec les mêmes mots,
/// les mêmes icônes et les mêmes actions. Ce qui manque encore est dit
/// dans le journal à l'ouverture plutôt que remplacé par du vide qui
/// ressemblerait à un défaut.
const LIGNES: [Ligne; 12] = [
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
}

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

/// Ce qu'on peut cliquer dans la carte.
#[derive(Clone, Copy, PartialEq)]
enum Cible {
    /// Une entrée, par son rang dans `LIGNES`.
    Entree(usize),
    /// Un côté d'un interrupteur : le rang de sa ligne, et lequel des
    /// deux côtés.
    Cote(usize, usize),
}

impl Cible {
    /// La ligne dont il s'agit, quelle que soit la sorte.
    fn ligne(self) -> usize {
        match self {
            Cible::Entree(rang) | Cible::Cote(rang, _) => rang,
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

/// De combien un pixel de page vaut de vrais pixels, en centièmes : un
/// nombre à virgule ne se range pas dans un entier partagé, et le
/// centième suffit à un écran agrandi de cent soixante-quinze pour cent.
static ECHELLE: AtomicU32 = AtomicU32::new(100);

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

fn echelle() -> f32 {
    ECHELLE.load(Ordering::Relaxed) as f32 / 100.0
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
            Ligne::Entree(_) | Ligne::Bascule(_) => tenue::LIGNE * echelle,
        }
    }
}

impl Bascule {
    /// Les deux côtés, chacun à sa place, dans le cadre de sa ligne.
    ///
    /// Poussés au bord droit, l'un contre l'autre : ils forment un seul
    /// objet, avec une bordure autour des deux et rien entre eux.
    fn cotes(&self, toile: &Toile, ou: Cadre, echelle: f32) -> [Cadre; 2] {
        let larges = self.larges(toile, echelle);
        let haute = tenue::BASCULE * echelle;
        let haut = ou.haut + (ou.bas - ou.haut - haute) / 2.0;
        let gauche = ou.droite - design::PAS_2 * echelle - larges[0] - larges[1];
        [
            Cadre::pose(gauche, haut, larges[0], haute),
            Cadre::pose(gauche + larges[0], haut, larges[1], haute),
        ]
    }

    /// Ce que chaque côté prend de large : son mot et ce qui l'entoure.
    fn larges(&self, toile: &Toile, echelle: f32) -> [f32; 2] {
        self.cotes.map(|mot| {
            toile.largeur(mot, design::LEGENDE * echelle, false) + design::PAS_3 * 2.0 * echelle
        })
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
    ECHELLE.store((echelle * 100.0).round() as u32, Ordering::Relaxed);
    CLAIR.store(clair, Ordering::Relaxed);
    OUVERT.store(false, Ordering::Relaxed);
    let _ = app.run_on_main_thread(move || build(owner));
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
    // Ce qui vit dans la carte ne vit que pendant qu'on la regarde. Les
    // interrupteurs se relisent à chaque ouverture parce qu'ils peuvent
    // avoir bougé sans elle.
    suis_les_mesures(&app, ouvert);
    if ouvert {
        let asked = app.clone();
        tauri::async_runtime::spawn(async move { relis_les_bascules(&asked).await });
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
        "bouton flottant : menu dessiné par ZyrDesk, {large}x{haute} px ; \
         les lignes à interrupteur, le curseur du débit et les deux \
         sous-menus sont encore dans la vue web, et un refus n'est dit \
         que dans ce journal"
    ));
}

/// Ce que la carte prend, en vrais pixels.
///
/// Aussi large que sa ligne la plus longue, ce que la feuille de style
/// demande depuis toujours et qu'aucun nombre écrit à la main ne saurait
/// tenir : un libellé rallongé couperait son raccourci.
fn taille(toile: &Toile) -> (i32, i32) {
    let echelle = echelle();
    let bord = design::PAS_2 * echelle;
    let mut large: f32 = 0.0;
    let mut haute = bord * 2.0;

    for ligne in &LIGNES {
        haute += ligne.haute(echelle);
        match ligne {
            Ligne::Mesures => {
                large = large.max(
                    (tenue::MESURE * 4.0 + tenue::ENTRE_MESURES * 3.0 + design::PAS_2 * 2.0)
                        * echelle,
                );
            }
            Ligne::Separateur => {}
            Ligne::Entree(entree) => {
                let droite = toile.largeur(&entree.droite.dit(), design::LEGENDE * echelle, false);
                large = large.max(autour(toile, entree.mot, droite, echelle));
            }
            Ligne::Bascule(bascule) => {
                let cotes: f32 = bascule.larges(toile, echelle).iter().sum();
                large = large.max(autour(toile, bascule.mot, cotes, echelle));
            }
        }
    }
    // L'ombre déborde de la carte, donc la fenêtre est plus grande
    // qu'elle : ce qui sort d'une fenêtre n'est dessiné nulle part.
    let debord = debord_de_l_ombre(echelle);
    (
        (large + debord * 2.0).ceil() as i32,
        (haute + debord * 2.0).ceil() as i32,
    )
}

/// Ce qu'une ligne prend de large : son icône, son mot, ce qui vient à
/// droite, et tout ce qui les entoure.
///
/// La même mesure pour une entrée et pour un interrupteur, parce que
/// c'est la même mise en page : ce qui change est ce qu'il y a à droite.
fn autour(toile: &Toile, mot: &str, droite: f32, echelle: f32) -> f32 {
    toile.largeur(mot, design::CORPS * echelle, false)
        + droite
        + (design::PAS_2 * 2.0 + tenue::ICONE + design::PAS_3 + tenue::APRES_LE_MOT) * echelle
}

/// De combien l'ombre sort de la carte, de chaque côté.
fn debord_de_l_ombre(echelle: f32) -> f32 {
    let ombre = palette().ombre_2;
    (ombre.soft + ombre.down.abs().max(ombre.across.abs())) * echelle
}

/// La hauteur de la barre des mesures.
fn hauteur_des_mesures(echelle: f32) -> f32 {
    (design::PAS_2
        + design::LEGENDE
        + tenue::SOUS_LE_MOT
        + design::CORPS
        + design::PAS_1
        + design::LEGENDE
        + design::PAS_1)
        * echelle
}

/// La carte dans sa fenêtre, qui est plus grande qu'elle de tout ce que
/// l'ombre déborde.
fn carte(echelle: f32) -> Cadre {
    let (large, haute) = (
        LARGE.load(Ordering::Relaxed) as f32,
        HAUTE.load(Ordering::Relaxed) as f32,
    );
    let debord = debord_de_l_ombre(echelle);
    Cadre::pose(debord, debord, large - debord * 2.0, haute - debord * 2.0)
}

/// Chaque ligne et la place qu'elle prend, du haut de la carte vers le
/// bas.
///
/// Lue par le dessin et par la souris, une seule fois écrite : une carte
/// dont les lignes sont dessinées à un endroit et cliquées à un autre est
/// une carte qui rend le mauvais menu.
fn parcours(echelle: f32) -> impl Iterator<Item = (&'static Ligne, Cadre)> {
    let carte = carte(echelle);
    let bord = design::PAS_2 * echelle;
    LIGNES.iter().scan(carte.haut + bord, move |haut, ligne| {
        let haute = ligne.haute(echelle);
        let ou = Cadre::pose(
            carte.gauche + bord,
            *haut,
            carte.droite - carte.gauche - bord * 2.0,
            haute,
        );
        *haut += haute;
        Some((ligne, ou))
    })
}

/// Ce qui est sous ce point de la fenêtre, quand c'est quelque chose
/// qu'on clique.
///
/// Les côtés d'un interrupteur demandent de savoir où ils tombent, donc
/// de quoi mesurer du texte : la toile de la fenêtre, celle-là même sur
/// laquelle ils ont été dessinés. Une souris qui viserait d'après une
/// autre mesure que le dessin viserait à côté.
fn sous(ou: (i32, i32)) -> Option<Cible> {
    let (x, y) = (ou.0 as f32, ou.1 as f32);
    let echelle = echelle();
    let dedans =
        |place: &Cadre| x >= place.gauche && x < place.droite && y >= place.haut && y < place.bas;
    let (rang, ligne, place) = parcours(echelle)
        .enumerate()
        .find(|(_, (_, place))| dedans(place))
        .map(|(rang, (ligne, place))| (rang, ligne, place))?;
    match ligne {
        Ligne::Entree(_) => Some(Cible::Entree(rang)),
        Ligne::Bascule(bascule) => TOILE.with_borrow(|toile| {
            let cotes = bascule.cotes(toile.as_ref()?, place, echelle);
            cotes
                .iter()
                .position(dedans)
                .map(|cote| Cible::Cote(rang, cote))
        }),
        Ligne::Mesures | Ligne::Separateur => None,
    }
}

/// Dessine la carte et la remet à la fenêtre.
fn repaint(window: windows_sys::Win32::Foundation::HWND) {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect;

    let (large, haute) = (
        LARGE.load(Ordering::Relaxed) as i32,
        HAUTE.load(Ordering::Relaxed) as i32,
    );
    if large <= 0 || haute <= 0 {
        return;
    }
    let echelle = echelle();
    let couleurs = palette();
    let carte = carte(echelle);
    let rayon = design::RAYON * echelle;
    let survol = *SURVOL.lock().expect("survol du menu");

    TOILE.with_borrow_mut(|toile| {
        if toile.is_none() {
            *toile = Toile::neuve(large, haute);
        }
        let Some(toile) = toile.as_ref() else {
            return;
        };
        toile.commence();
        toile.ombre(carte, rayon, couleurs.ombre_2, echelle);
        toile.remplis(carte, rayon, couleurs.surface_1);
        toile.trace_dedans(carte, rayon, tenue::TRAIT * echelle, couleurs.trait_fort);

        let pinceau = Pinceau {
            toile,
            echelle,
            couleurs,
        };
        for (rang, (ligne, ou)) in parcours(echelle).enumerate() {
            let sous_la_main = survol.filter(|cible| cible.ligne() == rang);
            match ligne {
                Ligne::Mesures => pinceau.mesures(ou),
                Ligne::Separateur => pinceau.separateur(ou),
                Ligne::Entree(entree) => pinceau.entree(ou, entree, sous_la_main.is_some()),
                Ligne::Bascule(bascule) => pinceau.bascule(
                    ou,
                    bascule,
                    match sous_la_main {
                        Some(Cible::Cote(_, cote)) => Some(cote),
                        _ => None,
                    },
                ),
            }
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
        toile.pose(window as isize, place.left, place.top);
    });
}

/// Ce qui ne change pas pendant qu'une carte se dessine : de quoi
/// dessiner, de combien un pixel de page compte, et le thème.
///
/// Porté ensemble plutôt que passé trois fois à chaque ligne. Le menu
/// n'a pour l'instant que trois sortes de lignes ; il en aura les
/// interrupteurs, le curseur et les listes, et chacune voudrait les
/// mêmes trois choses.
struct Pinceau<'a> {
    toile: &'a Toile,
    echelle: f32,
    couleurs: Palette,
}

impl Pinceau<'_> {
    /// Une entrée : son icône, son mot, ce qui est écrit à sa droite, et
    /// le fond que le survol lui met.
    fn entree(&self, ou: Cadre, entree: &Entree, sous_la_main: bool) {
        let (toile, echelle, couleurs) = (self.toile, self.echelle, self.couleurs);
        let bord = design::PAS_2 * echelle;
        let encre = if entree.grave {
            couleurs.erreur
        } else {
            couleurs.texte
        };
        if sous_la_main {
            // La ligne qui coupe la session s'allume de sa propre couleur
            // plutôt que du gris des autres : ce n'est pas un survol de
            // plus, c'est celui dont il faut se méfier.
            let fond = if entree.grave {
                couleurs.erreur.voile(VOILE)
            } else {
                couleurs.surface_3
            };
            toile.remplis(ou, design::RAYON_PETIT * echelle, fond);
        }
        self.tete(ou, entree.icone, entree.mot, encre);
        let droite = entree.droite.dit();
        if !droite.is_empty() {
            toile.ecris(
                &droite,
                design::LEGENDE * echelle,
                false,
                couleurs.texte_faible,
                Cadre {
                    droite: ou.droite - bord,
                    ..ou
                },
                Cale::Droite,
            );
        }
    }

    /// Une ligne à interrupteur : son icône, son mot, et ses deux côtés
    /// dont un seul est plein.
    fn bascule(&self, ou: Cadre, bascule: &Bascule, sous_la_main: Option<usize>) {
        let (toile, echelle, couleurs) = (self.toile, self.echelle, self.couleurs);
        self.tete(ou, bascule.icone, bascule.mot, couleurs.texte);

        let a_droite = bascule.ou.load(Ordering::Relaxed);
        let cotes = bascule.cotes(toile, ou, echelle);
        let entier = Cadre {
            gauche: cotes[0].gauche,
            ..cotes[1]
        };
        let rayon = design::RAYON_PETIT * echelle;
        for (rang, place) in cotes.iter().enumerate() {
            let en_place = (rang == 1) == a_droite;
            let (fond, encre) = if en_place {
                (Some(couleurs.accent_vif), couleurs.sur_accent)
            } else if sous_la_main == Some(rang) {
                (Some(couleurs.surface_3), couleurs.texte)
            } else {
                (None, couleurs.texte_faible)
            };
            if let Some(fond) = fond {
                // Le fond de l'interrupteur entier, vu au travers de ce
                // côté-là : les deux n'en forment qu'un, arrondi par
                // dehors et droit là où ils se touchent, ce qu'aucun
                // rectangle arrondi ne sait être à lui seul.
                toile.serre(*place, || toile.remplis(entier, rayon, fond));
            }
            toile.ecris(
                bascule.cotes[rang],
                design::LEGENDE * echelle,
                false,
                encre,
                *place,
                Cale::Centre,
            );
        }
        toile.trace_dedans(entier, rayon, tenue::TRAIT * echelle, couleurs.r#trait);
    }

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
                Cadre::pose(gauche, haut, colonne, design::LEGENDE * echelle),
                Cale::Gauche,
            );
            toile.ecris(
                &barre.chiffres[rang],
                design::CORPS * echelle,
                false,
                couleurs.texte,
                Cadre::pose(
                    gauche,
                    haut + (design::LEGENDE + tenue::SOUS_LE_MOT) * echelle,
                    colonne,
                    design::CORPS * echelle,
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
                    haut + (design::LEGENDE + tenue::SOUS_LE_MOT + design::CORPS + design::PAS_1)
                        * echelle,
                    ou.droite - ou.gauche - bord * 2.0,
                    design::LEGENDE * echelle,
                ),
                Cale::Gauche,
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
            *PRESSEE.lock().expect("appui du menu") = sous(point(with));
            0
        }
        // Au relâchement, et là où l'appui a commencé : c'est ce qu'un
        // clic veut dire, et c'est ce qui laisse repartir d'un bouton
        // qu'on n'aurait pas dû viser.
        WM_LBUTTONUP => {
            let pressee = PRESSEE.lock().expect("appui du menu").take();
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
    match (cible, LIGNES.get(cible.ligne())) {
        (Cible::Entree(_), Some(Ligne::Entree(entree))) => {
            note(&format!(
                "menu du bouton flottant : « {} » cliqué",
                entree.mot
            ));
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
                if let Err(refus) = refus {
                    note(&format!("menu du bouton flottant : {refus}"));
                }
            });
        }
        (Cible::Cote(_, cote), Some(Ligne::Bascule(bascule))) => {
            // Pousser un interrupteur du côté où il est déjà ne fait rien,
            // comme tout interrupteur.
            if bascule.ou.load(Ordering::Relaxed) == (cote == 1) {
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
                    Err(refus) => note(&format!("menu du bouton flottant : {refus}")),
                }
            });
        }
        _ => {}
    }
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
