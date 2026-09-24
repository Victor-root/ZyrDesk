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
//! style, et `scale` les passe en vrais pixels au moment de dessiner.
//! C'est le même partage que partout ailleurs, et c'est ce qui permet de
//! relire une mesure ici et de la retrouver là-bas.
//!
//! La fenêtre suit la carte : elle est remesurée à chaque dessin, et
//! l'image lui est remise en même temps que sa taille. Il n'existe donc
//! pas d'instant où elle soit grande sans être peinte, ce qui est ce
//! qu'une vue web ne savait pas faire et ce qui la forçait à se bâtir
//! une fois pour toutes à sa plus grande taille possible.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use crate::app::App;

use crate::design::{self, Colour, Palette};
use crate::floating::{Act, Opens};
use crate::icons;
use crate::measures::Measures;
use crate::paint::{Align, Canvas, Icon, Pen, Rect};
use crate::settings::{Offered, SessionMenu};
use crate::shortcuts::Doing;

/// Ce sous quoi ce module classe ses lignes du journal.
const TAG: &str = "floating";

/// Écrit une ligne sous l'étiquette de ce module.
fn note(what: &str) {
    crate::journal::note_about(TAG, what);
}

/// Une ligne de la carte.
///
/// La carte se décrit et se dessine ensuite : mesurer sa largeur demande
/// de connaître toutes ses lignes avant d'en poser une seule, et une
/// carte aussi large que sa plus longue ligne est ce que la feuille de
/// style demande depuis toujours.
enum Line {
    /// Ce que la session coûte : quatre nombres et une phrase.
    Measures,
    /// Un trait entre deux groupes.
    Separator,
    /// Ce que le menu vient de refuser de faire, et pourquoi.
    Refusal,
    /// Une ligne qu'on clique, comme la page les appelle.
    Entry(Entry),
    /// Une ligne qui porte un choix entre deux côtés.
    Toggle(Toggle),
    /// Une ligne qui porte quelques valeurs côte à côte.
    Choice(Choice),
    /// Une ligne qu'on pousse le long d'une barre.
    Slider(Slider),
    /// Une ligne qui ouvre une liste à elle.
    List(List),
}

/// Une ligne qui porte quelques valeurs sans ordre entre elles.
///
/// Des boutons et non une barre : le codec n'est pas une échelle, ce sont
/// quelques noms dont un « Automatique » qui n'est pas une valeur mais un
/// renoncement, et pousser un curseur promettrait un plus et un moins qui
/// n'existent pas.
struct Choice {
    icon: &'static Icon,
    label: &'static str,
    setting: Setting,
}

/// Une ligne qu'on règle en poussant un curseur, la valeur écrite
/// au-dessus.
///
/// Une échelle : plus grand, plus rapide, et on en cherche le bon cran en
/// regardant l'image bouger. Les crans viennent du produit, un par
/// mégabit, et le curseur va de zéro au nombre de valeurs moins une : il
/// pousse des rangs et non des nombres, comme les autres lignes à liste.
struct Slider {
    icon: &'static Icon,
    label: &'static str,
    setting: Setting,
}

