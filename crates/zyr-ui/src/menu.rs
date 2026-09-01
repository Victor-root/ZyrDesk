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
use crate::journal::note;
use crate::paint::{Cadre, Toile};

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
    /// Une ligne qu'on clique : un mot, et à droite ce qui la déclenche
    /// ou ce qu'elle vaut.
    Acte {
        mot: &'static str,
        droite: &'static str,
    },
}

/// Ce que la carte contient, dans l'ordre.
///
/// Les mêmes lignes que la page, dans le même ordre, avec les mêmes mots.
/// Ce qui manque encore est dit dans le journal à l'ouverture plutôt que
/// remplacé par du vide qui ressemblerait à un défaut.
const LIGNES: [Ligne; 9] = [
    Ligne::Mesures,
    Ligne::Separateur,
    Ligne::Acte {
        mot: "Fenêtré ou plein écran",
        droite: "",
    },
    Ligne::Acte {
        mot: "Statistiques",
        droite: "Ctrl+Alt+Maj+S",
    },
    Ligne::Acte {
        mot: "Ctrl+Alt+Suppr",
        droite: "sur l'ordinateur distant",
    },
    Ligne::Acte {
        mot: "Verrouiller",
        droite: "l'ordinateur distant",
    },
    Ligne::Separateur,
    Ligne::Acte {
        mot: "Masquer ce bouton",
        droite: "",
    },
    Ligne::Acte {
        mot: "Terminer la session",
        droite: "",
    },
];

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

/// Les quatre mesures, dans l'ordre où la barre les montre.
///
/// Les mots seulement : les nombres viennent de ce que le moteur écrit
/// une fois par seconde, et tant qu'ils ne sont pas branchés la barre
/// montre un tiret, ce qui est ce que la page montre elle aussi pour une
/// mesure manquante.
const MESURES: [&str; 4] = ["Latence", "Réseau", "Débit", "Images"];

/// Ce qu'une mesure montre tant qu'elle n'a rien à dire.
const RIEN: &str = "-";

/// La fenêtre de la carte, et ce qu'elle sait d'elle-même.
static ITS_WINDOW: AtomicIsize = AtomicIsize::new(0);
static LARGE: AtomicU32 = AtomicU32::new(0);
static HAUTE: AtomicU32 = AtomicU32::new(0);
static OUVERT: AtomicBool = AtomicBool::new(false);
static CLAIR: AtomicBool = AtomicBool::new(false);

/// De combien un pixel de page vaut de vrais pixels, en centièmes : un
/// nombre à virgule ne se range pas dans un entier partagé, et le
/// centième suffit à un écran agrandi de cent soixante-quinze pour cent.
static ECHELLE: AtomicU32 = AtomicU32::new(100);

/// Le programme, pour les endroits que le système appelle et à qui la
/// boîte à outils ne donne rien.
static PROGRAM: Mutex<Option<AppHandle>> = Mutex::new(None);

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
    // Collée au même bord droit que le logo, et séparée de lui de
    // l'espace que la feuille de style met entre les deux.
    let entre = (design::PAS_2 * echelle()).round() as i32;
    let haut = if upward {
        anchor.1 - logo - entre - haute
    } else {
        anchor.1 + logo + entre
    };
    // SAFETY: une fenêtre à nous, posée sans être activée ni
    // redimensionnée.
    unsafe {
        SetWindowPos(
            window as HWND,
            std::ptr::null_mut(),
            anchor.0 - large,
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
         sous-menus sont encore dans la vue web"
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
        match ligne {
            Ligne::Mesures => {
                large = large.max(
                    (tenue::MESURE * 4.0 + tenue::ENTRE_MESURES * 3.0 + design::PAS_2 * 2.0)
                        * echelle,
                );
                haute += hauteur_des_mesures(echelle);
            }
            Ligne::Separateur => haute += (design::PAS_2 * 2.0 + tenue::TRAIT) * echelle,
            Ligne::Acte { mot, droite } => {
                let mots = toile.largeur(mot, design::CORPS * echelle, false)
                    + toile.largeur(droite, design::LEGENDE * echelle, false);
                large = large.max(
                    mots + (design::PAS_2 * 2.0
                        + tenue::ICONE
                        + design::PAS_3
                        + tenue::APRES_LE_MOT)
                        * echelle,
                );
                haute += tenue::LIGNE * echelle;
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
    let debord = debord_de_l_ombre(echelle);
    let carte = Cadre::pose(
        debord,
        debord,
        large as f32 - debord * 2.0,
        haute as f32 - debord * 2.0,
    );
    let rayon = design::RAYON * echelle;
    let bord = design::PAS_2 * echelle;

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

        let mut y = carte.haut + bord;
        for ligne in &LIGNES {
            match ligne {
                Ligne::Mesures => {
                    mesures(toile, carte, y, echelle, couleurs);
                    y += hauteur_des_mesures(echelle);
                }
                Ligne::Separateur => {
                    let milieu = y + design::PAS_2 * echelle;
                    toile.remplis(
                        Cadre::pose(
                            carte.gauche + bord,
                            milieu,
                            carte.droite - carte.gauche - bord * 2.0,
                            tenue::TRAIT * echelle,
                        ),
                        0.0,
                        couleurs.r#trait,
                    );
                    y += (design::PAS_2 * 2.0 + tenue::TRAIT) * echelle;
                }
                Ligne::Acte { mot, droite } => {
                    acte(toile, carte, y, echelle, couleurs, mot, droite);
                    y += tenue::LIGNE * echelle;
                }
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

/// Une ligne qu'on clique : la place de son icône, son mot, et ce qui est
/// écrit à sa droite.
fn acte(
    toile: &Toile,
    carte: Cadre,
    y: f32,
    echelle: f32,
    couleurs: Palette,
    mot: &str,
    droite: &str,
) {
    let bord = design::PAS_2 * echelle;
    let dedans = Cadre::pose(
        carte.gauche + bord,
        y,
        carte.droite - carte.gauche - bord * 2.0,
        tenue::LIGNE * echelle,
    );
    // La place de l'icône est tenue, l'icône elle-même viendra : un mot
    // qui glisserait de dix-huit pixels le jour où elle arrive serait une
    // mise en page à refaire deux fois.
    let apres_icone = dedans.gauche + (design::PAS_2 + tenue::ICONE + design::PAS_3) * echelle;
    toile.ecris(
        mot,
        design::CORPS * echelle,
        false,
        couleurs.texte,
        Cadre {
            gauche: apres_icone,
            ..dedans
        },
        false,
    );
    if !droite.is_empty() {
        toile.ecris(
            droite,
            design::LEGENDE * echelle,
            false,
            couleurs.texte_faible,
            Cadre {
                droite: dedans.droite - design::PAS_2 * echelle,
                ..dedans
            },
            true,
        );
    }
}

/// La barre des quatre mesures : un mot par-dessus un nombre, quatre
/// fois, et la phrase du flux en dessous.
fn mesures(toile: &Toile, carte: Cadre, y: f32, echelle: f32, couleurs: Palette) {
    let bord = design::PAS_2 * echelle;
    let haut = y + bord;
    for (rang, mot) in MESURES.iter().enumerate() {
        let gauche = carte.gauche
            + bord * 2.0
            + rang as f32 * (tenue::MESURE + tenue::ENTRE_MESURES) * echelle;
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
    // SAFETY: la réponse du système à tout ce à quoi on ne répond pas
    // encore ici.
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::DefWindowProcW(window, message, holding, with)
    }
}
