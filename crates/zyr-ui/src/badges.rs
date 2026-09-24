//! Les deux voyants d'une session, dans le coin haut gauche de l'image.
//!
//! Deux petites pastilles posées par-dessus l'image, à l'opposé du bouton
//! flottant, et qui ne s'allument que quand il y a quelque chose à dire :
//! l'une pour le lien, quand l'image se fige ou que des images se perdent
//! en route, l'autre pour l'image elle-même, quand l'un des deux
//! ordinateurs ne suit plus à l'encodage ou au décodage.
//!
//! La seconde dit lequel des deux. Elle porte les deux écrans du logo du
//! produit, celui d'en face derrière et celui-ci devant, tous deux en
//! sourdine, et rallume celui qui coince ; les deux quand les deux
//! coincent. Ça se lit sans légende puisque c'est le dessin de la
//! marque, et ça tient en dix-huit pixels là où un mot n'y tiendrait pas.
//!
//! What they replace. The client engine has a warning of its own for the
//! first of the two: red letters at thirty-six points, burnt into the
//! frames it decodes, in a colour and a size it chose. Two things are
//! wrong with it and neither is a matter of taste. It is drawn into the
//! picture, which is the far computer's desktop and not ours to write on;
//! and it arrives late, several seconds after the person has watched the
//! picture stop, because it is worked out over a window of its own on the
//! far side of a decoder that has nothing to decode. The engine's switch
//! for it is thrown at the session's start (patch P-M16) and this says it
//! instead.
//!
//! Ce qui le rend vif. Le moteur écrit ce qu'une session coûte, et depuis
//! peu il l'écrit cinq fois par seconde plutôt qu'une, avec un nombre de
//! plus : depuis combien de temps l'image ne bouge plus. Celui-là ne se
//! moyenne pas et ne s'attend pas, il est vrai à l'instant où il est lu,
//! et c'est lui qui allume le premier voyant avant que la main n'ait eu
//! le temps de bouger la souris pour vérifier.
//!
//! Ce qui décide n'est pas de Windows et se compile partout : c'est de
//! l'arithmétique sur une lecture, et c'est la seule moitié dont un essai
//! puisse dire quoi que ce soit. Les pastilles, elles, sont une fenêtre,
//! donc de Windows, comme la session.

// Hors de Windows il n'y a pas d'image à couvrir, mais ce qui décide est
// compilé et éprouvé partout.
#![cfg_attr(not(windows), allow(dead_code))]

use std::time::{Duration, Instant};

use crate::measures::Measures;

/* ---- Ce qu'une lecture dit ------------------------------------------- */

/// Ce sous quoi ce module classe ses lignes du journal.
const TAG: &str = "voyants";

/// Écrit une ligne sous l'étiquette de ce module.
fn note(what: &str) {
    crate::journal::note_about(TAG, what);
}

/// Depuis combien de temps l'image doit être figée pour que ça se voie.
///
/// Un tiers de seconde. En dessous, c'est une image en retard comme il en
/// passe, et un voyant qui clignote à ce rythme-là ne veut plus rien
/// dire ; au-dessus, la personne a déjà remarqué et le voyant arrive
/// après elle.
const FROZEN_MS: u64 = 350;

/// Combien d'images perdues en route, en pour cent de la seconde écoulée,
/// avant que ça se dise.
///
/// Deux pour cent : une image sur cinquante, ce qui se voit sur un bureau
/// qu'on fait défiler et ne se voit pas sur un bureau immobile.
const LOST_PCT: f64 = 2.0;

/// Et combien arrivées trop tard pour être montrées.
///
/// Plus haut que les précédentes : celles-ci sont bien arrivées, et ce
/// qu'elles disent est que le lien tremble plutôt qu'il ne perd.
const TOO_LATE_PCT: f64 = 5.0;

/// Combien de temps un voyant reste allumé après que sa cause a cessé.
///
/// Sans ça il clignote : la cause tient sur une lecture, les lectures
/// arrivent cinq fois par seconde, et un réseau qui va mal va mal par
/// à-coups. Une seconde et demie est ce qu'il faut pour qu'une main qui
/// lève les yeux vers le coin de l'image y trouve encore quelque chose.
const HOLDS: Duration = Duration::from_millis(1500);

/// Ce qu'un voyant peut dire.
///
/// Trois et non deux, pour deux pastilles : celle de l'image porte les
/// deux ordinateurs et allume celui qui coince, donc elle compte pour
/// deux ici et pour une à l'écran.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Which {
    /// Le lien entre les deux ordinateurs.
    Link,
    /// L'image telle que l'ordinateur d'en face la fait.
    Far,
    /// Et telle que celui-ci la refait.
    Here,
}

impl std::fmt::Display for Which {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Which::Link => "lien",
            Which::Far => "image là-bas",
            Which::Here => "image ici",
        })
    }
}

/// Ce qu'une lecture dit de chacun : rien, ou ce qui ne va pas.
///
/// Les mots et non seulement le fait : un voyant qui s'allume sans que
/// rien ne dise pourquoi est un voyant qu'on finit par ignorer, et le
/// journal est le seul endroit où la raison tient.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Reads {
    pub link: Option<String>,
    pub far: Option<String>,
    pub here: Option<String>,
}

