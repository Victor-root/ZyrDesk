//! L'accueil de ZyrDesk, dessiné par ce programme.
//!
//! C'était la dernière page du produit. Ce qui la remplace tient dans une
//! fenêtre ordinaire, encadrée par Windows, dont l'intérieur est une
//! toile : le même dessin que le logo et le menu de la session, la même
//! palette lue dans la même feuille de style, les mêmes icônes.
//!
//! **Elle ne décide de rien.** Elle demande au service, par le coeur, et
//! elle dessine ce qui revient. Le vocabulaire suit celui du produit :
//! « ordinateur » et non « hôte », « accès distant » et non « service ».
//!
//! # Une seule marche
//!
//! Dessiner et savoir ce qui est sous la souris sont le même travail :
//! une passe pose chaque chose et note au passage ce qui répond au clic.
//! Deux marches se répondraient juste jusqu'au jour où l'une change.
//!
//! # Ce qui n'est pas dessiné ici
//!
//! Les champs de saisie. Écrire du texte est le seul endroit où le
//! système fait mieux que nous : le curseur, la sélection, le
//! presse-papiers, les claviers qui composent leurs signes. Ce sont donc
//! de vrais champs de Windows, posés dans le cadre que nous dessinons,
//! et qui ne vivent que le temps du dialogue qui les porte.

use std::sync::Mutex;
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};

use zyr_broker::rest::Access;
use zyr_control::{Account, Attach, Device, Registering};

use crate::app::App;

use crate::design::{self, Colour, Palette};
use crate::desk::{Attached, Peer, Standing, Watcher};
use crate::folders::Engines;
use crate::icons;
use crate::paint::{Align, Canvas, Icon, Pen, Rect};
use crate::session::Ongoing;
use crate::settings::Settings;
use crate::shortcuts::{Combination, Doing, Held};
use crate::theme::Choice;

/// Ce sous quoi ce module classe ses lignes du journal.
const TAG: &str = "home";

/// Écrit une ligne sous l'étiquette de ce module.
fn note(what: &str) {
    crate::journal::note_about(TAG, what);
}

/// Ce que le service peut changer sans que personne ne clique : une
/// session ouverte depuis l'autre bout, un moteur déposé dans son
/// dossier, le service arrêté. Redemandé à ce rythme.
const REFRESH: std::time::Duration = std::time::Duration::from_secs(3);

/// Le temps qu'un « Copié » reste lisible avant que le bouton reprenne
/// son mot.
const COPIED_TIME: std::time::Duration = std::time::Duration::from_millis(1600);

/// Le temps qu'une demande de confirmation reste armée.
const CONFIRM_TIME: std::time::Duration = std::time::Duration::from_secs(4);

/// Le temps qu'une bonne nouvelle reste à l'écran avant de s'effacer.
const NOTICE_TIME: std::time::Duration = std::time::Duration::from_secs(6);

/// Ce que le fil qui va et vient met à faire un aller.
const BACK_AND_FORTH: std::time::Duration = std::time::Duration::from_millis(1400);

/// Une empreinte fait toujours cette longueur. La vérifier ici évite
/// d'aller déranger le service pour rien.
const FINGERPRINT_LENGTH: usize = 64;

const MINUTE: u64 = 60;
const HOUR: u64 = 3600;

/* ---- Ce que l'accueil montre ----------------------------------------- */

/// Ce que le produit dit de lui-même.
///
/// Rien n'est décidé ici : tout vient du service, et la fenêtre ne fait
/// que le dessiner. Une session appartient au service et survit à cette
/// fenêtre fermée, mise à jour ou plantée.
#[derive(Default, PartialEq)]
struct Seen {
    machine: Option<Standing>,
    peers: Vec<Peer>,
    sessions: Vec<Ongoing>,
    /// Les ordinateurs connectés à celui-ci en ce moment, et qui le
    /// contrôlent : l'inverse de `sessions`.
    watching: Vec<Watcher>,
    engines: Option<Engines>,
    settings: Option<Settings>,
    /// Le compte, quand le service répond : le lien s'il y en a un, et
    /// les appareils qui y sont.
    account: Option<AccountState>,
    /// Les trois raccourcis, écrits comme ils sont gravés sur le clavier
    /// branché, et rien pour ceux qui n'ont pas de touche.
    shortcuts: Vec<(Doing, Option<String>)>,
    /// Ce que fait tourner cette fenêtre, et le dossier des journaux :
    /// demandés une fois, ils ne changent pas de la vie du programme.
    version: String,
    folder: String,
}

impl Seen {
    /// Une seule session à la fois depuis cet ordinateur : deux fenêtres
    /// vidéo en même temps ne se pilotent pas.
    fn busy(&self, state: &State) -> bool {
        state.opening.is_some() || !self.sessions.is_empty()
    }

    /// Le nom sous lequel on reconnaît la machine d'une session.
    ///
    /// À l'empreinte et non à l'adresse : c'est la seule chose qui ne
    /// bouge pas d'un réseau à l'autre.
    fn name_of(&self, session: &Ongoing) -> String {
        self.peers
            .iter()
            .find(|peer| peer.fingerprint == session.fingerprint)
            .map_or_else(|| session.towards.clone(), |peer| peer.name.clone())
    }
}

/// Le compte de cet ordinateur, tel que le service le tient.
#[derive(Default, PartialEq)]
struct AccountState {
    /// Le lien, ou rien : sans lien, le produit ne connaît aucun serveur.
    link: Option<Account>,
    /// Les appareils du compte, cet ordinateur compris, tels que le
    /// serveur les a dits. Vides tant qu'il n'a rien dit.
    devices: Vec<Device>,
}

/// Ce qu'il reste à faire pour que le produit marche, dit en clair et
/// avec de quoi y remédier.
///
/// Sans ça, un moteur absent se lit « démarrage en cours » pour toujours,
/// et un service arrêté ne se répare que par une commande.
struct ToDo {
    text: &'static str,
    button: &'static str,
    remedy: Remedy,
}

/// Ce que le bouton d'un tel bandeau va faire.
#[derive(Clone, Copy)]
enum Remedy {
    StartTheService,
    HostEngine,
    ClientEngine,
    SeeTheJournal,
}

/// Ce qui se passe pendant qu'une session s'ouvre.
///
/// Le titre ne bouge pas de toute l'ouverture : ce qui s'y passe est
/// toujours la même chose, et un titre qui change à chaque étape se lit
/// comme des nouvelles alors que ce n'en sont pas.
struct Opening {
    towards: String,
    detail: String,
    code: Option<String>,
    since: std::time::Instant,
}

/// Le bandeau du haut. Il sert aux deux : ce qui a échoué, et ce qui a
/// réussi sans laisser de trace ailleurs à l'écran. Un message rouge pour
/// dire que tout va bien se lirait comme une panne.
struct Notice {
    text: String,
    is_trouble: bool,
    since: std::time::Instant,
}

/* ---- Où en est l'écran ----------------------------------------------- */

/// Ce qui est ouvert par-dessus l'accueil.
#[derive(Clone, Copy, PartialEq)]
enum Screen {
    Home,
    Adding,
    Journal,
    Settings,
    /// Se rattacher à un serveur.
    Account,
    /// Renommer un appareil du compte.
    Renaming,
}

/// Ce qui défile, et où en est son défilement.
#[derive(Clone, Copy, PartialEq)]
enum Scroller {
    Page,
    Dialogue,
    /// Le texte du journal, qui défile chez lui dans le dialogue qui le
    /// porte, comme une page défile dans une fenêtre.
    Lines,
}

/// Où en est la fenêtre : ce qui est ouvert, ce qui est sous la main, ce
/// qui attend une réponse.
struct State {
    screen: Screen,
    /// Le défilement de la page, celui du dialogue ouvert, et celui du
    /// texte du journal. Le dernier défile aussi en travers : une ligne
    /// de journal ne se replie pas.
    scroll: f32,
    dialogue_scroll: f32,
    lines_scroll: (f32, f32),
    /// Ce que chaque chose défilante mesurait la dernière fois qu'elle a
    /// été dessinée, la place qu'elle avait, et la course de son pouce :
    /// de quoi ne jamais défiler au-delà, et traîner l'ascenseur du même
    /// pas que celui qui a été dessiné.
    extents: [(f32, f32, f32); 3],
    hover: Option<Target>,
    pressed: Option<Target>,
    /// L'ascenseur tenu par une main, et de combien le curseur était
    /// au-dessus de son haut quand elle l'a pris.
    held: Option<(Scroller, f32)>,
    /// Les interrupteurs poussés dont le service n'a pas encore pris
    /// acte. Sans eux, l'état qui revient est encore l'ancien et
    /// l'interrupteur reviendrait en arrière sous le doigt.
    pushed: Vec<(Toggle, bool)>,
    /// Le bouton qui vient d'être copié, et depuis quand.
    copied: Option<(Target, std::time::Instant)>,
    /// Le repli du jargon, dans les réglages.
    advanced: bool,
    /// La touche qui attend une combinaison. Une seule à la fois : deux
    /// boutons qui attendent la même touche se la partageraient.
    listening: Option<Doing>,
    /// De quel ordinateur est le journal ouvert. Rien pour celui-ci :
    /// c'est le seul dont on peut aussi vider les fichiers et ouvrir le
    /// dossier.
    journal_of: Option<Peer>,
    /// Ce que le journal ouvert montre, une ligne par ligne : le découper
    /// à chaque image reviendrait à le relire en entier pour n'en
    /// dessiner que trente lignes.
    lines: Vec<String>,
    /// Le tri auquel cette page répond, et rien tant qu'aucune réponse
    /// n'est arrivée.
    ///
    /// « Copier le tri » emporte la page telle qu'elle est à l'écran : il
    /// faut donc savoir si elle répond bien à ce qui est écrit dans la
    /// boîte, faute de quoi le bouton emporterait la page d'avant sous le
    /// nom du tri.
    sift: Option<String>,
    /// Les noms que la page ouverte dit porter, à cocher plutôt qu'à
    /// taper.
    ///
    /// Ils viennent de la page elle-même et jamais d'une liste tenue
    /// ici : la moitié du temps elle vient d'un autre ordinateur, et un
    /// nom proposé qu'aucune de ses lignes ne porte serait une impasse
    /// proposée.
    tags: Vec<String>,
    /// Le tri de la dernière question partie.
    ///
    /// Chaque question ouvre sa propre conversation avec le service :
    /// deux lectures lancées coup sur coup peuvent revenir dans l'autre
    /// sens, et la plus ancienne écraserait la plus récente.
    sift_asked: String,
    /// Depuis quand « Vider » attend sa confirmation.
    emptying: Option<std::time::Instant>,
    notice: Option<Notice>,
    /// Ce que les réglages ont à redire, qui vit dans leur dialogue.
    trouble: Option<String>,
    opening: Option<Opening>,
    /// Dans le dialogue de compte : créer le compte plutôt que d'y
    /// entrer.
    sign_up: bool,
    /// La clé qu'un serveur que personne ne garantit a présentée, en
    /// attente que la personne la compare et la confirme.
    pinning: Option<String>,
    /// Un rattachement en cours : le bouton attend la réponse.
    attaching: bool,
    /// Depuis quand « Se détacher » attend sa confirmation.
    detaching: Option<std::time::Instant>,
    /// Quel appareil « Révoquer » attend de confirmer, et depuis quand.
    revocation: Option<(usize, std::time::Instant)>,
    /// L'appareil en cours de renommage : son identifiant et son nom.
    renaming: Option<(String, String)>,
}

impl State {
    const fn new() -> Self {
        State {
            screen: Screen::Home,
            scroll: 0.0,
            dialogue_scroll: 0.0,
            lines_scroll: (0.0, 0.0),
            extents: [(0.0, 0.0, 0.0); 3],
            hover: None,
            pressed: None,
            held: None,
            pushed: Vec::new(),
            copied: None,
            advanced: false,
            listening: None,
            journal_of: None,
            lines: Vec::new(),
            sift: None,
            tags: Vec::new(),
            sift_asked: String::new(),
            emptying: None,
            notice: None,
            trouble: None,
            opening: None,
            sign_up: false,
            pinning: None,
            attaching: false,
            detaching: None,
            revocation: None,
            renaming: None,
        }
    }

    /// Ce qu'une chose défilante mesure, la place qu'elle a et la course
    /// de son pouce : ce qui borne son défilement et ce qui le traîne.
    fn measured(&self, which: Scroller) -> (f32, f32, f32) {
        self.extents[match which {
            Scroller::Page => 0,
            Scroller::Dialogue => 1,
            Scroller::Lines => 2,
        }]
    }

    fn keep(&mut self, which: Scroller, content: f32, visible: f32, travel: f32) {
        self.extents[match which {
            Scroller::Page => 0,
            Scroller::Dialogue => 1,
            Scroller::Lines => 2,
        }] = (content, visible, travel);
    }

    /// De combien cette chose-là défile en ce moment.
    fn scroll(&self, which: Scroller) -> f32 {
        match which {
            Scroller::Page => self.scroll,
            Scroller::Dialogue => self.dialogue_scroll,
            Scroller::Lines => self.lines_scroll.1,
        }
    }

    /// Fait défiler, sans jamais sortir de ce qu'il y a à voir.
    fn scroll_by(&mut self, which: Scroller, by: f32) {
        let (content, visible, _) = self.measured(which);
        let furthest = (content - visible).max(0.0);
        let now_at = (self.scroll(which) + by).clamp(0.0, furthest);
        match which {
            Scroller::Page => self.scroll = now_at,
            Scroller::Dialogue => self.dialogue_scroll = now_at,
            Scroller::Lines => self.lines_scroll.1 = now_at,
        }
    }
}

/* ---- Ce sur quoi on clique -------------------------------------------- */

/// Un interrupteur, et ce qu'il commande.
#[derive(Clone, Copy, PartialEq)]
enum Toggle {
    Access,
    Trust,
    AtBoot,
    SteadyRate,
    Sound,
    Stats,
    /// Les deux interrupteurs d'essai réseau : marquer les paquets, et
    /// écouter sur le port du produit.
    Marking,
    FixedPort,
}

/// Un choix segmenté : plusieurs possibilités qui s'excluent, montrées
/// toutes ensemble.
#[derive(Clone, Copy, PartialEq)]
enum Pick {
    Theme,
    Capture,
    Codec,
    Display,
    Mouse,
    /// Dans le dialogue de compte : y entrer, ou le créer.
    SignUp,
}

/// Ce sur quoi on peut cliquer, et ce que ça fait.
#[derive(Clone, PartialEq)]
enum Target {
    OpenJournal,
    OpenSettings,
    CopyFingerprint,
    /// Le bouton d'un bandeau « ce qu'il reste à faire ».
    ToFix(usize),
    /// Une carte d'ordinateur, et le journal de cet ordinateur-là.
    Peer(usize),
    JournalOf(usize),
    /// La même carte, mais par ce réseau-ci et rien d'autre : aucun
    /// serveur consulté, aucune sortie de la maison.
    Local(usize),
    /// Déconnecte l'ordinateur qui contrôle celui-ci en ce moment, sur
    /// cette carte.
    Disconnect(usize),
    Add,
    Switch(Toggle),
    /// Un des noms que la page du journal porte, par son rang : coché, il
    /// s'ajoute à la boîte de tri, décoché il en part.
    Tag(usize),
    Segment(Pick, usize),
    Shortcut(Doing),
    /// Fermer le dialogue ouvert, quel qu'il soit.
    Close,
    Connect,
    /// Ce que la touche Entrée fait dans le dialogue ouvert.
    Confirm,
    Forget(usize),
    Empty,
    Refresh,
    CopyJournal,
    /// Ouvrir le dossier des journaux, depuis le journal ou les réglages.
    OpenTheJournals,
    Advanced,
    Scrollbar(Scroller),
    /// Le compte : ouvrir le dialogue, se rattacher, confirmer la clé
    /// d'un serveur, se détacher.
    OpenAccount,
    Attach,
    Pin,
    Detach,
    /// Un appareil du compte, par son rang : ouvrir son renommage, ou le
    /// révoquer.
    OpenRenaming(usize),
    Rename,
    Revoke(usize),
}

impl Toggle {
    /// Où est l'interrupteur, d'après ce que le produit dit, et d'après
    /// ce qu'une main vient de pousser sans réponse encore.
    fn is_on(self, seen: &Seen, state: &State) -> bool {
        if let Some((_, wanted)) = state.pushed.iter().find(|(target, _)| *target == self) {
            return *wanted;
        }
        match self {
            Toggle::Access => seen.machine.as_ref().is_some_and(|said| said.wanted),
            Toggle::Trust => seen.machine.as_ref().is_some_and(|said| said.trusting),
            Toggle::AtBoot => seen.machine.as_ref().is_some_and(|said| said.at_boot),
            Toggle::SteadyRate => seen.machine.as_ref().is_some_and(|said| said.steady_rate),
            Toggle::Marking => seen.machine.as_ref().is_some_and(|said| said.ecn),
            Toggle::FixedPort => seen.machine.as_ref().is_some_and(|said| said.fixed_port),
            Toggle::Sound => seen
                .settings
                .as_ref()
                .is_some_and(|said| said.mute_far_speakers),
            Toggle::Stats => seen
                .settings
                .as_ref()
                .is_some_and(|said| said.stats_overlay),
        }
    }

    /// Si on peut le pousser.
    ///
    /// Un service arrêté n'est pas un accès distant désactivé : l'un est
    /// un choix, l'autre une panne. L'interrupteur reste alors sur la
    /// position choisie et devient inactionnable, plutôt que de sauter à
    /// « non » et de faire croire à une décision que personne n'a prise.
    fn enabled(self, seen: &Seen, state: &State) -> bool {
        if state.pushed.iter().any(|(target, _)| *target == self) {
            return false;
        }
        match self {
            Toggle::Sound | Toggle::Stats => seen.settings.is_some(),
            _ => seen
                .machine
                .as_ref()
                .is_some_and(|said| said.unreachable.is_none()),
        }
    }
}

impl Pick {
    /// Les mots des côtés, dans l'ordre où ils se montrent.
    fn words(self) -> Vec<&'static str> {
        match self {
            Pick::Theme => Choice::ALL.iter().map(|choice| choice.word()).collect(),
            Pick::Capture => vec!["Compatible", "Rapide"],
            Pick::Codec => vec!["Auto", "H.264", "HEVC", "AV1"],
            Pick::Display => vec!["Plein écran", "Fenêtre"],
            Pick::Mouse => vec!["Bureau", "Jeu"],
            Pick::SignUp => vec!["J'ai un compte", "Créer un compte"],
        }
    }

    /// La valeur que porte chaque côté, telle qu'elle voyage et telle
    /// qu'elle s'écrit dans les réglages.
    fn values(self) -> Vec<&'static str> {
        match self {
            Pick::Theme | Pick::SignUp => Vec::new(),
            Pick::Capture => vec!["ddx", "wgc"],
            Pick::Codec => vec!["auto", "H.264", "HEVC", "AV1"],
            Pick::Display => vec!["fullscreen", "windowed"],
            Pick::Mouse => vec!["desktop", "game"],
        }
    }

    /// Lequel est choisi, d'après ce que le produit dit, ou d'après ce
    /// que la fenêtre tient elle-même pour les deux qui ne voyagent pas.
    fn current(self, seen: &Seen, state: &State) -> Option<usize> {
        let said = match self {
            Pick::Theme => {
                return Choice::ALL
                    .iter()
                    .position(|choice| *choice == crate::theme::chosen());
            }
            Pick::SignUp => return Some(usize::from(state.sign_up)),
            Pick::Capture => seen.machine.as_ref()?.capture.clone(),
            Pick::Codec => seen.settings.as_ref()?.codec.clone(),
            Pick::Display => seen.settings.as_ref()?.display.clone(),
            Pick::Mouse => {
                let desktop = seen.settings.as_ref()?.absolute_mouse;
                (if desktop { "desktop" } else { "game" }).to_string()
            }
        };
        self.values().iter().position(|value| *value == said)
    }

    /// Si on peut en changer.
    fn enabled(self, seen: &Seen) -> bool {
        match self {
            Pick::Theme | Pick::SignUp => true,
            Pick::Capture => seen
                .machine
                .as_ref()
                .is_some_and(|said| said.unreachable.is_none()),
            _ => seen.settings.is_some(),
        }
    }
}

/* ---- Ce que l'écran des réglages contient ----------------------------- */

