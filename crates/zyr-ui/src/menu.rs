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

use crate::design::{self, Palette};
use crate::floating::Act;
use crate::journal::note;
use crate::paint::{Cadre, Icone, Toile};
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
const LIGNES: [Ligne; 9] = [
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
}

/// Les quatre mesures, dans l'ordre où elles se lisent.
///
/// Les mêmes mots que la page, parce que ce sont les mêmes mesures : les
/// inventer ici en donnerait quatre autres, et deux barres qui ne disent
/// pas la même chose sur le même moteur.
///
/// Les mots seulement : les nombres viennent de ce que le moteur écrit
/// une fois par seconde, et tant qu'ils ne sont pas branchés la barre
/// montre un tiret, ce que la page montre elle aussi pour une mesure
/// manquante.
const MESURES: [&str; 4] = ["Décodage", "Encodage", "Réseau", "Débit"];

/// Ce qu'une mesure montre tant qu'elle n'a rien à dire.
const RIEN: &str = "-";

/// La ligne grise sous les chiffres, qui dit de quoi l'image est faite.
/// Vide tant que le moteur n'a rien dit, comme dans la page.
const FLUX: &str = "";

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

/// Ce qu'on désigne quand on ne désigne aucune ligne.
const HORS: i32 = -1;

/// La ligne sous la souris, celle sur laquelle un clic a commencé, et si
/// la souris est dans cette fenêtre.
///
/// Les trois sont lus et écrits par la réponse de la fenêtre, que le
/// système appelle, et par le dessin : des entiers partagés plutôt qu'un
/// verrou, comme partout où le système nous appelle.
static SURVOL: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(HORS);
static PRESSEE: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(HORS);
static DEDANS: AtomicBool = AtomicBool::new(false);

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
            Ligne::Entree(_) => tenue::LIGNE * echelle,
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
    SURVOL.store(HORS, Ordering::Relaxed);
    PRESSEE.store(HORS, Ordering::Relaxed);
    DEDANS.store(false, Ordering::Relaxed);
    let Some(app) = PROGRAM.lock().expect("programme du menu").clone() else {
        return;
    };
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
                let mots = toile.largeur(entree.mot, design::CORPS * echelle, false)
                    + toile.largeur(&entree.droite.dit(), design::LEGENDE * echelle, false);
                large = large.max(
                    mots + (design::PAS_2 * 2.0
                        + tenue::ICONE
                        + design::PAS_3
                        + tenue::APRES_LE_MOT)
                        * echelle,
                );
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

/// La ligne sous ce point de la fenêtre, quand c'en est une qu'on clique.
fn sous(ou: (i32, i32)) -> Option<usize> {
    let (x, y) = (ou.0 as f32, ou.1 as f32);
    parcours(echelle()).position(|(ligne, place)| {
        matches!(ligne, Ligne::Entree(_))
            && x >= place.gauche
            && x < place.droite
            && y >= place.haut
            && y < place.bas
    })
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
    let survol = SURVOL.load(Ordering::Relaxed);

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
            match ligne {
                Ligne::Mesures => pinceau.mesures(ou),
                Ligne::Separateur => pinceau.separateur(ou),
                Ligne::Entree(entree) => pinceau.entree(ou, entree, rang as i32 == survol),
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
        let cote = tenue::ICONE * echelle;
        toile.icone(
            entree.icone,
            Cadre::pose(
                ou.gauche + bord,
                ou.haut + (ou.bas - ou.haut - cote) / 2.0,
                cote,
                cote,
            ),
            encre,
        );
        toile.ecris(
            entree.mot,
            design::CORPS * echelle,
            false,
            encre,
            Cadre {
                gauche: ou.gauche + (design::PAS_2 + tenue::ICONE + design::PAS_3) * echelle,
                ..ou
            },
            false,
        );
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
                true,
            );
        }
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
        for (rang, mot) in MESURES.iter().enumerate() {
            let gauche =
                ou.gauche + bord + rang as f32 * (tenue::MESURE + tenue::ENTRE_MESURES) * echelle;
            let colonne = tenue::MESURE * echelle;
            toile.ecris(
                mot,
                design::LEGENDE * echelle,
                false,
                couleurs.texte_faible,
                Cadre::pose(gauche, haut, colonne, design::LEGENDE * echelle),
                false,
            );
            toile.ecris(
                // Un tiret tant que le moteur n'a rien dit, le même que celui
                // que la page montre pour une mesure qui manque.
                RIEN,
                design::CORPS * echelle,
                false,
                couleurs.texte,
                Cadre::pose(
                    gauche,
                    haut + (design::LEGENDE + tenue::SOUS_LE_MOT) * echelle,
                    colonne,
                    design::CORPS * echelle,
                ),
                false,
            );
        }
        if !FLUX.is_empty() {
            toile.ecris(
                FLUX,
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
                false,
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
            PRESSEE.store(
                sous(point(with)).map_or(HORS, |rang| rang as i32),
                Ordering::Relaxed,
            );
            0
        }
        // Au relâchement, et sur la ligne où l'appui a commencé : c'est
        // ce qu'un clic veut dire, et c'est ce qui laisse repartir d'un
        // bouton qu'on n'aurait pas dû viser.
        WM_LBUTTONUP => {
            let pressee = PRESSEE.swap(HORS, Ordering::Relaxed);
            if let Some(rang) = sous(point(with))
                && rang as i32 == pressee
            {
                agit(rang);
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

/// La ligne sous le pointeur, demandée au système.
fn sous_la_souris(window: windows_sys::Win32::Foundation::HWND) -> Option<usize> {
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

/// Allume cette ligne-là, et redessine quand ce n'est plus la même.
fn survole(window: windows_sys::Win32::Foundation::HWND, rang: Option<usize>) {
    let rang = rang.map_or(HORS, |rang| rang as i32);
    if SURVOL.swap(rang, Ordering::Relaxed) != rang {
        repaint(window);
    }
}

/// Fait ce que cette ligne demande, et referme la carte.
///
/// Refermée avant que ce soit parti, comme la page le fait : ce qui suit
/// prend le temps qu'il prend, et une carte laissée ouverte par-dessus
/// serait une nappe posée sur l'image.
///
/// Un refus ne va qu'au journal tant que le menu de la vue web est encore
/// là : c'est lui qui porte la ligne rouge qui le dit, et en dessiner une
/// deuxième ici ferait deux endroits à tenir pour la même phrase.
fn agit(rang: usize) {
    let Some(Ligne::Entree(entree)) = LIGNES.get(rang) else {
        return;
    };
    let Some(app) = PROGRAM.lock().expect("programme du menu").clone() else {
        return;
    };
    let fait = entree.fait;
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