/// Ce qu'une lecture dit, sans mémoire d'aucune sorte.
///
/// Le temps disponible pour une image est calculé sur la cadence mesurée
/// et non sur celle demandée, et ce n'est pas un pis-aller : ce qu'on
/// cherche est de savoir si un des deux ordinateurs est ce qui donne le
/// rythme. Un hôte qui met vingt-cinq millisecondes par image sert
/// quarante images par seconde, donc son temps d'encodage **est** le
/// temps disponible, et le voyant s'allume ; le même hôte à trois
/// millisecondes sur une session à trente images a trente-trois
/// millisecondes devant lui et ne gêne personne. Demander la cadence
/// voulue aurait coûté un aller-retour au service à chaque lecture, et
/// aurait eu tort dès que quelqu'un la change en cours de session.
pub fn read(measures: &Measures) -> Reads {
    let mut reads = Reads::default();

    if let Some(frozen) = measures.since_frame_ms.filter(|held| *held >= FROZEN_MS) {
        reads.link = Some(format!("l'image est figée depuis {frozen} ms"));
    } else if let Some(lost) = measures.dropped_network_pct.filter(|pct| *pct >= LOST_PCT) {
        reads.link = Some(format!("{lost:.1} % des images se perdent en route"));
    } else if let Some(late) = measures
        .dropped_jitter_pct
        .filter(|pct| *pct >= TOO_LATE_PCT)
    {
        reads.link = Some(format!("{late:.1} % des images arrivent trop tard"));
    }

    // Une cadence qui manque laisse ces deux-là éteints : sans elle il
    // n'y a pas de temps disponible, donc rien à comparer, et un voyant
    // allumé faute de mesure serait un voyant allumé pour rien.
    //
    // Les deux sont pesés chacun de son côté et non l'un ou l'autre : ils
    // peuvent très bien coincer ensemble, sur deux machines fatiguées ou
    // sur une session trop grande pour les deux, et la pastille sait le
    // dire.
    if let Some(budget) = measures
        .fps
        .filter(|rate| *rate > 0.0)
        .map(|rate| 1000.0 / rate)
    {
        if let Some(host) = measures.host_ms.filter(|each| *each >= budget) {
            reads.far = Some(format!(
                "l'ordinateur d'en face met {host:.0} ms par image, \
                 pour {budget:.0} ms disponibles"
            ));
        }
        if let Some(decode) = measures.decode_ms.filter(|each| *each >= budget) {
            reads.here = Some(format!(
                "cet ordinateur met {decode:.0} ms à décoder une image, \
                 pour {budget:.0} ms disponibles"
            ));
        }
    }
    reads
}

/// Ce que les pastilles montrent, une fois la lecture calmée.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Shown {
    pub link: bool,
    pub far: bool,
    pub here: bool,
}

impl Shown {
    /// Rien du tout.
    pub fn nothing(self) -> bool {
        !self.link && !self.far && !self.here
    }
}

/// Ce qui tient les voyants allumés d'une lecture à l'autre.
///
/// Une seule chose, et c'est tout ce qui sépare un voyant utile d'une
/// guirlande : allumé à la lecture qui le dit, éteint seulement quand
/// plus rien ne l'a dit depuis un moment.
#[derive(Default)]
pub struct Steady {
    link: Option<Instant>,
    far: Option<Instant>,
    here: Option<Instant>,
}

impl Steady {
    /// Ce que cette lecture-là laisse allumé.
    pub fn after(&mut self, reads: &Reads, now: Instant) -> Shown {
        Shown {
            link: still(&mut self.link, reads.link.is_some(), now),
            far: still(&mut self.far, reads.far.is_some(), now),
            here: still(&mut self.here, reads.here.is_some(), now),
        }
    }
}

/// Un voyant, rallumé pour un moment quand sa cause est là.
fn still(until: &mut Option<Instant>, wrong: bool, now: Instant) -> bool {
    if wrong {
        *until = Some(now + HOLDS);
    }
    until.is_some_and(|end| now < end)
}

/* ---- La boucle qui les tient ----------------------------------------- */

/// Combien de fois par seconde la lecture est relue.
///
/// Le moteur en écrit cinq ; celle-ci en lit un peu plus souvent, pour
/// que le retard ajouté de ce côté-ci soit plus petit que celui de
/// l'autre. Ce qu'on vise est qu'un voyant soit là dans le tiers de
/// seconde qui suit ce qu'il annonce.
const LOOK_EVERY: Duration = Duration::from_millis(80);

#[cfg(windows)]
static WATCHING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Suit la santé de la session jusqu'à la fin de celle-ci.
///
/// Appelée à chaque tour de la veille du bouton flottant, comme la forme
/// du curseur : elle ne fait rien tant qu'une boucle tourne déjà, et en
/// relance une quand la précédente s'est arrêtée.
#[cfg(windows)]
pub fn watch(app: &crate::app::App) {
    use std::sync::atomic::Ordering;

    if WATCHING.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    crate::app::spawn(async move {
        keep_up(&app).await;
        show(&app, Shown::default(), &Reads::default(), false);
        WATCHING.store(false, Ordering::SeqCst);
    });
}