/// De quoi décider, à droite d'une ligne de réglage.
enum Control {
    /// Rien : la ligne dit seulement où en est le produit.
    Status,
    Switch(Toggle),
    Segments(Pick),
    Key(Doing),
    /// Un bouton qui ouvre quelque chose hors de la fenêtre.
    Opens(&'static str, Target),
}

/// Une ligne de l'écran des réglages : ce dont il s'agit à gauche, de
/// quoi en décider à droite.
struct Setting {
    label: &'static str,
    caption: &'static str,
    control: Control,
}

/// Ce que l'écran des réglages porte, dans l'ordre.
enum Element {
    /// Une étiquette de section, et le mot qui l'explique.
    Section(&'static str, &'static str),
    /// Le repli du jargon : ce qui suit ne se montre qu'ouvert.
    Fold,
    Setting(Setting),
    /// Le compte : le lien tel qu'il est, et les appareils qui y sont.
    /// Dessiné à part, parce qu'il n'a pas la forme d'une ligne.
    Account,
}

/// L'écran des réglages, ligne par ligne.
///
/// Une table et non une suite d'appels : c'est la même mise en page pour
/// toutes, qu'elles portent un choix segmenté, un interrupteur, une
/// touche ou un bouton, et une table se lit comme l'écran se lit.
///
/// Ce qu'une session demande, taille, débit et codec, se règle dans son
/// propre menu et pas ici : ce sont les trois nombres qu'on change en
/// regardant l'image qu'ils changent, et revenir sur cet écran pour en
/// essayer un, c'est s'éloigner de la seule chose qui dit si ça a marché.
/// La première ligne rappelle où ils en sont.
const SETTINGS: &[Element] = &[
    Element::Setting(Setting {
        label: "Ce qu'une session demande",
        caption: "",
        control: Control::Status,
    }),
    Element::Setting(Setting {
        label: "Thème",
        caption: "Suit Windows tant qu'on ne choisit pas.",
        control: Control::Segments(Pick::Theme),
    }),
    Element::Setting(Setting {
        label: "Ordinateurs du réseau local",
        caption: "Ceux qui s'annoncent sur ce réseau peuvent joindre celui-ci sans rien à \
                  recopier. Ne concerne que le réseau local.",
        control: Control::Switch(Toggle::Trust),
    }),
    Element::Setting(Setting {
        label: "Renvoyer un écran immobile",
        caption: "Quand quelqu'un regarde cet ordinateur : réenvoyer l'écran à pleine cadence \
                  même quand rien ne bouge dessus. Le pointeur est plus fluide, mais c'est une \
                  image complète encodée soixante fois par seconde pour rien. À couper si cet \
                  ordinateur n'arrive pas à suivre.",
        control: Control::Switch(Toggle::SteadyRate),
    }),
    Element::Setting(Setting {
        label: "Façon de filmer l'écran",
        caption: "La façon dont cet ordinateur prend ses images quand quelqu'un le regarde. \
                  « Compatible » voit aussi les demandes de mot de passe administrateur et \
                  l'écran de connexion. « Rapide » va plus vite sur certaines machines, et ne \
                  les voit pas : elles apparaissent alors comme un écran noir.",
        control: Control::Segments(Pick::Capture),
    }),
    Element::Setting(Setting {
        label: "Démarrer avec Windows",
        caption: "Cet ordinateur répond dès l'allumage, avant même qu'on ouvre une session \
                  dessus, et ZyrDesk revient tout seul avec l'icône. Sans cela, rien ne tourne \
                  tant que vous n'avez pas ouvert ZyrDesk.",
        control: Control::Switch(Toggle::AtBoot),
    }),
    Element::Section(
        "Essais réseau",
        "Deux façons de parler sur le fil, à comparer quand les sessions se coupent : changez \
         un seul interrupteur à la fois, sur les deux ordinateurs. Chaque changement rouvre la \
         porte de cet ordinateur, donc coupe une session ouverte vers lui.",
    ),
    Element::Setting(Setting {
        label: "Marquer les paquets (ECN)",
        caption: "Le tunnel pose sur chaque paquet le marquage de congestion que QUIC pose \
                  partout. Certains équipements traitent ces paquets à part : à couper pour \
                  voir si les coupures cessent.",
        control: Control::Switch(Toggle::Marking),
    }),
    Element::Setting(Setting {
        label: "Écouter sur le port 47000",
        caption: "Coupé, cet ordinateur écoute sur un port que Windows choisit à chaque \
                  démarrage. Une session par le compte le trouve ; une session en réseau local \
                  ou un renvoi de port fait sur la box, non.",
        control: Control::Switch(Toggle::FixedPort),
    }),
    Element::Section(
        "Compte",
        "Un serveur ZyrDesk retrouve vos ordinateurs où qu'ils soient et les présente l'un à \
         l'autre. Facultatif : sans compte, tout marche comme avant sur le réseau local.",
    ),
    Element::Account,
    Element::Section(
        "Raccourcis clavier",
        "Ils marchent pendant une session, par-dessus l'image. Cliquez sur une combinaison pour \
         la changer, puis tapez-la. Échap annule, Retour arrière la retire.",
    ),
    Element::Setting(Setting {
        label: "Terminer la session",
        caption: "Rend son bureau à l'ordinateur distant. Une session est en cours ou terminée, \
                  jamais entre les deux.",
        control: Control::Key(Doing::End),
    }),
    Element::Setting(Setting {
        label: "Ouvrir le menu flottant",
        caption: "Le seul chemin de retour après avoir masqué le bouton.",
        control: Control::Key(Doing::Menu),
    }),
    Element::Setting(Setting {
        label: "Fenêtré ou plein écran",
        caption: "Bascule l'image de l'un à l'autre.",
        control: Control::Key(Doing::Fullscreen),
    }),
    Element::Setting(Setting {
        label: "Écran suivant de l'hôte",
        caption: "Passe d'un écran de l'ordinateur distant au suivant, sans rien relancer. Ne \
                  fait rien quand cet ordinateur n'en a qu'un.",
        control: Control::Key(Doing::NextScreen),
    }),
    Element::Fold,
    Element::Setting(Setting {
        label: "Codec vidéo",
        caption: "Auto prend le meilleur que les deux ordinateurs savent lire.",
        control: Control::Segments(Pick::Codec),
    }),
    Element::Setting(Setting {
        label: "Fenêtre de la session",
        caption: "L'image s'affiche dans la fenêtre ZyrDesk : ce réglage dit si cette fenêtre \
                  prend l'écran entier.",
        control: Control::Segments(Pick::Display),
    }),
    Element::Setting(Setting {
        label: "Souris",
        caption: "La souris de jeu vise en mouvements plutôt qu'en position.",
        control: Control::Segments(Pick::Mouse),
    }),
    Element::Setting(Setting {
        label: "Couper le son de l'ordinateur distant",
        caption: "Ses enceintes se taisent pendant toute la session : la pièce où il se trouve \
                  reste silencieuse, et vous entendez tout. Le son y revient tout seul à la fin.",
        control: Control::Switch(Toggle::Sound),
    }),
    Element::Setting(Setting {
        label: "Statistiques par-dessus l'image",
        caption: "Images par seconde, débit, pertes.",
        control: Control::Switch(Toggle::Stats),
    }),
    Element::Setting(Setting {
        label: "Journaux",
        caption: "",
        control: Control::Opens("Ouvrir", Target::OpenTheJournals),
    }),
];

/* ---- Ce que la feuille de style dit, en pixels de page ---------------- */

mod layout {
    /// La largeur au-delà de laquelle la page ne s'étale plus, et ce qui
    /// l'entoure : en haut et en bas, puis sur les côtés.
    pub const PAGE: f32 = 820.0;
    pub const EDGE: f32 = 32.0;
    pub const SIDE: f32 = 24.0;

    /// La marque en haut de la page, et celle de l'écran d'ouverture.
    pub const BRAND: f32 = 40.0;
    pub const BIG_BRAND: f32 = 56.0;

    /// Un bouton, un grand bouton, et le dessin d'un bouton à icône.
    pub const BUTTON: f32 = 36.0;
    pub const BIG_BUTTON: f32 = 44.0;
    pub const GLYPH: f32 = 18.0;

    /// L'interrupteur : sa taille, son pouce et le jeu autour.
    pub const SWITCH: (f32, f32) = (44.0, 26.0);
    pub const THUMB: f32 = 18.0;
    pub const SLACK: f32 = 3.0;

    /// Un côté de choix segmenté, et ce qui entoure le groupe.
    pub const SEGMENT: f32 = 26.0;
    pub const AROUND: f32 = 2.0;
    pub const SEGMENT_RADIUS: f32 = 6.0;

    /// La pastille de présence, et l'anneau autour de celle qui est
    /// vivante.
    pub const DOT: f32 = 8.0;
    pub const RING: f32 = 3.0;

    /// Un champ de saisie.
    pub const FIELD: f32 = 40.0;

    /// L'épaisseur d'un trait et d'une bordure.
    pub const HAIRLINE: f32 = 1.0;

    /// Une carte d'ordinateur n'est jamais plus étroite que ça.
    pub const CARD: f32 = 240.0;
    /// Ce qu'une carte d'ordinateur fait de haut, la place du mot qui
    /// n'apparaît qu'au survol comprise.
    pub const HINT: f32 = 20.0;

    /// La largeur des trois dialogues.
    pub const DIALOGUE: f32 = 460.0;
    pub const DIALOGUE_SETTINGS: f32 = 560.0;
    pub const DIALOGUE_JOURNAL: f32 = 880.0;

    /// La touche d'un raccourci n'est jamais plus étroite que ça.
    pub const KEY: f32 = 150.0;

    /// Le fil qui va et vient pendant qu'une session s'ouvre, et la part
    /// de sa longueur que parcourt le morceau qui s'y déplace.
    pub const THREAD: (f32, f32) = (260.0, 3.0);
    pub const PIECE: f32 = 0.4;

    /// Le code d'appairage, plus grand que tout le reste parce qu'il se
    /// lit de loin, en tapant sur un autre clavier.
    pub const CODE: f32 = 34.0;

    /// L'ascenseur, et ce qui le sépare du bord.
    pub const SCROLLBAR: f32 = 6.0;

    /// Le dessin de l'écran vide.
    pub const EMPTY: (f32, f32) = (64.0, 44.0);

    /// Ce qu'un cran de roulette fait défiler.
    pub const NOTCH: f32 = 60.0;

    /// Ce qu'une boîte de dialogue pose de noir sur ce qu'elle recouvre.
    pub const VEIL: f32 = 0.55;

    /// La hauteur du texte du journal : ce qu'il prend au plus, en part
    /// de la fenêtre, et jamais plus que ça.
    pub const JOURNAL: (f32, f32) = (0.6, 560.0);

    /// Et jamais moins que ça, quoi que prenne le reste du dialogue :
    /// une fenêtre de journal où l'on ne voit plus le journal n'en est
    /// plus une.
    pub const JOURNAL_AT_LEAST: f32 = 140.0;
}

/* ---- Ce que la fenêtre tient ------------------------------------------ */

/// La fenêtre qui porte le dessin, fille de celle que le système encadre.
static ITS_WINDOW: AtomicIsize = AtomicIsize::new(0);
/// De combien un pixel de page compte ici, en centièmes.
static SCALE: AtomicU32 = AtomicU32::new(100);

static SEEN: Mutex<Option<Seen>> = Mutex::new(None);
static STATE: Mutex<State> = Mutex::new(State::new());
/// Ce que la dernière image a posé, et qui répond au clic.
static CLICKABLES: Mutex<Vec<(Target, Rect)>> = Mutex::new(Vec::new());
/// Le programme, gardé ici parce que rien n'en donne un à une fenêtre du
/// système.
static PROGRAM: Mutex<Option<App>> = Mutex::new(None);

// La toile de cette fenêtre, tenue par le fil qui la possède : une
// surface de dessin et la fenêtre qu'elle habille appartiennent au fil
// qui les a faites.
thread_local! {
    static CANVAS: std::cell::RefCell<Option<Canvas>> = const { std::cell::RefCell::new(None) };
}

fn scale() -> f32 {
    SCALE.load(Ordering::Relaxed) as f32 / 100.0
}

fn palette() -> Palette {
    design::palette(crate::theme::light())
}

fn program() -> Option<App> {
    PROGRAM.lock().expect("programme de l'accueil").clone()
}

/* ---- La fenêtre -------------------------------------------------------- */

/// Ouvre la toile de l'accueil dans la fenêtre que le système encadre.
///
/// Une fenêtre fille et non la fenêtre elle-même : celle du dehors
/// appartient à la boîte à outils, qui la pose, la déplace et l'encadre.
/// Ce qui est à nous est son dedans, et c'est exactement ce qu'une
/// fenêtre fille est.
///
/// Sur le fil qui possède la fenêtre du dehors : une fenêtre appartient
/// au fil qui l'a faite, et une fenêtre faite ailleurs n'entendrait
/// jamais une souris.
pub fn raise(app: &App) {
    let outer = crate::main_window::handle() as windows_sys::Win32::Foundation::HWND;
    if outer.is_null() {
        note("accueil : pas de fenêtre où dessiner");
        return;
    }
    *PROGRAM.lock().expect("programme de l'accueil") = Some(app.clone());
    SCALE.store(
        (crate::main_window::scale() * 100.0).round() as u32,
        Ordering::Relaxed,
    );
    build(outer);
    watch(app.clone());
}

/// La toile, telle que le système la connaît.
///
/// Lue par la fenêtre qui la porte, qui la redimensionne avec elle et lui
/// donne le clavier.
pub fn its_canvas() -> isize {
    ITS_WINDOW.load(Ordering::Relaxed)
}

/// De combien un pixel de page compte sur l'écran où la fenêtre est.
///
/// Redemandé quand elle change d'écran ou que l'écran change
/// d'agrandissement : tout ce qui est dessiné en descend, la police des
/// champs de saisie comprise.
pub fn measure_the_screen(app: &App) {
    let wanted = (crate::main_window::scale() * 100.0).round() as u32;
    if SCALE.swap(wanted, Ordering::Relaxed) == wanted {
        return;
    }
    // Les champs de saisie sont des fenêtres du système : leur police ne
    // se remet pas à l'échelle avec le reste, il faut la leur refaire.
    dress_the_fields();
    redraw(app);
}

/// Bâtit la toile et se met devant les messages de la fenêtre qui la
/// porte.
fn build(outer: windows_sys::Win32::Foundation::HWND) {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CS_HREDRAW, CS_VREDRAW, CreateWindowExW, GetClientRect, IDC_ARROW, LoadCursorW,
        RegisterClassW, WNDCLASSW, WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_VISIBLE,
    };

    if ITS_WINDOW.load(Ordering::Relaxed) != 0 {
        return;
    }
    let class_name = wide("ZyrDeskAccueil");
    let mut inside = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: une fenêtre du programme, dont le rectangle est lu dans le
    // nôtre.
    if unsafe { GetClientRect(outer, &mut inside) } == 0 {
        note("accueil : la fenêtre ne dit pas sa taille");
        return;
    }

    // SAFETY: une classe déclarée une fois et une fenêtre bâtie dessus,
    // sur le fil qui pompera ses messages. Une classe déclarée deux fois
    // est refusée sans autre effet, d'où la réponse non lue.
    let window = unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let class = WNDCLASSW {
            // Redessinée en entier dès que sa taille change, comme les
            // deux autres surfaces que ce programme peint lui-même. Sans
            // ça, le système ne redemande une image que pour la bande
            // qui vient d'apparaître, et rien du tout quand la fenêtre
            // rétrécit : la page restait posée pour la taille d'avant,
            // centrée sur une largeur qui n'existait plus, donc décalée
            // à droite et coupée. Visible en revenant d'une fenêtre
            // agrandie, invisible en l'ouvrant à cette taille-là.
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(answer),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: std::ptr::null_mut(),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            // Aucun fond : tout ce que cette fenêtre montre est peint par
            // nous, et un fond posé par le système serait une couleur de
            // plus, vue le temps d'une image à chaque redimensionnement.
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
        };
        RegisterClassW(&class);
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            std::ptr::null(),
            // Rognée par ses soeurs : l'image d'une session est posée
            // par-dessus dans la même fenêtre, et sans ça l'accueil se
            // redessinerait derrière elle à chaque image.
            WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
            0,
            0,
            inside.right,
            inside.bottom,
            outer,
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        )
    };
    if window.is_null() {
        note("accueil : la toile n'a pas pu s'ouvrir");
        return;
    }
    ITS_WINDOW.store(window as isize, Ordering::Relaxed);
    note(&format!(
        "accueil dessiné par ZyrDesk, sans vue web : toile de {}x{} px",
        inside.right, inside.bottom
    ));
}

/// Redemande une image, depuis n'importe quel fil.
pub fn redraw(app: &App) {
    let window = ITS_WINDOW.load(Ordering::Relaxed);
    if window == 0 {
        return;
    }
    let _ = app.run_on_main_thread(move || {
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::Graphics::Gdi::InvalidateRect;

        // SAFETY: une fenêtre à nous, sur le fil qui la possède.
        unsafe { InvalidateRect(window as HWND, std::ptr::null(), 0) };
    });
}

/// Ce que la toile répond quand le système lui parle.
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
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DefWindowProcW, EN_CHANGE, HTCLIENT, IDC_ARROW, IDC_HAND, KillTimer, LoadCursorW,
        SetCursor, SetTimer, WM_COMMAND, WM_CTLCOLOREDIT, WM_ERASEBKGND, WM_KEYDOWN,
        WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT, WM_SETCURSOR,
        WM_SYSKEYDOWN, WM_TIMER,
    };

    match message {
        // Rien à effacer : chaque image couvre la fenêtre entière, et un
        // effacement du système entre deux serait un battement de fond nu.
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            repaint(window);
            0
        }
        // Demandée et non peinte tout de suite : peindre sans que rien
        // n'ait été invalidé ne peint rien du tout, le système ne
        // prêtant alors qu'une surface vide.
        ANIMATE => {
            invalidate(window);
            0
        }
        WM_MOUSEMOVE => {
            moves(window, where_is(with));
            0
        }
        WM_MOUSELEAVE => {
            mouse_left(window);
            0
        }
        WM_LBUTTONDOWN => {
            // SAFETY: une fenêtre à nous, à qui le clavier est donné pour
            // qu'Échap, Entrée et les combinaisons arrivent ici.
            unsafe { SetFocus(window) };
            presses(window, where_is(with));
            0
        }
        WM_LBUTTONUP => {
            releases(window, where_is(with));
            0
        }
        WM_MOUSEWHEEL => {
            let notches = ((holding >> 16) & 0xFFFF) as i16;
            let across = (holding & 0x0004) != 0;
            wheel(window, f32::from(notches) / 120.0, across);
            0
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            if key_down(window, holding as u32, with) {
                return 0;
            }
            // SAFETY: same.
            unsafe { DefWindowProcW(window, message, holding, with) }
        }
        WM_SETCURSOR if (with as u32 & 0xFFFF) == HTCLIENT => {
            let cursor_shape = if STATE.lock().expect("accueil").hover.is_some() {
                IDC_HAND
            } else {
                IDC_ARROW
            };
            // SAFETY: un curseur du système, demandé par son nom.
            unsafe { SetCursor(LoadCursorW(std::ptr::null_mut(), cursor_shape)) };
            1
        }
        // Le fond d'un champ de saisie, et l'encre dedans : ils
        // appartiennent au système, qui demande ici de quelle couleur les
        // peindre pour qu'ils soient de la couleur du reste.
        WM_CTLCOLOREDIT => tint_of_the_field(holding),
        // Un champ dont le texte change change aussi ce que le dialogue
        // dit sous lui et ce que son bouton permet.
        WM_COMMAND if (holding >> 16) as u32 & 0xFFFF == EN_CHANGE => {
            invalidate(window);
            // Et celui du tri relit le journal de lui-même : on écrit, la
            // page se resserre, sans rien à cliquer. Celui d'ici
            // seulement : relire celui d'en face ouvre une route jusqu'à
            // l'autre machine, et une par pause dans la frappe se paierait
            // en secondes. Là-bas, c'est « Actualiser » ou Entrée qui lit.
            //
            // Lu puis relâché : le dessin tient l'état pendant qu'il lit
            // les champs, et les prendre ici dans l'autre ordre serait
            // deux fils qui s'attendent.
            let sift_box = FIELDS.lock().expect("accueil")[Field::Sift.rank()];
            if with == sift_box && STATE.lock().expect("accueil").journal_of.is_none() {
                // SAFETY: une horloge posée sur une fenêtre à nous,
                // depuis le fil qui la possède. La reposer la repart de
                // zéro, ce qui fait qu'une lettre de plus repousse la
                // lecture au lieu d'en ajouter une.
                unsafe { SetTimer(window, SIFT_PAUSE, SIFT_PAUSE_MS, None) };
            }
            0
        }
        WM_TIMER if holding == SIFT_PAUSE => {
            // SAFETY: une horloge à nous, sur le fil qui l'a posée.
            unsafe { KillTimer(window, SIFT_PAUSE) };
            if let Some(app) = program() {
                reread_the_journal(&app, After::Show);
            }
            0
        }
        _ => {
            // Ce qu'un champ de saisie a demandé, fait ici parce que les
            // deux referment le dialogue et donc détruisent ce champ.
            if message == ACT {
                if let Some(app) = program() {
                    act(
                        &app,
                        if holding == 1 {
                            Target::Confirm
                        } else {
                            Target::Close
                        },
                    );
                }
                return 0;
            }
            // SAFETY: la réponse du système à tout ce qui n'est pas
            // répondu ici.
            unsafe { DefWindowProcW(window, message, holding, with) }
        }
    }
}

/// Le message que rien du système n'envoie, et par lequel un champ de
/// saisie demande à la toile de faire ce qu'il ne peut pas faire
/// lui-même.
const ACT: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 1;

/// Une image de plus du fil qui va et vient, demandée par le rythme.
const ANIMATE: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 2;

/// L'horloge qui laisse au tri le temps d'être écrit avant de relire.
const SIFT_PAUSE: usize = 1;

/// Ce qu'on laisse à la dernière lettre, en millisecondes.
///
/// La boîte de tri se comporte comme celle d'un logcat : on écrit, la
/// page se resserre, sans rien à cliquer. Une question par lettre ferait
/// relire les quatre fichiers treize fois pour « clipboard », donc
/// c'est la lettre que personne ne suit qui déclenche la lecture.
const SIFT_PAUSE_MS: u32 = 300;

/// Où la souris est, en vrais pixels depuis le coin de la toile.
fn where_is(with: windows_sys::Win32::Foundation::LPARAM) -> (f32, f32) {
    let x = (with & 0xFFFF) as i16;
    let y = ((with >> 16) & 0xFFFF) as i16;
    (f32::from(x), f32::from(y))
}

/// Un mot dans les caractères que Windows compte, fini par le zéro qu'il
/// cherche.
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

/// Dessine l'accueil et le verse dans la fenêtre.
fn repaint(window: windows_sys::Win32::Foundation::HWND) {
    use windows_sys::Win32::Graphics::Gdi::{BeginPaint, EndPaint, PAINTSTRUCT};
    use windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect;

    let mut inside = windows_sys::Win32::Foundation::RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: une fenêtre à nous, dont le rectangle est lu dans le nôtre.
    if unsafe { GetClientRect(window, &mut inside) } == 0 {
        return;
    }
    let (width, height) = (inside.right.max(1), inside.bottom.max(1));

    let mut paint: PAINTSTRUCT = unsafe { std::mem::zeroed() };
    // SAFETY: une fenêtre à nous, dont la surface est rendue plus bas.
    let surface = unsafe { BeginPaint(window, &mut paint) };
    if surface.is_null() {
        return;
    }
    // Chaque image redit où vont les champs : un champ que l'image ne
    // pose plus, parce que le dialogue a changé de forme, n'a plus de
    // place et se range.
    *PLACES.lock().expect("accueil") = [None; Field::COUNT];
    CANVAS.with_borrow_mut(|canvas| {
        if canvas
            .as_ref()
            .is_none_or(|already| already.size() != (width, height))
        {
            *canvas = Canvas::new(width, height);
        }
        let Some(canvas) = canvas.as_ref() else {
            return;
        };
        let colours = palette();
        canvas.begin(colours.background);
        let clickables = paint_page(canvas, width as f32, height as f32, colours);
        if !canvas.finish() {
            return;
        }
        *CLICKABLES.lock().expect("accueil") = clickables;
        canvas.copy_to(windows::Win32::Graphics::Gdi::HDC(surface), 0, 0);
    });
    // SAFETY: la peinture ouverte juste au-dessus.
    unsafe { EndPaint(window, &paint) };
    place_the_fields();
    clock(window);
}

/// Fait battre l'accueil tant qu'un fil va et vient, et l'arrête après.
///
/// La seule chose de l'accueil qui bouge sans que personne ne touche à
/// rien. Ailleurs, rien n'est redessiné tant que rien ne change.
fn clock(window: windows_sys::Win32::Foundation::HWND) {
    if STATE.lock().expect("accueil").opening.is_some() {
        crate::pulse::beat(window, ANIMATE);
    } else {
        crate::pulse::stop(window);
    }
}

/* ---- Le dessin --------------------------------------------------------- */

/// Ce qui pose l'accueil : la toile, ce qu'on montre, où on en est, et ce
/// qui répond au clic une fois posé.
struct Painter<'a> {
    canvas: &'a Canvas,
    scale: f32,
    colours: Palette,
    seen: &'a Seen,
    state: &'a State,
    clickables: Vec<(Target, Rect)>,
    /// Ce que chaque chose défilante mesure, relevé au passage et rendu
    /// à l'état une fois la marche finie.
    measures: Vec<(Scroller, f32, f32, f32)>,
    /// Faux quand un dialogue est ouvert : la page derrière ne répond
    /// plus au clic, et ce qui est dessiné dessous ne s'allume plus sous
    /// la souris.
    live: bool,
    /// Vrai quand la marche ne fait que mesurer : rien n'est posé, et ce
    /// qui revient est la hauteur que ça prendrait.
    silent: bool,
}