/// Une ligne qui ouvre une liste à elle, à côté de la carte.
///
/// Une liste plutôt qu'une barre pour deux raisons : ses premières
/// entrées ne sont pas des nombres mais disent lequel des deux
/// ordinateurs décide, ce qu'aucune barre ne sait dire, et il y en a
/// quinze en dessous, ce qui fait des crans qu'on ne vise plus.
struct List {
    icon: &'static Icon,
    label: &'static str,
    setting: Setting,
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
struct Toggle {
    icon: &'static Icon,
    label: &'static str,
    /// Les deux côtés, dans l'ordre où ils se lisent. Le second est celui
    /// qui vaut « oui ».
    sides: [&'static str; 2],
    /// Ce qu'on demande à la session pour passer d'un côté à l'autre.
    act: Act,
    /// Où l'on en est : vrai pour le côté de droite.
    state: &'static AtomicBool,
}

/// Une entrée du menu : une icône, un mot, ce qui est écrit à sa droite,
/// et ce qu'elle demande.
struct Entry {
    icon: &'static Icon,
    label: &'static str,
    trailing: Trailing,
    does: Does,
    /// Écrite dans la couleur des choses qui ne se défont pas. Une seule
    /// ligne du menu l'est, et c'est celle qui coupe la session.
    destructive: bool,
}

/// Ce qui s'écrit à droite d'une ligne.
enum Trailing {
    /// Ce que la ligne fait, dit en toutes lettres.
    Text(&'static str),
    /// La combinaison en place pour ça, ou ce mot-ci tant que personne
    /// ne lui en a donné une.
    Key(Doing, &'static str),
}

/// Ce qu'une ligne demande quand on clique dessus.
#[derive(Clone, Copy)]
enum Does {
    /// Ce que la session sait faire, dans sa langue.
    Session(Act),
    /// Ranger le bouton jusqu'à ce que le raccourci le rappelle.
    PutAway,
}

/// Un des réglages que la session porte.
///
/// Nommé ici comme le produit le nomme des deux côtés : c'est ce mot-là
/// qui voyage jusqu'au service, et en avoir un deuxième pour l'affichage
/// serait deux noms pour un réglage.
#[derive(Clone, Copy, PartialEq)]
enum Setting {
    Size,
    Screen,
    Bitrate,
    Codec,
    Steady,
}

/// Ce que la carte contient, dans l'ordre.
///
/// Les mêmes lignes que la page, dans le même ordre, avec les mêmes mots,
/// les mêmes icônes et les mêmes actions. Ce qui manque encore est dit
/// dans le journal à l'ouverture plutôt que remplacé par du vide qui
/// ressemblerait à un défaut.
const LINES: [Line; 21] = [
    Line::Measures,
    // Juste sous les mesures, donc en tête de ce qu'on lit : ce qui vient
    // d'être refusé se lit avant ce qu'on allait cliquer ensuite. Elle ne
    // prend aucune place tant qu'il n'y a rien à dire.
    Line::Refusal,
    Line::Separator,
    Line::Entry(Entry {
        icon: &icons::FULL_SCREEN,
        label: "Fenêtré ou plein écran",
        trailing: Trailing::Key(Doing::Fullscreen, ""),
        does: Does::Session(Act::Fullscreen),
        destructive: false,
    }),
    Line::Entry(Entry {
        icon: &icons::STATISTICS,
        label: "Statistiques",
        trailing: Trailing::Text("Ctrl+Alt+Maj+S"),
        does: Does::Session(Act::Stats),
        destructive: false,
    }),
    Line::Toggle(Toggle {
        icon: &icons::LINK,
        label: "Voyants",
        sides: ["Au besoin", "Tenus"],
        act: Act::Badges,
        state: &HELD,
    }),
    Line::Toggle(Toggle {
        icon: &icons::MOUSE,
        label: "Souris",
        sides: ["Bureau", "Jeu"],
        act: Act::MouseMode,
        state: &IN_GAME,
    }),
    Line::Toggle(Toggle {
        icon: &icons::SOUND,
        label: "Son",
        sides: ["Actif", "Coupé"],
        act: Act::Sound,
        state: &MUTED,
    }),
    Line::Toggle(Toggle {
        icon: &icons::KEYBOARD,
        label: "Clavier",
        sides: ["Partagé", "Immersif"],
        act: Act::SystemKeys,
        state: &IMMERSIVE,
    }),
    Line::Toggle(Toggle {
        icon: &icons::CLIPBOARD,
        label: "Presse-papiers",
        sides: ["Chacun le sien", "Partagé"],
        act: Act::Clipboard,
        state: &SHARED,
    }),
    Line::Entry(Entry {
        icon: &icons::CAD,
        label: "Ctrl+Alt+Suppr",
        trailing: Trailing::Text("sur l'ordinateur distant"),
        does: Does::Session(Act::SecureAttention),
        destructive: false,
    }),
    Line::Entry(Entry {
        icon: &icons::LOCK,
        label: "Verrouiller",
        trailing: Trailing::Text("l'ordinateur distant"),
        does: Does::Session(Act::LockScreen),
        destructive: false,
    }),
    Line::Separator,
    Line::List(List {
        icon: &icons::RESOLUTION,
        label: "Résolution",
        setting: Setting::Size,
    }),
    Line::List(List {
        icon: &icons::HOST_SCREEN,
        label: "Écran de l'hôte",
        setting: Setting::Screen,
    }),
    Line::Slider(Slider {
        icon: &icons::BITRATE,
        label: "Débit",
        setting: Setting::Bitrate,
    }),
    Line::Choice(Choice {
        icon: &icons::CODEC,
        label: "Codec",
        setting: Setting::Codec,
    }),
    Line::Choice(Choice {
        icon: &icons::FAR_SCREEN,
        label: "Écran d'en face",
        setting: Setting::Steady,
    }),
    Line::Separator,
    Line::Entry(Entry {
        icon: &icons::HIDE,
        label: "Masquer ce bouton",
        trailing: Trailing::Key(Doing::Menu, "jusqu'à la fin"),
        does: Does::PutAway,
        destructive: false,
    }),
    Line::Entry(Entry {
        icon: &icons::QUIT,
        label: "Terminer la session",
        trailing: Trailing::Key(Doing::End, "rend le bureau distant"),
        does: Does::Session(Act::End),
        destructive: true,
    }),
];

/// Ce que la feuille de style dit d'une ligne, en pixels de page.
mod layout {
    /// La hauteur qu'une ligne ne descend jamais en dessous.
    pub const LINE: f32 = 38.0;
    /// Le côté d'une icône, et l'espace entre elle et le mot.
    pub const ICON: f32 = 18.0;
    /// Ce qui sépare le mot de ce qui est écrit à sa droite.
    pub const AFTER_THE_LABEL: f32 = 24.0;
    /// L'épaisseur d'un trait de séparation, et celle d'une bordure.
    pub const HAIRLINE: f32 = 1.0;
    /// La largeur que chaque mesure garde quel que soit son nombre, pour
    /// que la barre ne respire pas au rythme des chiffres.
    pub const READING: f32 = 78.0;
    /// Ce qui sépare deux mesures, et ce qui sépare leur mot de leur
    /// nombre.
    pub const BETWEEN_READINGS: f32 = 16.0;
    pub const UNDER_THE_LABEL: f32 = 2.0;
    /// La hauteur d'un interrupteur : sa légende, ce qui l'entoure
    /// au-dessus et en dessous, et sa bordure. La page l'obtient de la
    /// hauteur de ligne du navigateur, qui n'existe pas ici : elle est
    /// donc dite.
    pub const TOGGLE: f32 = 24.0;
    /// La place que prend un curseur, son pouce compris.
    pub const SLIDER: f32 = 18.0;
    /// L'épaisseur de la barre d'un curseur, et le côté de son pouce.
    pub const BAR: f32 = 4.0;
    pub const THUMB: f32 = 14.0;
    /// Le côté d'un chevron et d'une coche : plus petits qu'une icône de
    /// ligne, parce que ce sont des marques et non des dessins.
    pub const BRAND: f32 = 16.0;
}

/// Un des quatre chiffres de la barre : ce qu'il coûte, comment il se
/// lit, et où il se prend dans ce que le moteur écrit.
struct Reading {
    label: &'static str,
    unit: &'static str,
    /// Combien de décimales : le réseau se lit en millisecondes rondes,
    /// le reste au centième.
    decimals: usize,
    read: fn(&Measures) -> Option<f64>,
}

/// Les quatre mesures, dans l'ordre où elles se lisent : ce que coûte une
/// image ici, ce qu'elle a coûté là-bas, ce qu'il y a entre les deux, et
/// ce que le fil porte vraiment.
///
/// Les mêmes mots et les mêmes unités que la page, parce que ce sont les
/// mêmes mesures : les inventer ici en donnerait quatre autres, et deux
/// barres qui ne disent pas la même chose sur le même moteur.
const READINGS: [Reading; 4] = [
    Reading {
        label: "Décodage",
        unit: "ms",
        decimals: 2,
        read: |said| said.decode_ms,
    },
    Reading {
        label: "Encodage",
        unit: "ms",
        decimals: 2,
        read: |said| said.host_ms,
    },
    Reading {
        label: "Réseau",
        unit: "ms",
        decimals: 0,
        read: |said| said.network_ms,
    },
    Reading {
        label: "Débit",
        unit: "Mb/s",
        decimals: 2,
        read: |said| said.bitrate_mbps,
    },
];

/// Ce qu'une mesure montre tant qu'elle n'a rien à dire.
///
/// Le moteur ne dit rien plutôt que zéro quand il n'a rien mesuré, et
/// zéro serait un mensonge : une seconde sans image décodée n'a pas un
/// temps de décodage nul.
const NO_READING: &str = "-";

/// Combien de temps une mesure qui manque garde ce qu'elle disait.
///
/// Une de ces quatre manque parfois à une seconde et revient à la
/// suivante : ce que l'ordinateur d'en face mesure ne voyage pas avec
/// chaque image, et une seconde peut passer sans qu'aucune ne le porte.
/// Effacée aussitôt, la mesure clignote entre un nombre et un tiret, et
/// un nombre qui clignote se lit plus mal qu'un nombre d'une seconde de
/// retard — qui est de toute façon ce qu'on lit, ces quatre-là étant des
/// moyennes sur la seconde écoulée.
///
/// Trois secondes et pas plus : au-delà ce n'est plus une lecture qui a
/// sauté mais une mesure qui n'existe plus, et le tiret dit alors vrai.
const KEEP_FOR: std::time::Duration = std::time::Duration::from_secs(3);

/// Le rythme du moteur, qui écrit une fois par seconde. Demander plus
/// souvent relirait le même fichier pour le même nombre.
const REFRESH: std::time::Duration = std::time::Duration::from_secs(1);

/// De combien une couleur teinte le fond quand elle sert de survol : ce
/// que la feuille de style écrit `color-mix(in srgb, ... 12%,
/// transparent)`.
const VEIL: f32 = 0.12;

/// La fenêtre de la carte, et ce qu'elle sait d'elle-même.
static ITS_WINDOW: AtomicIsize = AtomicIsize::new(0);
static WIDTH: AtomicU32 = AtomicU32::new(0);
static HEIGHT: AtomicU32 = AtomicU32::new(0);
static OPEN: AtomicBool = AtomicBool::new(false);
static LIGHT: AtomicBool = AtomicBool::new(false);

/// Ce qui est sous la souris, et ce sur quoi un clic a commencé.
///
/// Écrits par la réponse de la fenêtre, que le système appelle, et lus
/// par le dessin. Les deux tournent sur le fil qui possède la fenêtre,
/// donc ces verrous ne sont jamais disputés.
static HOVER: Mutex<Option<Target>> = Mutex::new(None);
static PRESSED: Mutex<Option<Target>> = Mutex::new(None);

/// Si la souris est dans cette fenêtre, pour ne demander qu'une fois à
/// être prévenu de son départ.
static HAND_INSIDE: AtomicBool = AtomicBool::new(false);

/// Ce qu'on peut cliquer, dans la carte ou dans le panneau ouvert.
#[derive(Clone, Copy, PartialEq)]
enum Target {
    /// Une ligne qu'on clique en entier, par son rang dans `LINES`.
    Line(usize),
    /// Un côté d'un interrupteur ou d'une ligne à boutons : le rang de sa
    /// ligne, et lequel des côtés.
    Side(usize, usize),
    /// La barre d'un curseur.
    Bar(usize),
    /// Une valeur du panneau ouvert, par son rang dans la liste.
    Value(usize),
}

impl Target {
    /// La ligne de la carte dont il s'agit, quand c'en est une.
    fn line(self) -> Option<usize> {
        match self {
            Target::Line(rank) | Target::Side(rank, _) | Target::Bar(rank) => Some(rank),
            Target::Value(_) => None,
        }
    }
}

/// Ce que la barre des mesures montre : quatre nombres déjà écrits et la
/// phrase du flux.
///
/// Écrits là où ils sont lus plutôt que gardés en nombres : la mise en
/// forme se fait alors une fois par seconde et non une fois par image, et
/// le fil qui dessine n'a plus qu'à poser du texte.
struct ReadingsBar {
    figures: [String; 4],
    /// Quand chacune a vraiment été lue, et non recopiée de la lecture
    /// d'avant. Hors de toute comparaison : ces instants bougent à chaque
    /// tour sans que rien ne se lise autrement.
    read_at: [Option<Instant>; 4],
    stream: String,
}

static READINGS_BAR: Mutex<ReadingsBar> = Mutex::new(ReadingsBar::empty());

/// Le tour de veille des mesures.
///
/// Il change à chaque ouverture et à chaque fermeture, ce qui arrête le
/// tour précédent : sans ça, ouvrir et refermer vite laisserait deux
/// veilles derrière la même carte.
static ROUND: AtomicU32 = AtomicU32::new(0);

/// Où en est chacun des six interrupteurs.
///
/// Relus à chaque ouverture de la carte plutôt que retenus : le raccourci
/// du produit bascule la souris, et le mélangeur de Windows est ouvert à
/// tout le monde. Un interrupteur qui montre ce qu'il croit plutôt que ce
/// qui est est un interrupteur qu'on ne croit pas deux fois.
static IN_GAME: AtomicBool = AtomicBool::new(false);
static MUTED: AtomicBool = AtomicBool::new(false);
static IMMERSIVE: AtomicBool = AtomicBool::new(false);
static SHARED: AtomicBool = AtomicBool::new(false);
static HELD: AtomicBool = AtomicBool::new(false);

/// De combien un pixel de page vaut de vrais pixels.
static SCALE: AtomicU32 = AtomicU32::new(100);

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
static CAPTION_HEIGHT: AtomicU32 = AtomicU32::new(0);
static BODY_HEIGHT: AtomicU32 = AtomicU32::new(0);

/// Vers où le menu s'ouvre, donc à quel bord de sa fenêtre la carte est
/// collée.
static UPWARD: AtomicBool = AtomicBool::new(false);

/// Si la carte est collée au bord gauche de sa fenêtre plutôt qu'au
/// droit, et le panneau à sa droite plutôt qu'à sa gauche : décidé par
/// le bouton quand son bord droit n'a pas la place de porter la carte.
static RIGHTWARD: AtomicBool = AtomicBool::new(false);

/// Ce que la carte prend de large, mesuré sur toutes ses lignes.
static CARD_WIDTH: AtomicU32 = AtomicU32::new(0);

/// Ce que le menu vient de refuser de faire, et depuis quand.
///
/// Le menu de la vue web portait une ligne rouge pour ça. Celui que
/// ZyrDesk dessine ne l'avait pas reprise, et un refus n'allait donc plus
/// que dans le journal : un interrupteur qui se refuse à bon droit et se
/// contente de ne pas basculer est un interrupteur cassé, même quand il
/// a parfaitement raison.
static REFUSAL: Mutex<Option<(String, Instant)>> = Mutex::new(None);

/// Ce que ce refus prend de haut, mesuré au dessin comme la carte l'est.
static REFUSAL_HEIGHT: AtomicU32 = AtomicU32::new(0);

/// Le temps qu'un refus reste sur la carte.
///
/// Long, parce qu'il porte ce qu'il y a à faire ailleurs et que c'est
/// ailleurs qu'on part le faire : un refus effacé pendant qu'on lit la
/// page de Windows serait un refus jamais lu.
const REFUSAL_TIME: Duration = Duration::from_secs(20);

/// Ce qu'il y a à dire d'un refus, tant qu'il est frais.
///
/// Ce qui a passé son temps est oublié au passage : la carte se rouvre
/// souvent, et un refus d'il y a une heure se relirait comme celui du
/// clic qu'on vient de faire.
fn refusal_to_say() -> Option<String> {
    let mut refusal = REFUSAL.lock().expect("refus du menu");
    if refusal
        .as_ref()
        .is_some_and(|(_, since)| since.elapsed() >= REFUSAL_TIME)
    {
        *refusal = None;
    }
    refusal.as_ref().map(|(said, _)| said.clone())
}

/// Le tour de veille des réglages, qui arrête le précédent.
static SESSION_MENU_ROUND: AtomicU32 = AtomicU32::new(0);

/// Ce que la session propose et où elle en est, demandé à l'ouverture de
/// la carte.
///
/// Demandé d'un coup plutôt qu'une liste à la fois : la carte se mesure
/// sur ce qu'elle contient, donc elle a besoin de tout avant de poser
/// quoi que ce soit.
static SESSION_MENU: Mutex<Option<SessionMenu>> = Mutex::new(None);

/// Le sous-menu ouvert, ou rien.
static PANEL: Mutex<Option<Setting>> = Mutex::new(None);

/// Le cran où une main tient le curseur du débit, tant qu'elle le tient.
///
/// Ce qui est choisi n'est écrit qu'au relâchement : un curseur poussé
/// d'un bout à l'autre traverse quinze crans, et chacun d'eux serait un
/// aller-retour jusqu'au service pour un débit que personne n'a voulu.
static PUSHED: Mutex<Option<usize>> = Mutex::new(None);

/// Le programme, pour les endroits que le système appelle et à qui la
/// boîte à outils ne donne rien.
static PROGRAM: Mutex<Option<App>> = Mutex::new(None);

/// Les combinaisons en place, lues à l'ouverture de la session.
///
/// Lues et non gravées : elles se choisissent dans les réglages. Lues une
/// fois, parce que la carte prend la largeur de sa plus longue ligne et
/// que cette largeur est celle de sa fenêtre, laquelle ne change pas de
/// taille d'une session à l'autre. C'est le moment que la page choisit
/// elle aussi.
static KEYS: Mutex<Vec<(Doing, Option<String>)>> = Mutex::new(Vec::new());

// La toile de cette fenêtre, tenue par le fil qui la possède : une
// surface de dessin et la fenêtre qu'elle habille appartiennent au fil
// qui les a faites.
thread_local! {
    static CANVAS: std::cell::RefCell<Option<Canvas>> = const { std::cell::RefCell::new(None) };
}

/// Une longueur rangée dans un entier partagé, en centièmes de pixel : un
/// nombre à virgule ne s'y range pas, et le centième suffit à un écran
/// agrandi de cent soixante-quinze pour cent.
fn store(cell: &AtomicU32, how_many: f32) {
    cell.store((how_many * 100.0).round() as u32, Ordering::Relaxed);
}

fn load(cell: &AtomicU32) -> f32 {
    cell.load(Ordering::Relaxed) as f32 / 100.0
}

fn scale() -> f32 {
    load(&SCALE)
}

fn palette() -> Palette {
    design::palette(LIGHT.load(Ordering::Relaxed))
}

impl ReadingsBar {
    const fn empty() -> Self {
        ReadingsBar {
            figures: [String::new(), String::new(), String::new(), String::new()],
            read_at: [None; 4],
            stream: String::new(),
        }
    }

    /// Ce qu'une lecture du moteur donne à lire, la précédente à la main.
    ///
    /// La précédente parce qu'une mesure qui manque garde un moment ce
    /// qu'elle disait plutôt que de s'effacer ; voir `KEEP_FOR`.
    fn of(readings: &Measures, before: &ReadingsBar, now: Instant) -> Self {
        let mut figures: [String; 4] = std::array::from_fn(|_| String::new());
        let mut read_at = [None; 4];
        for (rank, reading) in READINGS.iter().enumerate() {
            if let Some(number) = (reading.read)(readings) {
                figures[rank] = format!("{number:.*} {}", reading.decimals, reading.unit);
                read_at[rank] = Some(now);
                continue;
            }
            match before.read_at[rank] {
                Some(when) if now.duration_since(when) < KEEP_FOR => {
                    figures[rank].clone_from(&before.figures[rank]);
                    read_at[rank] = Some(when);
                }
                _ => figures[rank] = NO_READING.to_string(),
            }
        }
        ReadingsBar {
            figures,
            read_at,
            stream: stream(readings),
        }
    }

    /// Si ce qui se lit a changé, les instants mis à part.
    fn reads_differently(&self, other: &ReadingsBar) -> bool {
        self.figures != other.figures || self.stream != other.stream
    }
}

impl Setting {
    /// Le nom sous lequel il voyage, des deux côtés.
    fn name(self) -> &'static str {
        match self {
            Setting::Size => "asked",
            Setting::Screen => "screen",
            Setting::Bitrate => "bitrate",
            Setting::Codec => "codec",
            Setting::Steady => "steady",
        }
    }

    /// Les valeurs proposées, dans l'ordre du produit.
    fn values(self, menu: &SessionMenu) -> Vec<String> {
        match self {
            Setting::Size => menu.sizes.iter().map(|size| size.value.clone()).collect(),
            Setting::Screen => menu
                .screens
                .iter()
                .map(|screen| screen.id.clone())
                .collect(),
            Setting::Bitrate => menu.rates.iter().map(u32::to_string).collect(),
            Setting::Codec => menu.codecs.clone(),
            // Deux mots et non une liste : c'est un interrupteur, et ses
            // deux côtés se nomment dans la fenêtre comme ceux d'à côté.
            Setting::Steady => vec!["off".to_string(), "on".to_string()],
        }
    }

    /// Ce qui s'écrit pour cette valeur, là où on la choisit.
    fn label(self, menu: &SessionMenu, value: &str) -> String {
        match self {
            Setting::Size => match value {
                "client" => "Résolution du client".to_string(),
                "host" => "Résolution de l'hôte".to_string(),
                _ => menu
                    .sizes
                    .iter()
                    .find(|size| size.value == value)
                    .map_or_else(|| value.to_string(), in_pixels),
            },
            Setting::Screen => menu
                .screens
                .iter()
                .find(|screen| screen.id == value)
                .map_or_else(
                    || value.to_string(),
                    |screen| {
                        if screen.main {
                            format!("{} (principal)", screen.name)
                        } else {
                            screen.name.clone()
                        }
                    },
                ),
            Setting::Bitrate => format!(
                "{} Mb/s",
                (value.parse::<f64>().unwrap_or(0.0) / 1000.0).round()
            ),
            Setting::Codec => {
                if value == "auto" {
                    "Automatique".to_string()
                } else {
                    value.to_string()
                }
            }
            Setting::Steady => if value == "on" { "Fluide" } else { "Économe" }.to_string(),
        }
    }

    /// Ce qui s'écrit à droite de la ligne du menu, quand la valeur en
    /// place ne s'y lit pas déjà.
    fn summary(self, menu: &SessionMenu) -> String {
        let current = self.current(menu);
        match self {
            // Ce à quoi le choix revient réellement ici : « client » ne
            // dit pas si on demande du 4K ou du 1080p, et c'est justement
            // ce qu'on veut savoir avant d'ouvrir la session.
            Setting::Size => {
                if current == "host" {
                    return "hôte".to_string();
                }
                let pixels = menu
                    .sizes
                    .iter()
                    .find(|size| size.value == current)
                    .map_or_else(|| current.clone(), in_pixels);
                if current == "client" {
                    format!("client, {pixels}")
                } else {
                    pixels
                }
            }
            // Le nom seul : « (principal) » y prendrait la place du nom
            // sans rien apprendre, la liste le disant déjà.
            Setting::Screen => menu
                .screens
                .iter()
                .find(|screen| screen.id == current)
                .map_or_else(String::new, |screen| screen.name.clone()),
            _ => self.label(menu, &current),
        }
    }

    /// Ce qui s'écrit en colonne de droite dans la liste.
    fn aside(self, menu: &SessionMenu, value: &str) -> String {
        match self {
            // Le rapport de la taille, dit comme les écrans se vendent :
            // deux nombres se comparent mal, et 21:9 à côté de 16:9 dit
            // tout de suite ce qui va être coupé. Rien pour les deux
            // premières : ce à quoi elles reviennent dépend de l'écran
            // qu'on a en face.
            Setting::Size if value != "client" && value != "host" => menu
                .sizes
                .iter()
                .find(|size| size.value == value)
                .filter(|size| size.width > 0)
                .map_or_else(String::new, |size| ratio(size.width, size.height)),
            // La taille de l'écran, comme le rapport l'est pour la
            // résolution : deux écrans se distinguent d'abord par là, et
            // un nom de modèle ne dit rien à qui ne l'a pas acheté.
            Setting::Screen => menu
                .screens
                .iter()
                .find(|screen| screen.id == value)
                .map_or_else(String::new, |screen| {
                    format!("{}x{}", screen.wide, screen.high)
                }),
            _ => String::new(),
        }
    }

    /// Où l'on en est.
    fn current(self, menu: &SessionMenu) -> String {
        match self {
            Setting::Size => menu.now.asked.clone(),
            Setting::Screen => menu.now.screen.clone(),
            Setting::Bitrate => menu.now.bitrate_kbps.to_string(),
            Setting::Codec => menu.now.codec.clone(),
            Setting::Steady => if menu.now.steady { "on" } else { "off" }.to_string(),
        }
    }

    /// Ce que la machine d'en face a dit ne pas savoir faire.
    ///
    /// Rien du tout veut dire qu'elle n'a rien dit, jamais qu'elle ne sait
    /// rien faire : hors session, ou pendant que son moteur démarre, la
    /// question n'a pas de réponse, et une question sans réponse doit
    /// laisser le menu exactement comme il était.
    fn out_of_reach(self, menu: &SessionMenu, value: &str) -> bool {
        self == Setting::Codec && menu.beyond_it.iter().any(|other| other == value)
    }
}

/// Une taille, en pixels.
fn in_pixels(size: &Offered) -> String {
    format!("{}x{}", size.width, size.height)
}

/// Le rapport d'une taille, réduit comme on le lit sur une fiche d'écran.
///
/// Calculé plutôt qu'écrit à côté de chaque nombre : une deuxième table
/// s'écarterait de la première le jour où une taille s'ajoute. Les deux
/// rapports que personne n'écrit sous leur forme réduite sont dits comme
/// tout le monde les dit.
fn ratio(width: u32, top: u32) -> String {
    fn gcd(a: u32, b: u32) -> u32 {
        if b == 0 { a } else { gcd(b, a % b) }
    }

    let divisor = gcd(width, top).max(1);
    match (width / divisor, top / divisor) {
        (8, 5) => "16:10".to_string(),
        (683, 384) => "16:9".to_string(),
        (x, y) => format!("{x}:{y}"),
    }
}

/// La ligne grise sous les chiffres : de quoi l'image est faite. Ce qui
/// manque ne laisse pas de trou, il ne s'écrit pas.
fn stream(said: &Measures) -> String {
    let mut pieces: Vec<String> = Vec::new();
    if let Some(codec) = &said.codec {
        pieces.push(codec.clone());
    }
    if let (Some(width), Some(height)) = (said.width, said.height) {
        pieces.push(format!("{width}x{height}"));
    }
    if let Some(frames) = said.fps {
        pieces.push(format!("{frames:.0} images/s"));
    }
    pieces.join(" · ")
}

impl Trailing {
    /// Ce qui s'écrit, une fois les raccourcis connus.
    fn text(&self) -> String {
        match self {
            Trailing::Text(label) => (*label).to_string(),
            Trailing::Key(doing, otherwise) => KEYS
                .lock()
                .expect("raccourcis du menu")
                .iter()
                .find(|(other, _)| other == doing)
                .and_then(|(_, said)| said.clone())
                .unwrap_or_else(|| (*otherwise).to_string()),
        }
    }
}

impl Line {
    /// La hauteur que cette ligne prend, en vrais pixels.
    fn height(&self, scale: f32) -> f32 {
        match self {
            Line::Measures => readings_height(scale),
            Line::Separator => (design::SPACE_2 * 2.0 + layout::HAIRLINE) * scale,
            // Mesurée au dessin, où se trouve de quoi mesurer du texte
            // replié, et relue ici comme la largeur de la carte l'est.
            Line::Refusal => load(&REFUSAL_HEIGHT),
            Line::Slider(_) => slider_height(scale),
            _ => layout::LINE * scale,
        }
    }