#[cfg(not(windows))]
pub fn watch(_app: &crate::app::App) {}

/// La boucle elle-même.
#[cfg(windows)]
async fn keep_up(app: &crate::app::App) {
    // À partir de quand une lecture est celle de cette session-ci. Le
    // fichier survit à la session qui l'a écrit, et une session qui
    // s'ouvre le trouverait tel que la précédente l'a laissé, donc avec
    // une image figée depuis des heures : un voyant allumé sur la
    // première image d'une session parfaitement saine.
    let started = std::time::SystemTime::now();
    let mut steady = Steady::default();
    let mut was = Shown::default();
    loop {
        tokio::time::sleep(LOOK_EVERY).await;
        if !crate::floating::a_session_is_up(app) {
            return;
        }
        let held = crate::floating::the_badges_are_held_up(app);
        let Some(measures) = fresh(started) else {
            // Tenues à l'écran, elles sont là avant même que le moteur
            // d'en face ait écrit une seule lecture : ce qu'on regarde
            // alors est les pastilles elles-mêmes, et une session dont
            // les lectures n'ont pas commencé est précisément le moment
            // où quelqu'un les regarde.
            show(app, Shown::default(), &Reads::default(), held);
            continue;
        };
        let reads = read(&measures);
        let now = Instant::now();
        let shown = steady.after(&reads, now);
        if shown != was {
            said(&reads, was, shown);
            was = shown;
        }
        // Dit à chaque tour et non au seul changement : cette boucle
        // commence avant que la fenêtre des pastilles existe, et un
        // voyant allumé pendant ce temps-là n'aurait plus jamais
        // l'occasion de changer d'avis. Ce qui est dit deux fois ne coûte
        // rien : la fenêtre garde ce qu'elle montre et ne se redessine
        // que sur une vraie différence.
        show(app, shown, &reads, held);
    }
}

/// La lecture, si elle est de cette session-ci.
#[cfg(windows)]
fn fresh(started: std::time::SystemTime) -> Option<Measures> {
    let path = zyr_proto::paths::session_stats();
    // L'heure du fichier plutôt que son contenu : rien dans la ligne ne
    // dit quelle session l'a écrite, et son âge le dit sans rien ajouter
    // à ce que le moteur écrit.
    let written = std::fs::metadata(&path).ok()?.modified().ok()?;
    if written < started {
        return None;
    }
    Some(crate::measures::session_measures())
}

/// Dit ce qui vient de changer, et rien d'autre.
///
/// Une ligne par allumage et une par extinction, jamais une par lecture :
/// il en passe une douzaine par seconde, et un journal qui les porterait
/// toutes ne porterait plus rien d'autre.
#[cfg(windows)]
fn said(reads: &Reads, was: Shown, shown: Shown) {
    for (which, before, after, why) in [
        (Which::Link, was.link, shown.link, &reads.link),
        (Which::Far, was.far, shown.far, &reads.far),
        (Which::Here, was.here, shown.here, &reads.here),
    ] {
        if before == after {
            continue;
        }
        note(&match (after, why) {
            (true, Some(why)) => format!("voyant {which} : {why}"),
            // Allumé sans raison dans cette lecture-ci : la cause est
            // passée entre deux lectures et le voyant tient encore.
            (true, None) => format!("voyant {which} allumé"),
            (false, _) => format!("voyant {which} éteint"),
        });
    }
}

/* ---- Les pastilles elles-mêmes --------------------------------------- */

/// Le côté d'une pastille, en pixels de page.
///
/// Un dixième de moins que le bouton flottant, qui fait quarante-quatre.
/// Assez près pour qu'on les voie de la même famille, assez loin pour
/// qu'on ne prenne pas l'une pour l'autre : ce bouton-là est ce qu'on
/// vise avec la main, celles-ci ne sont qu'à lire. Plus petites, elles ne
/// se lisaient pas : celle de l'image porte deux écrans dont un seul est
/// allumé, et il faut de la place pour que les deux se distinguent.
const BADGE: f32 = 40.0;

/// Ce qui sépare les deux.
const BETWEEN: f32 = 8.0;

/// La marge autour d'elles dans la fenêtre.
///
/// Assez pour que le bord d'une pastille ne touche pas celui de la
/// fenêtre, et c'est la même marge qui sert de gouttière à la bulle.
const ROOM: f32 = 8.0;

/// Ce qui sépare le dessin du bord de sa pastille.
const INSET: f32 = 7.0;

/// La largeur de la bulle qui dit pourquoi, en pixels de page.
const BUBBLE: f32 = 260.0;

/// Ce qui la sépare des pastilles.
const UNDER: f32 = 6.0;

/// Sa marge intérieure.
const PADDING: f32 = 10.0;

/// Le rayon de ses coins.
const CORNER: f32 = 8.0;

/// La taille de ce qui s'y écrit.
const WORDS: f32 = 12.0;

/// Ce que la bulle peut prendre de haut, au plus.
///
/// Réservé dans la fenêtre sans être forcément dessiné : la hauteur vraie
/// d'un texte ne se mesure qu'une fois la toile faite, et la fenêtre est
/// taillée avant elle. Trois lignes suffisent aux phrases que ces deux-là
/// ont à dire, et ce qui reste est clair, donc invisible.
const BUBBLE_AT_MOST: f32 = 2.0 * PADDING + 3.0 * 17.0;