impl Painter<'_> {
    /// Une longueur du système de design, en vrais pixels.
    fn px(&self, page: f32) -> f32 {
        page * self.scale
    }

    /// Une plume de cette taille de page.
    fn pen(&self, size: f32) -> Pen {
        Pen::of(self.px(size))
    }

    fn body(&self) -> Pen {
        self.pen(design::BODY)
    }

    fn caption(&self) -> Pen {
        self.pen(design::CAPTION)
    }

    fn subtitle(&self) -> Pen {
        self.pen(design::SUBTITLE).in_bold()
    }

    /// L'étiquette d'une section : petite, en capitales, écartée, et
    /// jamais criarde.
    fn section(&self, at: Rect, text: &str) {
        self.draw_text(
            &text.to_uppercase(),
            self.caption().in_bold().spaced(0.08),
            self.colours.text_faint,
            at,
        );
    }

    /// La hauteur d'une ligne écrite de cette plume.
    fn line_height(&self, pen: Pen) -> f32 {
        self.canvas.line_height(pen)
    }

    /// La hauteur d'un bloc replié à cette largeur.
    fn height_of(&self, text: &str, pen: Pen, width: f32) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        self.canvas.height_of(text, pen, width)
    }

    /// Écrit un bloc dans cette largeur, à partir de ce haut, et rend ce
    /// qu'il a pris.
    fn block(&self, left: f32, top: f32, width: f32, text: &str, pen: Pen, ink: Colour) -> f32 {
        let height = self.height_of(text, pen, width);
        if height > 0.0 {
            self.draw_text(text, pen, ink, Rect::at(left, top, width, height));
        }
        height
    }

    /// Une carte : son ombre, son fond et son trait.
    fn card(&self, at: Rect) {
        let radius = self.px(design::RADIUS_LARGE);
        self.shadow(at, radius, self.colours.shadow_1);
        self.fill(at, radius, self.colours.surface_1);
        self.stroke(at, radius, self.colours.border);
    }

    /// Une carte qui attend d'être remplie : pas de fond, un trait en
    /// pointillés.
    fn waiting_card(&self, at: Rect) {
        let radius = self.px(design::RADIUS_LARGE);
        self.dashed(at, radius, self.colours.border_strong);
    }

    /// Un trait de séparation, sur toute cette largeur.
    fn separator(&self, left: f32, top: f32, width: f32) {
        self.fill(
            Rect::at(left, top, width, self.px(layout::HAIRLINE)),
            0.0,
            self.colours.border,
        );
    }

    /// Si cette chose est sous la main, et si elle est enfoncée.
    fn under_the_hand(&self, target: &Target) -> bool {
        self.live && self.state.hover.as_ref() == Some(target)
    }

    fn is_pressed(&self, target: &Target) -> bool {
        self.live && self.state.pressed.as_ref() == Some(target)
    }

    /// Note que ceci répond au clic.
    ///
    /// Jamais pendant une mesure : une chose mesurée n'est pas posée, et
    /// ce qui n'est pas posé ne peut pas être cliqué. Un dialogue est
    /// mesuré tout entier avant d'être dessiné, à un endroit qui n'est
    /// pas le sien, et prendre ces places-là pour des boutons rendrait
    /// cliquable un coin de fenêtre où il n'y a rien.
    fn answers(&mut self, target: Target, at: Rect) {
        if self.live && !self.silent {
            self.clickables.push((target, at));
        }
    }

    /// La pastille de présence.
    fn dot(&self, at: Rect, ink: Colour, live: bool) {
        let radius = (at.right - at.left) / 2.0;
        if live {
            let ring = self.px(layout::RING);
            self.fill(at.grown(ring), radius + ring, ink.faded(0.18));
        }
        self.fill(at, radius, ink);
    }
}

/// Ce qu'un bouton est : ce qui appelle le clic, ce qui l'accompagne, et
/// ce qui prévient avant de détruire.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Primary,
    Quiet,
    Warning,
}

impl Painter<'_> {
    /// Ce qu'un bouton prend de large : son mot et ce qui l'entoure.
    fn button_width(&self, text: &str, big: bool) -> f32 {
        let pen = if big { self.subtitle() } else { self.body() };
        let around = if big {
            design::SPACE_5
        } else {
            design::SPACE_4
        };
        self.canvas.width_of(text, pen) + self.px(around) * 2.0
    }

    /// Un bouton portant un mot, à cet endroit.
    fn button(&mut self, at: Rect, text: &str, kind: Kind, target: Target, enabled: bool) {
        let radius = self.px(design::RADIUS_SMALL);
        let colours = self.colours;
        let hovered = enabled && self.under_the_hand(&target);
        let (background, ink, edge) = match kind {
            Kind::Primary if !enabled => (colours.surface_3, colours.text_faint, None),
            Kind::Primary if hovered => (colours.accent_bright, colours.on_accent, None),
            Kind::Primary => (colours.accent, colours.on_accent, None),
            Kind::Quiet if hovered => {
                (colours.surface_2, colours.text, Some(colours.border_strong))
            }
            Kind::Quiet => (
                Colour::TRANSPARENT,
                colours.text_soft,
                Some(colours.border_strong),
            ),
            Kind::Warning => (
                Colour::TRANSPARENT,
                colours.warning,
                Some(colours.warning.mixed_with(colours.border, 0.55)),
            ),
        };
        let opacity = if enabled { 1.0 } else { 0.45 };
        self.fill(at, radius, background.faded(opacity));
        if let Some(edge) = edge {
            self.stroke(at, radius, edge.faded(opacity));
        }
        let big = at.bottom - at.top > self.px((layout::BUTTON + layout::BIG_BUTTON) / 2.0);
        let pen = if big { self.subtitle() } else { self.body() }.aligned(Align::Centre);
        self.draw_text(text, pen, ink.faded(opacity), at);
        if enabled {
            self.answers(target, at);
        }
    }

    /// Un bouton qui ne porte qu'un dessin, et garde la même hauteur que
    /// ceux qui portent un mot.
    fn icon_button(&mut self, at: Rect, icon: &'static Icon, target: Target, quiet: bool) {
        let radius = self.px(design::RADIUS_SMALL);
        let hovered = self.under_the_hand(&target);
        if hovered {
            self.fill(at, radius, self.colours.surface_2);
        }
        let ink = if hovered {
            self.colours.text
        } else if quiet {
            self.colours.text_soft.faded(0.4)
        } else {
            self.colours.text_soft
        };
        let side = self.px(layout::GLYPH);
        let middle = ((at.left + at.right) / 2.0, (at.top + at.bottom) / 2.0);
        self.icon(
            icon,
            Rect::at(middle.0 - side / 2.0, middle.1 - side / 2.0, side, side),
            ink,
        );
        self.answers(target, at);
    }

    /// L'interrupteur : son rail, et le pouce qui glisse dedans.
    fn switch(&mut self, left: f32, middle: f32, button: Toggle) -> Rect {
        let (width, height) = (self.px(layout::SWITCH.0), self.px(layout::SWITCH.1));
        let at = Rect::at(left, middle - height / 2.0, width, height);
        let is_on = button.is_on(self.seen, self.state);
        let enabled = button.enabled(self.seen, self.state);
        let opacity = if enabled { 1.0 } else { 0.45 };
        let radius = height / 2.0;
        let (background, stroke, thumb) = if is_on {
            (
                self.colours.accent,
                self.colours.accent,
                self.colours.on_accent,
            )
        } else {
            (
                self.colours.surface_3,
                self.colours.border_strong,
                self.colours.text_soft,
            )
        };
        self.fill(at, radius, background.faded(opacity));
        self.stroke(at, radius, stroke.faded(opacity));

        let side = self.px(layout::THUMB);
        let slack = self.px(layout::SLACK);
        let x = if is_on {
            at.right - slack - side
        } else {
            at.left + slack
        };
        self.fill(
            Rect::at(x, at.top + slack, side, side),
            side / 2.0,
            thumb.faded(opacity),
        );
        if enabled {
            self.answers(Target::Switch(button), at);
        }
        at
    }

    /// Ce qu'un choix segmenté prend de large.
    fn segments_width(&self, target: Pick) -> f32 {
        let around = self.px(layout::AROUND);
        let sides: f32 = target
            .words()
            .iter()
            .map(|text| self.side_width(text))
            .sum();
        sides + around * 2.0 + self.px(layout::AROUND) * (target.words().len() as f32 - 1.0)
    }

    fn side_width(&self, text: &str) -> f32 {
        self.canvas.width_of(text, self.caption()) + self.px(design::SPACE_3) * 2.0
    }

    /// Un choix segmenté, posé à partir de ce bord droit.
    fn segments(&mut self, right: f32, middle: f32, target: Pick) -> Rect {
        let width = self.segments_width(target);
        let around = self.px(layout::AROUND);
        let height = self.px(layout::SEGMENT) + around * 2.0;
        let at = Rect::at(right - width, middle - height / 2.0, width, height);
        let radius = self.px(design::RADIUS_SMALL);
        let enabled = target.enabled(self.seen);
        let opacity = if enabled { 1.0 } else { 0.45 };
        self.fill(at, radius, self.colours.surface_2.faded(opacity));
        self.stroke(at, radius, self.colours.border.faded(opacity));

        let picked = target.current(self.seen, self.state);
        let mut x = at.left + around;
        for (rank, text) in target.words().iter().enumerate() {
            let side = Rect::at(
                x,
                at.top + around,
                self.side_width(text),
                self.px(layout::SEGMENT),
            );
            let this_one = Target::Segment(target, rank);
            let ink = if picked == Some(rank) {
                let round = self.px(layout::SEGMENT_RADIUS);
                self.shadow(side, round, self.colours.shadow_1);
                self.fill(side, round, self.colours.surface_1.faded(opacity));
                self.colours.text
            } else if enabled && self.under_the_hand(&this_one) {
                self.colours.text_soft
            } else {
                self.colours.text_faint
            };
            self.draw_text(
                text,
                self.caption().aligned(Align::Centre),
                ink.faded(opacity),
                side,
            );
            if enabled {
                self.answers(this_one, side);
            }
            x = side.right + around;
        }
        at
    }

    /// Un bandeau : ce qu'il a à dire, et de quoi y remédier quand il y a
    /// quelque chose à faire.
    fn banner(
        &mut self,
        left: f32,
        top: f32,
        width: f32,
        text: &str,
        alert: bool,
        action: Option<(&str, Target)>,
    ) -> f32 {
        let inside = self.px(design::SPACE_4);
        let button = action.map(|(text, target)| (self.button_width(text, false), text, target));
        let button_room = button
            .as_ref()
            .map_or(0.0, |(width, _, _)| width + self.px(design::SPACE_4));
        let text_width = width - inside * 2.0 - button_room;
        let text_height = self.height_of(text, self.body(), text_width);
        let height = (text_height + self.px(design::SPACE_3) * 2.0).max(if button.is_some() {
            self.px(layout::BUTTON) + self.px(design::SPACE_3) * 2.0
        } else {
            0.0
        });
        let at = Rect::at(left, top, width, height);
        let radius = self.px(design::RADIUS);
        let (background, edge) = if alert {
            (
                self.colours.error.mixed_with(self.colours.surface_2, 0.08),
                self.colours.error.mixed_with(self.colours.border, 0.4),
            )
        } else {
            (self.colours.surface_2, self.colours.border)
        };
        self.fill(at, radius, background);
        self.stroke(at, radius, edge);
        self.draw_text(
            text,
            self.body(),
            self.colours.text_soft,
            Rect::at(
                at.left + inside,
                (at.top + at.bottom) / 2.0 - text_height / 2.0,
                text_width,
                text_height,
            ),
        );
        if let Some((button_width, text, target)) = button {
            let button_height = self.px(layout::BUTTON);
            self.button(
                Rect::at(
                    at.right - inside - button_width,
                    (at.top + at.bottom) / 2.0 - button_height / 2.0,
                    button_width,
                    button_height,
                ),
                text,
                Kind::Quiet,
                target,
                true,
            );
        }
        height
    }

    /// L'ascenseur d'une chose qui défile, quand il y a plus à voir que
    /// de place.
    ///
    /// `corner` est l'arrondi de ce qui défile : le pouce s'arrête là où
    /// le coin commence, faute de quoi il dépasserait de la forme qu'il
    /// longe.
    fn scrollbar(&mut self, which: Scroller, at: Rect, content: f32, corner: f32) {
        let visible = at.bottom - at.top;
        if content <= visible + 1.0 {
            self.measures.push((which, content, visible, 0.0));
            return;
        }
        let width = self.px(layout::SCROLLBAR);
        let slack = self.px(design::SPACE_1);
        let rail = (visible - corner * 2.0).max(0.0);
        let top = thumb_of(rail, visible, content, self.scale);
        let travel = rail - top;
        self.measures.push((which, content, visible, travel));
        let position = (self.state.scroll(which) / (content - visible)).clamp(0.0, 1.0);
        let thumb = Rect::at(
            at.right - width - slack,
            at.top + corner + travel * position,
            width,
            top,
        );
        let hovered = self.under_the_hand(&Target::Scrollbar(which));
        let ink = if hovered {
            self.colours.text_faint
        } else {
            self.colours.border_strong
        };
        self.fill(thumb, width / 2.0, ink);
        self.answers(Target::Scrollbar(which), thumb);
    }
}

/// La hauteur du pouce d'un ascenseur : sur son rail, la part de ce
/// qu'on voit dans ce qu'il y a, et jamais si petit qu'on ne puisse plus
/// l'attraper.
fn thumb_of(rail: f32, visible: f32, content: f32, scale: f32) -> f32 {
    (rail * visible / content)
        .max(design::SPACE_5 * scale)
        .min(rail)
}

/* ---- La page ----------------------------------------------------------- */

/// Dessine tout ce qui est à l'écran et rend ce qui répond au clic.
fn paint_page(canvas: &Canvas, width: f32, height: f32, colours: Palette) -> Vec<(Target, Rect)> {
    let nothing = Seen::default();
    let guard = SEEN.lock().expect("accueil");
    let seen = guard.as_ref().unwrap_or(&nothing);
    let mut state = STATE.lock().expect("accueil");

    let (clickables, measures) = {
        let dialogue_open = state.screen != Screen::Home;
        let opening = state.opening.is_some();
        let mut painter = Painter {
            canvas,
            scale: scale(),
            colours,
            seen,
            state: &state,
            clickables: Vec::new(),
            measures: Vec::new(),
            live: !dialogue_open && !opening,
            silent: false,
        };
        painter.page(width, height);
        if dialogue_open && !opening {
            // Le fond noirci : ce qui est derrière n'est plus d'actualité
            // et ne répond plus au clic, ce que la page disait déjà en
            // rendant son dialogue modal.
            painter.fill(
                Rect::at(0.0, 0.0, width, height),
                0.0,
                Colour::BLACK.faded(layout::VEIL),
            );
            painter.live = true;
            painter.dialogue(width, height);
        }
        if opening {
            painter.opening(width, height);
        }
        (painter.clickables, painter.measures)
    };
    for (which, content, visible, travel) in measures {
        state.keep(which, content, visible, travel);
    }
    clickables
}

impl Painter<'_> {
    /// L'accueil lui-même : ce qu'est cet ordinateur, puis les autres.
    fn page(&mut self, width: f32, height: f32) {
        let side = self.px(layout::SIDE);
        let inside = (width - side * 2.0).clamp(self.px(200.0), self.px(layout::PAGE));
        let x = ((width - inside) / 2.0).max(side);
        let start = self.px(layout::EDGE) - self.state.scroll;
        let mut y = start;

        y += self.header(x, y, inside);
        y += self.px(design::SPACE_5);
        y += self.this_computer(x, y, inside);
        y += self.px(design::SPACE_5);
        y += self.my_computers(x, y, inside, height);

        // La version est sous les yeux sans jamais peser : en bas de la
        // fenêtre quand la page n'en remplit pas la hauteur, et à la
        // suite du reste quand elle la dépasse.
        let version = self.line_height(self.caption());
        let content = y - start + self.px(layout::EDGE) + version;
        let version_y =
            (height - self.px(layout::EDGE) - version).max(y + self.px(design::SPACE_2));
        let (text, ink) = self.the_version();
        self.draw_text(
            &text,
            self.caption().aligned(Align::Centre),
            ink,
            Rect::at(x, version_y, inside, version),
        );

        self.scrollbar(
            Scroller::Page,
            Rect::at(0.0, 0.0, width, height),
            content,
            0.0,
        );
    }

    /// La marque, le nom du produit, et les deux commandes rangées à
    /// droite : à portée, jamais au centre de l'attention.
    fn header(&mut self, x: f32, y: f32, width: f32) -> f32 {
        let brand = self.px(layout::BRAND);
        self.brand(Rect::at(x, y, brand, brand));

        let button = self.px(layout::BUTTON);
        let gap = self.px(design::SPACE_2);
        let middle = y + (brand - button) / 2.0;
        let settings = Rect::at(x + width - button, middle, button, button);
        let journal = settings.shifted(-(button + gap), 0.0);
        self.icon_button(settings, &icons::SETTINGS, Target::OpenSettings, false);
        self.icon_button(journal, &icons::JOURNAL, Target::OpenJournal, false);

        let since = x + brand + self.px(design::SPACE_3);
        self.draw_text(
            "ZyrDesk",
            self.pen(design::TITLE).in_bold().ellipsized(),
            self.colours.text,
            Rect {
                left: since,
                top: y,
                right: journal.left - gap,
                bottom: y + brand,
            },
        );
        brand
    }

    /// Ce qu'est cet ordinateur : son nom, son état, son empreinte, et ce
    /// qu'il reste à faire pour qu'il marche.
    fn this_computer(&mut self, x: f32, y: f32, width: f32) -> f32 {
        let tag = self.line_height(self.caption().in_bold());
        self.section(Rect::at(x, y, width, tag), "Cet ordinateur");
        let mut taken = tag + self.px(design::SPACE_3);
        taken += self.machine_card(x, y + taken, width);
        for (rank, to_do) in what_is_missing(self.seen).into_iter().enumerate() {
            taken += self.px(design::SPACE_3);
            taken += self.banner(
                x,
                y + taken,
                width,
                to_do.text,
                true,
                Some((to_do.button, Target::ToFix(rank))),
            );
        }
        taken
    }

    /// La carte de cette machine : son identité en haut, son empreinte en
    /// bas sur son propre fond.
    fn machine_card(&mut self, x: f32, y: f32, width: f32) -> f32 {
        let inside = self.px(design::SPACE_5);
        let (name, state) = (
            self.line_height(self.subtitle()),
            self.line_height(self.body()),
        );
        let gap = self.px(design::SPACE_1);
        let top_part = (name + gap + state).max(self.px(layout::SWITCH.1)) + inside * 2.0;

        let caption = self.line_height(self.caption());
        let copy_width = self.button_width("Copier", false);
        let seen = self.seen;
        let said = seen.machine.as_ref();
        let fingerprint = said.map_or_else(String::new, |said| {
            if said.fingerprint.is_empty() {
                "indisponible".to_string()
            } else {
                said.fingerprint.clone()
            }
        });
        let fingerprint_width = width - inside * 2.0 - copy_width - self.px(design::SPACE_4);
        let fingerprint_pen = self.caption().monospaced().spaced(0.02);
        let fingerprint_height = self.height_of(&fingerprint, fingerprint_pen, fingerprint_width);
        let bottom_part = (caption + gap + fingerprint_height).max(self.px(layout::BUTTON))
            + self.px(design::SPACE_3) * 2.0;

        let at = Rect::at(x, y, width, top_part + bottom_part);
        self.card(at);
        let bottom = Rect {
            top: at.top + top_part,
            ..at
        };
        // Le fond du bas de la carte : le même rectangle arrondi, vu au
        // travers de sa moitié basse, ce qu'aucun rectangle arrondi ne
        // sait être à lui seul.
        let radius = self.px(design::RADIUS_LARGE);
        if !self.silent {
            let (canvas, background) = (self.canvas, self.colours.surface_2);
            canvas.clipped(bottom, || canvas.fill(at, radius, background));
        }
        self.separator(bottom.left, bottom.top, width);
        self.stroke(at, radius, self.colours.border);

        // Le haut : le nom, l'état, et l'interrupteur d'accès distant.
        let middle = (at.top + bottom.top) / 2.0;
        let access_caption = "Accès distant";
        let access_width = self.canvas.width_of(access_caption, self.caption());
        let switch = self.switch(
            at.right - inside - self.px(layout::SWITCH.0),
            middle,
            Toggle::Access,
        );
        self.draw_text(
            access_caption,
            self.caption(),
            self.colours.text_soft,
            Rect::at(
                switch.left - self.px(design::SPACE_3) - access_width,
                middle - caption / 2.0,
                access_width,
                caption,
            ),
        );

        let name_width = switch.left - self.px(design::SPACE_4) - (at.left + inside);
        let name_top = middle - (name + gap + state) / 2.0;
        let status_text =
            said.map_or_else(|| "Recherche du service…".to_string(), words_of_the_state);
        self.draw_text(
            &said.map_or_else(|| "…".to_string(), |said| said.name.clone()),
            self.subtitle().ellipsized(),
            self.colours.text,
            Rect::at(at.left + inside, name_top, name_width, name),
        );
        let dot = self.px(layout::DOT);
        let below_the_text = name_top + name + gap;
        self.dot(
            Rect::at(
                at.left + inside,
                below_the_text + (state - dot) / 2.0,
                dot,
                dot,
            ),
            said.map_or(self.colours.offline, |said| {
                colour_of_the_state(said, self.colours)
            }),
            said.is_some_and(|said| said.hosting),
        );
        self.draw_text(
            &status_text,
            self.body().ellipsized(),
            self.colours.text_soft,
            Rect::at(
                at.left + inside + dot + self.px(design::SPACE_2),
                below_the_text,
                name_width - dot - self.px(design::SPACE_2),
                state,
            ),
        );

        // Le bas : l'empreinte, et de quoi la copier.
        let bottom_top =
            (bottom.top + bottom.bottom) / 2.0 - (caption + gap + fingerprint_height) / 2.0;
        self.draw_text(
            "Empreinte de cet ordinateur",
            self.caption(),
            self.colours.text_soft,
            Rect::at(bottom.left + inside, bottom_top, fingerprint_width, caption),
        );
        self.draw_text(
            &fingerprint,
            fingerprint_pen,
            self.colours.text_faint,
            Rect::at(
                bottom.left + inside,
                bottom_top + caption + gap,
                fingerprint_width,
                fingerprint_height,
            ),
        );
        let height = self.px(layout::BUTTON);
        let can = said.is_some_and(|said| !said.fingerprint.is_empty());
        self.button(
            Rect::at(
                bottom.right - inside - copy_width,
                (bottom.top + bottom.bottom) / 2.0 - height / 2.0,
                copy_width,
                height,
            ),
            if self.state.copied.as_ref().map(|(target, _)| target)
                == Some(&Target::CopyFingerprint)
            {
                "Copié"
            } else {
                "Copier"
            },
            Kind::Quiet,
            Target::CopyFingerprint,
            can,
        );
        top_part + bottom_part
    }
}