    /// Si cette ligne a lieu d'être en ce moment.
    ///
    /// Une machine d'en face qui n'a qu'un écran, ou dont le moteur n'a
    /// pas encore dit lesquels, ne laisse rien à choisir : la ligne
    /// s'efface plutôt que d'ouvrir une liste vide.
    fn is_visible(&self, menu: Option<&SessionMenu>) -> bool {
        // Sans refus à dire, la ligne n'est pas là du tout : elle ne doit
        // rien coûter les neuf cent quatre-vingt-dix-neuf fois où tout se
        // passe bien.
        if matches!(self, Line::Refusal) {
            return refusal_to_say().is_some();
        }
        let Some(menu) = menu else {
            // Sans réponse, la carte se réduit à ce qui ne dépend pas de
            // la session : mieux vaut une carte courte qu'une carte de
            // lignes vides.
            return !matches!(self, Line::Choice(_) | Line::Slider(_) | Line::List(_));
        };
        match self {
            Line::List(list) => !list.setting.values(menu).is_empty(),
            _ => true,
        }
    }
}

impl Toggle {
    /// Ce qui s'écrit sur ses deux côtés.
    fn words(&self) -> Vec<String> {
        self.sides
            .iter()
            .map(|label| (*label).to_string())
            .collect()
    }

    /// Lequel des deux est en place.
    fn current_side(&self) -> usize {
        usize::from(self.state.load(Ordering::Relaxed))
    }
}

/// Ce qui s'écrit sur les côtés d'une ligne à choix.
///
/// À part de la ligne pour qu'on puisse le demander avec les réglages
/// déjà en main : les redemander à ce moment-là reprendrait un verrou
/// qu'on tient.
fn words_of(menu: &SessionMenu, setting: Setting) -> Vec<String> {
    setting
        .values(menu)
        .iter()
        .map(|value| setting.label(menu, value))
        .collect()
}

impl Choice {
    /// Ce qui s'écrit sur ses côtés, tel que la session les propose.
    fn words(&self) -> Option<Vec<String>> {
        let session_menu = SESSION_MENU.lock().expect("réglages du menu");
        Some(words_of(session_menu.as_ref()?, self.setting))
    }

    /// Lequel est en place, et ceux que la machine d'en face ne sait pas
    /// faire.
    fn current(&self) -> Option<(usize, Vec<bool>)> {
        let session_menu = SESSION_MENU.lock().expect("réglages du menu");
        let menu = session_menu.as_ref()?;
        let values = self.setting.values(menu);
        let current = self.setting.current(menu);
        Some((
            values.iter().position(|value| *value == current)?,
            values
                .iter()
                .map(|value| self.setting.out_of_reach(menu, value))
                .collect(),
        ))
    }
}

impl Slider {
    /// Le cran où il en est : celui qu'une main tient, sinon celui qui est
    /// écrit.
    fn notch(&self) -> Option<(usize, usize)> {
        let session_menu = SESSION_MENU.lock().expect("réglages du menu");
        let menu = session_menu.as_ref()?;
        let values = self.setting.values(menu);
        if values.is_empty() {
            return None;
        }
        let current = self.setting.current(menu);
        let written = values
            .iter()
            .position(|value| *value == current)
            .unwrap_or(0);
        let pushed = *PUSHED.lock().expect("curseur du menu");
        Some((
            pushed.unwrap_or(written).min(values.len() - 1),
            values.len(),
        ))
    }