/// Le rayon des coins d'une pastille : la moitié de son côté, donc un
/// rond.
const ROUNDED: f32 = BADGE / 2.0;

/// L'épaisseur de l'anneau qui cerne une pastille.
///
/// C'est lui qui porte l'état : sombre quand la pastille n'a rien à dire,
/// couleur d'alerte quand elle en a. Deux points et non un, parce qu'un
/// trait d'un point se lit comme un bord et pas comme un voyant, et
/// qu'entre les deux il reste toute la place qu'il faut au dessin.
const RING: f32 = 2.0;

/// La fenêtre, et ce qu'elle montre.
#[cfg(windows)]
static ITS_WINDOW: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
#[cfg(windows)]
static LIT: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// Ce que ce nombre-là veut dire.
///
/// Trois bits pour deux pastilles : celle de l'image est là dès que l'un
/// des deux ordinateurs coince, et allume celui des deux qui coince.
#[cfg(windows)]
mod bit {
    pub const LINK: u8 = 1;
    pub const FAR: u8 = 2;
    pub const HERE: u8 = 4;
    pub const PICTURE: u8 = FAR | HERE;
    /// Les deux pastilles tenues à l'écran, allumées ou non.
    ///
    /// Rangé avec les autres et non à côté, parce que c'est la même
    /// question : ce nombre dit ce que la fenêtre montre, et ce qui est
    /// montré n'est plus seulement ce qui est allumé.
    pub const HELD: u8 = 8;
    /// La pastille sur laquelle la main est posée, s'il y en a une.
    ///
    /// Rangée ici avec le reste parce que c'est encore la même question :
    /// ce nombre dit ce que la fenêtre montre, et une bulle ouverte sous
    /// une pastille en fait partie. Y être lui vaut aussi d'être
    /// redessinée quand la main arrive et quand elle part, sans que rien
    /// d'autre ait à s'en occuper.
    pub const OVER_LINK: u8 = 16;
    pub const OVER_PICTURE: u8 = 32;
    pub const OVER: u8 = OVER_LINK | OVER_PICTURE;
}

/// Ce que les pastilles ont à dire, mot pour mot.
///
/// Gardé à côté de ce qui est allumé parce que la bulle en a besoin au
/// moment où elle se dessine, et que ce moment-là n'est pas celui où la
/// lecture a été faite.
#[cfg(windows)]
static WHY: std::sync::Mutex<Reads> = std::sync::Mutex::new(Reads {
    link: None,
    far: None,
    here: None,
});

#[cfg(windows)]
thread_local! {
    static CANVAS: std::cell::RefCell<Option<crate::paint::Canvas>> =
        const { std::cell::RefCell::new(None) };
}

/// Ce que la fenêtre prend, en vrais pixels sur l'écran qu'elle couvre.
#[cfg(windows)]
fn its_size() -> (i32, i32) {
    let scale = crate::main_window::scale();
    // Taillée pour la bulle dès le départ, et non agrandie quand elle
    // s'ouvre : redimensionner une fenêtre à calque sous une main qui
    // passe se verrait. Ce qui n'est pas dessiné ne coûte qu'au
    // compositeur, lequel ne mêle cette fenêtre que lorsqu'une pastille
    // est déjà là.
    let wide = (2.0 * BADGE + BETWEEN).max(BUBBLE) + 2.0 * ROOM;
    let high = BADGE + UNDER + BUBBLE_AT_MOST + 2.0 * ROOM;
    ((wide * scale).ceil() as i32, (high * scale).ceil() as i32)
}

/// Ouvre la fenêtre des voyants, une fois par session.
///
/// `anchor` est le coin haut gauche de l'image, déjà écarté de la marge :
/// c'est là que la première pastille se pose, et la fenêtre déborde
/// autour d'elle de ce que l'ombre demande.
#[cfg(windows)]
pub fn raise(app: &crate::app::App, anchor: (i32, i32)) {
    use std::sync::atomic::Ordering;

    if ITS_WINDOW.load(Ordering::Relaxed) != 0 {
        return;
    }
    let owner = crate::main_window::handle();
    LIT.store(0, Ordering::Relaxed);
    let _ = app.run_on_main_thread(move || build(owner, anchor));
}

#[cfg(not(windows))]
pub fn raise(_app: &crate::app::App, _anchor: (i32, i32)) {}

/// Les range avec la session.
#[cfg(windows)]
pub fn lower(app: &crate::app::App) {
    use std::sync::atomic::Ordering;

    let window = ITS_WINDOW.swap(0, Ordering::Relaxed);
    if window == 0 {
        return;
    }
    LIT.store(0, Ordering::Relaxed);
    let _ = app.run_on_main_thread(move || {
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow;

        // SAFETY: une fenêtre à nous, détruite sur le fil qui l'a faite.
        unsafe { DestroyWindow(window as HWND) };
    });
}

#[cfg(not(windows))]
pub fn lower(_app: &crate::app::App) {}