/// Ce qui empêche le produit de marcher, dit en clair et avec de quoi y
/// remédier.
///
/// Hors de la marche parce que le clic la relit : le bouton d'un bandeau
/// ne porte que son rang, et c'est ici que ce rang retrouve ce qu'il
/// répare.
fn what_is_missing(seen: &Seen) -> Vec<ToDo> {
    let mut missings = Vec::new();
    if let Some(said) = seen.machine.as_ref() {
        if said.unreachable.is_some() {
            missings.push(ToDo {
                text: "Le service ZyrDesk ne tourne pas. Cet ordinateur ne peut ni être \
                        contrôlé ni en contrôler un autre.",
                button: "Démarrer le service",
                remedy: Remedy::StartTheService,
            });
        } else if said.wanted && said.holdup == "engineMissing" {
            missings.push(ToDo {
                text: "Le moteur hôte n'est pas installé : cet ordinateur ne peut pas être \
                        contrôlé. Déposez-le dans son dossier, il sera repris tout seul.",
                button: "Ouvrir le dossier",
                remedy: Remedy::HostEngine,
            });
        } else if said.wanted && said.holdup == "engineWontStand" {
            missings.push(ToDo {
                text: "Le moteur hôte ne tient pas en marche. Coupez puis rallumez l'accès \
                        distant pour réessayer ; le journal dit pourquoi.",
                button: "Voir le journal",
                remedy: Remedy::SeeTheJournal,
            });
        }
    }
    if seen.engines.as_ref().is_some_and(|said| !said.client_here) {
        missings.push(ToDo {
            text: "Le moteur client n'est pas installé : cet ordinateur ne peut en contrôler \
                    aucun autre.",
            button: "Ouvrir le dossier",
            remedy: Remedy::ClientEngine,
        });
    }
    missings
}
impl Painter<'_> {
    /// Les autres ordinateurs, et ce qui se passe en ce moment.
    ///
    /// Une session en cours passe avant la liste : c'est la première
    /// chose à voir en ouvrant la fenêtre, y compris quand ce n'est pas
    /// elle qui l'a lancée.
    fn my_computers(&mut self, x: f32, y: f32, width: f32, height: f32) -> f32 {
        let tag = self.line_height(self.caption().in_bold());
        self.section(Rect::at(x, y, width, tag), "Mes ordinateurs");
        let mut taken = tag + self.px(design::SPACE_3);

        for session in &self.seen.sessions {
            taken += self.session_card(x, y + taken, width, session);
            taken += self.px(design::SPACE_3);
        }
        if let Some(notice) = &self.state.notice {
            let text = notice.text.clone();
            let is_trouble = notice.is_trouble;
            taken += self.banner(x, y + taken, width, &text, is_trouble, None);
            taken += self.px(design::SPACE_3);
        }

        if self.seen.peers.is_empty() && !self.seen.busy(self.state) {
            return taken + self.no_computer(x, y + taken, width, height - y - taken);
        }
        // Ce qu'un contact a partagé se range à part : ce n'est pas un
        // ordinateur à soi, et le dire sur chaque carte ne suffit pas à
        // les distinguer d'un coup d'oeil.
        let (mine_ranks, shared): (Vec<usize>, Vec<usize>) =
            (0..self.seen.peers.len()).partition(|rank| {
                self.seen.peers[*rank]
                    .account
                    .as_ref()
                    .is_none_or(|account| account.shared_by.is_none())
            });
        taken += self.grid(x, y + taken, width, &mine_ranks, true);
        if !shared.is_empty() {
            taken += self.px(design::SPACE_5);
            self.section(Rect::at(x, y + taken, width, tag), "Partagés avec moi");
            taken += tag + self.px(design::SPACE_3);
            taken += self.grid(x, y + taken, width, &shared, false);
        }
        taken
    }

    /// Le bandeau d'une session en cours.
    ///
    /// La carte de l'ordinateur, plus bas, porte déjà son adresse et son
    /// état : ceci dit ce qui se passe, il ne le répète pas.
    fn session_card(&mut self, x: f32, y: f32, width: f32, session: &Ongoing) -> f32 {
        let inside = self.px(design::SPACE_5);
        let (name, text) = (
            self.line_height(self.subtitle()),
            self.line_height(self.caption()),
        );
        let gap = self.px(design::SPACE_1);
        let at = Rect::at(x, y, width, name + gap + text + inside * 2.0);
        let radius = self.px(design::RADIUS_LARGE);
        self.shadow(at, radius, self.colours.shadow_1);
        self.fill(
            at,
            radius,
            self.colours.online.mixed_with(self.colours.surface_1, 0.07),
        );
        self.stroke(
            at,
            radius,
            self.colours.online.mixed_with(self.colours.border, 0.4),
        );

        let dot = self.px(layout::DOT);
        self.dot(
            Rect::at(
                at.left + inside,
                at.top + inside + (name - dot) / 2.0,
                dot,
                dot,
            ),
            self.colours.online,
            true,
        );
        let since = at.left + inside + dot + self.px(design::SPACE_2);
        self.draw_text(
            &format!("Session en cours vers {}", self.seen.name_of(session)),
            self.subtitle().ellipsized(),
            self.colours.text,
            Rect::at(since, at.top + inside, at.right - inside - since, name),
        );
        self.draw_text(
            &format!(
                "Ouverte depuis {}{}. Fermer la fenêtre termine la session.",
                duration(session.since),
                path_of(session)
            ),
            self.caption(),
            self.colours.text_soft,
            Rect::at(
                at.left + inside,
                at.top + inside + name + gap,
                width - inside * 2.0,
                text,
            ),
        );
        at.bottom - at.top
    }

    /// La grille de ces ordinateurs-là, par leur rang, et la tuile qui en
    /// ajoute un quand elle a sa place ici.
    fn grid(&mut self, x: f32, y: f32, width: f32, ranks: &[usize], with_add: bool) -> f32 {
        let gap = self.px(design::SPACE_3);
        let narrowest = self.px(layout::CARD);
        let columns = (((width + gap) / (narrowest + gap)).floor() as usize).max(1);
        let column = (width - gap * (columns as f32 - 1.0)) / columns as f32;
        let inside = self.px(design::SPACE_4);
        let (name, address) = (
            self.line_height(self.subtitle()),
            self.line_height(self.caption()),
        );
        let height = inside * 2.0
            + name
            + self.px(design::SPACE_2)
            + address
            + self.px(design::SPACE_2)
            + self.px(layout::HINT);

        let how_many = ranks.len() + usize::from(with_add);
        for cell in 0..how_many {
            let at = Rect::at(
                x + (cell % columns) as f32 * (column + gap),
                y + (cell / columns) as f32 * (height + gap),
                column,
                height,
            );
            match ranks.get(cell) {
                Some(rank) => self.computer_card(at, *rank, inside, name, address),
                None => self.add_tile(at),
            }
        }
        let lines = how_many.div_ceil(columns) as f32;
        lines * height + (lines - 1.0) * gap
    }

    /// La pastille d'un ordinateur : verte quand il répond ou que le
    /// compte le dit prêt, orange quand le compte le dit en ligne sans
    /// accès distant, grise sinon. Le mot sous le nom dit pourquoi.
    fn presence_of(&self, peer: &Peer) -> (Colour, bool) {
        if peer.seen {
            return (self.colours.online, true);
        }
        match &peer.account {
            Some(account) if account.online && account.access == Access::Ready => {
                (self.colours.online, true)
            }
            Some(account) if account.online => (self.colours.warning, false),
            _ => (self.colours.offline, false),
        }
    }

    /// Une carte d'ordinateur : cliquer n'importe où s'y connecte, et les
    /// boutons de son journal et de sa voie locale se posent dans un coin.
    fn computer_card(&mut self, at: Rect, rank: usize, inside: f32, name: f32, address: f32) {
        let peer = &self.seen.peers[rank];
        let busy = self.seen.busy(self.state);
        // La voie locale ne s'offre que pour un ordinateur que ce réseau
        // annonce : c'est la seule chose qu'elle sait joindre, puisque
        // c'est la seule adresse qui vienne d'ici.
        let here = peer.seen;
        let local_only = here && !busy && self.under_the_hand(&Target::Local(rank));
        let handle = self
            .seen
            .sessions
            .iter()
            .any(|session| session.fingerprint == peer.fingerprint);
        // L'inverse de « sienne » : non pas un ordinateur que cette
        // fenêtre a joint, mais celui qui la contrôle en ce moment.
        let controlling = self
            .seen
            .watching
            .iter()
            .any(|watching| watching.fingerprint == peer.fingerprint);
        let target = Target::Peer(rank);
        let hovered = !busy && self.under_the_hand(&target);
        // Enfoncée sous le doigt : un pixel vers le bas, ce que la
        // feuille de style faisait et qui est tout ce qui dit qu'un clic
        // a été pris.
        let at = if self.is_pressed(&target) {
            at.shifted(0.0, self.px(1.0))
        } else {
            at
        };

        let radius = self.px(design::RADIUS_LARGE);
        self.shadow(at, radius, self.colours.shadow_1);
        self.fill(
            at,
            radius,
            if hovered {
                self.colours.surface_2
            } else {
                self.colours.surface_1
            },
        );
        let edge = if handle {
            self.colours.online.mixed_with(self.colours.border, 0.4)
        } else if controlling {
            self.colours.warning.mixed_with(self.colours.border, 0.4)
        } else if hovered {
            self.colours.accent
        } else {
            self.colours.border
        };
        self.stroke(at, radius, edge);

        // Une carte occupée s'efface par ses mots, pour que le bouton de
        // son journal reste allumé : c'est justement pendant une session
        // qu'on veut lire ce que la machine d'en face a écrit. Celle qui
        // contrôle cet ordinateur ne s'efface pas non plus : c'est
        // justement elle qu'on veut voir.
        let opacity = if busy && !handle && !controlling {
            0.5
        } else {
            1.0
        };
        let dot = self.px(layout::DOT);
        let (ink, live) = self.presence_of(peer);
        self.dot(
            Rect::at(
                at.left + inside,
                at.top + inside + (name - dot) / 2.0,
                dot,
                dot,
            ),
            ink.faded(opacity),
            live,
        );
        let since = at.left + inside + dot + self.px(design::SPACE_2);
        let button = self.px(layout::BUTTON);
        // La place des boutons du coin est réservée : sans elle, un nom
        // un peu long passerait dessous, et il y en a un de plus quand
        // cet ordinateur est joignable d'ici, et encore un quand il
        // contrôle celui-ci.
        let buttons = self.px(design::SPACE_6)
            + if here { button } else { 0.0 }
            + if controlling { button } else { 0.0 };
        self.draw_text(
            &peer.name,
            self.subtitle().ellipsized(),
            self.colours.text.faded(opacity),
            Rect::at(
                since,
                at.top + inside,
                (at.right - buttons - since).max(0.0),
                name,
            ),
        );
        // La pastille grise ne dit rien à elle seule : ce qui l'explique
        // est écrit à côté.
        let below_the_name = at.top + inside + name + self.px(design::SPACE_2);
        self.draw_text(
            &below_the_name_of(peer),
            self.caption().ellipsized(),
            self.colours.text_soft.faded(opacity),
            Rect::at(
                at.left + inside,
                below_the_name,
                at.right - inside - (at.left + inside),
                address,
            ),
        );
        // Ce qui n'apparaît qu'au survol ne fait pas bouger la carte : sa
        // place est réservée d'avance. Le mot dit laquelle des deux voies
        // la main est en train de choisir : sans lui, la maison du coin
        // serait un dessin sans nom.
        let hint = self.px(layout::HINT);
        if handle || hovered || local_only || controlling {
            self.draw_text(
                match (handle, local_only, controlling) {
                    (true, _, _) => "Session en cours",
                    (false, true, _) => "Se connecter en local",
                    (false, false, true) => "Vous contrôle actuellement",
                    (false, false, false) => "Se connecter",
                },
                self.caption(),
                if handle {
                    self.colours.online
                } else if controlling {
                    self.colours.warning
                } else {
                    self.colours.accent
                },
                Rect::at(
                    at.left + inside,
                    below_the_name + address + self.px(design::SPACE_2),
                    at.right - inside * 2.0,
                    hint,
                ),
            );
        }

        if !busy {
            self.answers(target, at);
        }
        // Toujours là et jamais au premier plan : il attend d'être
        // cherché, et il ne s'efface pas quand une session occupe la
        // fenêtre.
        let corner = self.px(design::SPACE_3);
        self.icon_button(
            Rect::at(at.right - corner - button, at.top + corner, button, button),
            &icons::JOURNAL,
            Target::JournalOf(rank),
            !hovered,
        );
        // La maison, à côté : par ce réseau et rien d'autre. Elle suit la
        // carte plutôt que le journal, puisqu'elle ouvre une session et
        // qu'une session de plus ne s'ouvre pas.
        if here && !busy {
            self.icon_button(
                Rect::at(
                    at.right - corner - button * 2.0,
                    at.top + corner,
                    button,
                    button,
                ),
                &icons::LOCAL_NETWORK,
                Target::Local(rank),
                !hovered,
            );
        }
        // Toujours là quand cet ordinateur contrôle celui-ci, occupé ou
        // non : c'est justement là qu'on veut pouvoir le rendre. Prend
        // la place réservée après le journal et, s'il y en a une, la
        // maison locale, exactement comme le calcul de largeur plus haut
        // les a comptées.
        if controlling {
            let slot = if here { 3.0 } else { 2.0 };
            self.icon_button(
                Rect::at(
                    at.right - corner - button * slot,
                    at.top + corner,
                    button,
                    button,
                ),
                &icons::CROSS,
                Target::Disconnect(rank),
                !hovered,
            );
        }
    }

    /// La tuile qui ajoute un ordinateur : elle suit le rythme des autres
    /// sans se faire passer pour un ordinateur.
    fn add_tile(&mut self, at: Rect) {
        let busy = self.seen.busy(self.state);
        let hovered = !busy && self.under_the_hand(&Target::Add);
        if hovered {
            self.fill(at, self.px(design::RADIUS_LARGE), self.colours.surface_2);
        }
        self.waiting_card(at);
        let ink = if busy {
            self.colours.text_soft.faded(0.5)
        } else if hovered {
            self.colours.text
        } else {
            self.colours.text_soft
        };
        let sign = self.px(layout::GLYPH);
        let text = self.line_height(self.caption());
        let gap = self.px(design::SPACE_1);
        let top = (at.top + at.bottom) / 2.0 - (sign + gap + text) / 2.0;
        self.icon(
            &icons::PLUS,
            Rect::at((at.left + at.right) / 2.0 - sign / 2.0, top, sign, sign),
            ink,
        );
        self.draw_text(
            "Ajouter un ordinateur",
            self.caption().aligned(Align::Centre),
            ink,
            Rect::at(at.left, top + sign + gap, at.right - at.left, text),
        );
        if !busy {
            self.answers(Target::Add, at);
        }
    }

    /// L'écran vide : ce qu'on voit sur une machine qui n'a encore trouvé
    /// personne.
    fn no_computer(&mut self, x: f32, y: f32, width: f32, rest: f32) -> f32 {
        let drawing = self.px(layout::EMPTY.1);
        let title = self.line_height(self.subtitle());
        let text_width = width.min(self.px(420.0)) - self.px(design::SPACE_5) * 2.0;
        let text = "Les ZyrDesk allumés sur ce réseau apparaissent ici tout seuls, sans rien à \
                   recopier. Si le réseau ne laisse pas passer les annonces, ajoutez l'autre \
                   ordinateur à la main, sur les deux machines. Avec un compte, dans les \
                   réglages, vos ordinateurs apparaissent où qu'ils soient.";
        let explanation = self.height_of(text, self.caption(), text_width);
        let button = self.px(layout::BIG_BUTTON);
        let gap = self.px(design::SPACE_3);
        let inside = self.px(design::SPACE_6);
        let content =
            drawing + gap + title + gap + explanation + gap + self.px(design::SPACE_2) + button;
        let height = (content + inside * 2.0).max(rest - self.px(layout::EDGE));

        let at = Rect::at(x, y, width, height);
        self.waiting_card(at);
        let mut top = (at.top + at.bottom) / 2.0 - content / 2.0;
        let middle = (at.left + at.right) / 2.0;
        self.icon(
            &icons::NO_COMPUTER,
            Rect::at(
                middle - self.px(layout::EMPTY.0) / 2.0,
                top,
                self.px(layout::EMPTY.0),
                drawing,
            ),
            self.colours.text_faint.faded(0.6),
        );
        top += drawing + gap;
        self.draw_text(
            "Aucun ordinateur pour l'instant",
            self.subtitle().aligned(Align::Centre),
            self.colours.text,
            Rect::at(at.left, top, width, title),
        );
        top += title + gap;
        self.draw_text(
            text,
            self.caption().aligned(Align::Centre),
            self.colours.text_soft,
            Rect::at(middle - text_width / 2.0, top, text_width, explanation),
        );
        top += explanation + gap + self.px(design::SPACE_2);
        let button_width = self.button_width("Ajouter un ordinateur", true);
        self.button(
            Rect::at(middle - button_width / 2.0, top, button_width, button),
            "Ajouter un ordinateur",
            Kind::Primary,
            Target::Add,
            true,
        );
        height
    }

    /// Ce que fait tourner cette fenêtre, et ce que fait tourner le
    /// service quand les deux ne datent pas du même jour.
    fn the_version(&self) -> (String, Colour) {
        let mine = &self.seen.version;
        if mine.is_empty() {
            return (String::new(), self.colours.text_faint);
        }
        let service = self
            .seen
            .machine
            .as_ref()
            .map_or("", |said| said.service_build.as_str());
        if service.is_empty() || mine.contains(service) {
            return (mine.clone(), self.colours.text_faint);
        }
        (
            format!("{mine}, mais le service tourne encore en {service}"),
            self.colours.warning,
        )
    }
}

/// Ce que l'état de cette machine se lit.
fn words_of_the_state(said: &Standing) -> String {
    if said.unreachable.is_some() {
        return "Service arrêté".to_string();
    }
    if !said.wanted {
        return "Accès distant désactivé".to_string();
    }
    if said.hosting {
        return "Prêt à être contrôlé".to_string();
    }
    match said.holdup {
        "engineMissing" => "Moteur hôte absent".to_string(),
        "engineWontStand" => "Le moteur hôte ne démarre pas".to_string(),
        _ => "Démarrage en cours…".to_string(),
    }
}

/// Et la couleur de sa pastille. L'état ne se lit jamais à la couleur
/// seule : le texte à côté le dit.
fn colour_of_the_state(said: &Standing, colours: Palette) -> Colour {
    if said.unreachable.is_some() || !said.wanted {
        return colours.offline;
    }
    if said.hosting {
        return colours.online;
    }
    if said.holdup == "starting" {
        colours.warning
    } else {
        colours.error
    }
}

/// Ce qui s'écrit sous le nom d'un ordinateur : ce qu'on en sait, et
/// d'où il vient quand ce n'est pas du réseau.
///
/// Un ordinateur qui s'annonce montre son adresse. Un ordinateur que
/// seul le compte porte montre ce que le compte en dit : en ligne et
/// prêt, en ligne sans accès distant et pourquoi, ou hors ligne.
fn below_the_name_of(peer: &Peer) -> String {
    let origin = match &peer.account {
        Some(account) => match &account.shared_by {
            Some(by_whom) => format!("partagé par {by_whom}"),
            None => "compte".to_string(),
        },
        None if peer.written => "ajouté à la main".to_string(),
        None => String::new(),
    };
    let state = match &peer.account {
        Some(account) if !peer.seen => {
            if account.online {
                format!("en ligne · {}", account.access.explanation())
            } else {
                "hors ligne".to_string()
            }
        }
        _ => peer.address.clone(),
    };
    if origin.is_empty() {
        state
    } else {
        format!("{state} · {origin}")
    }
}

/// Où en est un appareil du compte, en mots.
fn words_of_the_presence(device: &Device) -> String {
    if device.online {
        return format!("En ligne · {}", device.access.explanation());
    }
    match device.last_seen {
        Some(seen) => format!(
            "Hors ligne · vu il y a {}",
            duration(zyr_broker::now().saturating_sub(seen))
        ),
        None => "Hors ligne".to_string(),
    }
}

/// Par où passe une session, et combien la route prend, quand le
/// service le sait.
fn path_of(session: &Ongoing) -> String {
    if session.via.is_empty() {
        return String::new();
    }
    format!(", par {} en {} ms", session.via, session.round_trip_ms)
}

/// Depuis combien de temps une session est ouverte, en mots.
fn duration(seconds: u64) -> String {
    if seconds < MINUTE {
        return "moins d'une minute".to_string();
    }
    let minutes = (seconds % HOUR) / MINUTE;
    if seconds < HOUR {
        return format!("{minutes} minute{}", if minutes > 1 { "s" } else { "" });
    }
    let hours = seconds / HOUR;
    if minutes == 0 {
        format!("{hours} h")
    } else {
        format!("{hours} h {minutes:02}")
    }
}

/* ---- Les dialogues ------------------------------------------------------ */