    /// Ce qui s'écrit à droite de son mot : ce qu'il vaut au cran où il
    /// est, y compris pendant qu'une main le pousse.
    fn value(&self) -> String {
        let session_menu = SESSION_MENU.lock().expect("réglages du menu");
        let Some(menu) = session_menu.as_ref() else {
            return String::new();
        };
        let values = self.setting.values(menu);
        match *PUSHED.lock().expect("curseur du menu") {
            Some(notch) if notch < values.len() => self.setting.label(menu, &values[notch]),
            _ => self.setting.summary(menu),
        }
    }
}

/// Ouvre la fenêtre de la carte, une fois par session.
///
/// Bâtie sur le fil qui dessine, comme celle du logo : une fenêtre
/// appartient au fil qui l'a faite, et une fenêtre faite sur le fil de la
/// veille n'entendrait jamais une souris.
pub fn raise(app: &App, scale: f32, light: bool) {
    if ITS_WINDOW.load(Ordering::Relaxed) != 0 {
        return;
    }
    let owner = crate::main_window::handle();
    *PROGRAM.lock().expect("programme du menu") = Some(app.clone());
    *KEYS.lock().expect("raccourcis du menu") = crate::shortcuts::engraved();
    // Quatre tirets avant la première lecture, et non quatre vides : la
    // barre est là dès la première ouverture, et ce qu'elle montre alors
    // est ce que le produit montre pour une mesure qui manque.
    *READINGS_BAR.lock().expect("mesures du menu") =
        ReadingsBar::of(&Measures::default(), &ReadingsBar::empty(), Instant::now());
    store(&SCALE, scale);
    LIGHT.store(light, Ordering::Relaxed);
    OPEN.store(false, Ordering::Relaxed);
    *PANEL.lock().expect("panneau du menu") = None;
    let _ = app.run_on_main_thread(move || build(owner));
    // Ce que la session propose, demandé une fois : les crans ne changent
    // pas d'un clic à l'autre. La fenêtre est bâtie sans attendre, parce
    // qu'une carte fermée n'a rien à montrer et que la réponse la
    // rattrapera avant la première ouverture.
    reread_the_session_menu(app);
}

/// Redemande ce que la session propose et où elle en est, et recommence
/// tant que la machine d'en face n'a pas dit ce qu'elle sait encoder.
///
/// Elle met quelques secondes à le dire : son moteur démarre, puis le
/// chemin se met à servir la session. Demandée une seule fois à
/// l'ouverture du bouton, la question tombait toujours avant, et le menu
/// s'ouvrait en proposant un codec que cette machine-là ne sait pas
/// faire ; il ne se reprenait qu'une fois la carte déjà sous les yeux, ce
/// qui se voit.
///
/// Un numéro de tour, comme pour les mesures : deux ouvertures rapprochées
/// ne laissent pas deux veilles derrière la même carte.
fn reread_the_session_menu(app: &App) {
    let app = app.clone();
    let round = SESSION_MENU_ROUND.fetch_add(1, Ordering::Relaxed) + 1;
    crate::app::spawn(async move {
        while SESSION_MENU_ROUND.load(Ordering::Relaxed) == round
            && ITS_WINDOW.load(Ordering::Relaxed) != 0
        {
            let read = crate::settings::session_menu(app.clone()).await;
            // Rien du tout veut dire qu'elle n'a rien dit, jamais qu'elle
            // ne sait rien faire : c'est donc là-dessus que la question se
            // repose, et nulle part ailleurs.
            let answered = !read.beyond_it.is_empty();
            let change = {
                let mut session_menu = SESSION_MENU.lock().expect("réglages du menu");
                let change = session_menu.as_ref() != Some(&read);
                *session_menu = Some(read);
                change
            };
            if change {
                redraw(&app);
            }
            if answered {
                return;
            }
            tokio::time::sleep(REFRESH).await;
        }
    });
}

/// Referme la carte et rend sa fenêtre avec la session.
pub fn lower(app: &App) {
    let window = ITS_WINDOW.swap(0, Ordering::Relaxed);
    if window == 0 {
        return;
    }
    OPEN.store(false, Ordering::Relaxed);
    // La veille des mesures ne se range pas d'elle-même : elle suit la
    // carte, et une carte ouverte à la fin d'une session ne se referme
    // pas, elle disparaît.
    follow_the_readings(app, false);
    *PROGRAM.lock().expect("programme du menu") = None;
    let _ = app.run_on_main_thread(move || {
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow;

        // SAFETY: une fenêtre à nous, défaite sur le fil qui l'a faite.
        unsafe { DestroyWindow(window as HWND) };
        CANVAS.with_borrow_mut(|canvas| *canvas = None);
    });
}

/// Montre la carte, ou la range.
pub fn show(is_open: bool) {
    if ITS_WINDOW.load(Ordering::Relaxed) == 0 || OPEN.swap(is_open, Ordering::Relaxed) == is_open {
        return;
    }
    // Une carte rangée ne garde rien de la main qui la lisait : rouverte,
    // elle montrerait une ligne allumée sous une souris posée ailleurs.
    *HOVER.lock().expect("survol du menu") = None;
    *PRESSED.lock().expect("appui du menu") = None;
    HAND_INSIDE.store(false, Ordering::Relaxed);
    let Some(app) = PROGRAM.lock().expect("programme du menu").clone() else {
        return;
    };
    // Un menu qu'on rouvre s'ouvre sur lui-même : rester dans une liste
    // choisie il y a deux sessions serait un menu qui a l'air d'un autre.
    *PANEL.lock().expect("panneau du menu") = None;
    *PUSHED.lock().expect("curseur du menu") = None;
    // Ce qui vit dans la carte ne vit que pendant qu'on la regarde. Les
    // interrupteurs et les réglages se relisent à chaque ouverture parce
    // qu'ils peuvent avoir bougé sans elle.
    follow_the_readings(&app, is_open);
    if is_open {
        let asked = app.clone();
        crate::app::spawn(async move { reread_the_toggles(&asked).await });
        reread_the_session_menu(&app);
    }
    let _ = app.run_on_main_thread(move || {
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::UI::WindowsAndMessaging::{SW_HIDE, SW_SHOWNOACTIVATE, ShowWindow};

        let window = ITS_WINDOW.load(Ordering::Relaxed) as HWND;
        if window.is_null() {
            return;
        }
        if is_open {
            repaint(window);
        }
        // SAFETY: une fenêtre à nous, montrée sans prendre le premier
        // plan.
        unsafe {
            ShowWindow(window, if is_open { SW_SHOWNOACTIVATE } else { SW_HIDE });
        }
    });
}

/// Dit si la carte est ouverte, pour qui a besoin de la basculer.
pub fn is_open() -> bool {
    OPEN.load(Ordering::Relaxed)
}

/// Ce que sa fenêtre prend de haut.
///
/// Pour le bouton, qui s'en sert à décider si le menu a la place de
/// s'ouvrir vers le bas.
pub fn height() -> i32 {
    HEIGHT.load(Ordering::Relaxed) as i32
}

/// Ce que sa fenêtre prend de large en tout, pour ce sens vertical-là.
///
/// À côté, elle compte aussi le bouton et l'espace qui l'en sépare :
/// c'est sa fenêtre entière qui se pose à côté de lui, jamais sa seule
/// carte, voir `lay`. Pour le bouton, qui s'en sert à décider de quel
/// bord il y a la place de la faire partir.
pub fn width(opens: Opens, logo: i32) -> i32 {
    let width = WIDTH.load(Ordering::Relaxed) as i32;
    match opens {
        Opens::Side => logo + (design::SPACE_2 * scale()).round() as i32 + width,
        _ => width,
    }
}

/// Pose la carte sous le logo, au-dessus, ou à côté, selon le sens que
/// le bouton a décidé ; et son bord droit ou son bord gauche, selon
/// celui qu'il a décidé avoir la place de porter la carte.
///
/// La même ancre que le logo, dans le même geste : les deux fenêtres ne
/// peuvent donc pas être en désaccord sur l'endroit où se trouve le
/// bouton.
pub fn lay(
    anchor: (i32, i32),
    opens: Opens,
    on_the_right: bool,
    logo: i32,
    picture: (i32, i32, i32, i32),
) {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
    };