/// Les pose dans le coin haut gauche de l'image.
///
/// Appelée d'où le bouton flottant est posé, donc cent vingt fois par
/// seconde sous une main qui redimensionne : rien ici n'attend quoi que
/// ce soit.
#[cfg(windows)]
pub fn lay(anchor: (i32, i32)) {
    use std::sync::atomic::Ordering;

    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
    };

    let window = ITS_WINDOW.load(Ordering::Relaxed);
    if window == 0 {
        return;
    }
    let (left, top) = window_corner(anchor);
    // SAFETY: une fenêtre à nous, posée sans être activée ni
    // redimensionnée. Poser une fenêtre depuis un autre fil se demande au
    // système, ce qui est ce qui rend ceci sûr depuis celui qui suit une
    // main.
    unsafe {
        SetWindowPos(
            window as HWND,
            std::ptr::null_mut(),
            left,
            top,
            0,
            0,
            SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER,
        )
    };
}

#[cfg(not(windows))]
pub fn lay(_anchor: (i32, i32)) {}

/// Le coin de la fenêtre, pour une première pastille posée là.
#[cfg(windows)]
fn window_corner(anchor: (i32, i32)) -> (i32, i32) {
    let room = (ROOM * crate::main_window::scale()).round() as i32;
    (anchor.0 - room, anchor.1 - room)
}

/// Allume ce qui doit l'être, et range la fenêtre quand plus rien ne
/// l'est.
///
/// Rangée et non peinte vide : une fenêtre à calque entièrement claire ne
/// se voit pas, mais elle reste une fenêtre que le compositeur mêle à
/// chaque image de la session.
#[cfg(windows)]
fn show(app: &crate::app::App, shown: Shown, reads: &Reads, held: bool) {
    use std::sync::atomic::Ordering;

    let window = ITS_WINDOW.load(Ordering::Relaxed);
    if window == 0 {
        return;
    }
    let mut lit = 0;
    for (on, bit) in [
        (shown.link, bit::LINK),
        (shown.far, bit::FAR),
        (shown.here, bit::HERE),
        (held, bit::HELD),
    ] {
        if on {
            lit |= bit;
        }
    }
    let anything = !shown.nothing() || held;
    // La main n'est cherchée que sur des pastilles qui sont là : sans
    // elles la fenêtre est rangée, et une bulle sous une pastille
    // invisible n'expliquerait rien.
    if anything {
        lit |= the_hand_over_them(window);
    }
    *WHY.lock().expect("raisons des voyants") = reads.clone();
    // Redessinée à chaque tour tant que la main est posée, et au seul
    // changement sinon : ce que la bulle dit porte des nombres qui
    // bougent, et une bulle qui garderait ceux de la première lecture
    // dirait une chose fausse tout le temps qu'on la regarde.
    let under_the_hand = lit & bit::OVER != 0;
    if LIT.swap(lit, Ordering::Relaxed) == lit && !under_the_hand {
        return;
    }
    let _ = app.run_on_main_thread(move || {
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::UI::WindowsAndMessaging::{SW_HIDE, SW_SHOWNOACTIVATE, ShowWindow};

        let window = window as HWND;
        if anything {
            repaint(window);
        }
        // SAFETY: une fenêtre à nous, montrée ou rangée sans prendre le
        // premier plan, sur le fil qui l'a faite.
        unsafe { ShowWindow(window, if anything { SW_SHOWNOACTIVATE } else { SW_HIDE }) };
    });
}

#[cfg(not(windows))]
fn show(_app: &crate::app::App, _shown: Shown, _reads: &Reads, _held: bool) {}

/// Sur laquelle des deux la main est posée, s'il y en a une.
///
/// Lue au système plutôt que reçue en messages, et c'est ce qui permet à
/// cette fenêtre de rester traversante. Le coin où elle se pose
/// appartient à l'ordinateur d'en face : une main qui vise son menu
/// Démarrer ne doit pas tomber sur un voyant, donc les clics passent au
/// travers, donc les mouvements aussi. Demander où est le curseur ne
/// prend rien à personne et répond à la seule question posée.
#[cfg(windows)]
fn the_hand_over_them(window: isize) -> u8 {
    use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetCursorPos, GetWindowRect};

    let mut hand = POINT { x: 0, y: 0 };
    // SAFETY: un point à nous, que le système remplit.
    if unsafe { GetCursorPos(&mut hand) } == 0 {
        return 0;
    }
    let mut place = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: une fenêtre à nous, dont le rectangle est lu dans le nôtre.
    if unsafe { GetWindowRect(window as HWND, &mut place) } == 0 {
        return 0;
    }
    let scale = crate::main_window::scale();
    let side = (BADGE * scale).round() as i32;
    let top = place.top + (ROOM * scale).round() as i32;
    if hand.y < top || hand.y >= top + side {
        return 0;
    }
    for (rank, which) in [bit::OVER_LINK, bit::OVER_PICTURE].into_iter().enumerate() {
        let left = place.left + ((ROOM + rank as f32 * (BADGE + BETWEEN)) * scale).round() as i32;
        if hand.x >= left && hand.x < left + side {
            return which;
        }
    }
    0
}