impl Painter<'_> {
    /// Pose le dialogue ouvert : ce qu'il porte, mesuré à la largeur
    /// qu'il aura, puis dessiné dedans.
    ///
    /// La mesure et le dessin sont la même marche : un dialogue mesuré à
    /// une largeur et dessiné à une autre se répondrait juste jusqu'au
    /// premier mot qui se replie.
    fn dialogue(&mut self, width: f32, height: f32) {
        let wanted = self.px(match self.state.screen {
            Screen::Adding | Screen::Account | Screen::Renaming => layout::DIALOGUE,
            Screen::Journal => layout::DIALOGUE_JOURNAL,
            Screen::Settings | Screen::Home => layout::DIALOGUE_SETTINGS,
        });
        let inside = self.px(design::SPACE_5);
        let margin = self.px(design::SPACE_6);
        let dialogue_width = wanted.min(width - margin);

        let content = self.inside(
            Rect::at(0.0, 0.0, dialogue_width - inside * 2.0, 0.0),
            true,
            height,
        ) + inside * 2.0;
        let dialogue_height = content.min(height - margin);
        let at = Rect::at(
            (width - dialogue_width) / 2.0,
            (height - dialogue_height) / 2.0,
            dialogue_width,
            dialogue_height,
        );
        self.dialogue_background(at);
        // Serré à sa carte : ce qui a défilé au-dessus du haut du
        // dialogue, ou sous son bas, se dessinerait sinon par-dessus le
        // fond noirci.
        let canvas = self.canvas;
        canvas.clipped(at, || {
            self.inside(
                Rect {
                    left: at.left + inside,
                    top: at.top + inside - self.state.dialogue_scroll,
                    right: at.right - inside,
                    bottom: at.bottom,
                },
                false,
                height,
            );
        });
        self.scrollbar(
            Scroller::Dialogue,
            at,
            content,
            self.px(design::RADIUS_LARGE),
        );
    }

    /// Ce que le dialogue ouvert porte, mesuré quand `silent` et dessiné
    /// sinon.
    fn inside(&mut self, at: Rect, silent: bool, height: f32) -> f32 {
        let before = self.silent;
        self.silent = before || silent;
        let taken = match self.state.screen {
            Screen::Adding => self.in_the_adding(at),
            Screen::Journal => self.in_the_journal(at, height),
            Screen::Settings => self.in_the_settings(at),
            Screen::Account => self.in_the_account(at),
            Screen::Renaming => self.in_the_renaming(at),
            // Il n'y a alors aucun dialogue, et rien ne l'appelle : dit
            // plutôt que rangé sous un autre écran, qu'il ne serait pas.
            Screen::Home => 0.0,
        };
        self.silent = before;
        taken
    }

    /// Pose le fond d'un dialogue.
    fn dialogue_background(&mut self, at: Rect) {
        let radius = self.px(design::RADIUS_LARGE);
        self.shadow(at, radius, self.colours.shadow_2);
        self.fill(at, radius, self.colours.surface_1);
        self.stroke(at, radius, self.colours.border_strong);
    }

    /// L'en-tête d'un dialogue : ce dont il s'agit, et la croix qui le
    /// ferme.
    fn dialogue_header(&mut self, at: Rect, title: &str, text: &str) -> f32 {
        let button = self.px(layout::BUTTON);
        let text_width = at.right - at.left - button - self.px(design::SPACE_4);
        let title_height = self.line_height(self.subtitle());
        let explanation = self.height_of(text, self.caption(), text_width);
        let gap = self.px(design::SPACE_1);

        self.draw_text(
            title,
            self.subtitle(),
            self.colours.text,
            Rect::at(at.left, at.top, text_width, title_height),
        );
        self.block(
            at.left,
            at.top + title_height + gap,
            text_width,
            text,
            self.caption(),
            self.colours.text_soft,
        );
        self.icon_button(
            Rect::at(at.right - button, at.top, button, button),
            &icons::CROSS,
            Target::Close,
            false,
        );
        (title_height + gap + explanation).max(button)
    }

    /// Ce qu'une chose prendrait, sans la poser.
    ///
    /// Pour ce qui doit être mesuré avant que ce qui vient au-dessus soit
    /// posé : le dialogue se dessine de haut en bas, et rien d'autre ne
    /// permet de rendre à l'un la place qu'un autre prendra plus bas.
    fn measure_only(&mut self, pass: impl FnOnce(&mut Self) -> f32) -> f32 {
        let before = self.silent;
        self.silent = true;
        let taken = pass(self);
        self.silent = before;
        taken
    }

    /// Les noms que la page ouverte porte, en rangées qui se replient, et
    /// rendus de la hauteur qu'ils ont prise.
    ///
    /// Cochés plutôt que tapés, parce que ce sont eux qu'on veut neuf
    /// fois sur dix et que les retenir par coeur n'est le travail de
    /// personne. Rien du tout pour une page qui n'en annonce aucun, ce
    /// qui est le cas d'une page venue d'une moitié plus ancienne du
    /// produit : tout se tape alors, comme avant.
    fn the_tags(&mut self, x: f32, y: f32, width: f32) -> f32 {
        let names = self.state.tags.clone();
        if names.is_empty() {
            return 0.0;
        }
        let ticked: Vec<String> = text_of_the_field(Field::Sift)
            .split_whitespace()
            .map(str::to_string)
            .collect();
        let height = self.px(layout::BUTTON);
        let between = self.px(design::SPACE_2);
        let (mut line, mut bottom) = (x, y);
        for (rank, name) in names.iter().enumerate() {
            let taken = self.button_width(name, false);
            // Replié dès qu'un nom déborderait, jamais avant : un
            // dialogue étroit en met deux par rangée et un large les met
            // tous sur une, sans que rien n'ait à être compté d'avance.
            if line > x && line + taken > x + width {
                line = x;
                bottom += height + between;
            }
            self.button(
                Rect::at(line, bottom, taken, height),
                name,
                if ticked.iter().any(|text| text == name) {
                    Kind::Primary
                } else {
                    Kind::Quiet
                },
                Target::Tag(rank),
                true,
            );
            line += taken + between;
        }
        bottom + height + self.px(design::SPACE_3) - y
    }

    /// Une rangée d'actions, rangées à droite, et rendue de sa hauteur.
    ///
    /// Ce qui détruit se pose à gauche, écarté du reste : il ne doit pas
    /// se trouver sous le doigt qui vise à côté.
    fn actions(&mut self, at: Rect, top: f32, actions: &[(String, Kind, Target, bool)]) -> f32 {
        let height = self.px(layout::BUTTON);
        let gap = self.px(design::SPACE_3);
        let mut right = at.right;
        for (text, kind, target, enabled) in actions.iter().rev() {
            let width = self.button_width(text, false);
            self.button(
                Rect::at(right - width, top, width, height),
                text,
                *kind,
                target.clone(),
                *enabled,
            );
            right -= width + gap;
        }
        height
    }

    /// Ajouter un ordinateur, et retirer ceux qui ont été ajoutés.
    fn in_the_adding(&mut self, at: Rect) -> f32 {
        let width = at.right - at.left;
        let mut y = at.top;
        let gap = self.px(design::SPACE_4);

        let title = self.line_height(self.subtitle());
        self.draw_text(
            "Ajouter un ordinateur",
            self.subtitle(),
            self.colours.text,
            Rect::at(at.left, y, width, title),
        );
        y += title + self.px(design::SPACE_2);
        y += self.block(
            at.left,
            y,
            width,
            "À n'utiliser que si l'ordinateur n'apparaît pas tout seul. Les deux informations se \
             lisent dans sa fenêtre ZyrDesk. À faire sur les deux machines : chacune doit \
             connaître l'autre.",
            self.caption(),
            self.colours.text_soft,
        );
        y += gap;

        for field in Field::ADD {
            y += self.field(at.left, y, width, field);
            y += gap;
        }

        let towards_it = !text_of_the_field(Field::Address).trim().is_empty();
        let can = text_of_the_field(Field::Fingerprint).trim().len() == FINGERPRINT_LENGTH
            && !(towards_it && self.seen.busy(self.state));
        y += self.px(design::SPACE_1);
        y += self.actions(
            at,
            y,
            &[
                ("Annuler".to_string(), Kind::Quiet, Target::Close, true),
                (
                    if towards_it {
                        "Se connecter"
                    } else {
                        "Autoriser"
                    }
                    .to_string(),
                    Kind::Primary,
                    Target::Connect,
                    can,
                ),
            ],
        );

        // Ce qui a été ajouté à la main se retire là où il a été ajouté :
        // une carte d'accueil est un bouton entier, et un second bouton
        // posé dessus lui prendrait son clic.
        let written_ranks = self.written_ranks();
        if !written_ranks.is_empty() {
            y += self.px(design::SPACE_5);
            let inside = self.px(design::SPACE_5);
            self.separator(at.left - inside, y, width + inside * 2.0);
            y += self.px(design::SPACE_4);
            let tag = self.line_height(self.caption().in_bold());
            self.section(
                Rect::at(at.left, y, width, tag),
                "Ordinateurs ajoutés à la main",
            );
            y += tag + self.px(design::SPACE_3);
            let button = self.px(layout::BUTTON);
            let forget_width = self.button_width("Oublier", false);
            for rank in written_ranks {
                let peer = &self.seen.peers[rank];
                let text = format!("{} · {}", peer.name, peer.address);
                self.draw_text(
                    &text,
                    self.caption().ellipsized(),
                    self.colours.text_soft,
                    Rect::at(
                        at.left,
                        y,
                        (width - forget_width - self.px(design::SPACE_3)).max(0.0),
                        button,
                    ),
                );
                self.button(
                    Rect::at(at.left + width - forget_width, y, forget_width, button),
                    "Oublier",
                    Kind::Quiet,
                    Target::Forget(rank),
                    true,
                );
                y += button + self.px(design::SPACE_2);
            }
            y -= self.px(design::SPACE_2);
        }
        y - at.top
    }

    /// Les ordinateurs écrits à la main, par leur rang.
    fn written_ranks(&self) -> Vec<usize> {
        self.seen
            .peers
            .iter()
            .enumerate()
            .filter(|(_, peer)| peer.written)
            .map(|(rank, _)| rank)
            .collect()
    }

    /// Un champ de saisie : son étiquette, la place du vrai champ que
    /// Windows porte, et ce qu'il a à redire.
    fn field(&mut self, x: f32, y: f32, width: f32, field: Field) -> f32 {
        let tag = self.line_height(self.caption());
        let gap = self.px(design::SPACE_2);
        let height = self.px(layout::FIELD);
        self.draw_text(
            field.label(),
            self.caption(),
            self.colours.text_soft,
            Rect::at(x, y, width, tag),
        );
        let place = Rect::at(x, y + tag + gap, width, height);
        let radius = self.px(design::RADIUS_SMALL);
        self.fill(place, radius, self.colours.surface_2);
        self.stroke(place, radius, self.colours.border);
        if !self.silent {
            place_the_field(field, place);
        }

        let text = field.hint();
        let helper = self.height_of(&text, self.caption(), width);
        if !text.is_empty() {
            self.block(
                x,
                place.bottom + self.px(design::SPACE_1),
                width,
                &text,
                self.caption(),
                if field == Field::Fingerprint {
                    self.colours.warning
                } else {
                    self.colours.text_soft
                },
            );
        }
        tag + gap
            + height
            + if helper > 0.0 {
                self.px(design::SPACE_1) + helper
            } else {
                0.0
            }
    }

    /// Le journal, celui de cet ordinateur ou celui d'en face.
    fn in_the_journal(&mut self, at: Rect, window_height: f32) -> f32 {
        let width = at.right - at.left;
        let distant = self.state.journal_of.clone();
        let (title, text) = match &distant {
            Some(peer) => (
                format!("Journal de {}", peer.name),
                "Ce que l'ordinateur distant a écrit chez lui, lu d'ici, à copier tel quel en cas \
                 de problème."
                    .to_string(),
            ),
            None => (
                "Journal".to_string(),
                "Tout ce que le produit a écrit, à copier tel quel en cas de problème.".to_string(),
            ),
        };
        let mut y = at.top;
        y += self.dialogue_header(at, &title, &text);
        y += self.px(design::SPACE_4);

        // Ce que les noms prendront, mesuré avant de poser les lignes.
        // C'est aux lignes de leur rendre cette place : le dialogue doit
        // tenir dans la fenêtre, et une rangée de noms de plus qui le
        // ferait grandir mettrait « Copier » hors d'atteinte.
        let names = self.measure_only(|painter| painter.the_tags(at.left, y, width));

        // Le journal se lit sur des lignes entières : il prend la place
        // qu'il peut, sans jamais pousser son dialogue hors de la
        // fenêtre, et jamais moins que de quoi en lire quelques-unes.
        let lines = ((window_height * layout::JOURNAL.0).min(self.px(layout::JOURNAL.1)) - names)
            .max(self.px(layout::JOURNAL_AT_LEAST));
        self.the_lines(Rect::at(at.left, y, width, lines));
        y += lines + self.px(design::SPACE_4);

        // Les noms que cette page porte, puis la boîte qu'ils
        // remplissent : la page se resserre d'elle-même sur ce qui est
        // écrit là, et c'est cette page que « Copier » emporte. Vide,
        // rien n'est trié.
        y += self.the_tags(at.left, y, width);
        y += self.field(at.left, y, width, Field::Sift);
        y += self.px(design::SPACE_3);

        let emptying = self
            .state
            .emptying
            .is_some_and(|since| since.elapsed() < CONFIRM_TIME);
        let copied =
            self.state.copied.as_ref().map(|(target, _)| target) == Some(&Target::CopyJournal);
        let mut row: Vec<(String, Kind, Target, bool)> = Vec::new();
        // Ouvrir le dossier n'a de sens que chez soi : celui d'en face est
        // sur l'autre machine. Vider, si : on vide les deux journaux, on
        // refait ce qui ne marche pas, et on lit les deux.
        if distant.is_none() {
            row.push((
                "Ouvrir le dossier".to_string(),
                Kind::Quiet,
                Target::OpenTheJournals,
                true,
            ));
        }
        row.push(("Actualiser".to_string(), Kind::Quiet, Target::Refresh, true));
        let sifted = !text_of_the_field(Field::Sift).trim().is_empty();
        row.push((
            if copied {
                "Copié"
            } else if sifted {
                "Copier le tri"
            } else {
                "Copier tout"
            }
            .to_string(),
            Kind::Primary,
            Target::CopyJournal,
            true,
        ));
        let height = self.actions(at, y, &row);
        // Vider est à l'opposé de Copier : les deux se cliquent dans la
        // même minute, et se tromper coûte tout ce qu'on allait copier.
        let empty_width = self.button_width(if emptying { "Confirmer" } else { "Vider" }, false);
        self.button(
            Rect::at(at.left, y, empty_width, height),
            if emptying { "Confirmer" } else { "Vider" },
            if emptying { Kind::Warning } else { Kind::Quiet },
            Target::Empty,
            true,
        );
        y + height - at.top
    }

    /// Le texte du journal, qui défile chez lui : le plus récent est en
    /// bas, et une ligne de journal ne se replie pas.
    fn the_lines(&mut self, at: Rect) {
        let radius = self.px(design::RADIUS_SMALL);
        self.fill(at, radius, self.colours.surface_2);
        self.stroke(at, radius, self.colours.border);

        let inside = self.px(design::SPACE_4);
        let pen = self.caption().monospaced().overflowing();
        let height = self.line_height(pen);
        let inside_the_box = at.grown(-inside);
        let lines = &self.state.lines;
        let content = height * lines.len() as f32;
        let (across, asked) = self.state.lines_scroll;
        let scroll = asked.min((content - (inside_the_box.bottom - inside_the_box.top)).max(0.0));

        let canvas = self.canvas;
        let silent = self.silent;
        let colours = self.colours;
        canvas.clipped(inside_the_box, || {
            if silent {
                return;
            }
            let first = (scroll / height).floor().max(0.0) as usize;
            let how_many =
                ((inside_the_box.bottom - inside_the_box.top) / height).ceil() as usize + 1;
            for (rank, line) in lines.iter().enumerate().skip(first).take(how_many) {
                canvas.draw_text(
                    line,
                    pen,
                    colours.text_soft,
                    Rect::at(
                        inside_the_box.left - across,
                        inside_the_box.top + rank as f32 * height - scroll,
                        FAR_AWAY,
                        height,
                    ),
                );
            }
        });
        self.scrollbar(Scroller::Lines, inside_the_box, content, 0.0);
    }

    /// Les réglages, ligne par ligne.
    fn in_the_settings(&mut self, at: Rect) -> f32 {
        let width = at.right - at.left;
        let mut y = at.top;
        y += self.dialogue_header(
            at,
            "Réglages",
            "Ils valent pour les prochaines sessions, pas pour celle en cours.",
        );
        y += self.px(design::SPACE_4);

        let mut hide = false;
        for element in SETTINGS {
            match element {
                Element::Section(title, text) => {
                    y += self.px(design::SPACE_4);
                    let tag = self.line_height(self.caption().in_bold());
                    self.section(Rect::at(at.left, y, width, tag), title);
                    y += tag + self.px(design::SPACE_1);
                    y += self.block(
                        at.left,
                        y,
                        width,
                        text,
                        self.caption(),
                        self.colours.text_soft,
                    );
                    y += self.px(design::SPACE_2);
                }
                Element::Fold => {
                    self.separator(at.left, y, width);
                    let height = self.px(layout::BUTTON);
                    let tag = self.line_height(self.caption().in_bold());
                    self.section(
                        Rect::at(at.left, y + (height - tag) / 2.0, width, tag),
                        "Avancé",
                    );
                    let sign = self.px(layout::GLYPH);
                    self.icon(
                        if self.state.advanced {
                            &icons::CHEVRON_DOWN
                        } else {
                            &icons::CHEVRON
                        },
                        Rect::at(
                            at.left + width - sign,
                            y + (height - sign) / 2.0,
                            sign,
                            sign,
                        ),
                        self.colours.text_faint,
                    );
                    self.answers(Target::Advanced, Rect::at(at.left, y, width, height));
                    y += height;
                    hide = !self.state.advanced;
                }
                Element::Setting(setting) => {
                    if hide {
                        continue;
                    }
                    self.separator(at.left, y, width);
                    y += self.setting_line(at.left, y, width, setting);
                }
                Element::Account => {
                    self.separator(at.left, y, width);
                    y += self.account(at.left, y, width);
                }
            }
        }

        if let Some(trouble) = self.state.trouble.clone() {
            y += self.px(design::SPACE_4);
            y += self.banner(at.left, y, width, &trouble, true, None);
        }
        y - at.top
    }

    /// Le compte : le lien tel qu'il est, de quoi en faire un ou le
    /// défaire, et les appareils qui y sont.
    fn account(&mut self, x: f32, y: f32, width: f32) -> f32 {
        let inside = self.px(design::SPACE_3);
        let button = self.px(layout::BUTTON);
        let mut taken = inside;
        let Some(account) = self.seen.account.as_ref() else {
            taken += self.block(
                x,
                y + taken,
                width,
                "Le service ne répond pas : le compte se lit quand il tourne.",
                self.caption(),
                self.colours.text_soft,
            );
            return taken + inside;
        };
        let Some(link) = account.link.as_ref() else {
            let text = self.line_height(self.body());
            let button_width = self.button_width("Se connecter à un serveur", false);
            self.draw_text(
                "Aucun compte",
                self.body(),
                self.colours.text,
                Rect::at(
                    x,
                    y + taken + (button - text) / 2.0,
                    (width - button_width - self.px(design::SPACE_4)).max(0.0),
                    text,
                ),
            );
            self.button(
                Rect::at(x + width - button_width, y + taken, button_width, button),
                "Se connecter à un serveur",
                Kind::Primary,
                Target::OpenAccount,
                true,
            );
            return taken + button + inside;
        };

        // Le lien : qui, où, et si le serveur répond.
        let (text, caption) = (
            self.line_height(self.body()),
            self.line_height(self.caption()),
        );
        let gap = self.px(design::SPACE_1);
        let armed = self
            .state
            .detaching
            .is_some_and(|since| since.elapsed() < CONFIRM_TIME);
        let detach_label = if armed { "Confirmer" } else { "Se détacher" };
        let button_width = self.button_width(detach_label, false);
        let text_width = (width - button_width - self.px(design::SPACE_4)).max(0.0);
        let top = y + taken;
        self.draw_text(
            &format!(
                "{} sur {}",
                link.username,
                if link.name.is_empty() {
                    &link.server
                } else {
                    &link.name
                }
            ),
            self.body().ellipsized(),
            self.colours.text,
            Rect::at(x, top, text_width, text),
        );
        let (link_state, ink) = if link.connected {
            ("relié".to_string(), self.colours.online)
        } else {
            (
                format!(
                    "injoignable{}",
                    link.trouble
                        .as_ref()
                        .map_or_else(String::new, |why| format!(" : {}", why.replace('\n', " ")))
                ),
                self.colours.warning,
            )
        };
        let dot = self.px(layout::DOT);
        let helper = top + text + gap;
        self.dot(
            Rect::at(x, helper + (caption - dot) / 2.0, dot, dot),
            ink,
            link.connected,
        );
        self.draw_text(
            &format!("{} · {link_state}", link.server),
            self.caption().ellipsized(),
            self.colours.text_soft,
            Rect::at(
                x + dot + self.px(design::SPACE_2),
                helper,
                (text_width - dot - self.px(design::SPACE_2)).max(0.0),
                caption,
            ),
        );
        let height = (text + gap + caption).max(button);
        self.button(
            Rect::at(
                x + width - button_width,
                top + (height - button) / 2.0,
                button_width,
                button,
            ),
            detach_label,
            if armed { Kind::Warning } else { Kind::Quiet },
            Target::Detach,
            true,
        );
        taken += height + self.px(design::SPACE_4);

        // Les appareils, cet ordinateur compris.
        let tag = self.line_height(self.caption().in_bold());
        self.section(Rect::at(x, y + taken, width, tag), "Appareils du compte");
        taken += tag + self.px(design::SPACE_2);
        if account.devices.is_empty() {
            taken += self.block(
                x,
                y + taken,
                width,
                if link.connected {
                    "Aucun appareil pour l'instant."
                } else {
                    "La liste arrivera quand le serveur répondra."
                },
                self.caption(),
                self.colours.text_soft,
            );
        }
        for rank in 0..account.devices.len() {
            taken += self.device_line(x, y + taken, width, rank);
        }
        taken + inside
    }

    /// Un appareil du compte : son nom, où il en est, et de quoi le
    /// renommer ou le révoquer.
    ///
    /// Cet ordinateur-ci ne se révoque pas d'ici : « Se détacher », juste
    /// au-dessus, fait exactement cela et le dit avec le bon mot.
    fn device_line(&mut self, x: f32, y: f32, width: f32, rank: usize) -> f32 {
        let Some(device) = self
            .seen
            .account
            .as_ref()
            .and_then(|account| account.devices.get(rank))
        else {
            return 0.0;
        };
        let (text, caption) = (
            self.line_height(self.body()),
            self.line_height(self.caption()),
        );
        let gap = self.px(design::SPACE_1);
        let button = self.px(layout::BUTTON);
        let armed = self
            .state
            .revocation
            .is_some_and(|(which, since)| which == rank && since.elapsed() < CONFIRM_TIME);
        let revoke_label = if armed { "Confirmer" } else { "Révoquer" };
        let rename_width = self.button_width("Renommer", false);
        let revoke_width = if device.this {
            0.0
        } else {
            self.button_width(revoke_label, false) + self.px(design::SPACE_2)
        };
        let text_width = (width - rename_width - revoke_width - self.px(design::SPACE_4)).max(0.0);
        let height = (text + gap + caption).max(button) + self.px(design::SPACE_2) * 2.0;
        let middle = y + height / 2.0;
        let top = middle - (text + gap + caption) / 2.0;

        let dot = self.px(layout::DOT);
        let ready = device.online && device.access == Access::Ready;
        self.dot(
            Rect::at(x, top + (text - dot) / 2.0, dot, dot),
            if ready {
                self.colours.online
            } else if device.online {
                self.colours.warning
            } else {
                self.colours.offline
            },
            ready,
        );
        let since = x + dot + self.px(design::SPACE_2);
        self.draw_text(
            &format!(
                "{}{}",
                device.name,
                if device.this {
                    " · cet ordinateur"
                } else {
                    ""
                }
            ),
            self.body().ellipsized(),
            self.colours.text,
            Rect::at(
                since,
                top,
                (text_width - dot - self.px(design::SPACE_2)).max(0.0),
                text,
            ),
        );
        self.draw_text(
            &words_of_the_presence(device),
            self.caption().ellipsized(),
            self.colours.text_soft,
            Rect::at(
                since,
                top + text + gap,
                (text_width - dot - self.px(design::SPACE_2)).max(0.0),
                caption,
            ),
        );

        let this = device.this;
        self.button(
            Rect::at(
                x + width - rename_width,
                middle - button / 2.0,
                rename_width,
                button,
            ),
            "Renommer",
            Kind::Quiet,
            Target::OpenRenaming(rank),
            true,
        );
        if !this {
            let button_width = revoke_width - self.px(design::SPACE_2);
            self.button(
                Rect::at(
                    x + width - rename_width - revoke_width,
                    middle - button / 2.0,
                    button_width,
                    button,
                ),
                revoke_label,
                if armed { Kind::Warning } else { Kind::Quiet },
                Target::Revoke(rank),
                true,
            );
        }
        height
    }

    /// Se rattacher à un serveur : le serveur, le compte, et le nom de
    /// cet ordinateur ; puis, quand le serveur n'est garanti par
    /// personne, sa clé à comparer.
    fn in_the_account(&mut self, at: Rect) -> f32 {
        let width = at.right - at.left;
        let mut y = at.top;
        let gap = self.px(design::SPACE_4);

        let title = self.line_height(self.subtitle());
        self.draw_text(
            "Se connecter à un serveur ZyrDesk",
            self.subtitle(),
            self.colours.text,
            Rect::at(at.left, y, width, title),
        );
        y += title + self.px(design::SPACE_2);
        y += self.block(
            at.left,
            y,
            width,
            "Indiquez le serveur de votre installation, puis votre compte. Cet ordinateur y sera \
             rattaché sous son nom, et vos autres ordinateurs apparaîtront sur l'accueil.",
            self.caption(),
            self.colours.text_soft,
        );
        y += gap;

        let height = self.px(layout::SEGMENT) + self.px(layout::AROUND) * 2.0;
        self.segments(
            at.left + self.segments_width(Pick::SignUp),
            y + height / 2.0,
            Pick::SignUp,
        );
        y += height + gap;

        let sign_up = self.state.sign_up;
        for field in Field::ACCOUNT {
            if field.for_sign_up() && !sign_up {
                continue;
            }
            y += self.field(at.left, y, width, field);
            y += gap;
        }

        if let Some(fingerprint) = self.state.pinning.clone() {
            y += self.pinning_box(at.left, y, width, &fingerprint);
            y += gap;
        }

        let filled = [Field::Server, Field::User, Field::Password]
            .iter()
            .all(|field| !text_of_the_field(*field).trim().is_empty());
        let in_progress = self.state.attaching;
        let (text, target) = if self.state.pinning.is_some() {
            ("C'est bien lui, continuer", Target::Pin)
        } else if sign_up {
            ("Créer le compte", Target::Attach)
        } else {
            ("Se connecter", Target::Attach)
        };
        y += self.px(design::SPACE_1);
        y += self.actions(
            at,
            y,
            &[
                ("Annuler".to_string(), Kind::Quiet, Target::Close, true),
                (
                    if in_progress { "Connexion…" } else { text }.to_string(),
                    Kind::Primary,
                    target,
                    filled && !in_progress,
                ),
            ],
        );
        if let Some(trouble) = self.state.trouble.clone() {
            y += self.px(design::SPACE_4);
            y += self.banner(at.left, y, width, &trouble, true, None);
        }
        y - at.top
    }

    /// La clé d'un serveur que personne ne garantit, à comparer avec ce
    /// que son installation a affiché avant de la croire.
    fn pinning_box(&mut self, x: f32, y: f32, width: f32, fingerprint: &str) -> f32 {
        let inside = self.px(design::SPACE_4);
        let text_width = width - inside * 2.0;
        let text = "Ce serveur présente un certificat que personne ne garantit. Comparez cette \
                   empreinte avec celle que son installation a affichée. Si c'est bien la même, \
                   continuez : elle sera retenue, et un serveur qui en présenterait une autre \
                   serait refusé.";
        let fingerprint_pen = self.caption().monospaced().spaced(0.02);
        let text_height = self.height_of(text, self.caption(), text_width);
        let key_height = self.height_of(fingerprint, fingerprint_pen, text_width);
        let gap = self.px(design::SPACE_2);
        let at = Rect::at(
            x,
            y,
            width,
            text_height + gap + key_height + self.px(design::SPACE_3) * 2.0,
        );
        let radius = self.px(design::RADIUS);
        self.fill(
            at,
            radius,
            self.colours
                .warning
                .mixed_with(self.colours.surface_2, 0.08),
        );
        self.stroke(
            at,
            radius,
            self.colours.warning.mixed_with(self.colours.border, 0.4),
        );
        let top = at.top + self.px(design::SPACE_3);
        self.block(
            at.left + inside,
            top,
            text_width,
            text,
            self.caption(),
            self.colours.text_soft,
        );
        self.draw_text(
            fingerprint,
            fingerprint_pen,
            self.colours.text,
            Rect::at(
                at.left + inside,
                top + text_height + gap,
                text_width,
                key_height,
            ),
        );
        at.bottom - at.top
    }

    /// Renommer un appareil du compte.
    fn in_the_renaming(&mut self, at: Rect) -> f32 {
        let Some((_, name)) = self.state.renaming.clone() else {
            return 0.0;
        };
        let width = at.right - at.left;
        let mut y = at.top;
        let gap = self.px(design::SPACE_4);

        let title = self.line_height(self.subtitle());
        self.draw_text(
            &format!("Renommer « {name} »"),
            self.subtitle().ellipsized(),
            self.colours.text,
            Rect::at(at.left, y, width, title),
        );
        y += title + self.px(design::SPACE_2);
        y += self.block(
            at.left,
            y,
            width,
            "Le nom que le compte montre de cet appareil, sur tous les autres.",
            self.caption(),
            self.colours.text_soft,
        );
        y += gap;
        y += self.field(at.left, y, width, Field::NewName);
        y += gap + self.px(design::SPACE_1);

        let new_name = text_of_the_field(Field::NewName).trim().to_string();
        y += self.actions(
            at,
            y,
            &[
                ("Annuler".to_string(), Kind::Quiet, Target::Close, true),
                (
                    "Renommer".to_string(),
                    Kind::Primary,
                    Target::Rename,
                    !new_name.is_empty() && new_name != name,
                ),
            ],
        );
        y - at.top
    }

    /// Une ligne de réglage : ce dont il s'agit à gauche, de quoi en
    /// décider à droite.
    fn setting_line(&mut self, x: f32, y: f32, width: f32, setting: &Setting) -> f32 {
        let inside = self.px(design::SPACE_3);
        let control = self.control_width(&setting.control);
        let text_width = (width
            - control
            - if control > 0.0 {
                self.px(design::SPACE_4)
            } else {
                0.0
            })
        .max(self.px(80.0));
        let text = self.line_height(self.body());
        let caption = self.caption_of_the_setting(setting);
        let helper = self.height_of(&caption, self.caption(), text_width);
        let gap = if helper > 0.0 {
            self.px(design::SPACE_1)
        } else {
            0.0
        };
        let height =
            (text + gap + helper).max(self.control_height(&setting.control)) + inside * 2.0;
        let middle = y + height / 2.0;
        let top = middle - (text + gap + helper) / 2.0;

        self.draw_text(
            setting.label,
            self.body(),
            self.colours.text,
            Rect::at(x, top, text_width, text),
        );
        let mono = matches!(setting.control, Control::Opens(_, Target::OpenTheJournals));
        self.block(
            x,
            top + text + gap,
            text_width,
            &caption,
            if mono {
                self.caption().monospaced()
            } else {
                self.caption()
            },
            if mono {
                self.colours.text_faint
            } else {
                self.colours.text_soft
            },
        );

        let right = x + width;
        match &setting.control {
            Control::Status => {}
            Control::Switch(button) => {
                self.switch(right - self.px(layout::SWITCH.0), middle, *button);
            }
            Control::Segments(target) => {
                self.segments(right, middle, *target);
            }
            Control::Key(doing) => {
                let height = self.px(layout::BUTTON);
                self.key(
                    Rect::at(right - control, middle - height / 2.0, control, height),
                    *doing,
                );
            }
            Control::Opens(text, target) => {
                let height = self.px(layout::BUTTON);
                self.button(
                    Rect::at(right - control, middle - height / 2.0, control, height),
                    text,
                    Kind::Quiet,
                    target.clone(),
                    true,
                );
            }
        }
        height
    }

    /// Ce que la commande d'une ligne prend de large.
    fn control_width(&self, control: &Control) -> f32 {
        match control {
            Control::Status => 0.0,
            Control::Switch(_) => self.px(layout::SWITCH.0),
            Control::Segments(target) => self.segments_width(*target),
            Control::Key(doing) => {
                self.canvas
                    .width_of(&self.words_of_the_key(*doing), self.caption().monospaced())
                    .max(self.px(layout::KEY) - self.px(design::SPACE_3) * 2.0)
                    + self.px(design::SPACE_3) * 2.0
            }
            Control::Opens(text, _) => self.button_width(text, false),
        }
    }

    fn control_height(&self, control: &Control) -> f32 {
        match control {
            Control::Status => 0.0,
            Control::Switch(_) => self.px(layout::SWITCH.1),
            Control::Segments(_) => self.px(layout::SEGMENT) + self.px(layout::AROUND) * 2.0,
            Control::Key(_) | Control::Opens(_, _) => self.px(layout::BUTTON),
        }
    }

    /// Ce qu'une ligne de réglage a à dire sous son mot.
    fn caption_of_the_setting(&self, setting: &Setting) -> String {
        match &setting.control {
            // Ce qu'une session demanderait maintenant, dit par le produit
            // et non recalculé ici.
            Control::Status => self
                .seen
                .settings
                .as_ref()
                .map_or_else(String::new, |said| {
                    format!(
                        "{} x {}, {} images par seconde, {} Mb/s",
                        said.width,
                        said.height,
                        said.fps,
                        (said.bitrate_kbps as f32 / 1000.0).round() as u32
                    )
                }),
            Control::Opens(_, Target::OpenTheJournals) => self.seen.folder.clone(),
            _ => setting.caption.to_string(),
        }
    }

    /// La combinaison d'un raccourci, telle qu'on la lit.
    fn words_of_the_key(&self, doing: Doing) -> String {
        if self.state.listening == Some(doing) {
            return "Tapez la combinaison…".to_string();
        }
        self.seen
            .shortcuts
            .iter()
            .find(|(other, _)| *other == doing)
            .and_then(|(_, said)| said.clone())
            .unwrap_or_else(|| "Aucune".to_string())
    }

    /// Une combinaison se lit comme des touches et non comme une phrase :
    /// le caractère fixe met le même espace sous chaque signe, et le
    /// cadre dit qu'on peut cliquer dessus pour la changer.
    fn key(&mut self, at: Rect, doing: Doing) {
        let listening = self.state.listening == Some(doing);
        let target = Target::Shortcut(doing);
        let hovered = self.under_the_hand(&target);
        let radius = self.px(design::RADIUS_SMALL);
        self.fill(at, radius, self.colours.surface_2);
        self.stroke(
            at,
            radius,
            if listening || hovered {
                self.colours.accent
            } else {
                self.colours.border_strong
            },
        );
        let text = self.words_of_the_key(doing);
        let empty = text == "Aucune";
        self.draw_text(
            &text,
            self.caption().monospaced().aligned(Align::Centre),
            if listening {
                self.colours.accent_bright
            } else if empty {
                self.colours.text_faint
            } else {
                self.colours.text
            },
            at,
        );
        self.answers(target, at);
    }

    /// Ce qui est à l'écran pendant qu'une session s'ouvre.
    ///
    /// Il prend la fenêtre entière parce que c'est la seule chose qui se
    /// passe, et parce que c'est la dernière chose qu'on lit avant que le
    /// moteur pose sa propre image par-dessus : entre les deux il ne doit
    /// jamais y avoir de trou où l'on se demande si ça marche.
    fn opening(&mut self, width: f32, height: f32) {
        let Some(opening) = self.state.opening.as_ref() else {
            return;
        };
        let whole = Rect::at(0.0, 0.0, width, height);
        self.fill(whole, 0.0, self.colours.background);

        let brand = self.px(layout::BIG_BRAND);
        let title = self.line_height(self.pen(design::TITLE).in_bold());
        let words_width = (width - self.px(design::SPACE_6) * 2.0).min(self.px(420.0));
        let towards = self.height_of(&opening.towards, self.body(), words_width);
        let (wire_width, wire_height) = (
            self.px(layout::THREAD.0).min(width * 0.6),
            self.px(layout::THREAD.1),
        );
        let detail = self
            .height_of(&opening.detail, self.caption(), words_width)
            .max(self.line_height(self.caption()));
        let code = opening.code.as_ref().map(|code| {
            (
                code.clone(),
                self.line_height(self.pen(layout::CODE).in_bold()),
            )
        });
        let gap = self.px(design::SPACE_4);
        let content = brand
            + gap
            + title
            + gap
            + towards
            + gap
            + wire_height
            + gap
            + detail
            + code.as_ref().map_or(0.0, |(_, height)| gap + height);

        let middle = width / 2.0;
        let mut y = (height - content) / 2.0;
        self.brand(Rect::at(middle - brand / 2.0, y, brand, brand));
        y += brand + gap;
        self.draw_text(
            "Établissement de la connexion",
            self.pen(design::TITLE).in_bold().aligned(Align::Centre),
            self.colours.text,
            Rect::at(0.0, y, width, title),
        );
        y += title + gap;
        self.draw_text(
            &opening.towards,
            self.body().aligned(Align::Centre),
            self.colours.text_soft,
            Rect::at(middle - words_width / 2.0, y, words_width, towards),
        );
        y += towards + gap;

        // Une barre qui va et vient. Elle ne mesure rien : ce qu'on
        // attend ici ne se découpe pas en pourcentages, et une barre qui
        // prétendrait le contraire mentirait.
        let track = Rect::at(middle - wire_width / 2.0, y, wire_width, wire_height);
        self.fill(track, wire_height / 2.0, self.colours.surface_3);
        let part = opening.since.elapsed().as_secs_f32() / BACK_AND_FORTH.as_secs_f32();
        let position = part.fract() * (1.0 + layout::PIECE * 2.0) - layout::PIECE;
        let piece = wire_width * layout::PIECE;
        let canvas = self.canvas;
        let accent = self.colours.accent;
        let silent = self.silent;
        canvas.clipped(track, || {
            if !silent {
                canvas.fill(
                    Rect::at(track.left + position * wire_width, y, piece, wire_height),
                    wire_height / 2.0,
                    accent,
                );
            }
        });
        y += wire_height + gap;

        self.draw_text(
            &opening.detail,
            self.caption().aligned(Align::Centre),
            self.colours.text_faint,
            Rect::at(middle - words_width / 2.0, y, words_width, detail),
        );
        if let Some((code, code_height)) = code {
            y += detail + gap;
            self.draw_text(
                &code,
                self.pen(layout::CODE)
                    .in_bold()
                    .monospaced()
                    .aligned(Align::Centre)
                    .spaced(0.22),
                self.colours.accent_bright,
                Rect::at(0.0, y, width, code_height),
            );
        }
    }
}