    let window = ITS_WINDOW.load(Ordering::Relaxed);
    if window == 0 {
        return;
    }
    let (width, height) = (
        WIDTH.load(Ordering::Relaxed) as i32,
        HEIGHT.load(Ordering::Relaxed) as i32,
    );
    // La fenêtre est plus grande que la carte, de tout ce que l'ombre
    // déborde : c'est donc la **carte** qu'on pose, et la fenêtre autour
    // d'elle. Posée comme si les deux ne faisaient qu'une, la carte
    // tombait vingt pixels trop bas et vingt trop à gauche, ce qui se
    // voit au premier coup d'oeil à côté de l'ancien menu.
    let scale = scale();
    // La toile porte la carte dessinée pour le bord d'où elle est
    // partie, et rien ne la redessine d'elle-même : sans ceci, un bord
    // qui vient de changer déplaçait la fenêtre tout de suite, sur une
    // image encore posée pour l'ancien, ce qui se voyait le temps d'un
    // reflet avant le prochain dessin.
    let vertical_change =
        UPWARD.swap(opens == Opens::Up, Ordering::Relaxed) != (opens == Opens::Up);
    let horizontal_change = RIGHTWARD.swap(on_the_right, Ordering::Relaxed) != on_the_right;
    if (vertical_change || horizontal_change)
        && let Some(app) = PROGRAM.lock().expect("programme du menu").clone()
    {
        // Redemandé au fil qui possède la fenêtre : c'est lui qui tient
        // la toile, et ceci court sur celui qui suit la main.
        let _ = app.run_on_main_thread(move || repaint(window as HWND));
    }
    let overflow_px = shadow_overflow(scale).round() as i32;
    let card_height = height - overflow_px * 2;
    // Collée au même bord que le logo, et séparée de lui de l'espace
    // que la feuille de style met entre les deux : son bord droit
    // d'habitude, son bord gauche quand le premier n'a pas la place, ce
    // que le bouton a déjà décidé.
    let between = (design::SPACE_2 * scale).round() as i32;
    // Le coin que `SetWindowPos` reçoit plus bas prend encore un debord
    // de plus, pour une raison qui reste au-dessus de cette fonction :
    // posée telle quelle, la carte tombait vingt pixels trop à gauche.
    // Quand c'est la carte qui est collée à ce bord-là plutôt que
    // laissée au bord droit, elle porte elle-même un second debord (son
    // ombre à elle, `card` la posant à `overflow_px` et non à zéro), et les
    // deux s'ajoutent sans se répondre : sans le retirer ici deux fois,
    // le bord de la carte serait tombé deux debords après le bouton
    // plutôt qu'au même endroit que lui.
    let horizontal = if on_the_right {
        anchor.0 - logo - overflow_px * 2
    } else {
        anchor.0 - width
    };
    let (left, top) = match opens {
        Opens::Down => (horizontal, anchor.1 + logo + between - overflow_px),
        Opens::Up => (
            horizontal,
            anchor.1 - logo - between - card_height - overflow_px,
        ),
        // À côté, la carte part du haut du bouton et glisse de ce qu'il
        // faut pour tenir dans l'image : c'est toute sa raison d'être là
        // plutôt que dessous. Sa fenêtre entière et non sa seule carte,
        // le panneau d'une liste s'ouvrant dedans.
        Opens::Side => (
            if on_the_right {
                anchor.0 + between - overflow_px * 2
            } else {
                anchor.0 - logo - between - width
            },
            (anchor.1 - overflow_px).clamp(picture.1, (picture.3 - height).max(picture.1)),
        ),
    };
    // SAFETY: une fenêtre à nous, posée sans être activée ni
    // redimensionnée.
    unsafe {
        SetWindowPos(
            window as HWND,
            std::ptr::null_mut(),
            left + overflow_px,
            top,
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
    let Some(measure) = Canvas::new(1, 1) else {
        note("bouton flottant : le menu n'a pas pu être mesuré");
        return;
    };
    let scale = scale();
    // La hauteur d'une ligne de texte, demandée à la police une fois pour
    // toutes : tout ce qui est empilé dans cette carte s'appuie dessus.
    store(
        &CAPTION_HEIGHT,
        measure.line_height(Pen::of(design::CAPTION * scale)),
    );
    store(
        &BODY_HEIGHT,
        measure.line_height(Pen::of(design::BODY * scale)),
    );
    measure_the_card(&measure, scale);
    let (width, height) = size(&measure);
    WIDTH.store(width as u32, Ordering::Relaxed);
    HEIGHT.store(height as u32, Ordering::Relaxed);
    drop(measure);

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
            width,
            height,
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
        "bouton flottant : menu dessiné par ZyrDesk, {width}x{height} px au \
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
fn size(canvas: &Canvas) -> (i32, i32) {
    let scale = scale();
    let overflow_px = shadow_overflow(scale);
    let panel = panels_width(canvas, scale);
    let width = card_width(scale)
        + if panel > 0.0 {
            panel + design::SPACE_2 * scale
        } else {
            0.0
        };
    let height = content(scale).max(panels_height(scale));
    (
        (width + overflow_px * 2.0).ceil() as i32,
        (height + overflow_px * 2.0).ceil() as i32,
    )
}

/// Ce que la carte prend de large : sa ligne la plus longue.
///
/// Ce que la feuille de style demande depuis toujours et qu'aucun nombre
/// écrit à la main ne saurait tenir : un libellé rallongé couperait son
/// raccourci. Mesurée sur **toutes** ses lignes, y compris celles qui ne
/// se voient pas en ce moment : une carte qui rétrécit quand une ligne
/// s'en va est une carte qui change de largeur sous la main.
fn card_width(scale: f32) -> f32 {
    load(&CARD_WIDTH).max(design::SPACE_2 * 2.0 * scale)
}

/// La même, mesurée. Rangée ensuite, parce que la mise en page la
/// redemande à chaque image et que mesurer du texte coûte.
fn measure_the_card(canvas: &Canvas, scale: f32) {
    let session_menu = SESSION_MENU.lock().expect("réglages du menu");
    let mut width: f32 = 0.0;
    for line in &LINES {
        width = width.max(match line {
            Line::Measures => {
                (layout::READING * 4.0 + layout::BETWEEN_READINGS * 3.0 + design::SPACE_2 * 2.0)
                    * scale
            }
            // Replié sur la largeur que les autres lignes décident : un
            // refus est une phrase, et une carte large comme une phrase
            // serait une carte deux fois trop large pour tout le reste.
            Line::Separator | Line::Refusal => 0.0,
            Line::Entry(entry) => {
                let right =
                    canvas.width_of(&entry.trailing.text(), Pen::of(design::CAPTION * scale));
                around(canvas, entry.label, right, scale)
            }
            Line::Toggle(toggle) => around(
                canvas,
                toggle.label,
                sides_width(canvas, &toggle.words(), scale),
                scale,
            ),
            // Ses mots sont demandés avec les réglages déjà en main : les
            // redemander à la ligne reprendrait le verrou qu'on tient, ce
            // qui arrête le fil qui dessine pour de bon.
            Line::Choice(choice) => match session_menu.as_ref() {
                Some(menu) => around(
                    canvas,
                    choice.label,
                    sides_width(canvas, &words_of(menu, choice.setting), scale),
                    scale,
                ),
                None => 0.0,
            },
            // Sa barre prend toute la largeur, donc elle n'en demande
            // aucune : c'est sa tête qui décide, comme pour les autres.
            Line::Slider(slider) => {
                let value = session_menu
                    .as_ref()
                    .map_or_else(String::new, |menu| slider.setting.summary(menu));
                let right = canvas.width_of(&value, Pen::of(design::BODY * scale));
                around(canvas, slider.label, right, scale)
            }
            Line::List(list) => {
                let value = session_menu
                    .as_ref()
                    .map_or_else(String::new, |menu| list.setting.summary(menu));
                let right = canvas.width_of(&value, Pen::of(design::CAPTION * scale))
                    + (design::SPACE_2 + layout::BRAND) * scale;
                around(canvas, list.label, right, scale)
            }
        });
    }
    drop(session_menu);
    store(&CARD_WIDTH, width);
}

/// Ce qu'une ligne prend de large : son icône, son mot, ce qui vient à
/// droite, et tout ce qui les entoure.
///
/// La même mesure pour toutes les sortes de lignes, parce que c'est la
/// même mise en page : ce qui change est ce qu'il y a à droite.
fn around(canvas: &Canvas, label: &str, right: f32, scale: f32) -> f32 {
    canvas.width_of(label, Pen::of(design::BODY * scale))
        + right
        + (design::SPACE_2 * 2.0 + layout::ICON + design::SPACE_3 + layout::AFTER_THE_LABEL) * scale
}

/// Ce que les côtés d'une ligne à choix prennent de large, ensemble.
fn sides_width(canvas: &Canvas, words: &[String], scale: f32) -> f32 {
    words
        .iter()
        .map(|label| side_width(canvas, label, scale))
        .sum()
}

/// Et ce qu'un seul côté prend : son mot et ce qui l'entoure.
fn side_width(canvas: &Canvas, label: &str, scale: f32) -> f32 {
    canvas.width_of(label, Pen::of(design::CAPTION * scale)) + design::SPACE_3 * 2.0 * scale
}

/// Où tombent les côtés d'une ligne à choix, poussés au bord droit et
/// collés les uns aux autres.
///
/// Ils forment un seul objet, avec une bordure autour de tous et rien
/// entre eux.
fn sides_of(canvas: &Canvas, at: Rect, words: &[String], scale: f32) -> Vec<Rect> {
    let widths: Vec<f32> = words
        .iter()
        .map(|label| side_width(canvas, label, scale))
        .collect();
    let height = layout::TOGGLE * scale;
    let top = at.top + (at.bottom - at.top - height) / 2.0;
    let mut left = at.right - design::SPACE_2 * scale - widths.iter().sum::<f32>();
    widths
        .iter()
        .map(|width| {
            let place = Rect::at(left, top, *width, height);
            left += width;
            place
        })
        .collect()
}

/// La barre d'un curseur, sous la tête de sa ligne.
fn slider_bar(at: Rect, scale: f32) -> Rect {
    let edge = design::SPACE_2 * scale;
    let top = at.top
        + edge
        + load(&BODY_HEIGHT)
        + layout::UNDER_THE_LABEL * scale
        + (layout::SLIDER - layout::BAR) * scale / 2.0;
    Rect::at(
        at.left + edge,
        top,
        at.right - at.left - edge * 2.0,
        layout::BAR * scale,
    )
}

/// Les réglages qui ouvrent une liste, dans l'ordre de la carte.
///
/// Lus dans les lignes plutôt qu'écrits une seconde fois : ajouter une
/// liste au menu suffit alors à lui donner son panneau.
fn with_a_panel() -> impl Iterator<Item = Setting> {
    LINES.iter().filter_map(|line| match line {
        Line::List(list) => Some(list.setting),
        _ => None,
    })
}

/// Ce que le plus large des panneaux prend, ou rien quand aucun n'a de
/// quoi s'ouvrir.
///
/// Le plus large et non celui qui est ouvert : la fenêtre ne peut pas
/// changer de largeur au moment où l'on ouvre une liste sans que le
/// dessin qu'elle porte change de place au même instant.
fn panels_width(canvas: &Canvas, scale: f32) -> f32 {
    let session_menu = SESSION_MENU.lock().expect("réglages du menu");
    let Some(menu) = session_menu.as_ref() else {
        return 0.0;
    };
    with_a_panel()
        .map(|setting| panel_width(canvas, menu, setting, scale))
        .fold(0.0, f32::max)
}

/// Ce qu'un panneau prend de large : sa plus longue valeur.
fn panel_width(canvas: &Canvas, menu: &SessionMenu, setting: Setting, scale: f32) -> f32 {
    setting
        .values(menu)
        .iter()
        .map(|value| {
            let aside = canvas.width_of(
                &setting.aside(menu, value),
                Pen::of(design::CAPTION * scale),
            );
            around(canvas, &setting.label(menu, value), aside, scale)
        })
        .fold(0.0, f32::max)
}

/// La hauteur du plus haut des panneaux, pour la même raison.
fn panels_height(scale: f32) -> f32 {
    let session_menu = SESSION_MENU.lock().expect("réglages du menu");
    let Some(menu) = session_menu.as_ref() else {
        return 0.0;
    };
    with_a_panel()
        .map(|setting| panel_height(menu, setting, scale))
        .fold(0.0, f32::max)
}

/// Ce qu'un panneau prend de haut : ses valeurs, et rien d'autre.
///
/// Sans titre : on sait où l'on est, la ligne qui l'a ouvert est en face
/// et son chevron le dit. Une ligne de plus pour redire le mot d'à côté
/// serait une ligne de moins pour les valeurs.
fn panel_height(menu: &SessionMenu, setting: Setting, scale: f32) -> f32 {
    let how_many = setting.values(menu).len();
    if how_many == 0 {
        return 0.0;
    }
    (design::SPACE_2 * 2.0 + layout::LINE * how_many as f32) * scale
}

/// Le panneau ouvert dans sa fenêtre, du côté de la carte d'où elle
/// n'est pas partie : à sa gauche d'habitude, à sa droite quand elle
/// est elle-même collée au bord gauche de la fenêtre.
fn panel(canvas: &Canvas, setting: Setting, scale: f32) -> Option<Rect> {
    // La carte et la ligne d'abord, le verrou des réglages ensuite : les
    // mesurer demande ce même verrou, et un verrou repris pendant qu'on
    // le tient arrête le fil qui dessine pour de bon.
    let card = card(scale);
    let line = panel_line(setting, scale)?;
    let session_menu = SESSION_MENU.lock().expect("réglages du menu");
    let menu = session_menu.as_ref()?;
    let height = panel_height(menu, setting, scale);
    if height <= 0.0 {
        return None;
    }
    let width = panel_width(canvas, menu, setting, scale);
    // Ouvert en face de la ligne qui l'ouvre, sa première valeur sur
    // elle : un panneau de deux valeurs collé en haut de la carte
    // pendant qu'on clique une ligne du bas est un panneau qu'on cherche
    // des yeux. Il descend de ce qu'il faut pour tenir dans la fenêtre,
    // qui est bâtie assez haute pour le plus grand d'entre eux.
    let edge = design::SPACE_2 * scale;
    let inside = shadow_overflow(scale);
    let bottom = (HEIGHT.load(Ordering::Relaxed) as f32 - inside - height).max(inside);
    let left = if RIGHTWARD.load(Ordering::Relaxed) {
        card.right + edge
    } else {
        card.left - edge - width
    };
    Some(Rect::at(
        left,
        (line.top - edge).clamp(inside, bottom),
        width,
        height,
    ))
}

/// Où tombe la ligne qui ouvre ce panneau, quand elle se voit.
fn panel_line(setting: Setting, scale: f32) -> Option<Rect> {
    walk(scale)
        .into_iter()
        .find(|(_, line, _)| matches!(line, Line::List(list) if list.setting == setting))
        .map(|(_, _, place)| place)
}

/// La place de chacune des valeurs du panneau ouvert.
fn panel_walk(canvas: &Canvas, setting: Setting, scale: f32) -> Vec<Rect> {
    let Some(panel) = panel(canvas, setting, scale) else {
        return Vec::new();
    };
    let edge = design::SPACE_2 * scale;
    let how_many = SESSION_MENU
        .lock()
        .expect("réglages du menu")
        .as_ref()
        .map_or(0, |menu| setting.values(menu).len());
    let mut top = panel.top + edge;
    (0..how_many)
        .map(|_| {
            let place = Rect::at(
                panel.left + edge,
                top,
                panel.right - panel.left - edge * 2.0,
                layout::LINE * scale,
            );
            top = place.bottom;
            place
        })
        .collect()
}

/// De combien l'ombre sort de la carte, de chaque côté.
fn shadow_overflow(scale: f32) -> f32 {
    let shadow = palette().shadow_2;
    (shadow.soft + shadow.down.abs().max(shadow.across.abs())) * scale
}

/// La hauteur de la barre des mesures.
fn readings_height(scale: f32) -> f32 {
    design::SPACE_2 * scale
        + load(&CAPTION_HEIGHT)
        + layout::UNDER_THE_LABEL * scale
        + load(&BODY_HEIGHT)
        + design::SPACE_1 * scale
        + load(&CAPTION_HEIGHT)
        + design::SPACE_1 * scale
}

/// La hauteur d'une ligne à curseur : sa tête, puis la barre en dessous.
fn slider_height(scale: f32) -> f32 {
    (design::SPACE_2 + layout::UNDER_THE_LABEL + layout::SLIDER + design::SPACE_3) * scale
        + load(&BODY_HEIGHT)
}

/// La carte dans sa fenêtre.
///
/// Aussi haute que ce qu'elle montre, et pas plus. Des lignes vont et
/// viennent selon la session, et la fenêtre est bâtie une fois pour la
/// plus grande des cartes possibles : celle-ci est donc collée au bord
/// d'où le menu s'ouvre, qui est le seul que personne ne doit voir bouger.
fn card(scale: f32) -> Rect {
    let (width, height) = (
        WIDTH.load(Ordering::Relaxed) as f32,
        HEIGHT.load(Ordering::Relaxed) as f32,
    );
    let overflow_px = shadow_overflow(scale);
    let inside = height - overflow_px * 2.0;
    let show = content(scale).min(inside);
    let top = if UPWARD.load(Ordering::Relaxed) {
        overflow_px + inside - show
    } else {
        overflow_px
    };
    let left = if RIGHTWARD.load(Ordering::Relaxed) {
        overflow_px
    } else {
        width - overflow_px - card_width(scale)
    };
    Rect::at(left, top, card_width(scale), show)
}

/// La hauteur de ce que la carte montre en ce moment.
fn content(scale: f32) -> f32 {
    let session_menu = SESSION_MENU.lock().expect("réglages du menu");
    design::SPACE_2 * scale * 2.0
        + LINES
            .iter()
            .filter(|line| line.is_visible(session_menu.as_ref()))
            .map(|line| line.height(scale))
            .sum::<f32>()
}

/// Chaque ligne visible et la place qu'elle prend, du haut de la carte
/// vers le bas.
///
/// Lue par le dessin et par la souris, une seule fois écrite : une carte
/// dont les lignes sont dessinées à un endroit et cliquées à un autre est
/// une carte qui rend le mauvais menu.
fn walk(scale: f32) -> Vec<(usize, &'static Line, Rect)> {
    let card = card(scale);
    let edge = design::SPACE_2 * scale;
    let session_menu = SESSION_MENU.lock().expect("réglages du menu");
    let mut top = card.top + edge;
    let mut placed = Vec::with_capacity(LINES.len());
    for (rank, line) in LINES.iter().enumerate() {
        if !line.is_visible(session_menu.as_ref()) {
            continue;
        }
        let height = line.height(scale);
        placed.push((
            rank,
            line,
            Rect::at(
                card.left + edge,
                top,
                card.right - card.left - edge * 2.0,
                height,
            ),
        ));
        top += height;
    }
    placed
}

/// Ce qui est sous ce point de la fenêtre, quand c'est quelque chose
/// qu'on clique.
///
/// Ce qui est en morceaux, les côtés d'un interrupteur et les valeurs
/// d'un panneau, demande de savoir où ils tombent, donc de quoi mesurer
/// du texte : la toile de la fenêtre, celle-là même sur laquelle ils ont
/// été dessinés. Une souris qui viserait d'après une autre mesure que le
/// dessin viserait à côté.
fn under(point: (i32, i32)) -> Option<Target> {
    let (x, y) = (point.0 as f32, point.1 as f32);
    let scale = scale();
    let inside =
        |place: &Rect| x >= place.left && x < place.right && y >= place.top && y < place.bottom;

    if let Some(setting) = *PANEL.lock().expect("panneau du menu") {
        let in_the_panel = CANVAS.with_borrow(|canvas| {
            panel_walk(canvas.as_ref()?, setting, scale)
                .iter()
                .position(inside)
                .map(Target::Value)
        });
        if in_the_panel.is_some() {
            return in_the_panel;
        }
    }

    let (rank, line, place) = walk(scale)
        .into_iter()
        .find(|(_, _, place)| inside(place))?;
    match line {
        Line::Entry(_) | Line::List(_) => Some(Target::Line(rank)),
        Line::Toggle(toggle) => CANVAS.with_borrow(|canvas| {
            sides_of(canvas.as_ref()?, place, &toggle.words(), scale)
                .iter()
                .position(inside)
                .map(|side| Target::Side(rank, side))
        }),
        Line::Choice(choice) => CANVAS.with_borrow(|canvas| {
            let canvas = canvas.as_ref()?;
            sides_of(canvas, place, &choice.words()?, scale)
                .iter()
                .position(inside)
                .map(|side| Target::Side(rank, side))
        }),
        Line::Slider(_) => inside(&slider_bar(place, scale).grown(
            // La barre fait quatre pixels de haut : viser quatre pixels
            // avec une souris est un travail, et personne n'a demandé un
            // travail. Ce qu'on attrape est la hauteur du pouce.
            (layout::THUMB - layout::BAR) * scale / 2.0,
        ))
        .then_some(Target::Bar(rank)),
        Line::Measures | Line::Separator | Line::Refusal => None,
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

    let scale = scale();
    let colours = palette();
    let radius = design::RADIUS * scale;
    let hover = *HOVER.lock().expect("survol du menu");
    let is_open = *PANEL.lock().expect("panneau du menu");

    CANVAS.with_borrow_mut(|canvas| {
        // Ce qu'il faut de place, mesuré sur la toile qui est là : mesurer
        // du texte ne demande pas la bonne taille de toile, seulement une
        // toile.
        if canvas.is_none() {
            *canvas = Canvas::new(1, 1);
        }
        let Some(measure) = canvas.as_ref() else {
            return;
        };
        measure_the_card(measure, scale);
        let (width, height) = size(measure);
        if width <= 0 || height <= 0 {
            return;
        }
        WIDTH.store(width as u32, Ordering::Relaxed);
        HEIGHT.store(height as u32, Ordering::Relaxed);
        // Refaite dès qu'elle n'est plus à la bonne taille, ce qui est
        // aussi le cas de celle d'un pixel qui vient de servir à mesurer.
        if measure.size() != (width, height) {
            *canvas = Canvas::new(width, height);
        }
        let Some(canvas) = canvas.as_ref() else {
            return;
        };

        let card = card(scale);
        canvas.begin(Colour::TRANSPARENT);
        canvas.shadow(card, radius, colours.shadow_2, scale);
        canvas.fill(card, radius, colours.surface_1);
        canvas.stroke_inside(
            card,
            radius,
            layout::HAIRLINE * scale,
            colours.border_strong,
        );

        let painter = Painter {
            canvas,
            scale,
            colours,
        };
        for (rank, line, at) in walk(scale) {
            let under_the_hand = hover.filter(|target| target.line() == Some(rank));
            let side = match under_the_hand {
                Some(Target::Side(_, side)) => Some(side),
                _ => None,
            };
            match line {
                Line::Measures => painter.measures(at),
                Line::Separator => painter.separator(at),
                Line::Refusal => painter.refusal(at),
                Line::Entry(entry) => painter.entry(at, entry, under_the_hand.is_some()),
                Line::Toggle(toggle) => painter.sides(
                    at,
                    &Sides {
                        icon: toggle.icon,
                        label: toggle.label,
                        words: &toggle.words(),
                        current_side: toggle.current_side(),
                        struck: &[],
                    },
                    side,
                ),
                Line::Choice(choice) => {
                    if let (Some(words), Some((current_side, struck))) =
                        (choice.words(), choice.current())
                    {
                        painter.sides(
                            at,
                            &Sides {
                                icon: choice.icon,
                                label: choice.label,
                                words: &words,
                                current_side,
                                struck: &struck,
                            },
                            side,
                        );
                    }
                }
                Line::Slider(slider) => painter.slider(at, slider),
                Line::List(list) => painter.list(at, list, under_the_hand.is_some()),
            }
        }

        if let Some(setting) = is_open {
            painter.panel(setting, hover);
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
        // Accrochée par le bord d'où le menu s'ouvre, et par celui d'où
        // il part verticalement : ce sont les deux seuls que personne ne
        // doit voir bouger quand la fenêtre change de taille. Ce sont
        // aussi ceux que `lay` calcule, donc les deux tombent d'accord
        // d'eux-mêmes.
        let x = if RIGHTWARD.load(Ordering::Relaxed) {
            place.left
        } else {
            place.right - width
        };
        let y = if UPWARD.load(Ordering::Relaxed) {
            place.bottom - height
        } else {
            place.top
        };
        canvas.lay_on(window as isize, x, y);
    });
}

/// Ce qui ne change pas pendant qu'une carte se dessine : de quoi
/// dessiner, de combien un pixel de page compte, et le thème.
///
/// Porté ensemble plutôt que passé trois fois à chaque ligne, et le menu
/// en a maintenant sept sortes.
struct Painter<'a> {
    canvas: &'a Canvas,
    scale: f32,
    colours: Palette,
}

/// Ce qu'une ligne à côtés montre : sa tête, ses mots, celui qui est en
/// place, et ceux que la machine d'en face ne sait pas faire.
///
/// Porté ensemble parce que ça se dessine ensemble, et qu'un interrupteur
/// et une ligne à boutons ne s'en décrivent pas autrement.
struct Sides<'a> {
    icon: &'a Icon,
    label: &'a str,
    words: &'a [String],
    current_side: usize,
    struck: &'a [bool],
}

impl Painter<'_> {
    /// Le début d'une ligne, qui est le même pour toutes : son icône à sa
    /// place, et son mot après.
    fn head(&self, at: Rect, icon: &Icon, label: &str, ink: Colour) {
        let (canvas, scale) = (self.canvas, self.scale);
        let side = layout::ICON * scale;
        canvas.icon(
            icon,
            Rect::at(
                at.left + design::SPACE_2 * scale,
                at.top + (at.bottom - at.top - side) / 2.0,
                side,
                side,
            ),
            ink,
        );
        canvas.draw_text(
            label,
            Pen::of(design::BODY * scale),
            ink,
            Rect {
                left: at.left + (design::SPACE_2 + layout::ICON + design::SPACE_3) * scale,
                ..at
            },
        );
    }

    /// Le fond qu'une ligne prend sous la main.
    fn hover(&self, at: Rect, tint: Option<Colour>) {
        if let Some(tint) = tint {
            self.canvas
                .fill(at, design::RADIUS_SMALL * self.scale, tint);
        }
    }

    /// Ce qui s'écrit à droite d'une ligne, dans la couleur des choses
    /// qu'on lit sans les chercher.
    fn on_the_right(&self, at: Rect, label: &str, size: f32, ink: Colour) {
        if label.is_empty() {
            return;
        }
        self.canvas.draw_text(
            label,
            Pen::of(size).aligned(Align::Right),
            ink,
            Rect {
                right: at.right - design::SPACE_2 * self.scale,
                ..at
            },
        );
    }

    /// Une entrée : son icône, son mot, ce qui est écrit à sa droite, et
    /// le fond que le survol lui met.
    fn entry(&self, at: Rect, entry: &Entry, under_the_hand: bool) {
        let colours = self.colours;
        let ink = if entry.destructive {
            colours.error
        } else {
            colours.text
        };
        // La ligne qui coupe la session s'allume de sa propre couleur
        // plutôt que du gris des autres : ce n'est pas un survol de plus,
        // c'est celui dont il faut se méfier.
        self.hover(
            at,
            under_the_hand.then(|| {
                if entry.destructive {
                    colours.error.faded(VEIL)
                } else {
                    colours.surface_3
                }
            }),
        );
        self.head(at, entry.icon, entry.label, ink);
        self.on_the_right(
            at,
            &entry.trailing.text(),
            design::CAPTION * self.scale,
            colours.text_faint,
        );
    }

    /// Une ligne qui ouvre une liste : sa valeur en place, puis le chevron
    /// qui dit qu'elle mène ailleurs.
    fn list(&self, at: Rect, list: &List, under_the_hand: bool) {
        let (canvas, scale, colours) = (self.canvas, self.scale, self.colours);
        self.hover(at, under_the_hand.then_some(colours.surface_3));
        self.head(at, list.icon, list.label, colours.text);

        let brand = layout::BRAND * scale;
        let edge = design::SPACE_2 * scale;
        let open = *PANEL.lock().expect("panneau du menu") == Some(list.setting);
        canvas.icon(
            // Le chevron dit dans quel sens la liste s'ouvre, donc il se
            // retourne quand elle est ouverte : elle paraît à gauche
            // d'habitude, il pointe vers elle ; à droite quand la carte
            // est elle-même collée au bord gauche de la fenêtre, il
            // pointe vers elle en pointant tout simplement où il pointait
            // déjà, fermée.
            if open && !RIGHTWARD.load(Ordering::Relaxed) {
                &icons::BACK
            } else {
                &icons::CHEVRON
            },
            Rect::at(
                at.right - edge - brand,
                at.top + (at.bottom - at.top - brand) / 2.0,
                brand,
                brand,
            ),
            colours.text_faint,
        );
        let value = SESSION_MENU
            .lock()
            .expect("réglages du menu")
            .as_ref()
            .map_or_else(String::new, |menu| list.setting.summary(menu));
        self.on_the_right(
            Rect {
                right: at.right - brand - edge,
                ..at
            },
            &value,
            design::CAPTION * scale,
            colours.text_faint,
        );
    }

    /// Une ligne à côtés : un interrupteur ou une suite de boutons, dont
    /// un seul est plein.
    ///
    /// Les deux se dessinent ici parce qu'ils se dessinent pareil. Ce qui
    /// les sépare est ce qu'ils font, pas ce qu'ils montrent : l'un
    /// bascule la session tout de suite, l'autre écrit un choix que la
    /// session prend là où elle est.
    fn sides(&self, at: Rect, spec: &Sides, under_the_hand: Option<usize>) {
        let (canvas, scale, colours) = (self.canvas, self.scale, self.colours);
        let words = spec.words;
        self.head(at, spec.icon, spec.label, colours.text);

        let sides = sides_of(canvas, at, words, scale);
        let Some(whole) = sides.first().map(|first| Rect {
            left: first.left,
            ..*sides.last().unwrap_or(first)
        }) else {
            return;
        };
        let radius = design::RADIUS_SMALL * scale;
        for (rank, place) in sides.iter().enumerate() {
            let bar = spec.struck.get(rank).copied().unwrap_or(false);
            let (background, ink) = if rank == spec.current_side {
                (Some(colours.accent_bright), colours.on_accent)
            } else if bar {
                (None, colours.text_faint)
            } else if under_the_hand == Some(rank) {
                (Some(colours.surface_3), colours.text)
            } else {
                (None, colours.text_faint)
            };
            if let Some(background) = background {
                // Le fond de l'objet entier, vu au travers de ce côté-là :
                // les côtés n'en forment qu'un, arrondi par dehors et droit
                // là où ils se touchent, ce qu'aucun rectangle arrondi ne
                // sait être à lui seul.
                canvas.clipped(*place, || canvas.fill(whole, radius, background));
            }
            canvas.draw_text(
                &words[rank],
                Pen::of(design::CAPTION * scale).aligned(Align::Centre),
                ink,
                *place,
            );
            if bar {
                // Ce que la machine d'en face ne sait pas faire garde sa
                // place : une possibilité qui disparaît d'un ordinateur à
                // l'autre laisse croire à un menu qui change d'avis, là où
                // c'est la machine regardée qui n'a pas la même carte
                // graphique. Barré, donc, et non effacé.
                let middle = (place.top + place.bottom) / 2.0;
                let half_label =
                    canvas.width_of(&words[rank], Pen::of(design::CAPTION * scale)) / 2.0;
                let centre = (place.left + place.right) / 2.0;
                canvas.fill(
                    Rect::at(
                        centre - half_label,
                        middle,
                        half_label * 2.0,
                        layout::HAIRLINE * scale,
                    ),
                    0.0,
                    colours.text_faint,
                );
            }
        }
        canvas.stroke_inside(whole, radius, layout::HAIRLINE * scale, colours.border);
    }

    /// Une ligne à curseur : sa tête, sa valeur, et la barre en dessous.
    fn slider(&self, at: Rect, slider: &Slider) {
        let (canvas, scale, colours) = (self.canvas, self.scale, self.colours);
        // Sa tête tient sur la hauteur d'une ligne de corps, la barre
        // prenant le reste.
        let head = Rect {
            bottom: at.top + design::SPACE_2 * scale * 2.0 + load(&BODY_HEIGHT),
            ..at
        };
        self.head(head, slider.icon, slider.label, colours.text);
        // La valeur d'un réglage se lit là où se lisent les raccourcis,
        // mais elle n'en est pas un : c'est ce que la ligne vaut, donc elle
        // se lit comme le reste de la ligne et non en retrait.
        self.on_the_right(head, &slider.value(), design::BODY * scale, colours.text);

        let Some((notch, how_many)) = slider.notch() else {
            return;
        };
        let bar = slider_bar(at, scale);
        let radius = layout::BAR * scale / 2.0;
        canvas.fill(bar, radius, colours.border);
        let part = if how_many > 1 {
            notch as f32 / (how_many - 1) as f32
        } else {
            0.0
        };
        let thumb = layout::THUMB * scale;
        // Le pouce reste entier dans la barre à ses deux bouts : posé sur
        // sa seule part, il déborderait de la moitié de lui-même.
        let thumb_x = bar.left + thumb / 2.0 + (bar.right - bar.left - thumb) * part;
        let middle = (bar.top + bar.bottom) / 2.0;
        canvas.fill(
            Rect::at(bar.left, bar.top, thumb_x - bar.left, radius * 2.0),
            radius,
            colours.accent_bright,
        );
        canvas.fill(
            Rect::at(thumb_x - thumb / 2.0, middle - thumb / 2.0, thumb, thumb),
            thumb / 2.0,
            colours.accent_bright,
        );
    }

    /// Le trait entre deux groupes, au milieu de la place qu'il prend.
    ///
    /// Rentré d'un pas de chaque côté, comme la feuille de style le
    /// demande : un trait qui va d'un bord à l'autre coupe la carte en
    /// deux au lieu de séparer deux groupes de lignes.
    fn separator(&self, at: Rect) {
        let edge = design::SPACE_2 * self.scale;
        self.canvas.fill(
            Rect::at(
                at.left + edge,
                at.top + edge,
                at.right - at.left - edge * 2.0,
                layout::HAIRLINE * self.scale,
            ),
            0.0,
            self.colours.border,
        );
    }

    /// Ce que le menu vient de refuser de faire, écrit en toutes lettres.
    ///
    /// Replié sur la largeur de la carte : ce qu'un refus a à dire est ce
    /// qu'il faut faire ailleurs, et abréger cela reviendrait à ne rien
    /// dire du tout.
    fn refusal(&self, at: Rect) {
        let Some(said) = refusal_to_say() else {
            return;
        };
        let edge = design::SPACE_2 * self.scale;
        let width = at.right - at.left - edge * 2.0;
        let pen = Pen::of(design::CAPTION * self.scale);
        // Mesuré ici parce qu'ici est le seul endroit qui sache mesurer du
        // texte replié, et rangé pour que la carte s'ouvre dessus, comme
        // sa largeur l'est déjà.
        let height = self.canvas.height_of(&said, pen, width);
        store(&REFUSAL_HEIGHT, height + edge * 2.0);
        self.canvas.draw_text(
            &said,
            pen,
            self.colours.warning,
            Rect::at(at.left + edge, at.top + edge, width, height),
        );
    }

    /// La barre des quatre mesures : un mot par-dessus un nombre, quatre
    /// fois, et la phrase du flux en dessous.
    fn measures(&self, at: Rect) {
        let (canvas, scale, colours) = (self.canvas, self.scale, self.colours);
        let edge = design::SPACE_2 * scale;
        let top = at.top + edge;
        let bar = READINGS_BAR.lock().expect("mesures du menu");
        for (rank, reading) in READINGS.iter().enumerate() {
            let left =
                at.left + edge + rank as f32 * (layout::READING + layout::BETWEEN_READINGS) * scale;
            let column = layout::READING * scale;
            canvas.draw_text(
                reading.label,
                Pen::of(design::CAPTION * scale),
                colours.text_faint,
                Rect::at(left, top, column, load(&CAPTION_HEIGHT)),
            );
            canvas.draw_text(
                &bar.figures[rank],
                Pen::of(design::BODY * scale),
                colours.text,
                Rect::at(
                    left,
                    top + load(&CAPTION_HEIGHT) + layout::UNDER_THE_LABEL * scale,
                    column,
                    load(&BODY_HEIGHT),
                ),
            );
        }
        if !bar.stream.is_empty() {
            canvas.draw_text(
                &bar.stream,
                Pen::of(design::CAPTION * scale),
                colours.text_faint,
                Rect::at(
                    at.left + edge,
                    top + load(&CAPTION_HEIGHT)
                        + layout::UNDER_THE_LABEL * scale
                        + load(&BODY_HEIGHT)
                        + design::SPACE_1 * scale,
                    at.right - at.left - edge * 2.0,
                    load(&CAPTION_HEIGHT),
                ),
            );
        }
    }

    /// Le panneau d'un réglage, du côté de la carte d'où elle n'est pas
    /// partie : ses valeurs, dont une porte la marque.
    ///
    /// Sans titre. On sait où l'on est : la ligne qui l'a ouvert est en
    /// face, son chevron s'est retourné vers lui, et la cliquer à nouveau
    /// referme. Un titre qui redit le mot d'à côté prend une ligne pour
    /// n'apprendre rien.
    fn panel(&self, setting: Setting, hover: Option<Target>) {
        let (canvas, scale, colours) = (self.canvas, self.scale, self.colours);
        let Some(place) = panel(canvas, setting, scale) else {
            return;
        };
        let radius = design::RADIUS * scale;
        canvas.shadow(place, radius, colours.shadow_2, scale);
        canvas.fill(place, radius, colours.surface_1);
        canvas.stroke_inside(
            place,
            radius,
            layout::HAIRLINE * scale,
            colours.border_strong,
        );

        let values = panel_walk(canvas, setting, scale);
        let side = layout::BRAND * scale;
        let session_menu = SESSION_MENU.lock().expect("réglages du menu");
        let Some(menu) = session_menu.as_ref() else {
            return;
        };
        let values_here = setting.values(menu);
        let at = setting.current(menu);
        for (rank, place) in values.iter().enumerate() {
            let Some(value) = values_here.get(rank) else {
                break;
            };
            self.hover(
                *place,
                (hover == Some(Target::Value(rank))).then_some(colours.surface_3),
            );
            if *value == at {
                canvas.icon(
                    &icons::TICK,
                    Rect::at(
                        place.left + design::SPACE_2 * scale,
                        place.top + (place.bottom - place.top - side) / 2.0,
                        side,
                        side,
                    ),
                    colours.accent_bright,
                );
            }
            canvas.draw_text(
                &setting.label(menu, value),
                Pen::of(design::BODY * scale),
                colours.text,
                Rect {
                    left: place.left + (design::SPACE_2 + layout::ICON + design::SPACE_3) * scale,
                    ..*place
                },
            );
            self.on_the_right(
                *place,
                &setting.aside(menu, value),
                design::CAPTION * scale,
                colours.text_faint,
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
            if !HAND_INSIDE.swap(true, Ordering::Relaxed) {
                // Demandé dès qu'une main arrive : sans ça rien ne dit
                // jamais qu'elle est repartie, et la dernière ligne
                // survolée resterait allumée sous une souris qui n'est
                // plus là.
                let mut tracking = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: window,
                    dwHoverTime: 0,
                };
                // SAFETY: une fenêtre à nous, et la demande est à nous.
                unsafe { TrackMouseEvent(&mut tracking) };
            }
            if pushes(window, point(with)) {
                return 0;
            }
            hovers(window, under(point(with)));
            0
        }
        WM_MOUSELEAVE => {
            HAND_INSIDE.store(false, Ordering::Relaxed);
            hovers(window, None);
            0
        }
        WM_SETCURSOR if (with as u32 & 0xFFFF) == HTCLIENT => {
            // La ligne est demandée au système plutôt que reprise du
            // dernier survol : le curseur se décide avant que le
            // mouvement soit annoncé, et la main serait alors en retard
            // d'un geste.
            let cursor = if under_the_mouse(window).is_some() {
                IDC_HAND
            } else {
                IDC_ARROW
            };
            // SAFETY: un curseur du système, demandé par son nom.
            unsafe { SetCursor(LoadCursorW(std::ptr::null_mut(), cursor)) };
            1
        }
        WM_LBUTTONDOWN => {
            let target = under(point(with));
            *PRESSED.lock().expect("appui du menu") = target;
            // Un curseur se prend et se pousse : le geste commence ici et
            // ne finit qu'au relâchement, où seul le cran d'arrivée est
            // écrit.
            if matches!(target, Some(Target::Bar(_))) {
                pushes(window, point(with));
            }
            0
        }
        // Au relâchement, et là où l'appui a commencé : c'est ce qu'un
        // clic veut dire, et c'est ce qui laisse repartir d'un bouton
        // qu'on n'aurait pas dû viser.
        WM_LBUTTONUP => {
            let pressed = PRESSED.lock().expect("appui du menu").take();
            if let Some(Target::Bar(rank)) = pressed {
                released(window, rank);
                return 0;
            }
            if let Some(target) = under(point(with))
                && Some(target) == pressed
            {
                acts(target);
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
fn under_the_mouse(window: windows_sys::Win32::Foundation::HWND) -> Option<Target> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

    let mut at = POINT { x: 0, y: 0 };
    // SAFETY: un point à nous, et une fenêtre à nous dans laquelle il est
    // ramené.
    let read = unsafe { GetCursorPos(&mut at) != 0 && ScreenToClient(window, &mut at) != 0 };
    if !read {
        return None;
    }
    under((at.x, at.y))
}

/// Allume ce qui est sous la souris, et redessine quand ce n'est plus la
/// même chose.
fn hovers(window: windows_sys::Win32::Foundation::HWND, target: Option<Target>) {
    let mut hover = HOVER.lock().expect("survol du menu");
    if *hover == target {
        return;
    }
    *hover = target;
    drop(hover);
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
fn acts(target: Target) {
    let Some(app) = PROGRAM.lock().expect("programme du menu").clone() else {
        return;
    };
    match (target, target.line().and_then(|rank| LINES.get(rank))) {
        (Target::Line(_), Some(Line::Entry(entry))) => {
            say_the_click(entry.label);
            let does = entry.does;
            // Refermée avant que ce soit parti, comme la page le fait : ce
            // qui suit prend le temps qu'il prend, et une carte laissée
            // ouverte par-dessus serait une nappe posée sur l'image.
            show(false);
            crate::app::spawn(async move {
                let refusal = match does {
                    Does::Session(session_act) => crate::floating::ask(&app, session_act).await,
                    Does::PutAway => crate::floating::hide(&app),
                };
                say_the_refusal(refusal);
            });
        }
        (Target::Line(_), Some(Line::List(list))) => {
            // La même ligne ouvre et referme : une liste ouverte à côté du
            // menu se referme là où on l'a ouverte, et pas seulement par
            // son titre.
            let mut panel = PANEL.lock().expect("panneau du menu");
            *panel = (*panel != Some(list.setting)).then_some(list.setting);
            drop(panel);
            redraw(&app);
        }
        (Target::Side(_, side), Some(Line::Toggle(toggle))) => {
            // Pousser un interrupteur du côté où il est déjà ne fait rien,
            // comme tout interrupteur.
            if toggle.current_side() == side {
                return;
            }
            note(&format!(
                "menu du bouton flottant : « {} » mis sur « {} »",
                toggle.label, toggle.sides[side]
            ));
            // La carte reste ouverte : on regarde l'image après avoir
            // basculé, et la rouvrir pour la ligne d'à côté ferait deux
            // gestes pour un réglage.
            let act = toggle.act;
            crate::app::spawn(async move {
                match crate::floating::ask(&app, act).await {
                    // Relu plutôt que supposé : c'est la seule façon de
                    // montrer où l'on en est vraiment, et le son se lit
                    // dans le mélangeur de Windows et non ici.
                    Ok(()) => reread_the_toggles(&app).await,
                    Err(refusal) => say_the_refusal(Err(refusal)),
                }
            });
        }
        (Target::Side(_, side), Some(Line::Choice(choice))) => {
            let Some(value) = value_of(choice.setting, side) else {
                return;
            };
            // Ce que la machine d'en face ne sait pas faire n'est pas un
            // choix : le proposer barré dit pourquoi, le laisser cliquer
            // dirait le contraire.
            let refuse = SESSION_MENU
                .lock()
                .expect("réglages du menu")
                .as_ref()
                .is_some_and(|menu| choice.setting.out_of_reach(menu, &value));
            if refuse {
                return;
            }
            choose(&app, choice.setting, value);
        }
        (Target::Value(rank), _) => {
            let Some(setting) = *PANEL.lock().expect("panneau du menu") else {
                return;
            };
            let Some(value) = value_of(setting, rank) else {
                return;
            };
            // La liste se referme sur le choix : rester dedans après avoir
            // choisi laisserait croire qu'il reste quelque chose à y faire.
            *PANEL.lock().expect("panneau du menu") = None;
            // Et la carte avec elle : ce qui est choisi dans une liste se
            // voit tout de suite, ce qu'on veut regarder alors est
            // l'image, et une carte laissée par-dessus serait une nappe
            // posée dessus.
            show(false);
            choose(&app, setting, value);
        }
        _ => {}
    }
}

/// La valeur d'un réglage à ce rang-là.
fn value_of(setting: Setting, rank: usize) -> Option<String> {
    SESSION_MENU
        .lock()
        .expect("réglages du menu")
        .as_ref()
        .and_then(|menu| setting.values(menu).get(rank).cloned())
}

/// Écrit ce choix, le donne à la session là où elle est, et relit ce que
/// la session en dit.
///
/// Relu et non supposé : choisir une taille change ce que « client » vaut,
/// et c'est la réponse qui le porte.
fn choose(app: &App, setting: Setting, value: String) {
    note(&format!(
        "menu du bouton flottant : {} mis sur « {value} »",
        setting.name()
    ));
    let app = app.clone();
    crate::app::spawn(async move {
        match crate::settings::choose_session(app.clone(), setting.name().to_string(), value).await
        {
            Ok(choice) => {
                if let Some(menu) = SESSION_MENU.lock().expect("réglages du menu").as_mut() {
                    menu.now = choice;
                }
                redraw(&app);
            }
            Err(refusal) => say_the_refusal(Err(refusal)),
        }
    });
}

/// Dit qu'une ligne a été cliquée.
///
/// Dit avant que ce soit parti, et pas seulement quand ça refuse. Ce menu
/// est derrière l'image et ses lignes sont rares : sans cette ligne, une
/// entrée qui semble ne rien faire ne se distingue pas d'un clic qui n'est
/// jamais arrivé, et les deux se réparent ailleurs.
fn say_the_click(label: &str) {
    note(&format!("menu du bouton flottant : « {label} » cliqué"));
}

/// Et dit un refus, s'il y en a un.
///
/// Sur la carte et dans le journal. Sur la carte parce que c'est là que
/// regarde la personne qui vient de cliquer, et dans le journal parce que
/// la carte se referme et qu'une phrase lue une fois ne se retrouve plus.
fn say_the_refusal(refusal: Result<(), String>) {
    let Err(refusal) = refusal else {
        return;
    };
    note(&format!("menu du bouton flottant : {refusal}"));
    *REFUSAL.lock().expect("refus du menu") = Some((refusal, Instant::now()));
    if let Some(app) = PROGRAM.lock().expect("programme du menu").clone() {
        redraw(&app);
    }
}

/// Pousse le curseur là où la main est, et dit si elle en tenait un.
///
/// Rien n'est écrit tant qu'elle le tient : un curseur poussé d'un bout à
/// l'autre traverse tous ses crans, et chacun serait un aller-retour
/// jusqu'au service pour un débit que personne n'a voulu.
fn pushes(window: windows_sys::Win32::Foundation::HWND, at: (i32, i32)) -> bool {
    let Some(Target::Bar(rank)) = *PRESSED.lock().expect("appui du menu") else {
        return false;
    };
    let Some(Line::Slider(slider)) = LINES.get(rank) else {
        return false;
    };
    let Some((_, how_many)) = slider.notch() else {
        return false;
    };
    let scale = scale();
    let Some((_, _, place)) = walk(scale).into_iter().find(|(other, _, _)| *other == rank) else {
        return false;
    };
    let bar = slider_bar(place, scale);
    let thumb = layout::THUMB * scale;
    // Le pouce ne va pas d'un bord à l'autre mais d'un centre à l'autre :
    // compté sur la barre entière, les deux crans du bout ne se
    // laisseraient pas atteindre.
    let travel = (bar.right - bar.left - thumb).max(1.0);
    let part = ((at.0 as f32 - bar.left - thumb / 2.0) / travel).clamp(0.0, 1.0);
    let notch = (part * (how_many.max(1) - 1) as f32).round() as usize;
    let mut pushed = PUSHED.lock().expect("curseur du menu");
    if *pushed != Some(notch) {
        *pushed = Some(notch);
        drop(pushed);
        repaint(window);
    }
    true
}

/// Lâche le curseur, et écrit le cran où il a été laissé.
fn released(window: windows_sys::Win32::Foundation::HWND, rank: usize) {
    let Some(notch) = PUSHED.lock().expect("curseur du menu").take() else {
        return;
    };
    repaint(window);
    let Some(Line::Slider(slider)) = LINES.get(rank) else {
        return;
    };
    let Some(value) = value_of(slider.setting, notch) else {
        return;
    };
    let already = SESSION_MENU
        .lock()
        .expect("réglages du menu")
        .as_ref()
        .is_some_and(|menu| slider.setting.current(menu) == value);
    if already {
        return;
    }
    let Some(app) = PROGRAM.lock().expect("programme du menu").clone() else {
        return;
    };
    choose(&app, slider.setting, value);
}

/// Relit où en sont les quatre interrupteurs, et redessine si ça a bougé.
///
/// Trois d'entre eux sont ce que ce programme croit, parce que c'est lui
/// qui les bascule et que le moteur ne dit jamais où il en est ; le son se
/// demande au mélangeur de Windows, qui le sait et qui est ouvert à tout
/// le monde.
async fn reread_the_toggles(app: &App) {
    /// Pose où en est un interrupteur, et dit si ça a bougé.
    fn set(cell: &AtomicBool, value: bool) -> bool {
        cell.swap(value, Ordering::Relaxed) != value
    }

    let mut change = set(&IN_GAME, crate::floating::in_game_mouse(app));
    change |= set(&IMMERSIVE, crate::floating::keys_to_the_session(app));
    change |= set(&SHARED, crate::floating::the_clipboard_is_shared(app));
    change |= set(&HELD, crate::floating::the_badges_are_held_up(app));
    // Sans session le mélangeur n'a rien à dire, et la carte ne s'ouvre
    // pas sans session : un refus se laisse donc tel quel plutôt que
    // d'éteindre l'interrupteur.
    if let Ok(muted) = crate::floating::hushed(app).await {
        change |= set(&MUTED, muted);
    }
    if change {
        redraw(app);
    }
}

/// Suit ce que la session coûte tant que la carte est ouverte, et pas une
/// seconde de plus : des chiffres que personne ne regarde ne valent ni le
/// fichier ni le réveil.
fn follow_the_readings(app: &App, is_open: bool) {
    // Le tour change à chaque appel, ce qui arrête celui d'avant : sans
    // ça, ouvrir et refermer vite laisserait deux veilles derrière la
    // même carte.
    let round = ROUND.fetch_add(1, Ordering::Relaxed) + 1;
    if !is_open {
        return;
    }
    let app = app.clone();
    crate::app::spawn(async move {
        while ROUND.load(Ordering::Relaxed) == round {
            let said = crate::measures::session_measures();
            let now = Instant::now();
            // Le verrou est rendu avant l'attente : un verrou tenu à
            // travers une attente est un verrou tenu une seconde. Pris
            // avant la lecture et non après, parce que celle-ci part de
            // la précédente pour les mesures qui manquent.
            let change = {
                let mut bar = READINGS_BAR.lock().expect("mesures du menu");
                let load = ReadingsBar::of(&said, &bar, now);
                let change = bar.reads_differently(&load);
                *bar = load;
                change
            };
            if change {
                redraw(&app);
            }
            tokio::time::sleep(REFRESH).await;
        }
    });
}

/// Redessine la carte depuis un fil qui n'est pas celui qui la dessine.
fn redraw(app: &App) {
    let _ = app.run_on_main_thread(|| {
        use windows_sys::Win32::Foundation::HWND;

        let window = ITS_WINDOW.load(Ordering::Relaxed) as HWND;
        if !window.is_null() {
            repaint(window);
        }
    });
}