/// Ce que la pastille sous la main a à dire.
///
/// Une phrase même quand tout va bien : ces pastilles se tiennent à
/// l'écran sur demande, éteintes, et une bulle vide sous une pastille
/// éteinte laisserait croire que la question n'a pas de réponse.
fn what_it_says(rank: usize, why: &Reads) -> String {
    if rank == 0 {
        return why
            .link
            .clone()
            .unwrap_or_else(|| "le lien va bien : rien ne se perd et l'image suit".to_string());
    }
    let both: Vec<&str> = [why.far.as_deref(), why.here.as_deref()]
        .into_iter()
        .flatten()
        .collect();
    if both.is_empty() {
        return "les deux ordinateurs suivent : l'image est encodée et décodée à temps".to_string();
    }
    both.join("\n")
}

/// Bâtit la fenêtre, cachée : elle ne se montre qu'au premier voyant.
#[cfg(windows)]
fn build(owner: isize, anchor: (i32, i32)) {
    use std::sync::atomic::Ordering;

    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, RegisterClassW, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
        WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_POPUP,
    };

    let name = wide("ZyrDeskVoyants");
    let (wide_px, high) = its_size();
    let (left, top) = window_corner(anchor);
    // SAFETY: une classe enregistrée une fois et une fenêtre bâtie
    // dessus, sur le fil qui pompera ses messages. Une classe déjà
    // enregistrée est refusée et rien de plus, ce pour quoi la réponse
    // n'est pas lue : une deuxième session trouve celle de la première.
    let window = unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let class = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(nothing),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: std::ptr::null_mut(),
            hCursor: std::ptr::null_mut(),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: name.as_ptr(),
        };
        RegisterClassW(&class);
        // Transparente aux clics, et c'est le seul de ces quatre attributs
        // qui vaut d'être expliqué : ces pastilles ne se cliquent pas, et
        // le coin haut gauche de l'image appartient à l'ordinateur d'en
        // face. Une main qui vise son menu Démarrer ne doit pas tomber sur
        // un voyant.
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT,
            name.as_ptr(),
            std::ptr::null(),
            WS_POPUP,
            left,
            top,
            wide_px,
            high,
            owner as HWND,
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        )
    };
    if window.is_null() {
        note("voyants : la fenêtre n'a pas pu s'ouvrir");
        return;
    }
    ITS_WINDOW.store(window as isize, Ordering::Relaxed);
}

/// Cette fenêtre n'a rien à répondre : elle ne porte que son image, et
/// les clics la traversent.
#[cfg(windows)]
unsafe extern "system" fn nothing(
    window: windows_sys::Win32::Foundation::HWND,
    message: u32,
    holding: windows_sys::Win32::Foundation::WPARAM,
    with: windows_sys::Win32::Foundation::LPARAM,
) -> windows_sys::Win32::Foundation::LRESULT {
    // SAFETY: appelée par le système sur le fil qui a fait cette fenêtre,
    // avec les arguments qu'il documente.
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::DefWindowProcW(window, message, holding, with)
    }
}