/// Assez loin pour qu'une ligne de journal ne soit jamais coupée par son
/// propre cadre : c'est la boîte qui la retient, et elle défile.
const FAR_AWAY: f32 = 20_000.0;

/// Plus bas que n'importe quel journal, ce que le dessin ramène ensuite
/// au bas réel : demander « tout en bas » avant d'avoir mesuré est la
/// seule façon d'y être dès la première image.
const VERY_BOTTOM: f32 = 1.0e9;

/* ---- Ce qui pose, et ce qui se tait ------------------------------------ */

/// Les mêmes gestes que la toile, mais qui ne font rien quand la marche
/// ne fait que mesurer.
///
/// Mesurer et dessiner sont la même marche : ce qu'un dialogue prend de
/// haut est ce que ses lignes prennent, et l'écrire une seconde fois à
/// côté serait une arithmétique qui se répond juste jusqu'au premier mot
/// rallongé.
impl Painter<'_> {
    fn draw_text(&self, text: &str, pen: Pen, ink: Colour, at: Rect) {
        if !self.silent {
            self.canvas.draw_text(text, pen, ink, at);
        }
    }

    fn fill(&self, at: Rect, radius: f32, ink: Colour) {
        if !self.silent {
            self.canvas.fill(at, radius, ink);
        }
    }

    /// Une bordure, qui tient entièrement dans son cadre.
    fn stroke(&self, at: Rect, radius: f32, ink: Colour) {
        if !self.silent {
            self.canvas
                .stroke_inside(at, radius, self.px(layout::HAIRLINE), ink);
        }
    }

    /// Un trait qui attend d'être rempli.
    fn dashed(&self, at: Rect, radius: f32, ink: Colour) {
        if !self.silent {
            self.canvas.stroke_dashed(
                at.grown(-self.px(layout::HAIRLINE) / 2.0),
                radius,
                self.px(layout::HAIRLINE),
                ink,
            );
        }
    }

    fn shadow(&self, at: Rect, radius: f32, shadow: design::Shadow) {
        if !self.silent {
            self.canvas.shadow(at, radius, shadow, self.scale);
        }
    }

    fn icon(&self, icon: &Icon, at: Rect, ink: Colour) {
        if !self.silent {
            self.canvas.icon(icon, at, ink);
        }
    }

    fn brand(&self, at: Rect) {
        if !self.silent {
            crate::logo::brand(self.canvas, at, 1.0, false);
        }
    }
}

/* ---- La souris ---------------------------------------------------------- */

/// Ce qui est sous ce point, le dernier posé gagnant : ce qui a été
/// dessiné en dernier est ce qui est dessus.
fn under(x: f32, y: f32) -> Option<Target> {
    CLICKABLES
        .lock()
        .expect("accueil")
        .iter()
        .rev()
        .find(|(_, at)| x >= at.left && x < at.right && y >= at.top && y < at.bottom)
        .map(|(target, _)| target.clone())
}

fn moves(window: windows_sys::Win32::Foundation::HWND, (x, y): (f32, f32)) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
    };

    // Demandé à chaque passage : sans lui rien ne dit jamais qu'une main
    // est partie, et la dernière ligne survolée le resterait.
    let mut tracking = TRACKMOUSEEVENT {
        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
        dwFlags: TME_LEAVE,
        hwndTrack: window,
        dwHoverTime: 0,
    };
    // SAFETY: une fenêtre à nous, et la structure qu'elle demande.
    unsafe { TrackMouseEvent(&mut tracking) };

    let mut state = STATE.lock().expect("accueil");
    if let Some((which, since)) = state.held {
        let (content, visible, travel) = state.measured(which);
        if travel > 0.0 {
            let by = (y - since) * (content - visible) / travel;
            state.scroll_by(which, by);
        }
        state.held = Some((which, y));
        drop(state);
        invalidate(window);
        return;
    }
    let hit = under(x, y);
    if state.hover != hit {
        state.hover = hit;
        drop(state);
        invalidate(window);
    }
}

fn mouse_left(window: windows_sys::Win32::Foundation::HWND) {
    let mut state = STATE.lock().expect("accueil");
    if state.hover.is_none() {
        return;
    }
    state.hover = None;
    drop(state);
    invalidate(window);
}

fn presses(window: windows_sys::Win32::Foundation::HWND, (x, y): (f32, f32)) {
    let mut state = STATE.lock().expect("accueil");
    match under(x, y) {
        Some(Target::Scrollbar(which)) => state.held = Some((which, y)),
        hit => state.pressed = hit,
    }
    drop(state);
    invalidate(window);
}

fn releases(window: windows_sys::Win32::Foundation::HWND, (x, y): (f32, f32)) {
    let mut state = STATE.lock().expect("accueil");
    state.held = None;
    let pressed = state.pressed.take();
    drop(state);
    invalidate(window);

    let Some(target) = pressed else {
        return;
    };
    if under(x, y).as_ref() != Some(&target) {
        return;
    }
    if let Some(app) = program() {
        act(&app, target);
    }
}

fn wheel(window: windows_sys::Win32::Foundation::HWND, notches: f32, across: bool) {
    let mut state = STATE.lock().expect("accueil");
    let which = match state.screen {
        Screen::Home => Scroller::Page,
        // Le texte du journal défile chez lui : c'est ce qu'on lit dans
        // ce dialogue, et le dialogue lui-même est fait pour tenir dans
        // la fenêtre. Sauf quand il n'y tient pas malgré tout, sur un
        // écran très bas : la molette sert alors d'abord à atteindre ce
        // qui en dépasse, faute de quoi les boutons du bas sont
        // inatteignables.
        Screen::Journal => {
            let (content, visible, _) = state.measured(Scroller::Dialogue);
            if content > visible {
                Scroller::Dialogue
            } else {
                Scroller::Lines
            }
        }
        _ => Scroller::Dialogue,
    };
    let by = -notches * scale() * layout::NOTCH;
    if across && which == Scroller::Lines {
        // En travers : une ligne de journal ne se replie pas, et la lire
        // en entier demande de s'y déplacer.
        state.lines_scroll.0 = (state.lines_scroll.0 + by).max(0.0);
    } else {
        state.scroll_by(which, by);
    }
    drop(state);
    invalidate(window);
}

/// Redemande une image depuis le fil qui dessine, où l'on est déjà.
fn invalidate(window: windows_sys::Win32::Foundation::HWND) {
    use windows_sys::Win32::Graphics::Gdi::InvalidateRect;

    // SAFETY: une fenêtre à nous, sur le fil qui la possède.
    unsafe { InvalidateRect(window, std::ptr::null(), 0) };
}

/* ---- Le clavier --------------------------------------------------------- */

/// Ce que la toile fait d'une touche, et si elle l'a prise.
fn key_down(
    window: windows_sys::Win32::Foundation::HWND,
    vk: u32,
    with: windows_sys::Win32::Foundation::LPARAM,
) -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        VK_BACK, VK_DELETE, VK_ESCAPE, VK_RETURN,
    };

    let listening = STATE.lock().expect("accueil").listening;
    if let Some(doing) = listening {
        return the_combination(window, doing, vk, with);
    }

    let Some(app) = program() else {
        return false;
    };
    let screen = STATE.lock().expect("accueil").screen;
    match vk as u16 {
        VK_ESCAPE if screen != Screen::Home => {
            act(&app, Target::Close);
            true
        }
        VK_RETURN if matches!(screen, Screen::Adding | Screen::Account | Screen::Renaming) => {
            act(&app, Target::Confirm);
            true
        }
        VK_BACK | VK_DELETE => false,
        _ => false,
    }
}

/// Ce qu'une touche vaut quand un raccourci l'attend.
///
/// La place de la touche et non le signe dessus : c'est ce que le produit
/// retient, et c'est ce qui garde un raccourci sous le même doigt d'un
/// clavier à l'autre.
fn the_combination(
    window: windows_sys::Win32::Foundation::HWND,
    doing: Doing,
    vk: u32,
    with: windows_sys::Win32::Foundation::LPARAM,
) -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyState, VK_BACK, VK_CONTROL, VK_DELETE, VK_ESCAPE, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
    };

    let Some(app) = program() else {
        return false;
    };
    match vk as u16 {
        VK_ESCAPE => {
            STATE.lock().expect("accueil").listening = None;
            invalidate(window);
            return true;
        }
        VK_BACK | VK_DELETE => {
            set_the_combination(&app, doing, None);
            return true;
        }
        _ => {}
    }

    // La touche étendue est une autre touche que celle qui porte la même
    // place dans le bloc principal : refusée plutôt que confondue avec
    // elle.
    if with & (1 << 24) != 0 {
        return true;
    }
    let scan = ((with >> 16) & 0xFF) as u16;
    let Some(place) = crate::shortcuts::placed(scan) else {
        return true;
    };
    // SAFETY: quatre questions au système sur le clavier de ce fil.
    let modifiers = unsafe {
        Held {
            ctrl: GetKeyState(i32::from(VK_CONTROL)) < 0,
            alt: GetKeyState(i32::from(VK_MENU)) < 0,
            shift: GetKeyState(i32::from(VK_SHIFT)) < 0,
            win: GetKeyState(i32::from(VK_LWIN)) < 0 || GetKeyState(i32::from(VK_RWIN)) < 0,
        }
    };
    set_the_combination(
        &app,
        doing,
        Some(Combination {
            held: modifiers,
            key: place.to_string(),
        }),
    );
    true
}

/// Écrit une combinaison, ou la retire, et relit les trois.
fn set_the_combination(app: &App, doing: Doing, combination: Option<Combination>) {
    let mut state = STATE.lock().expect("accueil");
    state.listening = None;
    state.trouble = None;
    if let Err(refusal) = crate::shortcuts::bind(doing, combination) {
        state.trouble = Some(refusal);
    }
    drop(state);
    if let Some(seen) = SEEN.lock().expect("accueil").as_mut() {
        seen.shortcuts = crate::shortcuts::engraved();
    }
    redraw(app);
}

/* ---- Les champs de saisie ------------------------------------------------ */

/// Un champ de saisie, chacun à sa place, ouvert le temps du dialogue
/// qui le porte.
#[derive(Clone, Copy, PartialEq)]
enum Field {
    Fingerprint,
    Address,
    Name,
    Server,
    User,
    Password,
    DeviceName,
    Email,
    Invitation,
    NewName,
    Sift,
}

impl Field {
    /// Combien il y en a en tout : chacun a sa place, ouvert ou non.
    const COUNT: usize = 11;
    /// Ceux du dialogue d'ajout, dans l'ordre où on les remplit.
    const ADD: [Field; 3] = [Field::Fingerprint, Field::Address, Field::Name];
    /// Ceux du dialogue de compte, les deux derniers pour une inscription.
    const ACCOUNT: [Field; 6] = [
        Field::Server,
        Field::User,
        Field::Password,
        Field::DeviceName,
        Field::Email,
        Field::Invitation,
    ];
    /// Celui du renommage d'un appareil.
    const RENAMING: [Field; 1] = [Field::NewName];
    /// Celui qui trie le journal.
    const JOURNAL: [Field; 1] = [Field::Sift];

    fn rank(self) -> usize {
        match self {
            Field::Fingerprint => 0,
            Field::Address => 1,
            Field::Name => 2,
            Field::Server => 3,
            Field::User => 4,
            Field::Password => 5,
            Field::DeviceName => 6,
            Field::Email => 7,
            Field::Invitation => 8,
            Field::NewName => 9,
            Field::Sift => 10,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Field::Fingerprint => "Empreinte",
            Field::Address => "Adresse",
            Field::Name => "Nom (facultatif)",
            Field::Server => "Serveur",
            Field::User => "Nom d'utilisateur",
            Field::Password => "Mot de passe",
            Field::DeviceName => "Nom de cet ordinateur",
            Field::Email => "Adresse e-mail (facultatif)",
            Field::Invitation => "Code d'invitation (si le serveur en demande un)",
            Field::NewName => "Nouveau nom",
            Field::Sift => "Tri",
        }
    }

    /// Le mot en filigrane, qui dit à quoi ressemble ce qu'on attend.
    fn example(self) -> &'static str {
        match self {
            Field::Fingerprint => "0829cc7ecb9e9ba5…",
            Field::Address => "192.168.1.20",
            Field::Name => "PC du bureau",
            Field::Server => "zyr.exemple.fr ou 192.168.1.40:8443",
            Field::User => "victor",
            Field::Password => "",
            Field::DeviceName => "PC du bureau",
            Field::Email => "victor@exemple.fr",
            Field::Invitation => "AB12-CD34",
            Field::NewName => "PC du salon",
            Field::Sift => "clipboard files",
        }
    }

    /// Ce qui s'y tape ne se lit pas par-dessus l'épaule.
    fn secret(self) -> bool {
        self == Field::Password
    }

    /// Ceux qui ne se montrent qu'en créant un compte.
    fn for_sign_up(self) -> bool {
        matches!(self, Field::Email | Field::Invitation)
    }

    /// Ce que le champ a à redire, ou à expliquer, sous lui.
    fn hint(self) -> String {
        match self {
            Field::Fingerprint => {
                let how_many = text_of_the_field(self).trim().chars().count();
                if how_many == 0 || how_many == FINGERPRINT_LENGTH {
                    String::new()
                } else {
                    format!("{how_many} caractères sur {FINGERPRINT_LENGTH}")
                }
            }
            Field::Address => "Seulement si vous voulez contrôler cet ordinateur depuis ici. Il \
                               restera alors sur l'accueil, et il n'y aura plus rien à ressaisir."
                .to_string(),
            Field::Server => "Comme l'installation du serveur l'a affiché, avec le port s'il \
                               n'est pas 443. Toujours chiffré : une adresse en http:// est \
                               refusée."
                .to_string(),
            Field::Sift => "Un ou plusieurs noms ci-dessus, séparés par des espaces : la page \
                           garde l'un ou l'autre. « mot entre guillemets » cherche dans le \
                           texte, et un moins devant écarte."
                .to_string(),
            _ => String::new(),
        }
    }
}