/// Redessine les pastilles allumées.
#[cfg(windows)]
fn repaint(window: windows_sys::Win32::Foundation::HWND) {
    use std::sync::atomic::Ordering;

    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect;

    use crate::design::{Colour, DARK};
    use crate::paint::Rect;

    let lit = LIT.load(Ordering::Relaxed);
    // Tenues à l'écran, elles sont dessinées toutes les deux et chacune
    // dit quand même ce qu'elle lit : c'est où elles sont dessinées que
    // cela change et jamais ce qu'elles disent. Deux pastilles toujours
    // allumées ne montreraient rien de leur travail.
    let held = lit & bit::HELD != 0;
    let scale = crate::main_window::scale();
    let (wide_px, high) = its_size();
    CANVAS.with_borrow_mut(|canvas| {
        // Refaite quand l'écran a changé d'agrandissement : la toile est
        // une image d'une taille donnée, et la fenêtre a suivi.
        if canvas
            .as_ref()
            .is_none_or(|had| had.size() != (wide_px, high))
        {
            *canvas = crate::paint::Canvas::new(wide_px, high);
        }
        let Some(canvas) = canvas.as_ref() else {
            return;
        };
        canvas.begin(Colour::TRANSPARENT);
        // Chacune a sa place et la garde, même quand l'autre est éteinte :
        // celle de l'image reste la deuxième, avec un vide à sa gauche là
        // où serait celle du lien. Serrées l'une contre l'autre, la
        // seconde sauterait de place chaque fois que la première s'allume,
        // et un voyant qui bouge est un voyant qu'on relit au lieu de le
        // reconnaître. Le vide ne se voit pas : la fenêtre est claire
        // partout où rien n'est dessiné.
        for (rank, on) in [lit & bit::LINK != 0, lit & bit::PICTURE != 0]
            .into_iter()
            .enumerate()
        {
            if !on && !held {
                continue;
            }
            let left = (ROOM + rank as f32 * (BADGE + BETWEEN)) * scale;
            let dot = Rect::at(left, ROOM * scale, BADGE * scale, BADGE * scale);
            let radius = ROUNDED * scale;
            // Sans ombre portée, et c'est le bord qui dit tout. Ces
            // pastilles flottent sur le bureau d'un autre ordinateur, qui
            // peut être de n'importe quelle couleur : une ombre y est
            // invisible sur un fond noir et y fait une tache grise sur un
            // fond clair, ce qui est le contraire de ce qu'on lui
            // demandait.
            //
            // Deux traits plutôt qu'un, et chacun pour un fond : l'anneau
            // épais, sombre ou couleur d'alerte, se détache d'un bureau
            // clair ; le cheveu clair posé juste dehors détache la
            // pastille d'un bureau noir, où l'anneau seul se fondrait. Un
            // seul des deux se voit à la fois, et c'est pour ça qu'il en
            // faut deux.
            let ring = if on { DARK.warning } else { DARK.border_strong };
            canvas.fill(dot, radius, DARK.background.faded(0.94));
            canvas.stroke_inside(dot, radius, RING * scale, ring);
            canvas.stroke_on(
                dot.grown(scale / 2.0),
                radius + scale / 2.0,
                scale,
                DARK.text.faded(0.16),
            );
            let icon_area = dot.grown(-INSET * scale);
            if rank == 0 {
                let colour = if on { DARK.warning } else { DARK.text_faint };
                canvas.icon(&crate::icons::LINK, icon_area, colour);
                continue;
            }
            // La pastille de l'image porte les deux ordinateurs, celui
            // d'en face derrière et celui-ci devant, comme le logo du
            // produit les dessine. Les deux sont posés en sourdine, puis
            // celui qui coince est repassé par-dessus en clair : c'est
            // tout ce qu'il faut pour dire lequel des deux, et ça se lit
            // sans légende puisque c'est le dessin de la marque.
            canvas.icon(&crate::icons::HOST_SCREEN, icon_area, DARK.text_faint);
            if lit & bit::FAR != 0 {
                canvas.icon(&crate::icons::SCREEN_OVER_THERE, icon_area, DARK.warning);
            }
            if lit & bit::HERE != 0 {
                canvas.icon(&crate::icons::SCREEN_HERE, icon_area, DARK.warning);
            }
        }
        // La bulle après les pastilles, pour qu'elle passe par-dessus si
        // jamais les deux se touchaient.
        if lit & bit::OVER != 0 {
            let rank = usize::from(lit & bit::OVER_LINK == 0);
            let text = what_it_says(rank, &WHY.lock().expect("raisons des voyants"));
            let pen = crate::paint::Pen::of(WORDS * scale);
            let inside = (BUBBLE - 2.0 * PADDING) * scale;
            let bubble = Rect::at(
                ROOM * scale,
                (ROOM + BADGE + UNDER) * scale,
                BUBBLE * scale,
                canvas.height_of(&text, pen, inside) + 2.0 * PADDING * scale,
            );
            let radius = CORNER * scale;
            canvas.fill(bubble, radius, DARK.surface_1.faded(0.96));
            canvas.stroke_inside(bubble, radius, scale, DARK.border_strong);
            canvas.draw_text(&text, pen, DARK.text, bubble.grown(-PADDING * scale));
        }
        if !canvas.finish() {
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
        canvas.lay_on(window as isize, place.left, place.top);
    });
}

/// Un mot comme Windows les lit, terminé par un nought.
#[cfg(windows)]
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Une lecture d'une session qui va bien, à soixante images.
    fn healthy() -> Measures {
        Measures {
            fps: Some(60.0),
            decode_ms: Some(0.4),
            render_ms: Some(15.0),
            host_ms: Some(2.3),
            network_ms: Some(8.0),
            dropped_network_pct: Some(0.0),
            dropped_jitter_pct: Some(0.1),
            since_frame_ms: Some(12),
            ..Default::default()
        }
    }

    #[test]
    fn a_badge_under_the_hand_says_what_it_reads() {
        let reads = read(&Measures {
            since_frame_ms: Some(FROZEN_MS),
            ..healthy()
        });
        assert!(what_it_says(0, &reads).contains("figée"));
    }

    #[test]
    fn a_badge_with_nothing_to_report_says_so_rather_than_nothing() {
        // Elles se tiennent à l'écran sur demande, éteintes : une bulle
        // vide laisserait croire que la question n'a pas de réponse.
        let calm = Reads::default();
        assert!(!what_it_says(0, &calm).is_empty());
        assert!(!what_it_says(1, &calm).is_empty());
    }

    #[test]
    fn the_picture_badge_says_both_computers_when_both_are_late() {
        let reads = Reads {
            link: None,
            far: Some("là-bas".to_string()),
            here: Some("ici".to_string()),
        };
        let said = what_it_says(1, &reads);
        assert!(said.contains("là-bas") && said.contains("ici"));
    }

    #[test]
    fn a_session_that_is_going_well_lights_nothing() {
        assert_eq!(read(&healthy()), Reads::default());
    }

    #[test]
    fn a_reading_that_says_nothing_lights_nothing_either() {
        // Une session qui vient de s'ouvrir : le moteur n'a pas encore
        // écrit une seconde. Rien n'est su, donc rien ne s'allume.
        assert_eq!(read(&Measures::default()), Reads::default());
    }

    #[test]
    fn a_picture_that_has_stopped_lights_the_link() {
        let frozen = Measures {
            since_frame_ms: Some(FROZEN_MS),
            ..healthy()
        };
        let reads = read(&frozen);
        assert!(reads.link.is_some_and(|why| why.contains("figée")));
        assert!(reads.far.is_none() && reads.here.is_none());
    }

    #[test]
    fn frames_lost_on_the_way_light_the_link_too() {
        let lost = Measures {
            dropped_network_pct: Some(LOST_PCT),
            ..healthy()
        };
        assert!(read(&lost).link.is_some_and(|why| why.contains("perdent")));

        let late = Measures {
            dropped_jitter_pct: Some(TOO_LATE_PCT),
            ..healthy()
        };
        assert!(
            read(&late)
                .link
                .is_some_and(|why| why.contains("trop tard"))
        );
    }

    #[test]
    fn a_host_that_cannot_keep_up_lights_the_picture() {
        // Vingt-cinq millisecondes par image sur une session qui en sert
        // quarante : son encodage est ce qui donne le rythme.
        let slow = Measures {
            fps: Some(40.0),
            host_ms: Some(25.0),
            ..healthy()
        };
        let reads = read(&slow);
        assert!(reads.far.is_some_and(|why| why.contains("d'en face")));
        // Et celui-ci n'y est pour rien : la pastille doit allumer le bon
        // des deux écrans, pas les deux.
        assert!(reads.here.is_none());
        assert!(reads.link.is_none());
    }

    #[test]
    fn a_computer_that_cannot_decode_in_time_lights_it_as_well() {
        let slow = Measures {
            fps: Some(30.0),
            decode_ms: Some(40.0),
            ..healthy()
        };
        let reads = read(&slow);
        assert!(reads.here.is_some_and(|why| why.contains("cet ordinateur")));
        assert!(reads.far.is_none());
    }

    #[test]
    fn two_computers_that_both_struggle_light_both_screens() {
        // Une session trop grande pour les deux machines : la pastille
        // n'a pas à choisir laquelle nommer, elle les allume toutes deux.
        let both = Measures {
            fps: Some(24.0),
            host_ms: Some(45.0),
            decode_ms: Some(50.0),
            ..healthy()
        };
        let reads = read(&both);
        assert!(reads.far.is_some());
        assert!(reads.here.is_some());

        let mut steady = Steady::default();
        assert_eq!(
            steady.after(&reads, Instant::now()),
            Shown {
                link: false,
                far: true,
                here: true
            }
        );
    }

    #[test]
    fn a_slow_session_that_asked_for_slow_is_not_a_fault() {
        // Trente images par seconde laissent trente-trois millisecondes
        // par image : un hôte à vingt n'est en retard sur rien.
        let calm = Measures {
            fps: Some(30.0),
            host_ms: Some(20.0),
            decode_ms: Some(5.0),
            ..healthy()
        };
        assert_eq!(read(&calm), Reads::default());
    }

    #[test]
    fn the_time_a_frame_waits_for_the_screen_is_not_counted() {
        // Le temps de rendu comprend l'attente du rafraîchissement de
        // l'écran, donc il approche toujours le temps disponible :
        // compté, ce voyant serait allumé toute la session.
        let ordinary = Measures {
            render_ms: Some(16.6),
            ..healthy()
        };
        assert_eq!(read(&ordinary), Reads::default());
    }

    #[test]
    fn a_badge_stays_lit_for_a_moment_after_its_cause_has_gone() {
        // Sans ça il clignote : la cause tient sur une lecture, et il en
        // passe une douzaine par seconde.
        let start = Instant::now();
        let mut steady = Steady::default();
        let wrong = read(&Measures {
            since_frame_ms: Some(900),
            ..healthy()
        });

        assert_eq!(
            steady.after(&wrong, start),
            Shown {
                link: true,
                far: false,
                here: false
            }
        );
        let well = read(&healthy());
        assert!(steady.after(&well, start + Duration::from_millis(100)).link);
        assert!(
            steady
                .after(&well, start + HOLDS - Duration::from_millis(1))
                .link
        );
        assert!(!steady.after(&well, start + HOLDS).link);
    }

    #[test]
    fn a_cause_that_comes_back_holds_it_on_from_there() {
        let start = Instant::now();
        let mut steady = Steady::default();
        let wrong = read(&Measures {
            since_frame_ms: Some(900),
            ..healthy()
        });
        let well = read(&healthy());

        steady.after(&wrong, start);
        let again = start + HOLDS - Duration::from_millis(10);
        steady.after(&wrong, again);
        // La seconde cause repart d'où elle est, et non de la première :
        // sinon un réseau qui va mal par à-coups éteindrait le voyant au
        // milieu de ses à-coups.
        assert!(steady.after(&well, start + HOLDS).link);
        assert!(!steady.after(&well, again + HOLDS).link);
    }

    #[test]
    fn the_three_are_counted_apart() {
        let start = Instant::now();
        let mut steady = Steady::default();
        let only_the_far_one = read(&Measures {
            fps: Some(40.0),
            host_ms: Some(25.0),
            ..healthy()
        });
        assert_eq!(
            steady.after(&only_the_far_one, start),
            Shown {
                link: false,
                far: true,
                here: false
            }
        );
        assert!(
            !Shown {
                link: false,
                far: true,
                here: false
            }
            .nothing()
        );
        assert!(Shown::default().nothing());
    }
}