/// Les champs, et la place que la dernière image leur a donnée.
static FIELDS: Mutex<[isize; Field::COUNT]> = Mutex::new([0; Field::COUNT]);
static PLACES: Mutex<[Option<Rect>; Field::COUNT]> = Mutex::new([None; Field::COUNT]);
/// La police des champs, faite une fois pour la taille de l'écran.
static FONT: Mutex<isize> = Mutex::new(0);

/// Refait la police des champs à l'échelle de l'écran, et la leur pose.
///
/// Un champ est une fenêtre du système : il porte sa propre police, qui
/// ne suit pas ce que nous dessinons.
fn dress_the_fields() {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::Graphics::Gdi::{
        CLEARTYPE_QUALITY, CreateFontW, DEFAULT_CHARSET, DEFAULT_PITCH, DeleteObject, FF_DONTCARE,
        FW_NORMAL, OUT_DEFAULT_PRECIS,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_SETFONT};

    let family = wide(FIELD_FAMILY);
    // SAFETY: le nom survit à l'appel, et la police qui revient est à
    // nous jusqu'à ce qu'on la rende.
    let font = unsafe {
        CreateFontW(
            -((design::BODY * scale()).round() as i32),
            0,
            0,
            0,
            FW_NORMAL as i32,
            0,
            0,
            0,
            u32::from(DEFAULT_CHARSET),
            u32::from(OUT_DEFAULT_PRECIS),
            0,
            u32::from(CLEARTYPE_QUALITY),
            (DEFAULT_PITCH | FF_DONTCARE) as u32,
            family.as_ptr(),
        )
    };
    if font.is_null() {
        return;
    }
    let mut before = FONT.lock().expect("accueil");
    for edit in FIELDS.lock().expect("accueil").iter() {
        if *edit != 0 {
            // SAFETY: une fenêtre faite par nous, à qui l'on donne une
            // police qui lui survivra.
            unsafe { SendMessageW(*edit as HWND, WM_SETFONT, font as usize, 1) };
        }
    }
    if *before != 0 {
        // SAFETY: la police d'avant, rendue une fois plus personne ne
        // l'emploie.
        unsafe { DeleteObject(*before as _) };
    }
    *before = font as isize;
}

/// La famille des champs : la même que celle du reste du dessin, autant
/// que le système la connaisse.
const FIELD_FAMILY: &str = "Segoe UI Variable Text";

/// Ouvre ces vrais champs de Windows, vides.
fn open_the_fields(which_ones: &[Field]) {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::Controls::EM_SETCUEBANNER;
    use windows_sys::Win32::UI::Shell::SetWindowSubclass;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, ES_AUTOHSCROLL, ES_PASSWORD, SendMessageW, WS_CHILD, WS_VISIBLE,
    };

    let canvas = ITS_WINDOW.load(Ordering::Relaxed) as HWND;
    if canvas.is_null() {
        return;
    }
    let class_name = wide("EDIT");
    let mut fields = FIELDS.lock().expect("accueil");
    for field in which_ones.iter().copied() {
        let secret = if field.secret() {
            ES_PASSWORD as u32
        } else {
            0
        };
        // SAFETY: une fenêtre du système, fille de la nôtre, sur le fil
        // qui possède les deux.
        let edit = unsafe {
            CreateWindowExW(
                0,
                class_name.as_ptr(),
                std::ptr::null(),
                WS_CHILD | WS_VISIBLE | ES_AUTOHSCROLL as u32 | secret,
                0,
                0,
                0,
                0,
                canvas,
                std::ptr::null_mut(),
                GetModuleHandleW(std::ptr::null()),
                std::ptr::null(),
            )
        };
        if edit.is_null() {
            continue;
        }
        let cue = wide(field.example());
        // SAFETY: une fenêtre du système, à qui l'on donne un mot qui
        // survit à l'appel, puis un gardien qui lui survit : c'est une
        // simple fonction de ce programme.
        unsafe {
            SendMessageW(edit, EM_SETCUEBANNER, 1, cue.as_ptr() as isize);
            SetWindowSubclass(edit, Some(in_a_field), IN_A_FIELD, field.rank());
        }
        fields[field.rank()] = edit as isize;
    }
    drop(fields);
    dress_the_fields();
}

/// Le nom sous lequel notre gardien est posé sur un champ.
const IN_A_FIELD: usize = 3;

/// Ce que les touches d'un dialogue font dans un champ.
///
/// Un champ de Windows est une fenêtre à lui : la tabulation, Entrée et
/// Échap n'y arrivent jamais jusqu'à nous, et un dialogue où l'on ne
/// passe pas d'un champ au suivant n'est pas un dialogue. Elles sont donc
/// prises ici et rendues à qui de droit.
///
/// SAFETY: appelée par le système sur le fil qui possède ce champ, avec
/// les arguments qu'il documente.
unsafe extern "system" fn in_a_field(
    window: windows_sys::Win32::Foundation::HWND,
    message: u32,
    holding: windows_sys::Win32::Foundation::WPARAM,
    with: windows_sys::Win32::Foundation::LPARAM,
    _who: usize,
    rank: usize,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyState, SetFocus, VK_ESCAPE, VK_RETURN, VK_SHIFT, VK_TAB,
    };
    use windows_sys::Win32::UI::Shell::DefSubclassProc;
    use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CHAR, WM_KEYDOWN};

    let vk = holding as u16;
    let handle = vk == VK_TAB || vk == VK_RETURN || vk == VK_ESCAPE;
    // Le signe qui suit la touche est avalé avec elle : sans ça le champ
    // sonne, la tabulation n'étant pas un signe qu'il accepte.
    if message == WM_CHAR && (holding == 9 || holding == 13 || holding == 27) {
        return 0;
    }
    if message == WM_KEYDOWN && handle {
        match vk {
            VK_TAB => {
                // SAFETY: une question au système sur le clavier de ce
                // fil, puis le clavier donné à un champ à nous.
                let backwards = unsafe { GetKeyState(i32::from(VK_SHIFT)) } < 0;
                let fields = *FIELDS.lock().expect("accueil");
                // Le suivant de ceux qui sont ouverts, en tournant : les
                // places des autres dialogues sont vides.
                let how_many = fields.len();
                let mut next = rank;
                for _ in 0..how_many {
                    next = if backwards {
                        (next + how_many - 1) % how_many
                    } else {
                        (next + 1) % how_many
                    };
                    if fields[next] != 0 {
                        break;
                    }
                }
                if fields[next] != 0 {
                    // SAFETY: une fenêtre faite par nous, sur son fil.
                    unsafe { SetFocus(fields[next] as HWND) };
                }
            }
            // Posté et non fait tout de suite : les deux referment le
            // dialogue, donc détruisent le champ dans lequel on est en
            // train de répondre.
            other => {
                let canvas = ITS_WINDOW.load(Ordering::Relaxed) as HWND;
                if !canvas.is_null() {
                    // SAFETY: une fenêtre à nous, à qui l'on poste un
                    // message qui n'appartient qu'à nous.
                    unsafe { PostMessageW(canvas, ACT, usize::from(other == VK_RETURN), 0) };
                }
            }
        }
        return 0;
    }
    // SAFETY: les arguments que le système a donnés, rendus tels quels.
    unsafe { DefSubclassProc(window, message, holding, with) }
}

/// Referme les champs ouverts et rend leur place.
fn close_the_fields() {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::Graphics::Gdi::DeleteObject;
    use windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow;

    let mut fields = FIELDS.lock().expect("accueil");
    for edit in fields.iter_mut() {
        if *edit != 0 {
            // SAFETY: une fenêtre faite par nous, détruite une fois.
            unsafe { DestroyWindow(*edit as HWND) };
            *edit = 0;
        }
    }
    *PLACES.lock().expect("accueil") = [None; Field::COUNT];
    let mut font = FONT.lock().expect("accueil");
    if *font != 0 {
        // SAFETY: une police faite par nous, rendue une fois.
        unsafe { DeleteObject(*font as _) };
        *font = 0;
    }
}

/// Note où le dessin veut ce champ. Il y sera posé une fois l'image
/// finie : déplacer une fenêtre pendant qu'on peint la sienne mêle deux
/// dessins.
fn place_the_field(field: Field, at: Rect) {
    PLACES.lock().expect("accueil")[field.rank()] = Some(at);
}

/// Pose les champs là où la dernière image les a voulus.
fn place_the_fields() {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos};

    let fields = *FIELDS.lock().expect("accueil");
    let places = *PLACES.lock().expect("accueil");
    // Le texte respire dans son cadre comme la feuille de style le
    // demande : le vrai champ est posé dedans, jamais sur son trait.
    let inside = design::SPACE_3 * scale();
    for (edit, place) in fields.iter().zip(places.iter()) {
        if *edit == 0 {
            continue;
        }
        // Un champ ouvert que l'image n'a pas posé se range : réduit à
        // rien plutôt que laissé où la dernière image l'avait mis.
        let (x, y, width, height) = match place {
            Some(place) => (
                (place.left + inside).round() as i32,
                (place.top + inside / 2.0).round() as i32,
                (place.right - place.left - inside * 2.0).round() as i32,
                (place.bottom - place.top - inside).round() as i32,
            ),
            None => (0, 0, 0, 0),
        };
        // SAFETY: une fenêtre faite par nous, déplacée sur le fil qui la
        // possède.
        unsafe {
            SetWindowPos(
                *edit as HWND,
                std::ptr::null_mut(),
                x,
                y,
                width,
                height,
                SWP_NOACTIVATE | SWP_NOZORDER,
            )
        };
    }
}

/// Écrit ce texte dans un champ, à la place de ce qu'il portait.
///
/// Pour ce qu'un dialogue sait déjà : le nom de cette machine, le nom
/// d'un appareil à renommer. Un champ vide où il faudrait retaper ce que
/// la fenêtre affiche à côté serait une copie de plus.
fn write_in_the_field(field: Field, text: &str) {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::SetWindowTextW;

    let edit = FIELDS.lock().expect("accueil")[field.rank()];
    if edit == 0 {
        return;
    }
    let words = wide(text);
    // SAFETY: une fenêtre faite par nous, et un texte qui survit à
    // l'appel.
    unsafe { SetWindowTextW(edit as HWND, words.as_ptr()) };
}

/// Ce qui est écrit dans un champ.
fn text_of_the_field(field: Field) -> String {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowTextLengthW, GetWindowTextW};

    let edit = FIELDS.lock().expect("accueil")[field.rank()];
    if edit == 0 {
        return String::new();
    }
    // SAFETY: une fenêtre faite par nous, dont le texte est lu dans un
    // tampon de la longueur qu'elle vient d'annoncer.
    unsafe {
        let how_many = GetWindowTextLengthW(edit as HWND);
        if how_many <= 0 {
            return String::new();
        }
        let mut read = vec![0u16; how_many as usize + 1];
        let read_count = GetWindowTextW(edit as HWND, read.as_mut_ptr(), how_many + 1);
        String::from_utf16_lossy(&read[..read_count.max(0) as usize])
    }
}

/// De quelle couleur peindre le dedans d'un champ.
///
/// Le champ appartient au système, qui le dessine lui-même et demande
/// ici quelles couleurs employer : sans ça, un champ blanc trouerait une
/// fenêtre sombre.
fn tint_of_the_field(surface: windows_sys::Win32::Foundation::WPARAM) -> isize {
    use windows_sys::Win32::Graphics::Gdi::{
        CreateSolidBrush, DeleteObject, SetBkColor, SetTextColor,
    };

    let colours = palette();
    let (background, ink) = (colours.surface_2, colours.text);
    // SAFETY: la surface que le système vient de prêter, et un pinceau
    // qu'il rendra en même temps qu'il rendra celui d'avant.
    unsafe {
        SetTextColor(surface as _, rgb(ink));
        SetBkColor(surface as _, rgb(background));
        let mut brush = BRUSH.lock().expect("accueil");
        if *brush != 0 {
            DeleteObject(*brush as _);
        }
        *brush = CreateSolidBrush(rgb(background)) as isize;
        *brush
    }
}

/// Le pinceau du fond des champs, gardé pour être rendu au suivant : le
/// système lit celui qu'on rend et ne le garde pas.
static BRUSH: Mutex<isize> = Mutex::new(0);

/// Une couleur du système de design, dans le nombre que GDI attend.
fn rgb(colour: Colour) -> u32 {
    let part = |how_many: f32| (how_many.clamp(0.0, 1.0) * 255.0).round() as u32;
    part(colour.red) | (part(colour.green) << 8) | (part(colour.blue) << 16)
}

/* ---- Ce qu'un clic fait -------------------------------------------------- */

/// Agit sur ce qui vient d'être cliqué.
///
/// Rien n'attend ici : ce qui demande au service part sur son propre fil
/// et redessine en revenant. Le fil qui dessine ne doit jamais attendre
/// une réponse qui traverse un tuyau.
fn act(app: &App, target: Target) {
    match target {
        Target::OpenJournal => open_the_journal(app, None),
        Target::JournalOf(rank) => {
            let peer = SEEN
                .lock()
                .expect("accueil")
                .as_ref()
                .and_then(|seen| seen.peers.get(rank).cloned());
            if let Some(peer) = peer {
                open_the_journal(app, Some(peer));
            }
        }
        Target::OpenSettings => {
            {
                let mut state = STATE.lock().expect("accueil");
                state.screen = Screen::Settings;
                state.dialogue_scroll = 0.0;
                state.trouble = None;
            }
            reread_the_settings(app);
            redraw(app);
        }
        Target::CopyFingerprint => {
            let fingerprint = SEEN
                .lock()
                .expect("accueil")
                .as_ref()
                .and_then(|seen| seen.machine.as_ref().map(|said| said.fingerprint.clone()))
                .unwrap_or_default();
            copy(app, &fingerprint, Target::CopyFingerprint);
        }
        Target::CopyJournal => copy_the_journal(app),
        Target::Tag(rank) => toggle_the_tag(app, rank),
        Target::ToFix(rank) => remedy_it(app, rank),
        Target::Peer(rank) => launch_the_peer(app, rank, false),
        Target::Local(rank) => launch_the_peer(app, rank, true),
        Target::Disconnect(rank) => {
            let fingerprint = SEEN
                .lock()
                .expect("accueil")
                .as_ref()
                .and_then(|seen| seen.peers.get(rank).map(|peer| peer.fingerprint.clone()));
            if let Some(fingerprint) = fingerprint {
                disconnect(app, fingerprint);
            }
        }
        Target::Add => {
            {
                let mut state = STATE.lock().expect("accueil");
                state.screen = Screen::Adding;
                state.dialogue_scroll = 0.0;
            }
            // Vidés à chaque ouverture : rouverts pleins de la machine
            // précédente, ils laisseraient ajouter deux fois le même
            // ordinateur d'un simple double clic.
            close_the_fields();
            open_the_fields(&Field::ADD);
            redraw(app);
        }
        Target::Close => {
            let mut state = STATE.lock().expect("accueil");
            state.screen = Screen::Home;
            state.listening = None;
            // Sur la fermeture et non sur son bouton : la touche Échap
            // ferme aussi, et laissait la confirmation de vidage armée
            // derrière un dialogue clos.
            state.emptying = None;
            state.pinning = None;
            state.renaming = None;
            drop(state);
            close_the_fields();
            redraw(app);
        }
        Target::Connect => connect(app),
        // La touche Entrée fait ce que le bouton principal du dialogue
        // ouvert ferait.
        Target::Confirm => {
            let screen = STATE.lock().expect("accueil").screen;
            match screen {
                Screen::Adding => connect(app),
                Screen::Account => {
                    let pinning = STATE.lock().expect("accueil").pinning.clone();
                    attach(app, pinning);
                }
                Screen::Renaming => rename(app),
                // Entrée dans la boîte de tri relit tout de suite, sans
                // attendre le repos de l'horloge.
                Screen::Journal => reread_the_journal(app, After::Show),
                Screen::Home | Screen::Settings => {}
            }
        }
        Target::OpenAccount => open_the_account(app),
        Target::Attach => attach(app, None),
        Target::Pin => {
            let pinning = STATE.lock().expect("accueil").pinning.clone();
            attach(app, pinning);
        }
        Target::Detach => detach(app),
        Target::OpenRenaming(rank) => open_the_renaming(app, rank),
        Target::Rename => rename(app),
        Target::Revoke(rank) => revoke(app, rank),
        Target::Forget(rank) => {
            let peer = SEEN
                .lock()
                .expect("accueil")
                .as_ref()
                .and_then(|seen| seen.peers.get(rank).cloned());
            if let Some(peer) = peer {
                forget(app, peer.fingerprint);
            }
        }
        Target::Switch(button) => push(app, button),
        Target::Segment(target, rank) => pick(app, target, rank),
        Target::Shortcut(doing) => {
            let mut state = STATE.lock().expect("accueil");
            state.listening = if state.listening == Some(doing) {
                None
            } else {
                Some(doing)
            };
            drop(state);
            redraw(app);
        }
        Target::Advanced => {
            let mut state = STATE.lock().expect("accueil");
            state.advanced = !state.advanced;
            drop(state);
            redraw(app);
        }
        Target::Empty => empty_the_journal(app),
        Target::Refresh => reread_the_journal(app, After::Show),
        Target::OpenTheJournals => open_a_folder(app, "logs"),
        Target::Scrollbar(_) => {}
    }
}

/// Ce que le bouton d'un bandeau « à faire » répare.
fn remedy_it(app: &App, rank: usize) {
    let missing = SEEN
        .lock()
        .expect("accueil")
        .as_ref()
        .map(what_is_missing)
        .and_then(|missings| missings.get(rank).map(|missing| missing.remedy));
    match missing {
        Some(Remedy::StartTheService) => {
            let app = app.clone();
            crate::app::spawn(async move {
                if let Err(reason) = crate::desk::start_service().await {
                    notice(&app, &reason, true);
                }
                reread(&app).await;
                redraw(&app);
            });
        }
        Some(Remedy::HostEngine) => open_a_folder(app, "host-engine"),
        Some(Remedy::ClientEngine) => open_a_folder(app, "client-engine"),
        Some(Remedy::SeeTheJournal) => open_the_journal(app, None),
        None => {}
    }
}

fn open_a_folder(app: &App, which: &'static str) {
    if let Err(reason) = crate::folders::open_folder(which.to_string()) {
        notice(app, &reason, true);
    }
}

/// Pousse un interrupteur, et le tient à sa nouvelle place le temps que
/// le service en prenne acte.
fn push(app: &App, button: Toggle) {
    let wanted = {
        let nothing = Seen::default();
        let seen = SEEN.lock().expect("accueil");
        let mut state = STATE.lock().expect("accueil");
        let wanted = !button.is_on(seen.as_ref().unwrap_or(&nothing), &state);
        state.pushed.retain(|(target, _)| *target != button);
        state.pushed.push((button, wanted));
        state.trouble = None;
        state.notice = None;
        wanted
    };
    redraw(app);

    let app = app.clone();
    crate::app::spawn(async move {
        let serve = || async {
            let said = crate::desk::standing().await;
            crate::desk::set_serving(
                if button == Toggle::SteadyRate {
                    wanted
                } else {
                    said.steady_rate
                },
                said.capture,
            )
            .await
        };
        let target = match button {
            Toggle::Access => crate::desk::set_hosting(wanted).await,
            Toggle::Trust => crate::desk::set_trust(wanted).await,
            Toggle::AtBoot => crate::desk::set_at_boot(wanted).await,
            Toggle::Marking => crate::desk::set_ecn(wanted).await,
            Toggle::FixedPort => crate::desk::set_fixed_port(wanted).await,
            Toggle::SteadyRate => serve().await,
            Toggle::Sound | Toggle::Stats => {
                write_the_settings(|chosen| {
                    if button == Toggle::Sound {
                        chosen.mute_far_speakers = wanted;
                    } else {
                        chosen.stats_overlay = wanted;
                    }
                })
                .await
            }
        };
        if let Err(reason) = target {
            say_the_trouble(&app, &reason);
        }
        STATE
            .lock()
            .expect("accueil")
            .pushed
            .retain(|(target, _)| *target != button);
        reread(&app).await;
        redraw(&app);
    });
}

/// Choisit un des côtés d'un choix segmenté.
fn pick(app: &App, target: Pick, rank: usize) {
    if target == Pick::Theme {
        if let Some(choice) = Choice::ALL.get(rank) {
            crate::theme::choose(*choice);
            redraw(app);
        }
        return;
    }
    // Celui-ci ne voyage nulle part : il change la forme du dialogue de
    // compte, et rien d'autre.
    if target == Pick::SignUp {
        STATE.lock().expect("accueil").sign_up = rank == 1;
        redraw(app);
        return;
    }
    let Some(value) = target.values().get(rank).copied() else {
        return;
    };
    let app = app.clone();
    crate::app::spawn(async move {
        // Celui-ci ne décrit pas ce qu'on demande aux autres mais ce que
        // cet ordinateur fait quand c'est lui qu'on regarde : il ne passe
        // pas par les mêmes réglages.
        let done = if target == Pick::Capture {
            let said = crate::desk::standing().await;
            crate::desk::set_serving(said.steady_rate, value.to_string()).await
        } else {
            write_the_settings(|chosen| match target {
                Pick::Codec => chosen.codec = value.parse().unwrap_or(chosen.codec),
                Pick::Display => chosen.display = value.parse().unwrap_or(chosen.display),
                Pick::Mouse => chosen.absolute_mouse = value == "desktop",
                _ => {}
            })
            .await
        };
        if let Err(reason) = done {
            say_the_trouble(&app, &reason);
        }
        reread(&app).await;
        redraw(&app);
    });
}

/// Change un réglage de session, les autres restant ce qu'ils sont.
///
/// L'ensemble part au service pour qu'il n'ait jamais à deviner ce qui
/// est resté.
async fn write_the_settings(
    change: impl FnOnce(&mut crate::settings::Chosen),
) -> Result<(), String> {
    let mut chosen = crate::settings::Chosen::of(crate::settings::preferred().await);
    change(&mut chosen);
    crate::settings::choose(chosen).await
}

/* ---- Ajouter, oublier, se connecter -------------------------------------- */

/// Écrit un ordinateur et, s'il porte une adresse, s'y connecte.
///
/// L'empreinte va dans les deux sens : elle laisse entrer cet
/// ordinateur-là, et elle sert de repère pour aller vers lui. Sans le
/// premier des deux, la machine d'en face serait refusée à l'arrivée et
/// on n'aurait fait que la moitié du chemin.
fn connect(app: &App) {
    let fingerprint = text_of_the_field(Field::Fingerprint).trim().to_string();
    let address = text_of_the_field(Field::Address).trim().to_string();
    let name = text_of_the_field(Field::Name).trim().to_string();
    if fingerprint.len() != FINGERPRINT_LENGTH {
        return;
    }
    act(app, Target::Close);

    let app = app.clone();
    crate::app::spawn(async move {
        let written = crate::desk::authorize(
            fingerprint.clone(),
            (!address.is_empty()).then(|| address.clone()),
            (!name.is_empty()).then(|| name.clone()),
        )
        .await;
        if let Err(reason) = written {
            notice(&app, &reason, true);
            return;
        }
        reread(&app).await;
        if address.is_empty() {
            // Autoriser ne se voit nulle part ailleurs : sans un mot, le
            // geste ferait exactement le même effet à l'écran que ne rien
            // faire.
            notice(
                &app,
                "Cet ordinateur est autorisé à venir sur celui-ci.",
                false,
            );
            return;
        }
        redraw(&app);
        let seen_name = SEEN
            .lock()
            .expect("accueil")
            .as_ref()
            .and_then(|seen| {
                seen.peers
                    .iter()
                    .find(|peer| peer.fingerprint == fingerprint)
                    .map(|peer| peer.name.clone())
            })
            .unwrap_or_else(|| address.clone());
        // Un ordinateur qu'on vient d'écrire à la main se joint par la
        // meilleure voie : se priver du serveur se demande sur une carte,
        // pour une machine que ce réseau annonce déjà.
        launch(&app, &address, &fingerprint, &seen_name, false);
    });
}

/// Oublie un ordinateur écrit à la main, des deux listes à la fois.
fn forget(app: &App, fingerprint: String) {
    let app = app.clone();
    crate::app::spawn(async move {
        if let Err(reason) = crate::desk::forget(fingerprint).await {
            act(&app, Target::Close);
            notice(&app, &reason, true);
            return;
        }
        reread(&app).await;
        redraw(&app);
    });
}

/// Déconnecte l'ordinateur qui contrôle celui-ci en ce moment.
fn disconnect(app: &App, fingerprint: String) {
    let app = app.clone();
    crate::app::spawn(async move {
        if let Err(reason) = crate::desk::kick(fingerprint).await {
            notice(&app, &reason, true);
            return;
        }
        reread(&app).await;
        redraw(&app);
    });
}

/// Ouvre une session vers l'ordinateur de cette carte, par la meilleure
/// voie ou par ce réseau-ci et rien d'autre.
fn launch_the_peer(app: &App, rank: usize, local_only: bool) {
    let aimed = SEEN
        .lock()
        .expect("accueil")
        .as_ref()
        .and_then(|seen| seen.peers.get(rank).cloned());
    if let Some(peer) = aimed {
        launch(
            app,
            &peer.address,
            &peer.fingerprint,
            &peer.name,
            local_only,
        );
    }
}

/// Ouvre une session vers cet ordinateur.
///
/// `local_only` la tient sur ce réseau : l'adresse d'ici et rien d'autre,
/// sans qu'aucun serveur soit consulté.
fn launch(app: &App, address: &str, fingerprint: &str, name: &str, local_only: bool) {
    {
        let seen = SEEN.lock().expect("accueil");
        let state = STATE.lock().expect("accueil");
        if seen.as_ref().is_some_and(|seen| seen.busy(&state)) {
            return;
        }
    }
    {
        let mut state = STATE.lock().expect("accueil");
        state.notice = None;
        state.opening = Some(Opening {
            // Le nom plutôt que l'adresse : personne ne reconnaît son
            // ordinateur portable à ses quatre nombres.
            towards: name.to_string(),
            detail: "Ouverture du tunnel…".to_string(),
            code: None,
            since: std::time::Instant::now(),
        });
    }
    redraw(app);

    let (app, address, fingerprint) = (app.clone(), address.to_string(), fingerprint.to_string());
    crate::app::spawn(async move {
        if let Err(reason) =
            crate::session::connect(app.clone(), address, fingerprint, local_only).await
        {
            failed(&app, &reason);
        }
    });
}

/* ---- Le compte ------------------------------------------------------------ */

/// Ouvre le dialogue de compte, avec le nom de cette machine déjà écrit.
fn open_the_account(app: &App) {
    {
        let mut state = STATE.lock().expect("accueil");
        state.screen = Screen::Account;
        state.dialogue_scroll = 0.0;
        state.sign_up = false;
        state.pinning = None;
        state.attaching = false;
        state.trouble = None;
    }
    close_the_fields();
    open_the_fields(&Field::ACCOUNT);
    write_in_the_field(Field::DeviceName, &zyr_proto::machine::name());
    redraw(app);
}

/// Rattache cet ordinateur au compte écrit dans les champs.
///
/// `pinning` est la clé d'un serveur que personne ne garantit, une
/// fois que la personne l'a comparée : sans elle, un tel serveur répond
/// par sa clé et le dialogue la montre, avec de quoi la confirmer.
fn attach(app: &App, pinning: Option<String>) {
    let server = text_of_the_field(Field::Server).trim().to_string();
    let user = text_of_the_field(Field::User).trim().to_string();
    let password = text_of_the_field(Field::Password);
    if server.is_empty() || user.is_empty() || password.is_empty() {
        return;
    }
    let sign_up = STATE.lock().expect("accueil").sign_up;
    let empty_or = |field: Field| {
        let text = text_of_the_field(field).trim().to_string();
        (!text.is_empty()).then_some(text)
    };
    let request = Attach {
        server,
        username: user,
        password,
        register: sign_up.then(|| Registering {
            email: empty_or(Field::Email),
            invitation: empty_or(Field::Invitation),
        }),
        name: text_of_the_field(Field::DeviceName).trim().to_string(),
        pin: pinning.and_then(|fingerprint| fingerprint.parse().ok()),
    };
    {
        let mut state = STATE.lock().expect("accueil");
        state.attaching = true;
        state.trouble = None;
    }
    redraw(app);

    let app = app.clone();
    crate::app::spawn(async move {
        let outcome = crate::desk::attach(request).await;
        // Ce que l'état retient de la réponse, écrit sous son verrou et
        // rendu avant d'attendre quoi que ce soit d'autre.
        let attached = {
            let mut state = STATE.lock().expect("accueil");
            state.attaching = false;
            match outcome {
                Ok(Attached::Done) => true,
                Ok(Attached::Unpinned(presented)) => {
                    state.pinning = Some(presented);
                    false
                }
                Err(reason) => {
                    state.trouble = Some(reason);
                    false
                }
            }
        };
        if !attached {
            redraw(&app);
            return;
        }
        // Le dialogue se referme sur le fil qui possède ses champs : ce
        // sont des fenêtres du système, et détruire une fenêtre depuis
        // un autre fil ne détruit rien.
        let held = app.clone();
        let _ = app.run_on_main_thread(move || act(&held, Target::Close));
        reread(&app).await;
        notice(&app, "Cet ordinateur est rattaché au compte.", false);
    });
}

/// Détache cet ordinateur de son compte. Un deuxième clic est demandé,
/// et l'attente retombe d'elle-même.
fn detach(app: &App) {
    {
        let mut state = STATE.lock().expect("accueil");
        let armed = state
            .detaching
            .is_some_and(|since| since.elapsed() < CONFIRM_TIME);
        if !armed {
            state.detaching = Some(std::time::Instant::now());
            drop(state);
            redraw(app);
            return;
        }
        state.detaching = None;
        state.trouble = None;
    }
    redraw(app);

    let app = app.clone();
    crate::app::spawn(async move {
        if let Err(reason) = crate::desk::detach().await {
            say_the_trouble(&app, &reason);
        }
        reread(&app).await;
        redraw(&app);
    });
}

/// L'appareil du compte à ce rang, s'il y est encore.
fn device_of_the_account(rank: usize) -> Option<Device> {
    SEEN.lock()
        .expect("accueil")
        .as_ref()
        .and_then(|seen| seen.account.as_ref())
        .and_then(|account| account.devices.get(rank).cloned())
}

/// Ouvre le renommage d'un appareil, son nom déjà écrit.
fn open_the_renaming(app: &App, rank: usize) {
    let Some(device) = device_of_the_account(rank) else {
        return;
    };
    {
        let mut state = STATE.lock().expect("accueil");
        state.screen = Screen::Renaming;
        state.dialogue_scroll = 0.0;
        state.renaming = Some((device.id, device.name.clone()));
        state.trouble = None;
    }
    close_the_fields();
    open_the_fields(&Field::RENAMING);
    write_in_the_field(Field::NewName, &device.name);
    redraw(app);
}

/// Renomme l'appareil en cours de renommage avec ce qui est écrit.
fn rename(app: &App) {
    let new_name = text_of_the_field(Field::NewName).trim().to_string();
    let Some((device, before)) = STATE.lock().expect("accueil").renaming.clone() else {
        return;
    };
    if new_name.is_empty() || new_name == before {
        return;
    }
    act(app, Target::Close);
    {
        let mut state = STATE.lock().expect("accueil");
        state.screen = Screen::Settings;
        state.dialogue_scroll = 0.0;
    }
    redraw(app);

    let app = app.clone();
    crate::app::spawn(async move {
        if let Err(reason) = crate::desk::rename_device(device, new_name).await {
            say_the_trouble(&app, &reason);
        }
        reread(&app).await;
        redraw(&app);
    });
}

/// Révoque un appareil du compte. Un deuxième clic est demandé, sur le
/// même appareil, et l'attente retombe d'elle-même.
fn revoke(app: &App, rank: usize) {
    {
        let mut state = STATE.lock().expect("accueil");
        let armed = state
            .revocation
            .is_some_and(|(which, since)| which == rank && since.elapsed() < CONFIRM_TIME);
        if !armed {
            state.revocation = Some((rank, std::time::Instant::now()));
            drop(state);
            redraw(app);
            return;
        }
        state.revocation = None;
        state.trouble = None;
    }
    let Some(device) = device_of_the_account(rank) else {
        return;
    };
    redraw(app);

    let app = app.clone();
    crate::app::spawn(async move {
        if let Err(reason) = crate::desk::revoke_device(device.id).await {
            say_the_trouble(&app, &reason);
        }
        reread(&app).await;
        redraw(&app);
    });
}

/* ---- Ce que la session raconte ------------------------------------------- */

/// Une étape de l'ouverture d'une session.
///
/// Appelée par ce qui conduit la session : la fenêtre est la seule à
/// pouvoir dire où en est ce qui n'a pas encore d'image.
pub fn step(app: &App, detail: &str, code: Option<String>) {
    {
        let mut state = STATE.lock().expect("accueil");
        let Some(opening) = state.opening.as_mut() else {
            return;
        };
        opening.detail = detail.to_string();
        opening.code = code;
    }
    redraw(app);
}

/// L'image se relance avec de nouveaux réglages : personne n'a cliqué
/// pour ouvrir celle-là, donc c'est ici que l'écran d'ouverture revient.
pub fn relaunched(app: &App) {
    {
        let mut state = STATE.lock().expect("accueil");
        let towards = state
            .opening
            .as_ref()
            .map_or_else(String::new, |already| already.towards.clone());
        state.opening = Some(Opening {
            towards,
            detail: "Nouveaux réglages, l'image se relance…".to_string(),
            code: None,
            since: std::time::Instant::now(),
        });
    }
    redraw(app);
}

/// La session est tombée toute seule et l'image revient.
///
/// Le même écran que l'ouverture, pour la raison qu'il dit la même chose :
/// il n'y a rien à regarder et quelque chose est en train de se faire.
/// Le numéro d'essai n'apparaît qu'à partir du deuxième : le premier est
/// le cas ordinaire et se compte tout seul, alors qu'un troisième dit
/// quelque chose que la barre qui va et vient ne dira jamais, c'est que
/// ça ne se passe pas bien.
pub fn coming_back(app: &App, attempt: u32) {
    {
        let mut state = STATE.lock().expect("accueil");
        let towards = state
            .opening
            .as_ref()
            .map_or_else(String::new, |already| already.towards.clone());
        state.opening = Some(Opening {
            towards,
            detail: if attempt > 1 {
                format!("Connexion perdue, reprise en cours… ({attempt}ᵉ essai)")
            } else {
                "Connexion perdue, reprise en cours…".to_string()
            },
            code: None,
            since: std::time::Instant::now(),
        });
    }
    redraw(app);
}

/// La fenêtre n'a plus rien à raconter : ce qui se passe maintenant se
/// lit dans ce que tient le service.
pub fn put_the_opening_away(app: &App) {
    let app = app.clone();
    crate::app::spawn(async move {
        reread(&app).await;
        STATE.lock().expect("accueil").opening = None;
        redraw(&app);
    });
}

/// Une session qui s'est mal terminée, ou qui n'a pas pu s'ouvrir.
pub fn failed(app: &App, text: &str) {
    notice(app, text, true);
    put_the_opening_away(app);
}

/// Le bandeau du haut.
fn notice(app: &App, text: &str, is_trouble: bool) {
    STATE.lock().expect("accueil").notice = Some(Notice {
        text: text.to_string(),
        is_trouble,
        since: std::time::Instant::now(),
    });
    redraw(app);
}

/// Ce que les réglages ont à redire, qui vit dans leur dialogue.
fn say_the_trouble(app: &App, text: &str) {
    STATE.lock().expect("accueil").trouble = Some(text.to_string());
    redraw(app);
}

/* ---- Le journal ---------------------------------------------------------- */

fn open_the_journal(app: &App, from: Option<Peer>) {
    // Ce qui était écrit dans la boîte est repris : on ouvre ce journal
    // deux fois de suite pour un même tri, une fois ici et une fois en
    // face, et le retaper serait la moitié du travail.
    let sift = text_of_the_field(Field::Sift);
    {
        let mut state = STATE.lock().expect("accueil");
        state.screen = Screen::Journal;
        state.journal_of = from;
        state.emptying = None;
        state.lines_scroll = (0.0, 0.0);
        state.lines = vec!["Lecture…".to_string()];
        state.sift = None;
        // Ceux de la page précédente ne sont pas ceux de celle-ci, et
        // c'est le plus vrai en passant de son journal à celui d'en face.
        state.tags = Vec::new();
    }
    close_the_fields();
    open_the_fields(&Field::JOURNAL);
    write_in_the_field(Field::Sift, &sift);
    redraw(app);
    reread_the_journal(app, After::Show);
}

/// Ce qu'on fait de la page une fois lue.
#[derive(Clone, Copy, PartialEq, Eq)]
enum After {
    /// La montrer, et rien de plus.
    Show,
    /// La montrer et l'emporter : « Copier le tri » cliqué sur une page
    /// qui n'était pas encore celle du tri.
    Take,
}

/// Redemande la page du journal ouvert, triée comme la boîte le demande.
///
/// Appelée depuis le fil qui dessine, qui est le seul à pouvoir lire la
/// boîte et poser les horloges de cette fenêtre.
fn reread_the_journal(app: &App, after: After) {
    // Une lecture qui attendait son repos n'a plus lieu d'être : celle-ci
    // la remplace.
    let window = ITS_WINDOW.load(Ordering::Relaxed);
    if window != 0 {
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::UI::WindowsAndMessaging::KillTimer;
        // SAFETY: une horloge à nous, sur le fil qui l'a posée. Rien si
        // elle n'était pas posée.
        unsafe { KillTimer(window as HWND, SIFT_PAUSE) };
    }
    // Lu avant de partir : la question s'en va sur un autre fil, et le
    // champ appartient à celui qui dessine.
    let sift = text_of_the_field(Field::Sift).trim().to_string();
    let from = {
        let mut state = STATE.lock().expect("accueil");
        state.sift_asked = sift.clone();
        state.journal_of.clone()
    };
    let app = app.clone();
    crate::app::spawn(async move {
        let text = match &from {
            None => crate::journal::journal(&sift).await,
            Some(peer) => crate::journal::far_journal(
                peer.address.clone(),
                peer.fingerprint.clone(),
                sift.clone(),
            )
            .await
            // Montré dans le journal lui-même : c'est là que regarde la
            // personne qui vient de cliquer, et un ordinateur qui ne
            // répond pas est déjà la moitié de la réponse.
            .unwrap_or_else(|reason| reason),
        };
        // Joindre une machine distante prend le temps qu'il faut : le
        // journal a pu être refermé, avoir changé d'ordinateur ou de tri
        // entre-temps. Ce qui arrive en retard n'écrase pas ce qui est à
        // l'écran.
        let mut state = STATE.lock().expect("accueil");
        if state.journal_of != from || state.screen != Screen::Journal || state.sift_asked != sift {
            return;
        }
        state.lines = text.lines().map(str::to_string).collect();
        state.tags = zyr_proto::journal::names_in(&text);
        state.sift = Some(sift);
        // Le plus récent est en bas : c'est là que se trouve ce qui vient
        // d'arriver, et c'est ce qu'on ouvre le journal pour lire. Plus
        // bas que tout plutôt que d'une hauteur comptée : ce qui vient
        // d'être lu n'a pas encore été mesuré, et c'est le dessin qui
        // ramènera ce nombre à ce qu'il y a réellement à voir.
        state.lines_scroll = (0.0, VERY_BOTTOM);
        let taken = (after == After::Take).then(|| state.lines.join("\n"));
        drop(state);
        redraw(&app);
        if let Some(whole) = taken {
            // Posé depuis le fil qui dessine, comme toute copie de cette
            // fenêtre.
            let held = app.clone();
            let _ = app.run_on_main_thread(move || copy(&held, &whole, Target::CopyJournal));
        }
    });
}

/// Coche ou décoche ce nom-là dans la boîte de tri.
///
/// Le nom est ajouté ou retiré de ce qui est déjà écrit plutôt que de le
/// remplacer : cocher deux noms est ce qui garde les deux sujets à la
/// fois, et ce qu'on avait tapé à la main à côté reste là où il était.
fn toggle_the_tag(app: &App, rank: usize) {
    let Some(name) = STATE.lock().expect("accueil").tags.get(rank).cloned() else {
        return;
    };
    let mut words: Vec<String> = text_of_the_field(Field::Sift)
        .split_whitespace()
        .map(str::to_string)
        .collect();
    match words.iter().position(|text| *text == name) {
        Some(at) => {
            words.remove(at);
        }
        None => words.push(name),
    }
    write_in_the_field(Field::Sift, &words.join(" "));
    // Relu tout de suite : un clic a dit ce qu'il voulait, il n'y a plus
    // de lettre à attendre.
    reread_the_journal(app, After::Show);
}

/// Emporte la page du journal, qui doit répondre à ce qui est écrit dans
/// la boîte.
///
/// Le bouton dit « Copier le tri » et ne doit jamais emporter autre
/// chose : entre le tri collé dans la boîte et la page qui se resserre il
/// y a le repos de l'horloge et l'aller-retour du service, et c'est juste
/// assez pour cliquer entre les deux. Une page en retard est donc relue,
/// et c'est sa réponse qui part.
fn copy_the_journal(app: &App) {
    let sift = text_of_the_field(Field::Sift).trim().to_string();
    let page = {
        let state = STATE.lock().expect("accueil");
        (state.sift.as_deref() == Some(sift.as_str())).then(|| state.lines.join("\n"))
    };
    match page {
        Some(whole) => copy(app, &whole, Target::CopyJournal),
        None => reread_the_journal(app, After::Take),
    }
}

/// Vider efface la seule trace de ce qui vient de se passer. Un deuxième
/// clic est demandé, et l'attente retombe d'elle-même.
fn empty_the_journal(app: &App) {
    let from = {
        let mut state = STATE.lock().expect("accueil");
        let armed = state
            .emptying
            .is_some_and(|since| since.elapsed() < CONFIRM_TIME);
        if !armed {
            state.emptying = Some(std::time::Instant::now());
            drop(state);
            redraw(app);
            return;
        }
        state.emptying = None;
        state.lines = vec!["Vidage…".to_string()];
        state.journal_of.clone()
    };
    redraw(app);

    let app = app.clone();
    crate::app::spawn(async move {
        // Vidé là où il est écrit : celui de cette machine tout de suite,
        // celui d'en face en le lui demandant.
        let done = match &from {
            None => crate::journal::clear_journal(),
            Some(peer) => {
                crate::journal::clear_far_journal(peer.address.clone(), peer.fingerprint.clone())
                    .await
            }
        };
        if let Err(reason) = done {
            STATE.lock().expect("accueil").lines = reason.lines().map(str::to_string).collect();
            redraw(&app);
            return;
        }
        let held = app.clone();
        let _ = app.run_on_main_thread(move || reread_the_journal(&held, After::Show));
    });
}

/* ---- Le presse-papiers ---------------------------------------------------- */

/// Copie ce texte, et fait dire au bouton qu'il l'a fait.
///
/// Le presse-papiers peut refuser, et un bouton qui dit « Copié » sur un
/// refus enverrait quelqu'un coller du vide sur l'autre ordinateur.
fn copy(app: &App, text: &str, target: Target) {
    if let Err(e) = zyr_clipboard::hold_this(&zyr_proto::clipboard::Clip::text(text)) {
        note(&format!("copie refusée : {e}"));
        notice(app, "La copie a été refusée par Windows.", true);
        return;
    }
    // Ce qu'une pose incomplète rendrait ne concerne que les images, et
    // ce bouton ne copie que du texte : il n'y a qu'une forme à poser, et
    // elle est posée ou le refus ci-dessus l'a dit.
    STATE.lock().expect("accueil").copied = Some((target, std::time::Instant::now()));
    redraw(app);

    let app = app.clone();
    crate::app::spawn(async move {
        tokio::time::sleep(COPIED_TIME).await;
        STATE.lock().expect("accueil").copied = None;
        redraw(&app);
    });
}

/* ---- Ce qu'on redemande au service --------------------------------------- */

/// Redemande sans arrêt ce que le service tient.
///
/// Le service peut démarrer après cette fenêtre, ou s'arrêter pendant
/// qu'elle est ouverte ; une session peut s'ouvrir depuis l'autre bout.
/// Rien de tout cela ne passe par un clic.
fn watch(app: App) {
    crate::app::spawn(async move {
        // Ce qui ne bouge pas de toute la vie du programme : demandé une
        // fois.
        {
            let mut seen = SEEN.lock().expect("accueil");
            let new = seen.get_or_insert_with(Seen::default);
            new.version = crate::desk::build();
            new.folder = crate::folders::logs_folder();
            new.shortcuts = crate::shortcuts::engraved();
        }
        loop {
            if reread(&app).await {
                redraw(&app);
            }
            tokio::time::sleep(REFRESH).await;
        }
    });
}

/// Redemande ce que le service dit, et dit si quelque chose a changé.
///
/// Ce qui n'a pas changé n'est pas redessiné : la fenêtre reste souvent
/// ouverte pendant une session, et repeindre une image identique trois
/// fois par minute serait du processeur pris à l'image de la session.
async fn reread(app: &App) -> bool {
    let machine = crate::desk::standing().await;
    let peers = crate::desk::peers().await;
    let sessions = crate::session::sessions().await;
    let watching = crate::desk::watching().await;
    let engines = crate::folders::engines();
    let settings = crate::settings::settings(app.clone()).await;
    // Le compte, et ses appareils quand il y a un lien : sans lien il n'y
    // a rien à demander, et sans service rien à montrer.
    let account = match crate::desk::account().await {
        Ok(link) => Some(AccountState {
            devices: if link.is_some() {
                crate::desk::devices().await
            } else {
                Vec::new()
            },
            link,
        }),
        Err(_) => None,
    };

    let mut seen = SEEN.lock().expect("accueil");
    let new = seen.get_or_insert_with(Seen::default);
    let before = Seen {
        machine: new.machine.replace(machine),
        peers: std::mem::replace(&mut new.peers, peers),
        sessions: std::mem::replace(&mut new.sessions, sessions),
        watching: std::mem::replace(&mut new.watching, watching),
        engines: new.engines.replace(engines),
        settings: new.settings.replace(settings),
        account: std::mem::replace(&mut new.account, account),
        shortcuts: new.shortcuts.clone(),
        version: new.version.clone(),
        folder: new.folder.clone(),
    };
    let mut change = before != *new;
    drop(seen);

    // Une bonne nouvelle s'efface toute seule : restée à l'écran, elle
    // finit par se lire comme un état. Un ennui reste jusqu'au geste
    // suivant, puisqu'il attend qu'on y réponde.
    let mut state = STATE.lock().expect("accueil");
    if state
        .notice
        .as_ref()
        .is_some_and(|said| !said.is_trouble && said.since.elapsed() > NOTICE_TIME)
    {
        state.notice = None;
        change = true;
    }
    change
}

/// Relit ce que l'écran des réglages montre, et les trois raccourcis.
fn reread_the_settings(app: &App) {
    let app = app.clone();
    crate::app::spawn(async move {
        let settings = crate::settings::settings(app.clone()).await;
        if let Some(seen) = SEEN.lock().expect("accueil").as_mut() {
            seen.settings = Some(settings);
            seen.shortcuts = crate::shortcuts::engraved();
        }
        redraw(&app);
    });
}
