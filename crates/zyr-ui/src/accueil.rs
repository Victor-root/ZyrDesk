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
//! Les trois champs de saisie. Écrire du texte est le seul endroit où le
//! système fait mieux que nous : le curseur, la sélection, le
//! presse-papiers, les claviers qui composent leurs signes. Ce sont donc
//! trois vrais champs de Windows, posés dans le cadre que nous
//! dessinons, et qui ne vivent que le temps du dialogue qui les porte.

use std::sync::Mutex;
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};

use crate::app::App;

use crate::design::{self, Couleur, Palette};
use crate::desk::{Peer, Standing};
use crate::folders::Engines;
use crate::icones;
use crate::journal::note;
use crate::paint::{Cadre, Cale, Icone, Plume, Toile};
use crate::session::Ongoing;
use crate::settings::Settings;
use crate::shortcuts::{Combination, Doing, Held};
use crate::theme::Choix;

/// Ce que le service peut changer sans que personne ne clique : une
/// session ouverte depuis l'autre bout, un moteur déposé dans son
/// dossier, le service arrêté. Redemandé à ce rythme.
const RYTHME: std::time::Duration = std::time::Duration::from_secs(3);

/// Le temps qu'un « Copié » reste lisible avant que le bouton reprenne
/// son mot.
const TEMPS_COPIE: std::time::Duration = std::time::Duration::from_millis(1600);

/// Le temps qu'une demande de confirmation reste armée.
const TEMPS_CONFIRMATION: std::time::Duration = std::time::Duration::from_secs(4);

/// Le temps qu'une bonne nouvelle reste à l'écran avant de s'effacer.
const TEMPS_ANNONCE: std::time::Duration = std::time::Duration::from_secs(6);

/// Ce que le fil qui va et vient met à faire un aller, et le rythme
/// auquel il est redessiné.
const VA_ET_VIENT: std::time::Duration = std::time::Duration::from_millis(1400);
const IMAGE: u32 = 16;

/// L'horloge qui porte ce fil, nommée pour que rien d'autre n'y réponde.
const ANIME: usize = 1;

/// Une empreinte fait toujours cette longueur. La vérifier ici évite
/// d'aller déranger le service pour rien.
const TAILLE_EMPREINTE: usize = 64;

const MINUTE: u64 = 60;
const HEURE: u64 = 3600;

/* ---- Ce que l'accueil montre ----------------------------------------- */

/// Ce que le produit dit de lui-même.
///
/// Rien n'est décidé ici : tout vient du service, et la fenêtre ne fait
/// que le dessiner. Une session appartient au service et survit à cette
/// fenêtre fermée, mise à jour ou plantée.
#[derive(Default, PartialEq)]
struct Vu {
    machine: Option<Standing>,
    voisins: Vec<Peer>,
    sessions: Vec<Ongoing>,
    moteurs: Option<Engines>,
    reglages: Option<Settings>,
    /// Les trois raccourcis, écrits comme ils sont gravés sur le clavier
    /// branché, et rien pour ceux qui n'ont pas de touche.
    raccourcis: Vec<(Doing, Option<String>)>,
    /// Ce que fait tourner cette fenêtre, et le dossier des journaux :
    /// demandés une fois, ils ne changent pas de la vie du programme.
    version: String,
    dossier: String,
}

impl Vu {
    /// Une seule session à la fois depuis cet ordinateur : deux fenêtres
    /// vidéo en même temps ne se pilotent pas.
    fn occupe(&self, etat: &Etat) -> bool {
        etat.ouverture.is_some() || !self.sessions.is_empty()
    }

    /// Le nom sous lequel on reconnaît la machine d'une session.
    ///
    /// À l'empreinte et non à l'adresse : c'est la seule chose qui ne
    /// bouge pas d'un réseau à l'autre.
    fn nom_de(&self, session: &Ongoing) -> String {
        self.voisins
            .iter()
            .find(|voisin| voisin.fingerprint == session.fingerprint)
            .map_or_else(|| session.towards.clone(), |voisin| voisin.name.clone())
    }
}

/// Ce qu'il reste à faire pour que le produit marche, dit en clair et
/// avec de quoi y remédier.
///
/// Sans ça, un moteur absent se lit « démarrage en cours » pour toujours,
/// et un service arrêté ne se répare que par une commande.
struct AFaire {
    texte: &'static str,
    bouton: &'static str,
    remede: Remede,
}

/// Ce que le bouton d'un tel bandeau va faire.
#[derive(Clone, Copy)]
enum Remede {
    DemarrerLeService,
    MoteurHote,
    MoteurClient,
    VoirLeJournal,
}

/// Ce qui se passe pendant qu'une session s'ouvre.
///
/// Le titre ne bouge pas de toute l'ouverture : ce qui s'y passe est
/// toujours la même chose, et un titre qui change à chaque étape se lit
/// comme des nouvelles alors que ce n'en sont pas.
struct Ouverture {
    vers: String,
    detail: String,
    code: Option<String>,
    depuis: std::time::Instant,
}

/// Le bandeau du haut. Il sert aux deux : ce qui a échoué, et ce qui a
/// réussi sans laisser de trace ailleurs à l'écran. Un message rouge pour
/// dire que tout va bien se lirait comme une panne.
struct Annonce {
    texte: String,
    ennui: bool,
    depuis: std::time::Instant,
}

/* ---- Où en est l'écran ----------------------------------------------- */

/// Ce qui est ouvert par-dessus l'accueil.
#[derive(Clone, Copy, PartialEq)]
enum Ecran {
    Accueil,
    Ajout,
    Journal,
    Reglages,
}

/// Ce qui défile, et où en est son défilement.
#[derive(Clone, Copy, PartialEq)]
enum Ou {
    Page,
    Dialogue,
    /// Le texte du journal, qui défile chez lui dans le dialogue qui le
    /// porte, comme une page défile dans une fenêtre.
    Lignes,
}

/// Où en est la fenêtre : ce qui est ouvert, ce qui est sous la main, ce
/// qui attend une réponse.
struct Etat {
    ecran: Ecran,
    /// Le défilement de la page, celui du dialogue ouvert, et celui du
    /// texte du journal. Le dernier défile aussi en travers : une ligne
    /// de journal ne se replie pas.
    defile: f32,
    defile_dialogue: f32,
    defile_lignes: (f32, f32),
    /// Ce que chaque chose défilante mesurait la dernière fois qu'elle a
    /// été dessinée, la place qu'elle avait, et la course de son pouce :
    /// de quoi ne jamais défiler au-delà, et traîner l'ascenseur du même
    /// pas que celui qui a été dessiné.
    tenue: [(f32, f32, f32); 3],
    survol: Option<Quoi>,
    pressee: Option<Quoi>,
    /// L'ascenseur tenu par une main, et de combien le curseur était
    /// au-dessus de son haut quand elle l'a pris.
    tenu: Option<(Ou, f32)>,
    /// Les interrupteurs poussés dont le service n'a pas encore pris
    /// acte. Sans eux, l'état qui revient est encore l'ancien et
    /// l'interrupteur reviendrait en arrière sous le doigt.
    pousses: Vec<(Bouton, bool)>,
    /// Le bouton qui vient d'être copié, et depuis quand.
    copie: Option<(Quoi, std::time::Instant)>,
    /// Le repli du jargon, dans les réglages.
    avance: bool,
    /// La touche qui attend une combinaison. Une seule à la fois : deux
    /// boutons qui attendent la même touche se la partageraient.
    ecoute: Option<Doing>,
    /// De quel ordinateur est le journal ouvert. Rien pour celui-ci :
    /// c'est le seul dont on peut aussi vider les fichiers et ouvrir le
    /// dossier.
    journal_de: Option<Peer>,
    /// Ce que le journal ouvert montre, une ligne par ligne : le découper
    /// à chaque image reviendrait à le relire en entier pour n'en
    /// dessiner que trente lignes.
    lignes: Vec<String>,
    /// Depuis quand « Vider » attend sa confirmation.
    vidage: Option<std::time::Instant>,
    annonce: Option<Annonce>,
    /// Ce que les réglages ont à redire, qui vit dans leur dialogue.
    souci: Option<String>,
    ouverture: Option<Ouverture>,
}

impl Etat {
    const fn neuf() -> Self {
        Etat {
            ecran: Ecran::Accueil,
            defile: 0.0,
            defile_dialogue: 0.0,
            defile_lignes: (0.0, 0.0),
            tenue: [(0.0, 0.0, 0.0); 3],
            survol: None,
            pressee: None,
            tenu: None,
            pousses: Vec::new(),
            copie: None,
            avance: false,
            ecoute: None,
            journal_de: None,
            lignes: Vec::new(),
            vidage: None,
            annonce: None,
            souci: None,
            ouverture: None,
        }
    }

    /// Ce qu'une chose défilante mesure, la place qu'elle a et la course
    /// de son pouce : ce qui borne son défilement et ce qui le traîne.
    fn mesure(&self, ou: Ou) -> (f32, f32, f32) {
        self.tenue[match ou {
            Ou::Page => 0,
            Ou::Dialogue => 1,
            Ou::Lignes => 2,
        }]
    }

    fn retient(&mut self, ou: Ou, contenu: f32, place: f32, course: f32) {
        self.tenue[match ou {
            Ou::Page => 0,
            Ou::Dialogue => 1,
            Ou::Lignes => 2,
        }] = (contenu, place, course);
    }

    /// De combien cette chose-là défile en ce moment.
    fn defile(&self, ou: Ou) -> f32 {
        match ou {
            Ou::Page => self.defile,
            Ou::Dialogue => self.defile_dialogue,
            Ou::Lignes => self.defile_lignes.1,
        }
    }

    /// Fait défiler, sans jamais sortir de ce qu'il y a à voir.
    fn defile_de(&mut self, ou: Ou, de: f32) {
        let (contenu, place, _) = self.mesure(ou);
        let plus_loin = (contenu - place).max(0.0);
        let ou_maintenant = (self.defile(ou) + de).clamp(0.0, plus_loin);
        match ou {
            Ou::Page => self.defile = ou_maintenant,
            Ou::Dialogue => self.defile_dialogue = ou_maintenant,
            Ou::Lignes => self.defile_lignes.1 = ou_maintenant,
        }
    }
}

/* ---- Ce sur quoi on clique -------------------------------------------- */

/// Un interrupteur, et ce qu'il commande.
#[derive(Clone, Copy, PartialEq)]
enum Bouton {
    Acces,
    Confiance,
    AuDemarrage,
    Cadence,
    Son,
    Stats,
}

/// Un choix segmenté : plusieurs possibilités qui s'excluent, montrées
/// toutes ensemble.
#[derive(Clone, Copy, PartialEq)]
enum Choisi {
    Theme,
    Capture,
    Codec,
    Affichage,
    Souris,
}

/// Ce sur quoi on peut cliquer, et ce que ça fait.
#[derive(Clone, PartialEq)]
enum Quoi {
    OuvrirJournal,
    OuvrirReglages,
    CopierEmpreinte,
    /// Le bouton d'un bandeau « ce qu'il reste à faire ».
    ARegler(usize),
    /// Une carte d'ordinateur, et le journal de cet ordinateur-là.
    Voisin(usize),
    JournalDe(usize),
    Ajouter,
    Interrupteur(Bouton),
    Segment(Choisi, usize),
    Raccourci(Doing),
    /// Fermer le dialogue ouvert, quel qu'il soit.
    Fermer,
    Connecter,
    Oublier(usize),
    Vider,
    Actualiser,
    CopierJournal,
    /// Ouvrir le dossier des journaux, depuis le journal ou les réglages.
    OuvrirLesJournaux,
    Avance,
    Ascenseur(Ou),
}

impl Bouton {
    /// Où est l'interrupteur, d'après ce que le produit dit, et d'après
    /// ce qu'une main vient de pousser sans réponse encore.
    fn allume(self, vu: &Vu, etat: &Etat) -> bool {
        if let Some((_, veut)) = etat.pousses.iter().find(|(quoi, _)| *quoi == self) {
            return *veut;
        }
        match self {
            Bouton::Acces => vu.machine.as_ref().is_some_and(|dit| dit.wanted),
            Bouton::Confiance => vu.machine.as_ref().is_some_and(|dit| dit.trusting),
            Bouton::AuDemarrage => vu.machine.as_ref().is_some_and(|dit| dit.at_boot),
            Bouton::Cadence => vu.machine.as_ref().is_some_and(|dit| dit.steady_rate),
            Bouton::Son => vu
                .reglages
                .as_ref()
                .is_some_and(|dit| dit.mute_far_speakers),
            Bouton::Stats => vu.reglages.as_ref().is_some_and(|dit| dit.stats_overlay),
        }
    }

    /// Si on peut le pousser.
    ///
    /// Un service arrêté n'est pas un accès distant désactivé : l'un est
    /// un choix, l'autre une panne. L'interrupteur reste alors sur la
    /// position choisie et devient inactionnable, plutôt que de sauter à
    /// « non » et de faire croire à une décision que personne n'a prise.
    fn vivant(self, vu: &Vu, etat: &Etat) -> bool {
        if etat.pousses.iter().any(|(quoi, _)| *quoi == self) {
            return false;
        }
        match self {
            Bouton::Son | Bouton::Stats => vu.reglages.is_some(),
            _ => vu
                .machine
                .as_ref()
                .is_some_and(|dit| dit.unreachable.is_none()),
        }
    }
}

impl Choisi {
    /// Les mots des côtés, dans l'ordre où ils se montrent.
    fn mots(self) -> Vec<&'static str> {
        match self {
            Choisi::Theme => Choix::ALL.iter().map(|choix| choix.word()).collect(),
            Choisi::Capture => vec!["Compatible", "Rapide"],
            Choisi::Codec => vec!["Auto", "H.264", "HEVC", "AV1"],
            Choisi::Affichage => vec!["Plein écran", "Fenêtre"],
            Choisi::Souris => vec!["Bureau", "Jeu"],
        }
    }

    /// La valeur que porte chaque côté, telle qu'elle voyage et telle
    /// qu'elle s'écrit dans les réglages.
    fn valeurs(self) -> Vec<&'static str> {
        match self {
            Choisi::Theme => Vec::new(),
            Choisi::Capture => vec!["ddx", "wgc"],
            Choisi::Codec => vec!["auto", "H.264", "HEVC", "AV1"],
            Choisi::Affichage => vec!["fullscreen", "windowed"],
            Choisi::Souris => vec!["desktop", "game"],
        }
    }

    /// Lequel est choisi, d'après ce que le produit dit.
    fn ou(self, vu: &Vu) -> Option<usize> {
        let dit = match self {
            Choisi::Theme => {
                return Choix::ALL
                    .iter()
                    .position(|choix| *choix == crate::theme::chosen());
            }
            Choisi::Capture => vu.machine.as_ref()?.capture.clone(),
            Choisi::Codec => vu.reglages.as_ref()?.codec.clone(),
            Choisi::Affichage => vu.reglages.as_ref()?.display.clone(),
            Choisi::Souris => {
                let bureau = vu.reglages.as_ref()?.absolute_mouse;
                (if bureau { "desktop" } else { "game" }).to_string()
            }
        };
        self.valeurs().iter().position(|valeur| *valeur == dit)
    }

    /// Si on peut en changer.
    fn vivant(self, vu: &Vu) -> bool {
        match self {
            Choisi::Theme => true,
            Choisi::Capture => vu
                .machine
                .as_ref()
                .is_some_and(|dit| dit.unreachable.is_none()),
            _ => vu.reglages.is_some(),
        }
    }
}

/* ---- Ce que l'écran des réglages contient ----------------------------- */

/// De quoi décider, à droite d'une ligne de réglage.
enum Commande {
    /// Rien : la ligne dit seulement où en est le produit.
    Dit,
    Interrupteur(Bouton),
    Segments(Choisi),
    Touche(Doing),
    /// Un bouton qui ouvre quelque chose hors de la fenêtre.
    Ouvre(&'static str, Quoi),
}

/// Une ligne de l'écran des réglages : ce dont il s'agit à gauche, de
/// quoi en décider à droite.
struct Reglage {
    mot: &'static str,
    legende: &'static str,
    commande: Commande,
}

/// Ce que l'écran des réglages porte, dans l'ordre.
enum Element {
    /// Une étiquette de section, et le mot qui l'explique.
    Section(&'static str, &'static str),
    /// Le repli du jargon : ce qui suit ne se montre qu'ouvert.
    Repli,
    Reglage(Reglage),
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
const REGLAGES: &[Element] = &[
    Element::Reglage(Reglage {
        mot: "Ce qu'une session demande",
        legende: "",
        commande: Commande::Dit,
    }),
    Element::Reglage(Reglage {
        mot: "Thème",
        legende: "Suit Windows tant qu'on ne choisit pas.",
        commande: Commande::Segments(Choisi::Theme),
    }),
    Element::Reglage(Reglage {
        mot: "Ordinateurs du réseau local",
        legende: "Ceux qui s'annoncent sur ce réseau peuvent joindre celui-ci sans rien à \
                  recopier. Ne concerne que le réseau local.",
        commande: Commande::Interrupteur(Bouton::Confiance),
    }),
    Element::Reglage(Reglage {
        mot: "Renvoyer un écran immobile",
        legende: "Quand quelqu'un regarde cet ordinateur : réenvoyer l'écran à pleine cadence \
                  même quand rien ne bouge dessus. Le pointeur est plus fluide, mais c'est une \
                  image complète encodée soixante fois par seconde pour rien. À couper si cet \
                  ordinateur n'arrive pas à suivre.",
        commande: Commande::Interrupteur(Bouton::Cadence),
    }),
    Element::Reglage(Reglage {
        mot: "Façon de filmer l'écran",
        legende: "La façon dont cet ordinateur prend ses images quand quelqu'un le regarde. \
                  « Compatible » voit aussi les demandes de mot de passe administrateur et \
                  l'écran de connexion. « Rapide » va plus vite sur certaines machines, et ne \
                  les voit pas : elles apparaissent alors comme un écran noir.",
        commande: Commande::Segments(Choisi::Capture),
    }),
    Element::Reglage(Reglage {
        mot: "Démarrer avec Windows",
        legende: "Cet ordinateur répond dès l'allumage, avant même qu'on ouvre une session \
                  dessus, et ZyrDesk revient tout seul avec l'icône. Sans cela, rien ne tourne \
                  tant que vous n'avez pas ouvert ZyrDesk.",
        commande: Commande::Interrupteur(Bouton::AuDemarrage),
    }),
    Element::Section(
        "Raccourcis clavier",
        "Ils marchent pendant une session, par-dessus l'image. Cliquez sur une combinaison pour \
         la changer, puis tapez-la. Échap annule, Retour arrière la retire.",
    ),
    Element::Reglage(Reglage {
        mot: "Terminer la session",
        legende: "Rend son bureau à l'ordinateur distant. Une session est en cours ou terminée, \
                  jamais entre les deux.",
        commande: Commande::Touche(Doing::End),
    }),
    Element::Reglage(Reglage {
        mot: "Ouvrir le menu flottant",
        legende: "Le seul chemin de retour après avoir masqué le bouton.",
        commande: Commande::Touche(Doing::Menu),
    }),
    Element::Reglage(Reglage {
        mot: "Fenêtré ou plein écran",
        legende: "Bascule l'image de l'un à l'autre.",
        commande: Commande::Touche(Doing::Fullscreen),
    }),
    Element::Repli,
    Element::Reglage(Reglage {
        mot: "Codec vidéo",
        legende: "Auto prend le meilleur que les deux ordinateurs savent lire.",
        commande: Commande::Segments(Choisi::Codec),
    }),
    Element::Reglage(Reglage {
        mot: "Fenêtre de la session",
        legende: "L'image s'affiche dans la fenêtre ZyrDesk : ce réglage dit si cette fenêtre \
                  prend l'écran entier.",
        commande: Commande::Segments(Choisi::Affichage),
    }),
    Element::Reglage(Reglage {
        mot: "Souris",
        legende: "La souris de jeu vise en mouvements plutôt qu'en position.",
        commande: Commande::Segments(Choisi::Souris),
    }),
    Element::Reglage(Reglage {
        mot: "Couper le son de l'ordinateur distant",
        legende: "Ses enceintes se taisent pendant toute la session : la pièce où il se trouve \
                  reste silencieuse, et vous entendez tout. Le son y revient tout seul à la fin.",
        commande: Commande::Interrupteur(Bouton::Son),
    }),
    Element::Reglage(Reglage {
        mot: "Statistiques par-dessus l'image",
        legende: "Images par seconde, débit, pertes.",
        commande: Commande::Interrupteur(Bouton::Stats),
    }),
    Element::Reglage(Reglage {
        mot: "Journaux",
        legende: "",
        commande: Commande::Ouvre("Ouvrir", Quoi::OuvrirLesJournaux),
    }),
];

/* ---- Ce que la feuille de style dit, en pixels de page ---------------- */

mod tenue {
    /// La largeur au-delà de laquelle la page ne s'étale plus, et ce qui
    /// l'entoure : en haut et en bas, puis sur les côtés.
    pub const PAGE: f32 = 820.0;
    pub const HAUT: f32 = 32.0;
    pub const COTE: f32 = 24.0;

    /// La marque en haut de la page, et celle de l'écran d'ouverture.
    pub const MARQUE: f32 = 40.0;
    pub const GRANDE_MARQUE: f32 = 56.0;

    /// Un bouton, un grand bouton, et le dessin d'un bouton à icône.
    pub const BOUTON: f32 = 36.0;
    pub const GRAND_BOUTON: f32 = 44.0;
    pub const DESSIN: f32 = 18.0;

    /// L'interrupteur : sa taille, son pouce et le jeu autour.
    pub const INTERRUPTEUR: (f32, f32) = (44.0, 26.0);
    pub const POUCE: f32 = 18.0;
    pub const JEU: f32 = 3.0;

    /// Un côté de choix segmenté, et ce qui entoure le groupe.
    pub const SEGMENT: f32 = 26.0;
    pub const AUTOUR: f32 = 2.0;
    pub const RAYON_SEGMENT: f32 = 6.0;

    /// La pastille de présence, et l'anneau autour de celle qui est
    /// vivante.
    pub const PASTILLE: f32 = 8.0;
    pub const ANNEAU: f32 = 3.0;

    /// Un champ de saisie.
    pub const CHAMP: f32 = 40.0;

    /// L'épaisseur d'un trait et d'une bordure.
    pub const TRAIT: f32 = 1.0;

    /// Une carte d'ordinateur n'est jamais plus étroite que ça.
    pub const CARTE: f32 = 240.0;
    /// Ce qu'une carte d'ordinateur fait de haut, la place du mot qui
    /// n'apparaît qu'au survol comprise.
    pub const APPEL: f32 = 20.0;

    /// La largeur des trois dialogues.
    pub const DIALOGUE: f32 = 460.0;
    pub const DIALOGUE_REGLAGES: f32 = 560.0;
    pub const DIALOGUE_JOURNAL: f32 = 880.0;

    /// La touche d'un raccourci n'est jamais plus étroite que ça.
    pub const TOUCHE: f32 = 150.0;

    /// Le fil qui va et vient pendant qu'une session s'ouvre, et la part
    /// de sa longueur que parcourt le morceau qui s'y déplace.
    pub const FIL: (f32, f32) = (260.0, 3.0);
    pub const MORCEAU: f32 = 0.4;

    /// Le code d'appairage, plus grand que tout le reste parce qu'il se
    /// lit de loin, en tapant sur un autre clavier.
    pub const CODE: f32 = 34.0;

    /// L'ascenseur, et ce qui le sépare du bord.
    pub const ASCENSEUR: f32 = 6.0;

    /// Le dessin de l'écran vide.
    pub const VIDE: (f32, f32) = (64.0, 44.0);

    /// Ce qu'un cran de roulette fait défiler.
    pub const CRAN: f32 = 60.0;

    /// Ce qu'une boîte de dialogue pose de noir sur ce qu'elle recouvre.
    pub const VOILE: f32 = 0.55;

    /// La hauteur du texte du journal : ce qu'il prend au plus, en part
    /// de la fenêtre, et jamais plus que ça.
    pub const JOURNAL: (f32, f32) = (0.6, 560.0);
}

/* ---- Ce que la fenêtre tient ------------------------------------------ */

/// La fenêtre qui porte le dessin, fille de celle que le système encadre.
static ITS_WINDOW: AtomicIsize = AtomicIsize::new(0);
/// De combien un pixel de page compte ici, en centièmes.
static ECHELLE: AtomicU32 = AtomicU32::new(100);

static VU: Mutex<Option<Vu>> = Mutex::new(None);
static ETAT: Mutex<Etat> = Mutex::new(Etat::neuf());
/// Ce que la dernière image a posé, et qui répond au clic.
static CLIQUABLES: Mutex<Vec<(Quoi, Cadre)>> = Mutex::new(Vec::new());
/// Le programme, gardé ici parce que rien n'en donne un à une fenêtre du
/// système.
static PROGRAM: Mutex<Option<App>> = Mutex::new(None);

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
    design::palette(crate::theme::light())
}

fn programme() -> Option<App> {
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
    let dehors = crate::fenetre::sienne() as windows_sys::Win32::Foundation::HWND;
    if dehors.is_null() {
        note("accueil : pas de fenêtre où dessiner");
        return;
    }
    *PROGRAM.lock().expect("programme de l'accueil") = Some(app.clone());
    ECHELLE.store(
        (crate::fenetre::echelle() * 100.0).round() as u32,
        Ordering::Relaxed,
    );
    build(dehors);
    watch(app.clone());
}

/// La toile, telle que le système la connaît.
///
/// Lue par la fenêtre qui la porte, qui la redimensionne avec elle et lui
/// donne le clavier.
pub fn sa_toile() -> isize {
    ITS_WINDOW.load(Ordering::Relaxed)
}

/// De combien un pixel de page compte sur l'écran où la fenêtre est.
///
/// Redemandé quand elle change d'écran ou que l'écran change
/// d'agrandissement : tout ce qui est dessiné en descend, la police des
/// champs de saisie comprise.
pub fn mesure_l_ecran(app: &App) {
    let veut = (crate::fenetre::echelle() * 100.0).round() as u32;
    if ECHELLE.swap(veut, Ordering::Relaxed) == veut {
        return;
    }
    // Les champs de saisie sont des fenêtres du système : leur police ne
    // se remet pas à l'échelle avec le reste, il faut la leur refaire.
    habille_les_champs();
    redraw(app);
}

/// Bâtit la toile et se met devant les messages de la fenêtre qui la
/// porte.
fn build(dehors: windows_sys::Win32::Foundation::HWND) {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, GetClientRect, IDC_ARROW, LoadCursorW, RegisterClassW, WNDCLASSW,
        WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_VISIBLE,
    };

    if ITS_WINDOW.load(Ordering::Relaxed) != 0 {
        return;
    }
    let classe = wide("ZyrDeskAccueil");
    let mut dedans = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: une fenêtre du programme, dont le rectangle est lu dans le
    // nôtre.
    if unsafe { GetClientRect(dehors, &mut dedans) } == 0 {
        note("accueil : la fenêtre ne dit pas sa taille");
        return;
    }

    // SAFETY: une classe déclarée une fois et une fenêtre bâtie dessus,
    // sur le fil qui pompera ses messages. Une classe déclarée deux fois
    // est refusée sans autre effet, d'où la réponse non lue.
    let window = unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let class = WNDCLASSW {
            style: 0,
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
            lpszClassName: classe.as_ptr(),
        };
        RegisterClassW(&class);
        CreateWindowExW(
            0,
            classe.as_ptr(),
            std::ptr::null(),
            // Rognée par ses soeurs : l'image d'une session est posée
            // par-dessus dans la même fenêtre, et sans ça l'accueil se
            // redessinerait derrière elle à chaque image.
            WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
            0,
            0,
            dedans.right,
            dedans.bottom,
            dehors,
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
        dedans.right, dedans.bottom
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
        DefWindowProcW, EN_CHANGE, HTCLIENT, IDC_ARROW, IDC_HAND, LoadCursorW, SetCursor,
        WM_COMMAND, WM_CTLCOLOREDIT, WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP,
        WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT, WM_SETCURSOR, WM_SYSKEYDOWN, WM_TIMER,
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
        WM_TIMER if holding == ANIME => {
            invalide(window);
            0
        }
        WM_MOUSEMOVE => {
            bouge(window, ou_est(with));
            0
        }
        WM_MOUSELEAVE => {
            quitte(window);
            0
        }
        WM_LBUTTONDOWN => {
            // SAFETY: une fenêtre à nous, à qui le clavier est donné pour
            // qu'Échap, Entrée et les combinaisons arrivent ici.
            unsafe { SetFocus(window) };
            appuie(window, ou_est(with));
            0
        }
        WM_LBUTTONUP => {
            relache(window, ou_est(with));
            0
        }
        WM_MOUSEWHEEL => {
            let crans = ((holding >> 16) & 0xFFFF) as i16;
            let travers = (holding & 0x0004) != 0;
            roule(window, f32::from(crans) / 120.0, travers);
            0
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            if touche(window, holding as u32, with) {
                return 0;
            }
            // SAFETY: same.
            unsafe { DefWindowProcW(window, message, holding, with) }
        }
        WM_SETCURSOR if (with as u32 & 0xFFFF) == HTCLIENT => {
            let forme = if ETAT.lock().expect("accueil").survol.is_some() {
                IDC_HAND
            } else {
                IDC_ARROW
            };
            // SAFETY: un curseur du système, demandé par son nom.
            unsafe { SetCursor(LoadCursorW(std::ptr::null_mut(), forme)) };
            1
        }
        // Le fond d'un champ de saisie, et l'encre dedans : ils
        // appartiennent au système, qui demande ici de quelle couleur les
        // peindre pour qu'ils soient de la couleur du reste.
        WM_CTLCOLOREDIT => teinte_du_champ(holding),
        // Un champ dont le texte change change aussi ce que le dialogue
        // dit sous lui et ce que son bouton permet.
        WM_COMMAND if (holding >> 16) as u32 & 0xFFFF == EN_CHANGE => {
            invalide(window);
            0
        }
        _ => {
            // Ce qu'un champ de saisie a demandé, fait ici parce que les
            // deux referment le dialogue et donc détruisent ce champ.
            if message == AGIR {
                if let Some(app) = programme() {
                    fait(
                        &app,
                        if holding == 1 {
                            Quoi::Connecter
                        } else {
                            Quoi::Fermer
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
const AGIR: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 1;

/// Où la souris est, en vrais pixels depuis le coin de la toile.
fn ou_est(with: windows_sys::Win32::Foundation::LPARAM) -> (f32, f32) {
    let x = (with & 0xFFFF) as i16;
    let y = ((with >> 16) & 0xFFFF) as i16;
    (f32::from(x), f32::from(y))
}

/// Un mot dans les caractères que Windows compte, fini par le zéro qu'il
/// cherche.
fn wide(mot: &str) -> Vec<u16> {
    mot.encode_utf16().chain(Some(0)).collect()
}

/// Dessine l'accueil et le verse dans la fenêtre.
fn repaint(window: windows_sys::Win32::Foundation::HWND) {
    use windows_sys::Win32::Graphics::Gdi::{BeginPaint, EndPaint, PAINTSTRUCT};
    use windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect;

    let mut dedans = windows_sys::Win32::Foundation::RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: une fenêtre à nous, dont le rectangle est lu dans le nôtre.
    if unsafe { GetClientRect(window, &mut dedans) } == 0 {
        return;
    }
    let (large, haute) = (dedans.right.max(1), dedans.bottom.max(1));

    let mut peinture: PAINTSTRUCT = unsafe { std::mem::zeroed() };
    // SAFETY: une fenêtre à nous, dont la surface est rendue plus bas.
    let surface = unsafe { BeginPaint(window, &mut peinture) };
    if surface.is_null() {
        return;
    }
    TOILE.with_borrow_mut(|toile| {
        if toile
            .as_ref()
            .is_none_or(|deja| deja.taille() != (large, haute))
        {
            *toile = Toile::neuve(large, haute);
        }
        let Some(toile) = toile.as_ref() else {
            return;
        };
        let couleurs = palette();
        toile.commence(couleurs.fond);
        let cliquables = peins(toile, large as f32, haute as f32, couleurs);
        if !toile.finit() {
            return;
        }
        *CLIQUABLES.lock().expect("accueil") = cliquables;
        toile.verse(windows::Win32::Graphics::Gdi::HDC(surface), 0, 0);
    });
    // SAFETY: la peinture ouverte juste au-dessus.
    unsafe { EndPaint(window, &peinture) };
    range_les_champs();
    horloge(window);
}

/// Fait battre l'horloge tant qu'un fil va et vient, et l'arrête après.
///
/// La seule chose de l'accueil qui bouge sans que personne ne touche à
/// rien. Ailleurs, rien n'est redessiné tant que rien ne change.
fn horloge(window: windows_sys::Win32::Foundation::HWND) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{KillTimer, SetTimer};

    let anime = ETAT.lock().expect("accueil").ouverture.is_some();
    // SAFETY: une horloge nommée par nous sur une fenêtre à nous.
    unsafe {
        if anime {
            SetTimer(window, ANIME, IMAGE, None);
        } else {
            KillTimer(window, ANIME);
        }
    }
}

/* ---- Le dessin --------------------------------------------------------- */

/// Ce qui pose l'accueil : la toile, ce qu'on montre, où on en est, et ce
/// qui répond au clic une fois posé.
struct Mise<'a> {
    toile: &'a Toile,
    echelle: f32,
    couleurs: Palette,
    vu: &'a Vu,
    etat: &'a Etat,
    cliquables: Vec<(Quoi, Cadre)>,
    /// Ce que chaque chose défilante mesure, relevé au passage et rendu
    /// à l'état une fois la marche finie.
    mesures: Vec<(Ou, f32, f32, f32)>,
    /// Faux quand un dialogue est ouvert : la page derrière ne répond
    /// plus au clic, et ce qui est dessiné dessous ne s'allume plus sous
    /// la souris.
    vivante: bool,
    /// Vrai quand la marche ne fait que mesurer : rien n'est posé, et ce
    /// qui revient est la hauteur que ça prendrait.
    muet: bool,
}

impl Mise<'_> {
    /// Une longueur du système de design, en vrais pixels.
    fn px(&self, page: f32) -> f32 {
        page * self.echelle
    }

    /// Une plume de cette taille de page.
    fn plume(&self, taille: f32) -> Plume {
        Plume::de(self.px(taille))
    }

    fn corps(&self) -> Plume {
        self.plume(design::CORPS)
    }

    fn legende(&self) -> Plume {
        self.plume(design::LEGENDE)
    }

    fn sous_titre(&self) -> Plume {
        self.plume(design::SOUS_TITRE).en_gras()
    }

    /// L'étiquette d'une section : petite, en capitales, écartée, et
    /// jamais criarde.
    fn section(&self, ou: Cadre, mot: &str) {
        self.ecris(
            &mot.to_uppercase(),
            self.legende().en_gras().ecartee(0.08),
            self.couleurs.texte_faible,
            ou,
        );
    }

    /// La hauteur d'une ligne écrite de cette plume.
    fn haute(&self, plume: Plume) -> f32 {
        self.toile.haute(plume)
    }

    /// La hauteur d'un bloc replié à cette largeur.
    fn hauteur(&self, mot: &str, plume: Plume, large: f32) -> f32 {
        if mot.is_empty() {
            return 0.0;
        }
        self.toile.hauteur(mot, plume, large)
    }

    /// Écrit un bloc dans cette largeur, à partir de ce haut, et rend ce
    /// qu'il a pris.
    fn bloc(
        &self,
        gauche: f32,
        haut: f32,
        large: f32,
        mot: &str,
        plume: Plume,
        encre: Couleur,
    ) -> f32 {
        let haute = self.hauteur(mot, plume, large);
        if haute > 0.0 {
            self.ecris(mot, plume, encre, Cadre::pose(gauche, haut, large, haute));
        }
        haute
    }

    /// Une carte : son ombre, son fond et son trait.
    fn carte(&self, ou: Cadre) {
        let rayon = self.px(design::RAYON_GRAND);
        self.ombre(ou, rayon, self.couleurs.ombre_1);
        self.remplis(ou, rayon, self.couleurs.surface_1);
        self.trace(ou, rayon, self.couleurs.r#trait);
    }

    /// Une carte qui attend d'être remplie : pas de fond, un trait en
    /// pointillés.
    fn carte_en_attente(&self, ou: Cadre) {
        let rayon = self.px(design::RAYON_GRAND);
        self.pointille(ou, rayon, self.couleurs.trait_fort);
    }

    /// Un trait de séparation, sur toute cette largeur.
    fn separateur(&self, gauche: f32, haut: f32, large: f32) {
        self.remplis(
            Cadre::pose(gauche, haut, large, self.px(tenue::TRAIT)),
            0.0,
            self.couleurs.r#trait,
        );
    }

    /// Si cette chose est sous la main, et si elle est enfoncée.
    fn sous_la_main(&self, quoi: &Quoi) -> bool {
        self.vivante && self.etat.survol.as_ref() == Some(quoi)
    }

    fn enfoncee(&self, quoi: &Quoi) -> bool {
        self.vivante && self.etat.pressee.as_ref() == Some(quoi)
    }

    /// Note que ceci répond au clic.
    fn repond(&mut self, quoi: Quoi, ou: Cadre) {
        if self.vivante {
            self.cliquables.push((quoi, ou));
        }
    }

    /// La pastille de présence.
    fn pastille(&self, ou: Cadre, encre: Couleur, vivante: bool) {
        let rayon = (ou.droite - ou.gauche) / 2.0;
        if vivante {
            let anneau = self.px(tenue::ANNEAU);
            self.remplis(ou.elargi(anneau), rayon + anneau, encre.voile(0.18));
        }
        self.remplis(ou, rayon, encre);
    }
}

/// Ce qu'un bouton est : ce qui appelle le clic, ce qui l'accompagne, et
/// ce qui prévient avant de détruire.
#[derive(Clone, Copy, PartialEq)]
enum Sorte {
    Principal,
    Discret,
    Attention,
}

impl Mise<'_> {
    /// Ce qu'un bouton prend de large : son mot et ce qui l'entoure.
    fn large_du_bouton(&self, mot: &str, grand: bool) -> f32 {
        let plume = if grand {
            self.sous_titre()
        } else {
            self.corps()
        };
        let autour = if grand { design::PAS_5 } else { design::PAS_4 };
        self.toile.largeur(mot, plume) + self.px(autour) * 2.0
    }

    /// Un bouton portant un mot, à cet endroit.
    fn bouton(&mut self, ou: Cadre, mot: &str, sorte: Sorte, quoi: Quoi, vivant: bool) {
        let rayon = self.px(design::RAYON_PETIT);
        let couleurs = self.couleurs;
        let dessus = vivant && self.sous_la_main(&quoi);
        let (fond, encre, bord) = match sorte {
            Sorte::Principal if !vivant => (couleurs.surface_3, couleurs.texte_faible, None),
            Sorte::Principal if dessus => (couleurs.accent_vif, couleurs.sur_accent, None),
            Sorte::Principal => (couleurs.accent, couleurs.sur_accent, None),
            Sorte::Discret if dessus => (
                couleurs.surface_2,
                couleurs.texte,
                Some(couleurs.trait_fort),
            ),
            Sorte::Discret => (
                Couleur::RIEN,
                couleurs.texte_doux,
                Some(couleurs.trait_fort),
            ),
            Sorte::Attention => (
                Couleur::RIEN,
                couleurs.attention,
                Some(couleurs.attention.melee(couleurs.r#trait, 0.55)),
            ),
        };
        let voile = if vivant { 1.0 } else { 0.45 };
        self.remplis(ou, rayon, fond.voile(voile));
        if let Some(bord) = bord {
            self.trace(ou, rayon, bord.voile(voile));
        }
        let grand = ou.bas - ou.haut > self.px((tenue::BOUTON + tenue::GRAND_BOUTON) / 2.0);
        let plume = if grand {
            self.sous_titre()
        } else {
            self.corps()
        }
        .a(Cale::Centre);
        self.ecris(mot, plume, encre.voile(voile), ou);
        if vivant {
            self.repond(quoi, ou);
        }
    }

    /// Un bouton qui ne porte qu'un dessin, et garde la même hauteur que
    /// ceux qui portent un mot.
    fn bouton_icone(&mut self, ou: Cadre, icone: &'static Icone, quoi: Quoi, discret: bool) {
        let rayon = self.px(design::RAYON_PETIT);
        let dessus = self.sous_la_main(&quoi);
        if dessus {
            self.remplis(ou, rayon, self.couleurs.surface_2);
        }
        let encre = if dessus {
            self.couleurs.texte
        } else if discret {
            self.couleurs.texte_doux.voile(0.4)
        } else {
            self.couleurs.texte_doux
        };
        let cote = self.px(tenue::DESSIN);
        let milieu = ((ou.gauche + ou.droite) / 2.0, (ou.haut + ou.bas) / 2.0);
        self.icone(
            icone,
            Cadre::pose(milieu.0 - cote / 2.0, milieu.1 - cote / 2.0, cote, cote),
            encre,
        );
        self.repond(quoi, ou);
    }

    /// L'interrupteur : son rail, et le pouce qui glisse dedans.
    fn interrupteur(&mut self, gauche: f32, milieu: f32, bouton: Bouton) -> Cadre {
        let (large, haute) = (
            self.px(tenue::INTERRUPTEUR.0),
            self.px(tenue::INTERRUPTEUR.1),
        );
        let ou = Cadre::pose(gauche, milieu - haute / 2.0, large, haute);
        let allume = bouton.allume(self.vu, self.etat);
        let vivant = bouton.vivant(self.vu, self.etat);
        let voile = if vivant { 1.0 } else { 0.45 };
        let rayon = haute / 2.0;
        let (fond, trait_, pouce) = if allume {
            (
                self.couleurs.accent,
                self.couleurs.accent,
                self.couleurs.sur_accent,
            )
        } else {
            (
                self.couleurs.surface_3,
                self.couleurs.trait_fort,
                self.couleurs.texte_doux,
            )
        };
        self.remplis(ou, rayon, fond.voile(voile));
        self.trace(ou, rayon, trait_.voile(voile));

        let cote = self.px(tenue::POUCE);
        let jeu = self.px(tenue::JEU);
        let x = if allume {
            ou.droite - jeu - cote
        } else {
            ou.gauche + jeu
        };
        self.remplis(
            Cadre::pose(x, ou.haut + jeu, cote, cote),
            cote / 2.0,
            pouce.voile(voile),
        );
        if vivant {
            self.repond(Quoi::Interrupteur(bouton), ou);
        }
        ou
    }

    /// Ce qu'un choix segmenté prend de large.
    fn large_des_segments(&self, quoi: Choisi) -> f32 {
        let autour = self.px(tenue::AUTOUR);
        let cotes: f32 = quoi.mots().iter().map(|mot| self.large_du_cote(mot)).sum();
        cotes + autour * 2.0 + self.px(tenue::AUTOUR) * (quoi.mots().len() as f32 - 1.0)
    }

    fn large_du_cote(&self, mot: &str) -> f32 {
        self.toile.largeur(mot, self.legende()) + self.px(design::PAS_3) * 2.0
    }

    /// Un choix segmenté, posé à partir de ce bord droit.
    fn segments(&mut self, droite: f32, milieu: f32, quoi: Choisi) -> Cadre {
        let large = self.large_des_segments(quoi);
        let autour = self.px(tenue::AUTOUR);
        let haute = self.px(tenue::SEGMENT) + autour * 2.0;
        let ou = Cadre::pose(droite - large, milieu - haute / 2.0, large, haute);
        let rayon = self.px(design::RAYON_PETIT);
        let vivant = quoi.vivant(self.vu);
        let voile = if vivant { 1.0 } else { 0.45 };
        self.remplis(ou, rayon, self.couleurs.surface_2.voile(voile));
        self.trace(ou, rayon, self.couleurs.r#trait.voile(voile));

        let choisi = quoi.ou(self.vu);
        let mut x = ou.gauche + autour;
        for (rang, mot) in quoi.mots().iter().enumerate() {
            let cote = Cadre::pose(
                x,
                ou.haut + autour,
                self.large_du_cote(mot),
                self.px(tenue::SEGMENT),
            );
            let ici = Quoi::Segment(quoi, rang);
            let encre = if choisi == Some(rang) {
                let rond = self.px(tenue::RAYON_SEGMENT);
                self.ombre(cote, rond, self.couleurs.ombre_1);
                self.remplis(cote, rond, self.couleurs.surface_1.voile(voile));
                self.couleurs.texte
            } else if vivant && self.sous_la_main(&ici) {
                self.couleurs.texte_doux
            } else {
                self.couleurs.texte_faible
            };
            self.ecris(
                mot,
                self.legende().a(Cale::Centre),
                encre.voile(voile),
                cote,
            );
            if vivant {
                self.repond(ici, cote);
            }
            x = cote.droite + autour;
        }
        ou
    }

    /// Un bandeau : ce qu'il a à dire, et de quoi y remédier quand il y a
    /// quelque chose à faire.
    fn bandeau(
        &mut self,
        gauche: f32,
        haut: f32,
        large: f32,
        mot: &str,
        alerte: bool,
        action: Option<(&str, Quoi)>,
    ) -> f32 {
        let dedans = self.px(design::PAS_4);
        let bouton = action.map(|(mot, quoi)| (self.large_du_bouton(mot, false), mot, quoi));
        let place_du_bouton = bouton
            .as_ref()
            .map_or(0.0, |(large, _, _)| large + self.px(design::PAS_4));
        let pour_le_mot = large - dedans * 2.0 - place_du_bouton;
        let texte = self.hauteur(mot, self.corps(), pour_le_mot);
        let haute = (texte + self.px(design::PAS_3) * 2.0).max(if bouton.is_some() {
            self.px(tenue::BOUTON) + self.px(design::PAS_3) * 2.0
        } else {
            0.0
        });
        let ou = Cadre::pose(gauche, haut, large, haute);
        let rayon = self.px(design::RAYON);
        let (fond, bord) = if alerte {
            (
                self.couleurs.erreur.melee(self.couleurs.surface_2, 0.08),
                self.couleurs.erreur.melee(self.couleurs.r#trait, 0.4),
            )
        } else {
            (self.couleurs.surface_2, self.couleurs.r#trait)
        };
        self.remplis(ou, rayon, fond);
        self.trace(ou, rayon, bord);
        self.ecris(
            mot,
            self.corps(),
            self.couleurs.texte_doux,
            Cadre::pose(
                ou.gauche + dedans,
                (ou.haut + ou.bas) / 2.0 - texte / 2.0,
                pour_le_mot,
                texte,
            ),
        );
        if let Some((large_du_bouton, mot, quoi)) = bouton {
            let hauteur = self.px(tenue::BOUTON);
            self.bouton(
                Cadre::pose(
                    ou.droite - dedans - large_du_bouton,
                    (ou.haut + ou.bas) / 2.0 - hauteur / 2.0,
                    large_du_bouton,
                    hauteur,
                ),
                mot,
                Sorte::Discret,
                quoi,
                true,
            );
        }
        haute
    }

    /// L'ascenseur d'une chose qui défile, quand il y a plus à voir que
    /// de place.
    ///
    /// `coin` est l'arrondi de ce qui défile : le pouce s'arrête là où
    /// le coin commence, faute de quoi il dépasserait de la forme qu'il
    /// longe.
    fn ascenseur(&mut self, ou: Ou, place: Cadre, contenu: f32, coin: f32) {
        let voit = place.bas - place.haut;
        if contenu <= voit + 1.0 {
            self.mesures.push((ou, contenu, voit, 0.0));
            return;
        }
        let large = self.px(tenue::ASCENSEUR);
        let jeu = self.px(design::PAS_1);
        let rail = (voit - coin * 2.0).max(0.0);
        let haut = pouce_de(rail, voit, contenu, self.echelle);
        let course = rail - haut;
        self.mesures.push((ou, contenu, voit, course));
        let ou_est = (self.etat.defile(ou) / (contenu - voit)).clamp(0.0, 1.0);
        let pouce = Cadre::pose(
            place.droite - large - jeu,
            place.haut + coin + course * ou_est,
            large,
            haut,
        );
        let dessus = self.sous_la_main(&Quoi::Ascenseur(ou));
        let encre = if dessus {
            self.couleurs.texte_faible
        } else {
            self.couleurs.trait_fort
        };
        self.remplis(pouce, large / 2.0, encre);
        self.repond(Quoi::Ascenseur(ou), pouce);
    }
}

/// La hauteur du pouce d'un ascenseur : sur son rail, la part de ce
/// qu'on voit dans ce qu'il y a, et jamais si petit qu'on ne puisse plus
/// l'attraper.
fn pouce_de(rail: f32, voit: f32, contenu: f32, echelle: f32) -> f32 {
    (rail * voit / contenu)
        .max(design::PAS_5 * echelle)
        .min(rail)
}

/* ---- La page ----------------------------------------------------------- */

/// Dessine tout ce qui est à l'écran et rend ce qui répond au clic.
fn peins(toile: &Toile, large: f32, haute: f32, couleurs: Palette) -> Vec<(Quoi, Cadre)> {
    let rien = Vu::default();
    let garde = VU.lock().expect("accueil");
    let vu = garde.as_ref().unwrap_or(&rien);
    let mut etat = ETAT.lock().expect("accueil");

    let (cliquables, mesures) = {
        let ouvre = etat.ecran != Ecran::Accueil;
        let ouverture = etat.ouverture.is_some();
        let mut mise = Mise {
            toile,
            echelle: echelle(),
            couleurs,
            vu,
            etat: &etat,
            cliquables: Vec::new(),
            mesures: Vec::new(),
            vivante: !ouvre && !ouverture,
            muet: false,
        };
        mise.page(large, haute);
        if ouvre && !ouverture {
            // Le fond noirci : ce qui est derrière n'est plus d'actualité
            // et ne répond plus au clic, ce que la page disait déjà en
            // rendant son dialogue modal.
            mise.remplis(
                Cadre::pose(0.0, 0.0, large, haute),
                0.0,
                Couleur::NOIR.voile(tenue::VOILE),
            );
            mise.vivante = true;
            mise.dialogue(large, haute);
        }
        if ouverture {
            mise.ouverture(large, haute);
        }
        (mise.cliquables, mise.mesures)
    };
    for (ou, contenu, place, course) in mesures {
        etat.retient(ou, contenu, place, course);
    }
    cliquables
}

impl Mise<'_> {
    /// L'accueil lui-même : ce qu'est cet ordinateur, puis les autres.
    fn page(&mut self, large: f32, haute: f32) {
        let cote = self.px(tenue::COTE);
        let dedans = (large - cote * 2.0).clamp(self.px(200.0), self.px(tenue::PAGE));
        let x = ((large - dedans) / 2.0).max(cote);
        let depart = self.px(tenue::HAUT) - self.etat.defile;
        let mut y = depart;

        y += self.entete(x, y, dedans);
        y += self.px(design::PAS_5);
        y += self.cet_ordinateur(x, y, dedans);
        y += self.px(design::PAS_5);
        y += self.mes_ordinateurs(x, y, dedans, haute);

        // La version est sous les yeux sans jamais peser : en bas de la
        // fenêtre quand la page n'en remplit pas la hauteur, et à la
        // suite du reste quand elle la dépasse.
        let version = self.haute(self.legende());
        let contenu = y - depart + self.px(tenue::HAUT) + version;
        let en_bas = (haute - self.px(tenue::HAUT) - version).max(y + self.px(design::PAS_2));
        let (mot, encre) = self.la_version();
        self.ecris(
            &mot,
            self.legende().a(Cale::Centre),
            encre,
            Cadre::pose(x, en_bas, dedans, version),
        );

        self.ascenseur(Ou::Page, Cadre::pose(0.0, 0.0, large, haute), contenu, 0.0);
    }

    /// La marque, le nom du produit, et les deux commandes rangées à
    /// droite : à portée, jamais au centre de l'attention.
    fn entete(&mut self, x: f32, y: f32, large: f32) -> f32 {
        let marque = self.px(tenue::MARQUE);
        self.marque(Cadre::pose(x, y, marque, marque));

        let bouton = self.px(tenue::BOUTON);
        let ecart = self.px(design::PAS_2);
        let milieu = y + (marque - bouton) / 2.0;
        let reglages = Cadre::pose(x + large - bouton, milieu, bouton, bouton);
        let journal = reglages.decale(-(bouton + ecart), 0.0);
        self.bouton_icone(reglages, &icones::REGLAGES, Quoi::OuvrirReglages, false);
        self.bouton_icone(journal, &icones::JOURNAL, Quoi::OuvrirJournal, false);

        let depuis = x + marque + self.px(design::PAS_3);
        self.ecris(
            "ZyrDesk",
            self.plume(design::TITRE).en_gras().coupee(),
            self.couleurs.texte,
            Cadre {
                gauche: depuis,
                haut: y,
                droite: journal.gauche - ecart,
                bas: y + marque,
            },
        );
        marque
    }

    /// Ce qu'est cet ordinateur : son nom, son état, son empreinte, et ce
    /// qu'il reste à faire pour qu'il marche.
    fn cet_ordinateur(&mut self, x: f32, y: f32, large: f32) -> f32 {
        let etiquette = self.haute(self.legende().en_gras());
        self.section(Cadre::pose(x, y, large, etiquette), "Cet ordinateur");
        let mut pris = etiquette + self.px(design::PAS_3);
        pris += self.carte_de_la_machine(x, y + pris, large);
        for (rang, faire) in ce_qui_manque(self.vu).into_iter().enumerate() {
            pris += self.px(design::PAS_3);
            pris += self.bandeau(
                x,
                y + pris,
                large,
                faire.texte,
                true,
                Some((faire.bouton, Quoi::ARegler(rang))),
            );
        }
        pris
    }

    /// La carte de cette machine : son identité en haut, son empreinte en
    /// bas sur son propre fond.
    fn carte_de_la_machine(&mut self, x: f32, y: f32, large: f32) -> f32 {
        let dedans = self.px(design::PAS_5);
        let (nom, etat) = (self.haute(self.sous_titre()), self.haute(self.corps()));
        let ecart = self.px(design::PAS_1);
        let en_haut = (nom + ecart + etat).max(self.px(tenue::INTERRUPTEUR.1)) + dedans * 2.0;

        let legende = self.haute(self.legende());
        let copier = self.large_du_bouton("Copier", false);
        let vu = self.vu;
        let dit = vu.machine.as_ref();
        let empreinte = dit.map_or_else(String::new, |dit| {
            if dit.fingerprint.is_empty() {
                "indisponible".to_string()
            } else {
                dit.fingerprint.clone()
            }
        });
        let pour_l_empreinte = large - dedans * 2.0 - copier - self.px(design::PAS_4);
        let plume_empreinte = self.legende().a_chasse_fixe().ecartee(0.02);
        let haute_empreinte = self.hauteur(&empreinte, plume_empreinte, pour_l_empreinte);
        let en_bas = (legende + ecart + haute_empreinte).max(self.px(tenue::BOUTON))
            + self.px(design::PAS_3) * 2.0;

        let ou = Cadre::pose(x, y, large, en_haut + en_bas);
        self.carte(ou);
        let bas = Cadre {
            haut: ou.haut + en_haut,
            ..ou
        };
        // Le fond du bas de la carte : le même rectangle arrondi, vu au
        // travers de sa moitié basse, ce qu'aucun rectangle arrondi ne
        // sait être à lui seul.
        let rayon = self.px(design::RAYON_GRAND);
        if !self.muet {
            let (toile, fond) = (self.toile, self.couleurs.surface_2);
            toile.serre(bas, || toile.remplis(ou, rayon, fond));
        }
        self.separateur(bas.gauche, bas.haut, large);
        self.trace(ou, rayon, self.couleurs.r#trait);

        // Le haut : le nom, l'état, et l'interrupteur d'accès distant.
        let milieu = (ou.haut + bas.haut) / 2.0;
        let legende_acces = "Accès distant";
        let large_acces = self.toile.largeur(legende_acces, self.legende());
        let interrupteur = self.interrupteur(
            ou.droite - dedans - self.px(tenue::INTERRUPTEUR.0),
            milieu,
            Bouton::Acces,
        );
        self.ecris(
            legende_acces,
            self.legende(),
            self.couleurs.texte_doux,
            Cadre::pose(
                interrupteur.gauche - self.px(design::PAS_3) - large_acces,
                milieu - legende / 2.0,
                large_acces,
                legende,
            ),
        );

        let pour_le_nom = interrupteur.gauche - self.px(design::PAS_4) - (ou.gauche + dedans);
        let haut_du_nom = milieu - (nom + ecart + etat) / 2.0;
        let comment = dit.map_or_else(|| "Recherche du service…".to_string(), mot_de_l_etat);
        self.ecris(
            &dit.map_or_else(|| "…".to_string(), |dit| dit.name.clone()),
            self.sous_titre().coupee(),
            self.couleurs.texte,
            Cadre::pose(ou.gauche + dedans, haut_du_nom, pour_le_nom, nom),
        );
        let pastille = self.px(tenue::PASTILLE);
        let bas_du_mot = haut_du_nom + nom + ecart;
        self.pastille(
            Cadre::pose(
                ou.gauche + dedans,
                bas_du_mot + (etat - pastille) / 2.0,
                pastille,
                pastille,
            ),
            dit.map_or(self.couleurs.hors_ligne, |dit| {
                couleur_de_l_etat(dit, self.couleurs)
            }),
            dit.is_some_and(|dit| dit.hosting),
        );
        self.ecris(
            &comment,
            self.corps().coupee(),
            self.couleurs.texte_doux,
            Cadre::pose(
                ou.gauche + dedans + pastille + self.px(design::PAS_2),
                bas_du_mot,
                pour_le_nom - pastille - self.px(design::PAS_2),
                etat,
            ),
        );

        // Le bas : l'empreinte, et de quoi la copier.
        let haut_du_bas = (bas.haut + bas.bas) / 2.0 - (legende + ecart + haute_empreinte) / 2.0;
        self.ecris(
            "Empreinte de cet ordinateur",
            self.legende(),
            self.couleurs.texte_doux,
            Cadre::pose(bas.gauche + dedans, haut_du_bas, pour_l_empreinte, legende),
        );
        self.ecris(
            &empreinte,
            plume_empreinte,
            self.couleurs.texte_faible,
            Cadre::pose(
                bas.gauche + dedans,
                haut_du_bas + legende + ecart,
                pour_l_empreinte,
                haute_empreinte,
            ),
        );
        let hauteur = self.px(tenue::BOUTON);
        let peut = dit.is_some_and(|dit| !dit.fingerprint.is_empty());
        self.bouton(
            Cadre::pose(
                bas.droite - dedans - copier,
                (bas.haut + bas.bas) / 2.0 - hauteur / 2.0,
                copier,
                hauteur,
            ),
            if self.etat.copie.as_ref().map(|(quoi, _)| quoi) == Some(&Quoi::CopierEmpreinte) {
                "Copié"
            } else {
                "Copier"
            },
            Sorte::Discret,
            Quoi::CopierEmpreinte,
            peut,
        );
        en_haut + en_bas
    }
}

/// Ce qui empêche le produit de marcher, dit en clair et avec de quoi y
/// remédier.
///
/// Hors de la marche parce que le clic la relit : le bouton d'un bandeau
/// ne porte que son rang, et c'est ici que ce rang retrouve ce qu'il
/// répare.
fn ce_qui_manque(vu: &Vu) -> Vec<AFaire> {
    let mut manques = Vec::new();
    if let Some(dit) = vu.machine.as_ref() {
        if dit.unreachable.is_some() {
            manques.push(AFaire {
                texte: "Le service ZyrDesk ne tourne pas. Cet ordinateur ne peut ni être \
                        contrôlé ni en contrôler un autre.",
                bouton: "Démarrer le service",
                remede: Remede::DemarrerLeService,
            });
        } else if dit.wanted && dit.holdup == "engineMissing" {
            manques.push(AFaire {
                texte: "Le moteur hôte n'est pas installé : cet ordinateur ne peut pas être \
                        contrôlé. Déposez-le dans son dossier, il sera repris tout seul.",
                bouton: "Ouvrir le dossier",
                remede: Remede::MoteurHote,
            });
        } else if dit.wanted && dit.holdup == "engineWontStand" {
            manques.push(AFaire {
                texte: "Le moteur hôte ne tient pas en marche. Coupez puis rallumez l'accès \
                        distant pour réessayer ; le journal dit pourquoi.",
                bouton: "Voir le journal",
                remede: Remede::VoirLeJournal,
            });
        }
    }
    if vu.moteurs.as_ref().is_some_and(|dit| !dit.client_here) {
        manques.push(AFaire {
            texte: "Le moteur client n'est pas installé : cet ordinateur ne peut en contrôler \
                    aucun autre.",
            bouton: "Ouvrir le dossier",
            remede: Remede::MoteurClient,
        });
    }
    manques
}
impl Mise<'_> {
    /// Les autres ordinateurs, et ce qui se passe en ce moment.
    ///
    /// Une session en cours passe avant la liste : c'est la première
    /// chose à voir en ouvrant la fenêtre, y compris quand ce n'est pas
    /// elle qui l'a lancée.
    fn mes_ordinateurs(&mut self, x: f32, y: f32, large: f32, haute: f32) -> f32 {
        let etiquette = self.haute(self.legende().en_gras());
        self.section(Cadre::pose(x, y, large, etiquette), "Mes ordinateurs");
        let mut pris = etiquette + self.px(design::PAS_3);

        for session in &self.vu.sessions {
            pris += self.carte_de_session(x, y + pris, large, session);
            pris += self.px(design::PAS_3);
        }
        if let Some(annonce) = &self.etat.annonce {
            let texte = annonce.texte.clone();
            let ennui = annonce.ennui;
            pris += self.bandeau(x, y + pris, large, &texte, ennui, None);
            pris += self.px(design::PAS_3);
        }

        if self.vu.voisins.is_empty() && !self.vu.occupe(self.etat) {
            return pris + self.aucun_ordinateur(x, y + pris, large, haute - y - pris);
        }
        pris + self.grille(x, y + pris, large)
    }

    /// Le bandeau d'une session en cours.
    ///
    /// La carte de l'ordinateur, plus bas, porte déjà son adresse et son
    /// état : ceci dit ce qui se passe, il ne le répète pas.
    fn carte_de_session(&mut self, x: f32, y: f32, large: f32, session: &Ongoing) -> f32 {
        let dedans = self.px(design::PAS_5);
        let (nom, mot) = (self.haute(self.sous_titre()), self.haute(self.legende()));
        let ecart = self.px(design::PAS_1);
        let ou = Cadre::pose(x, y, large, nom + ecart + mot + dedans * 2.0);
        let rayon = self.px(design::RAYON_GRAND);
        self.ombre(ou, rayon, self.couleurs.ombre_1);
        self.remplis(
            ou,
            rayon,
            self.couleurs.en_ligne.melee(self.couleurs.surface_1, 0.07),
        );
        self.trace(
            ou,
            rayon,
            self.couleurs.en_ligne.melee(self.couleurs.r#trait, 0.4),
        );

        let pastille = self.px(tenue::PASTILLE);
        self.pastille(
            Cadre::pose(
                ou.gauche + dedans,
                ou.haut + dedans + (nom - pastille) / 2.0,
                pastille,
                pastille,
            ),
            self.couleurs.en_ligne,
            true,
        );
        let depuis = ou.gauche + dedans + pastille + self.px(design::PAS_2);
        self.ecris(
            &format!("Session en cours vers {}", self.vu.nom_de(session)),
            self.sous_titre().coupee(),
            self.couleurs.texte,
            Cadre::pose(depuis, ou.haut + dedans, ou.droite - dedans - depuis, nom),
        );
        self.ecris(
            &format!(
                "Ouverte depuis {}. Fermer la fenêtre termine la session.",
                duree(session.since)
            ),
            self.legende(),
            self.couleurs.texte_doux,
            Cadre::pose(
                ou.gauche + dedans,
                ou.haut + dedans + nom + ecart,
                large - dedans * 2.0,
                mot,
            ),
        );
        ou.bas - ou.haut
    }

    /// La grille des ordinateurs, et la tuile qui en ajoute un.
    fn grille(&mut self, x: f32, y: f32, large: f32) -> f32 {
        let ecart = self.px(design::PAS_3);
        let mini = self.px(tenue::CARTE);
        let colonnes = (((large + ecart) / (mini + ecart)).floor() as usize).max(1);
        let colonne = (large - ecart * (colonnes as f32 - 1.0)) / colonnes as f32;
        let dedans = self.px(design::PAS_4);
        let (nom, adresse) = (self.haute(self.sous_titre()), self.haute(self.legende()));
        let hauteur = dedans * 2.0
            + nom
            + self.px(design::PAS_2)
            + adresse
            + self.px(design::PAS_2)
            + self.px(tenue::APPEL);

        let combien = self.vu.voisins.len() + 1;
        for rang in 0..combien {
            let ou = Cadre::pose(
                x + (rang % colonnes) as f32 * (colonne + ecart),
                y + (rang / colonnes) as f32 * (hauteur + ecart),
                colonne,
                hauteur,
            );
            if rang < self.vu.voisins.len() {
                self.carte_d_ordinateur(ou, rang, dedans, nom, adresse);
            } else {
                self.tuile_d_ajout(ou);
            }
        }
        let lignes = combien.div_ceil(colonnes) as f32;
        lignes * hauteur + (lignes - 1.0) * ecart
    }

    /// Une carte d'ordinateur : cliquer n'importe où s'y connecte, et le
    /// bouton de son journal se pose dans un coin.
    fn carte_d_ordinateur(&mut self, ou: Cadre, rang: usize, dedans: f32, nom: f32, adresse: f32) {
        let voisin = &self.vu.voisins[rang];
        let occupe = self.vu.occupe(self.etat);
        let sienne = self
            .vu
            .sessions
            .iter()
            .any(|session| session.fingerprint == voisin.fingerprint);
        let quoi = Quoi::Voisin(rang);
        let dessus = !occupe && self.sous_la_main(&quoi);
        // Enfoncée sous le doigt : un pixel vers le bas, ce que la
        // feuille de style faisait et qui est tout ce qui dit qu'un clic
        // a été pris.
        let ou = if self.enfoncee(&quoi) {
            ou.decale(0.0, self.px(1.0))
        } else {
            ou
        };

        let rayon = self.px(design::RAYON_GRAND);
        self.ombre(ou, rayon, self.couleurs.ombre_1);
        self.remplis(
            ou,
            rayon,
            if dessus {
                self.couleurs.surface_2
            } else {
                self.couleurs.surface_1
            },
        );
        let bord = if sienne {
            self.couleurs.en_ligne.melee(self.couleurs.r#trait, 0.4)
        } else if dessus {
            self.couleurs.accent
        } else {
            self.couleurs.r#trait
        };
        self.trace(ou, rayon, bord);

        // Une carte occupée s'efface par ses mots, pour que le bouton de
        // son journal reste allumé : c'est justement pendant une session
        // qu'on veut lire ce que la machine d'en face a écrit.
        let voile = if occupe && !sienne { 0.5 } else { 1.0 };
        let pastille = self.px(tenue::PASTILLE);
        self.pastille(
            Cadre::pose(
                ou.gauche + dedans,
                ou.haut + dedans + (nom - pastille) / 2.0,
                pastille,
                pastille,
            ),
            if voisin.seen {
                self.couleurs.en_ligne.voile(voile)
            } else {
                self.couleurs.hors_ligne.voile(voile)
            },
            voisin.seen,
        );
        let depuis = ou.gauche + dedans + pastille + self.px(design::PAS_2);
        self.ecris(
            &voisin.name,
            self.sous_titre().coupee(),
            self.couleurs.texte.voile(voile),
            // La place du bouton du journal est réservée : sans elle, un
            // nom un peu long passerait dessous.
            Cadre::pose(
                depuis,
                ou.haut + dedans,
                (ou.droite - self.px(design::PAS_6) - depuis).max(0.0),
                nom,
            ),
        );
        // La pastille grise ne dit rien à elle seule : ce qui l'explique
        // est écrit à côté.
        let sous_le_nom = ou.haut + dedans + nom + self.px(design::PAS_2);
        self.ecris(
            &if voisin.seen {
                voisin.address.clone()
            } else {
                format!("{} · ajouté à la main", voisin.address)
            },
            self.legende().coupee(),
            self.couleurs.texte_doux.voile(voile),
            Cadre::pose(
                ou.gauche + dedans,
                sous_le_nom,
                ou.droite - dedans - (ou.gauche + dedans),
                adresse,
            ),
        );
        // Ce qui n'apparaît qu'au survol ne fait pas bouger la carte : sa
        // place est réservée d'avance.
        let appel = self.px(tenue::APPEL);
        if sienne || dessus {
            self.ecris(
                if sienne {
                    "Session en cours"
                } else {
                    "Se connecter"
                },
                self.legende(),
                if sienne {
                    self.couleurs.en_ligne
                } else {
                    self.couleurs.accent
                },
                Cadre::pose(
                    ou.gauche + dedans,
                    sous_le_nom + adresse + self.px(design::PAS_2),
                    ou.droite - dedans * 2.0,
                    appel,
                ),
            );
        }

        if !occupe {
            self.repond(quoi, ou);
        }
        // Toujours là et jamais au premier plan : il attend d'être
        // cherché, et il ne s'efface pas quand une session occupe la
        // fenêtre.
        let bouton = self.px(tenue::BOUTON);
        let coin = self.px(design::PAS_3);
        self.bouton_icone(
            Cadre::pose(ou.droite - coin - bouton, ou.haut + coin, bouton, bouton),
            &icones::JOURNAL,
            Quoi::JournalDe(rang),
            !dessus,
        );
    }

    /// La tuile qui ajoute un ordinateur : elle suit le rythme des autres
    /// sans se faire passer pour un ordinateur.
    fn tuile_d_ajout(&mut self, ou: Cadre) {
        let occupe = self.vu.occupe(self.etat);
        let dessus = !occupe && self.sous_la_main(&Quoi::Ajouter);
        if dessus {
            self.remplis(ou, self.px(design::RAYON_GRAND), self.couleurs.surface_2);
        }
        self.carte_en_attente(ou);
        let encre = if occupe {
            self.couleurs.texte_doux.voile(0.5)
        } else if dessus {
            self.couleurs.texte
        } else {
            self.couleurs.texte_doux
        };
        let signe = self.px(tenue::DESSIN);
        let mot = self.haute(self.legende());
        let ecart = self.px(design::PAS_1);
        let haut = (ou.haut + ou.bas) / 2.0 - (signe + ecart + mot) / 2.0;
        self.icone(
            &icones::PLUS,
            Cadre::pose(
                (ou.gauche + ou.droite) / 2.0 - signe / 2.0,
                haut,
                signe,
                signe,
            ),
            encre,
        );
        self.ecris(
            "Ajouter un ordinateur",
            self.legende().a(Cale::Centre),
            encre,
            Cadre::pose(ou.gauche, haut + signe + ecart, ou.droite - ou.gauche, mot),
        );
        if !occupe {
            self.repond(Quoi::Ajouter, ou);
        }
    }

    /// L'écran vide : ce qu'on voit sur une machine qui n'a encore trouvé
    /// personne.
    fn aucun_ordinateur(&mut self, x: f32, y: f32, large: f32, reste: f32) -> f32 {
        let dessin = self.px(tenue::VIDE.1);
        let titre = self.haute(self.sous_titre());
        let pour_le_mot = large.min(self.px(420.0)) - self.px(design::PAS_5) * 2.0;
        let mot = "Les ZyrDesk allumés sur ce réseau apparaissent ici tout seuls, sans rien à \
                   recopier. Si le réseau ne laisse pas passer les annonces, ajoutez l'autre \
                   ordinateur à la main, sur les deux machines.";
        let explication = self.hauteur(mot, self.legende(), pour_le_mot);
        let bouton = self.px(tenue::GRAND_BOUTON);
        let ecart = self.px(design::PAS_3);
        let dedans = self.px(design::PAS_6);
        let contenu =
            dessin + ecart + titre + ecart + explication + ecart + self.px(design::PAS_2) + bouton;
        let hauteur = (contenu + dedans * 2.0).max(reste - self.px(tenue::HAUT));

        let ou = Cadre::pose(x, y, large, hauteur);
        self.carte_en_attente(ou);
        let mut haut = (ou.haut + ou.bas) / 2.0 - contenu / 2.0;
        let milieu = (ou.gauche + ou.droite) / 2.0;
        self.icone(
            &icones::AUCUN_ORDINATEUR,
            Cadre::pose(
                milieu - self.px(tenue::VIDE.0) / 2.0,
                haut,
                self.px(tenue::VIDE.0),
                dessin,
            ),
            self.couleurs.texte_faible.voile(0.6),
        );
        haut += dessin + ecart;
        self.ecris(
            "Aucun ordinateur pour l'instant",
            self.sous_titre().a(Cale::Centre),
            self.couleurs.texte,
            Cadre::pose(ou.gauche, haut, large, titre),
        );
        haut += titre + ecart;
        self.ecris(
            mot,
            self.legende().a(Cale::Centre),
            self.couleurs.texte_doux,
            Cadre::pose(milieu - pour_le_mot / 2.0, haut, pour_le_mot, explication),
        );
        haut += explication + ecart + self.px(design::PAS_2);
        let large_du_bouton = self.large_du_bouton("Ajouter un ordinateur", true);
        self.bouton(
            Cadre::pose(
                milieu - large_du_bouton / 2.0,
                haut,
                large_du_bouton,
                bouton,
            ),
            "Ajouter un ordinateur",
            Sorte::Principal,
            Quoi::Ajouter,
            true,
        );
        hauteur
    }

    /// Ce que fait tourner cette fenêtre, et ce que fait tourner le
    /// service quand les deux ne datent pas du même jour.
    fn la_version(&self) -> (String, Couleur) {
        let mienne = &self.vu.version;
        if mienne.is_empty() {
            return (String::new(), self.couleurs.texte_faible);
        }
        let service = self
            .vu
            .machine
            .as_ref()
            .map_or("", |dit| dit.service_build.as_str());
        if service.is_empty() || mienne.contains(service) {
            return (mienne.clone(), self.couleurs.texte_faible);
        }
        (
            format!("{mienne}, mais le service tourne encore en {service}"),
            self.couleurs.attention,
        )
    }
}

/// Ce que l'état de cette machine se lit.
fn mot_de_l_etat(dit: &Standing) -> String {
    if dit.unreachable.is_some() {
        return "Service arrêté".to_string();
    }
    if !dit.wanted {
        return "Accès distant désactivé".to_string();
    }
    if dit.hosting {
        return "Prêt à être contrôlé".to_string();
    }
    match dit.holdup {
        "engineMissing" => "Moteur hôte absent".to_string(),
        "engineWontStand" => "Le moteur hôte ne démarre pas".to_string(),
        _ => "Démarrage en cours…".to_string(),
    }
}

/// Et la couleur de sa pastille. L'état ne se lit jamais à la couleur
/// seule : le texte à côté le dit.
fn couleur_de_l_etat(dit: &Standing, couleurs: Palette) -> Couleur {
    if dit.unreachable.is_some() || !dit.wanted {
        return couleurs.hors_ligne;
    }
    if dit.hosting {
        return couleurs.en_ligne;
    }
    if dit.holdup == "starting" {
        couleurs.attention
    } else {
        couleurs.erreur
    }
}

/// Depuis combien de temps une session est ouverte, en mots.
fn duree(secondes: u64) -> String {
    if secondes < MINUTE {
        return "moins d'une minute".to_string();
    }
    let minutes = (secondes % HEURE) / MINUTE;
    if secondes < HEURE {
        return format!("{minutes} minute{}", if minutes > 1 { "s" } else { "" });
    }
    let heures = secondes / HEURE;
    if minutes == 0 {
        format!("{heures} h")
    } else {
        format!("{heures} h {minutes:02}")
    }
}

/* ---- Les dialogues ------------------------------------------------------ */

impl Mise<'_> {
    /// Pose le dialogue ouvert : ce qu'il porte, mesuré à la largeur
    /// qu'il aura, puis dessiné dedans.
    ///
    /// La mesure et le dessin sont la même marche : un dialogue mesuré à
    /// une largeur et dessiné à une autre se répondrait juste jusqu'au
    /// premier mot qui se replie.
    fn dialogue(&mut self, large: f32, haute: f32) {
        let veut = self.px(match self.etat.ecran {
            Ecran::Ajout => tenue::DIALOGUE,
            Ecran::Journal => tenue::DIALOGUE_JOURNAL,
            Ecran::Reglages | Ecran::Accueil => tenue::DIALOGUE_REGLAGES,
        });
        let dedans = self.px(design::PAS_5);
        let marge = self.px(design::PAS_6);
        let largeur = veut.min(large - marge);

        let contenu = self.dedans(
            Cadre::pose(0.0, 0.0, largeur - dedans * 2.0, 0.0),
            true,
            haute,
        ) + dedans * 2.0;
        let hauteur = contenu.min(haute - marge);
        let ou = Cadre::pose(
            (large - largeur) / 2.0,
            (haute - hauteur) / 2.0,
            largeur,
            hauteur,
        );
        self.fond_du_dialogue(ou);
        // Serré à sa carte : ce qui a défilé au-dessus du haut du
        // dialogue, ou sous son bas, se dessinerait sinon par-dessus le
        // fond noirci.
        let toile = self.toile;
        toile.serre(ou, || {
            self.dedans(
                Cadre {
                    gauche: ou.gauche + dedans,
                    haut: ou.haut + dedans - self.etat.defile_dialogue,
                    droite: ou.droite - dedans,
                    bas: ou.bas,
                },
                false,
                haute,
            );
        });
        self.ascenseur(Ou::Dialogue, ou, contenu, self.px(design::RAYON_GRAND));
    }

    /// Ce que le dialogue ouvert porte, mesuré quand `muet` et dessiné
    /// sinon.
    fn dedans(&mut self, ou: Cadre, muet: bool, haute: f32) -> f32 {
        let avant = self.muet;
        self.muet = avant || muet;
        let pris = match self.etat.ecran {
            Ecran::Ajout => self.dans_l_ajout(ou),
            Ecran::Journal => self.dans_le_journal(ou, haute),
            Ecran::Reglages => self.dans_les_reglages(ou),
            // Il n'y a alors aucun dialogue, et rien ne l'appelle : dit
            // plutôt que rangé sous un autre écran, qu'il ne serait pas.
            Ecran::Accueil => 0.0,
        };
        self.muet = avant;
        pris
    }

    /// Pose le fond d'un dialogue.
    fn fond_du_dialogue(&mut self, ou: Cadre) {
        let rayon = self.px(design::RAYON_GRAND);
        self.ombre(ou, rayon, self.couleurs.ombre_2);
        self.remplis(ou, rayon, self.couleurs.surface_1);
        self.trace(ou, rayon, self.couleurs.trait_fort);
    }

    /// L'en-tête d'un dialogue : ce dont il s'agit, et la croix qui le
    /// ferme.
    fn entete_du_dialogue(&mut self, ou: Cadre, titre: &str, mot: &str) -> f32 {
        let bouton = self.px(tenue::BOUTON);
        let pour_le_mot = ou.droite - ou.gauche - bouton - self.px(design::PAS_4);
        let haut_du_titre = self.haute(self.sous_titre());
        let explication = self.hauteur(mot, self.legende(), pour_le_mot);
        let ecart = self.px(design::PAS_1);

        self.ecris(
            titre,
            self.sous_titre(),
            self.couleurs.texte,
            Cadre::pose(ou.gauche, ou.haut, pour_le_mot, haut_du_titre),
        );
        self.bloc(
            ou.gauche,
            ou.haut + haut_du_titre + ecart,
            pour_le_mot,
            mot,
            self.legende(),
            self.couleurs.texte_doux,
        );
        self.bouton_icone(
            Cadre::pose(ou.droite - bouton, ou.haut, bouton, bouton),
            &icones::CROIX,
            Quoi::Fermer,
            false,
        );
        (haut_du_titre + ecart + explication).max(bouton)
    }

    /// Une rangée d'actions, rangées à droite, et rendue de sa hauteur.
    ///
    /// Ce qui détruit se pose à gauche, écarté du reste : il ne doit pas
    /// se trouver sous le doigt qui vise à côté.
    fn actions(&mut self, ou: Cadre, haut: f32, actions: &[(String, Sorte, Quoi, bool)]) -> f32 {
        let hauteur = self.px(tenue::BOUTON);
        let ecart = self.px(design::PAS_3);
        let mut droite = ou.droite;
        for (mot, sorte, quoi, vivant) in actions.iter().rev() {
            let large = self.large_du_bouton(mot, false);
            self.bouton(
                Cadre::pose(droite - large, haut, large, hauteur),
                mot,
                *sorte,
                quoi.clone(),
                *vivant,
            );
            droite -= large + ecart;
        }
        hauteur
    }

    /// Ajouter un ordinateur, et retirer ceux qui ont été ajoutés.
    fn dans_l_ajout(&mut self, ou: Cadre) -> f32 {
        let large = ou.droite - ou.gauche;
        let mut y = ou.haut;
        let ecart = self.px(design::PAS_4);

        let titre = self.haute(self.sous_titre());
        self.ecris(
            "Ajouter un ordinateur",
            self.sous_titre(),
            self.couleurs.texte,
            Cadre::pose(ou.gauche, y, large, titre),
        );
        y += titre + self.px(design::PAS_2);
        y += self.bloc(
            ou.gauche,
            y,
            large,
            "À n'utiliser que si l'ordinateur n'apparaît pas tout seul. Les deux informations se \
             lisent dans sa fenêtre ZyrDesk. À faire sur les deux machines : chacune doit \
             connaître l'autre.",
            self.legende(),
            self.couleurs.texte_doux,
        );
        y += ecart;

        for champ in Champ::ALL {
            y += self.champ(ou.gauche, y, large, champ);
            y += ecart;
        }

        let vers_lui = !texte_du_champ(Champ::Adresse).trim().is_empty();
        let peut = texte_du_champ(Champ::Empreinte).trim().len() == TAILLE_EMPREINTE
            && !(vers_lui && self.vu.occupe(self.etat));
        y += self.px(design::PAS_1);
        y += self.actions(
            ou,
            y,
            &[
                ("Annuler".to_string(), Sorte::Discret, Quoi::Fermer, true),
                (
                    if vers_lui {
                        "Se connecter"
                    } else {
                        "Autoriser"
                    }
                    .to_string(),
                    Sorte::Principal,
                    Quoi::Connecter,
                    peut,
                ),
            ],
        );

        // Ce qui a été ajouté à la main se retire là où il a été ajouté :
        // une carte d'accueil est un bouton entier, et un second bouton
        // posé dessus lui prendrait son clic.
        let ecrits = self.ecrits();
        if !ecrits.is_empty() {
            y += self.px(design::PAS_5);
            let dedans = self.px(design::PAS_5);
            self.separateur(ou.gauche - dedans, y, large + dedans * 2.0);
            y += self.px(design::PAS_4);
            let etiquette = self.haute(self.legende().en_gras());
            self.section(
                Cadre::pose(ou.gauche, y, large, etiquette),
                "Ordinateurs ajoutés à la main",
            );
            y += etiquette + self.px(design::PAS_3);
            let bouton = self.px(tenue::BOUTON);
            let oublier = self.large_du_bouton("Oublier", false);
            for rang in ecrits {
                let voisin = &self.vu.voisins[rang];
                let mot = format!("{} · {}", voisin.name, voisin.address);
                self.ecris(
                    &mot,
                    self.legende().coupee(),
                    self.couleurs.texte_doux,
                    Cadre::pose(
                        ou.gauche,
                        y,
                        (large - oublier - self.px(design::PAS_3)).max(0.0),
                        bouton,
                    ),
                );
                self.bouton(
                    Cadre::pose(ou.gauche + large - oublier, y, oublier, bouton),
                    "Oublier",
                    Sorte::Discret,
                    Quoi::Oublier(rang),
                    true,
                );
                y += bouton + self.px(design::PAS_2);
            }
            y -= self.px(design::PAS_2);
        }
        y - ou.haut
    }

    /// Les ordinateurs écrits à la main, par leur rang.
    fn ecrits(&self) -> Vec<usize> {
        self.vu
            .voisins
            .iter()
            .enumerate()
            .filter(|(_, voisin)| voisin.written)
            .map(|(rang, _)| rang)
            .collect()
    }

    /// Un champ de saisie : son étiquette, la place du vrai champ que
    /// Windows porte, et ce qu'il a à redire.
    fn champ(&mut self, x: f32, y: f32, large: f32, champ: Champ) -> f32 {
        let etiquette = self.haute(self.legende());
        let ecart = self.px(design::PAS_2);
        let hauteur = self.px(tenue::CHAMP);
        self.ecris(
            champ.mot(),
            self.legende(),
            self.couleurs.texte_doux,
            Cadre::pose(x, y, large, etiquette),
        );
        let place = Cadre::pose(x, y + etiquette + ecart, large, hauteur);
        let rayon = self.px(design::RAYON_PETIT);
        self.remplis(place, rayon, self.couleurs.surface_2);
        self.trace(place, rayon, self.couleurs.r#trait);
        if !self.muet {
            pose_le_champ(champ, place);
        }

        let mot = champ.dit();
        let sous = self.hauteur(&mot, self.legende(), large);
        if !mot.is_empty() {
            self.bloc(
                x,
                place.bas + self.px(design::PAS_1),
                large,
                &mot,
                self.legende(),
                if champ == Champ::Empreinte {
                    self.couleurs.attention
                } else {
                    self.couleurs.texte_doux
                },
            );
        }
        etiquette
            + ecart
            + hauteur
            + if sous > 0.0 {
                self.px(design::PAS_1) + sous
            } else {
                0.0
            }
    }

    /// Le journal, celui de cet ordinateur ou celui d'en face.
    fn dans_le_journal(&mut self, ou: Cadre, fenetre: f32) -> f32 {
        let large = ou.droite - ou.gauche;
        let distant = self.etat.journal_de.clone();
        let (titre, mot) = match &distant {
            Some(voisin) => (
                format!("Journal de {}", voisin.name),
                "Ce que l'ordinateur distant a écrit chez lui, lu d'ici, à copier tel quel en cas \
                 de problème."
                    .to_string(),
            ),
            None => (
                "Journal".to_string(),
                "Tout ce que le produit a écrit, à copier tel quel en cas de problème.".to_string(),
            ),
        };
        let mut y = ou.haut;
        y += self.entete_du_dialogue(ou, &titre, &mot);
        y += self.px(design::PAS_4);

        // Le journal se lit sur des lignes entières : il prend la place
        // qu'il peut, sans jamais pousser son dialogue hors de la
        // fenêtre.
        let lignes = (fenetre * tenue::JOURNAL.0).min(self.px(tenue::JOURNAL.1));
        self.les_lignes(Cadre::pose(ou.gauche, y, large, lignes));
        y += lignes + self.px(design::PAS_4);

        let vidage = self
            .etat
            .vidage
            .is_some_and(|depuis| depuis.elapsed() < TEMPS_CONFIRMATION);
        let copie = self.etat.copie.as_ref().map(|(quoi, _)| quoi) == Some(&Quoi::CopierJournal);
        let mut rangee: Vec<(String, Sorte, Quoi, bool)> = Vec::new();
        // Ouvrir le dossier n'a de sens que chez soi : celui d'en face est
        // sur l'autre machine. Vider, si : on vide les deux journaux, on
        // refait ce qui ne marche pas, et on lit les deux.
        if distant.is_none() {
            rangee.push((
                "Ouvrir le dossier".to_string(),
                Sorte::Discret,
                Quoi::OuvrirLesJournaux,
                true,
            ));
        }
        rangee.push((
            "Actualiser".to_string(),
            Sorte::Discret,
            Quoi::Actualiser,
            true,
        ));
        rangee.push((
            if copie { "Copié" } else { "Copier tout" }.to_string(),
            Sorte::Principal,
            Quoi::CopierJournal,
            true,
        ));
        let hauteur = self.actions(ou, y, &rangee);
        // Vider est à l'opposé de Copier : les deux se cliquent dans la
        // même minute, et se tromper coûte tout ce qu'on allait copier.
        let vider = self.large_du_bouton(if vidage { "Confirmer" } else { "Vider" }, false);
        self.bouton(
            Cadre::pose(ou.gauche, y, vider, hauteur),
            if vidage { "Confirmer" } else { "Vider" },
            if vidage {
                Sorte::Attention
            } else {
                Sorte::Discret
            },
            Quoi::Vider,
            true,
        );
        y + hauteur - ou.haut
    }

    /// Le texte du journal, qui défile chez lui : le plus récent est en
    /// bas, et une ligne de journal ne se replie pas.
    fn les_lignes(&mut self, ou: Cadre) {
        let rayon = self.px(design::RAYON_PETIT);
        self.remplis(ou, rayon, self.couleurs.surface_2);
        self.trace(ou, rayon, self.couleurs.r#trait);

        let dedans = self.px(design::PAS_4);
        let plume = self.legende().a_chasse_fixe().qui_depasse();
        let haute = self.haute(plume);
        let dedans_la_boite = ou.elargi(-dedans);
        let lignes = &self.etat.lignes;
        let contenu = haute * lignes.len() as f32;
        let (travers, demande) = self.etat.defile_lignes;
        let defile = demande.min((contenu - (dedans_la_boite.bas - dedans_la_boite.haut)).max(0.0));

        let toile = self.toile;
        let muet = self.muet;
        let couleurs = self.couleurs;
        toile.serre(dedans_la_boite, || {
            if muet {
                return;
            }
            let premiere = (defile / haute).floor().max(0.0) as usize;
            let combien =
                ((dedans_la_boite.bas - dedans_la_boite.haut) / haute).ceil() as usize + 1;
            for (rang, ligne) in lignes.iter().enumerate().skip(premiere).take(combien) {
                toile.ecris(
                    ligne,
                    plume,
                    couleurs.texte_doux,
                    Cadre::pose(
                        dedans_la_boite.gauche - travers,
                        dedans_la_boite.haut + rang as f32 * haute - defile,
                        AU_LOIN,
                        haute,
                    ),
                );
            }
        });
        self.ascenseur(Ou::Lignes, dedans_la_boite, contenu, 0.0);
    }

    /// Les réglages, ligne par ligne.
    fn dans_les_reglages(&mut self, ou: Cadre) -> f32 {
        let large = ou.droite - ou.gauche;
        let mut y = ou.haut;
        y += self.entete_du_dialogue(
            ou,
            "Réglages",
            "Ils valent pour les prochaines sessions, pas pour celle en cours.",
        );
        y += self.px(design::PAS_4);

        let mut cache = false;
        for element in REGLAGES {
            match element {
                Element::Section(titre, mot) => {
                    y += self.px(design::PAS_4);
                    let etiquette = self.haute(self.legende().en_gras());
                    self.section(Cadre::pose(ou.gauche, y, large, etiquette), titre);
                    y += etiquette + self.px(design::PAS_1);
                    y += self.bloc(
                        ou.gauche,
                        y,
                        large,
                        mot,
                        self.legende(),
                        self.couleurs.texte_doux,
                    );
                    y += self.px(design::PAS_2);
                }
                Element::Repli => {
                    self.separateur(ou.gauche, y, large);
                    let hauteur = self.px(tenue::BOUTON);
                    let etiquette = self.haute(self.legende().en_gras());
                    self.section(
                        Cadre::pose(ou.gauche, y + (hauteur - etiquette) / 2.0, large, etiquette),
                        "Avancé",
                    );
                    let signe = self.px(tenue::DESSIN);
                    self.icone(
                        if self.etat.avance {
                            &icones::CHEVRON_BAS
                        } else {
                            &icones::CHEVRON
                        },
                        Cadre::pose(
                            ou.gauche + large - signe,
                            y + (hauteur - signe) / 2.0,
                            signe,
                            signe,
                        ),
                        self.couleurs.texte_faible,
                    );
                    self.repond(Quoi::Avance, Cadre::pose(ou.gauche, y, large, hauteur));
                    y += hauteur;
                    cache = !self.etat.avance;
                }
                Element::Reglage(reglage) => {
                    if cache {
                        continue;
                    }
                    self.separateur(ou.gauche, y, large);
                    y += self.ligne_de_reglage(ou.gauche, y, large, reglage);
                }
            }
        }

        if let Some(souci) = self.etat.souci.clone() {
            y += self.px(design::PAS_4);
            y += self.bandeau(ou.gauche, y, large, &souci, true, None);
        }
        y - ou.haut
    }

    /// Une ligne de réglage : ce dont il s'agit à gauche, de quoi en
    /// décider à droite.
    fn ligne_de_reglage(&mut self, x: f32, y: f32, large: f32, reglage: &Reglage) -> f32 {
        let dedans = self.px(design::PAS_3);
        let commande = self.large_de_la_commande(&reglage.commande);
        let pour_le_mot = (large
            - commande
            - if commande > 0.0 {
                self.px(design::PAS_4)
            } else {
                0.0
            })
        .max(self.px(80.0));
        let mot = self.haute(self.corps());
        let legende = self.legende_du_reglage(reglage);
        let sous = self.hauteur(&legende, self.legende(), pour_le_mot);
        let ecart = if sous > 0.0 {
            self.px(design::PAS_1)
        } else {
            0.0
        };
        let hauteur =
            (mot + ecart + sous).max(self.hauteur_de_la_commande(&reglage.commande)) + dedans * 2.0;
        let milieu = y + hauteur / 2.0;
        let haut = milieu - (mot + ecart + sous) / 2.0;

        self.ecris(
            reglage.mot,
            self.corps(),
            self.couleurs.texte,
            Cadre::pose(x, haut, pour_le_mot, mot),
        );
        let fixe = matches!(
            reglage.commande,
            Commande::Ouvre(_, Quoi::OuvrirLesJournaux)
        );
        self.bloc(
            x,
            haut + mot + ecart,
            pour_le_mot,
            &legende,
            if fixe {
                self.legende().a_chasse_fixe()
            } else {
                self.legende()
            },
            if fixe {
                self.couleurs.texte_faible
            } else {
                self.couleurs.texte_doux
            },
        );

        let droite = x + large;
        match &reglage.commande {
            Commande::Dit => {}
            Commande::Interrupteur(bouton) => {
                self.interrupteur(droite - self.px(tenue::INTERRUPTEUR.0), milieu, *bouton);
            }
            Commande::Segments(quoi) => {
                self.segments(droite, milieu, *quoi);
            }
            Commande::Touche(doing) => {
                let hauteur = self.px(tenue::BOUTON);
                self.touche(
                    Cadre::pose(droite - commande, milieu - hauteur / 2.0, commande, hauteur),
                    *doing,
                );
            }
            Commande::Ouvre(mot, quoi) => {
                let hauteur = self.px(tenue::BOUTON);
                self.bouton(
                    Cadre::pose(droite - commande, milieu - hauteur / 2.0, commande, hauteur),
                    mot,
                    Sorte::Discret,
                    quoi.clone(),
                    true,
                );
            }
        }
        hauteur
    }

    /// Ce que la commande d'une ligne prend de large.
    fn large_de_la_commande(&self, commande: &Commande) -> f32 {
        match commande {
            Commande::Dit => 0.0,
            Commande::Interrupteur(_) => self.px(tenue::INTERRUPTEUR.0),
            Commande::Segments(quoi) => self.large_des_segments(*quoi),
            Commande::Touche(doing) => {
                self.toile
                    .largeur(
                        &self.mot_de_la_touche(*doing),
                        self.legende().a_chasse_fixe(),
                    )
                    .max(self.px(tenue::TOUCHE) - self.px(design::PAS_3) * 2.0)
                    + self.px(design::PAS_3) * 2.0
            }
            Commande::Ouvre(mot, _) => self.large_du_bouton(mot, false),
        }
    }

    fn hauteur_de_la_commande(&self, commande: &Commande) -> f32 {
        match commande {
            Commande::Dit => 0.0,
            Commande::Interrupteur(_) => self.px(tenue::INTERRUPTEUR.1),
            Commande::Segments(_) => self.px(tenue::SEGMENT) + self.px(tenue::AUTOUR) * 2.0,
            Commande::Touche(_) | Commande::Ouvre(_, _) => self.px(tenue::BOUTON),
        }
    }

    /// Ce qu'une ligne de réglage a à dire sous son mot.
    fn legende_du_reglage(&self, reglage: &Reglage) -> String {
        match &reglage.commande {
            // Ce qu'une session demanderait maintenant, dit par le produit
            // et non recalculé ici.
            Commande::Dit => self.vu.reglages.as_ref().map_or_else(String::new, |dit| {
                format!(
                    "{} x {}, {} images par seconde, {} Mb/s",
                    dit.width,
                    dit.height,
                    dit.fps,
                    (dit.bitrate_kbps as f32 / 1000.0).round() as u32
                )
            }),
            Commande::Ouvre(_, Quoi::OuvrirLesJournaux) => self.vu.dossier.clone(),
            _ => reglage.legende.to_string(),
        }
    }

    /// La combinaison d'un raccourci, telle qu'on la lit.
    fn mot_de_la_touche(&self, doing: Doing) -> String {
        if self.etat.ecoute == Some(doing) {
            return "Tapez la combinaison…".to_string();
        }
        self.vu
            .raccourcis
            .iter()
            .find(|(autre, _)| *autre == doing)
            .and_then(|(_, dit)| dit.clone())
            .unwrap_or_else(|| "Aucune".to_string())
    }

    /// Une combinaison se lit comme des touches et non comme une phrase :
    /// le caractère fixe met le même espace sous chaque signe, et le
    /// cadre dit qu'on peut cliquer dessus pour la changer.
    fn touche(&mut self, ou: Cadre, doing: Doing) {
        let ecoute = self.etat.ecoute == Some(doing);
        let quoi = Quoi::Raccourci(doing);
        let dessus = self.sous_la_main(&quoi);
        let rayon = self.px(design::RAYON_PETIT);
        self.remplis(ou, rayon, self.couleurs.surface_2);
        self.trace(
            ou,
            rayon,
            if ecoute || dessus {
                self.couleurs.accent
            } else {
                self.couleurs.trait_fort
            },
        );
        let mot = self.mot_de_la_touche(doing);
        let vide = mot == "Aucune";
        self.ecris(
            &mot,
            self.legende().a_chasse_fixe().a(Cale::Centre),
            if ecoute {
                self.couleurs.accent_vif
            } else if vide {
                self.couleurs.texte_faible
            } else {
                self.couleurs.texte
            },
            ou,
        );
        self.repond(quoi, ou);
    }

    /// Ce qui est à l'écran pendant qu'une session s'ouvre.
    ///
    /// Il prend la fenêtre entière parce que c'est la seule chose qui se
    /// passe, et parce que c'est la dernière chose qu'on lit avant que le
    /// moteur pose sa propre image par-dessus : entre les deux il ne doit
    /// jamais y avoir de trou où l'on se demande si ça marche.
    fn ouverture(&mut self, large: f32, haute: f32) {
        let Some(ouverture) = self.etat.ouverture.as_ref() else {
            return;
        };
        let tout = Cadre::pose(0.0, 0.0, large, haute);
        self.remplis(tout, 0.0, self.couleurs.fond);

        let marque = self.px(tenue::GRANDE_MARQUE);
        let titre = self.haute(self.plume(design::TITRE).en_gras());
        let pour_les_mots = (large - self.px(design::PAS_6) * 2.0).min(self.px(420.0));
        let vers = self.hauteur(&ouverture.vers, self.corps(), pour_les_mots);
        let (fil_large, fil_haut) = (
            self.px(tenue::FIL.0).min(large * 0.6),
            self.px(tenue::FIL.1),
        );
        let detail = self
            .hauteur(&ouverture.detail, self.legende(), pour_les_mots)
            .max(self.haute(self.legende()));
        let code = ouverture
            .code
            .as_ref()
            .map(|code| (code.clone(), self.haute(self.plume(tenue::CODE).en_gras())));
        let ecart = self.px(design::PAS_4);
        let contenu = marque
            + ecart
            + titre
            + ecart
            + vers
            + ecart
            + fil_haut
            + ecart
            + detail
            + code.as_ref().map_or(0.0, |(_, haute)| ecart + haute);

        let milieu = large / 2.0;
        let mut y = (haute - contenu) / 2.0;
        self.marque(Cadre::pose(milieu - marque / 2.0, y, marque, marque));
        y += marque + ecart;
        self.ecris(
            "Établissement de la connexion",
            self.plume(design::TITRE).en_gras().a(Cale::Centre),
            self.couleurs.texte,
            Cadre::pose(0.0, y, large, titre),
        );
        y += titre + ecart;
        self.ecris(
            &ouverture.vers,
            self.corps().a(Cale::Centre),
            self.couleurs.texte_doux,
            Cadre::pose(milieu - pour_les_mots / 2.0, y, pour_les_mots, vers),
        );
        y += vers + ecart;

        // Une barre qui va et vient. Elle ne mesure rien : ce qu'on
        // attend ici ne se découpe pas en pourcentages, et une barre qui
        // prétendrait le contraire mentirait.
        let piste = Cadre::pose(milieu - fil_large / 2.0, y, fil_large, fil_haut);
        self.remplis(piste, fil_haut / 2.0, self.couleurs.surface_3);
        let part = ouverture.depuis.elapsed().as_secs_f32() / VA_ET_VIENT.as_secs_f32();
        let ou_est = part.fract() * (1.0 + tenue::MORCEAU * 2.0) - tenue::MORCEAU;
        let morceau = fil_large * tenue::MORCEAU;
        let toile = self.toile;
        let accent = self.couleurs.accent;
        let muet = self.muet;
        toile.serre(piste, || {
            if !muet {
                toile.remplis(
                    Cadre::pose(piste.gauche + ou_est * fil_large, y, morceau, fil_haut),
                    fil_haut / 2.0,
                    accent,
                );
            }
        });
        y += fil_haut + ecart;

        self.ecris(
            &ouverture.detail,
            self.legende().a(Cale::Centre),
            self.couleurs.texte_faible,
            Cadre::pose(milieu - pour_les_mots / 2.0, y, pour_les_mots, detail),
        );
        if let Some((code, hauteur)) = code {
            y += detail + ecart;
            self.ecris(
                &code,
                self.plume(tenue::CODE)
                    .en_gras()
                    .a_chasse_fixe()
                    .a(Cale::Centre)
                    .ecartee(0.22),
                self.couleurs.accent_vif,
                Cadre::pose(0.0, y, large, hauteur),
            );
        }
    }
}

/// Assez loin pour qu'une ligne de journal ne soit jamais coupée par son
/// propre cadre : c'est la boîte qui la retient, et elle défile.
const AU_LOIN: f32 = 20_000.0;

/// Plus bas que n'importe quel journal, ce que le dessin ramène ensuite
/// au bas réel : demander « tout en bas » avant d'avoir mesuré est la
/// seule façon d'y être dès la première image.
const TOUT_EN_BAS: f32 = 1.0e9;

/* ---- Ce qui pose, et ce qui se tait ------------------------------------ */

/// Les mêmes gestes que la toile, mais qui ne font rien quand la marche
/// ne fait que mesurer.
///
/// Mesurer et dessiner sont la même marche : ce qu'un dialogue prend de
/// haut est ce que ses lignes prennent, et l'écrire une seconde fois à
/// côté serait une arithmétique qui se répond juste jusqu'au premier mot
/// rallongé.
impl Mise<'_> {
    fn ecris(&self, mot: &str, plume: Plume, encre: Couleur, ou: Cadre) {
        if !self.muet {
            self.toile.ecris(mot, plume, encre, ou);
        }
    }

    fn remplis(&self, ou: Cadre, rayon: f32, encre: Couleur) {
        if !self.muet {
            self.toile.remplis(ou, rayon, encre);
        }
    }

    /// Une bordure, qui tient entièrement dans son cadre.
    fn trace(&self, ou: Cadre, rayon: f32, encre: Couleur) {
        if !self.muet {
            self.toile
                .trace_dedans(ou, rayon, self.px(tenue::TRAIT), encre);
        }
    }

    /// Un trait qui attend d'être rempli.
    fn pointille(&self, ou: Cadre, rayon: f32, encre: Couleur) {
        if !self.muet {
            self.toile.trace_pointille(
                ou.elargi(-self.px(tenue::TRAIT) / 2.0),
                rayon,
                self.px(tenue::TRAIT),
                encre,
            );
        }
    }

    fn ombre(&self, ou: Cadre, rayon: f32, ombre: design::Ombre) {
        if !self.muet {
            self.toile.ombre(ou, rayon, ombre, self.echelle);
        }
    }

    fn icone(&self, icone: &Icone, ou: Cadre, encre: Couleur) {
        if !self.muet {
            self.toile.icone(icone, ou, encre);
        }
    }

    fn marque(&self, ou: Cadre) {
        if !self.muet {
            crate::logo::marque(self.toile, ou, 1.0);
        }
    }
}

/* ---- La souris ---------------------------------------------------------- */

/// Ce qui est sous ce point, le dernier posé gagnant : ce qui a été
/// dessiné en dernier est ce qui est dessus.
fn sous(x: f32, y: f32) -> Option<Quoi> {
    CLIQUABLES
        .lock()
        .expect("accueil")
        .iter()
        .rev()
        .find(|(_, ou)| x >= ou.gauche && x < ou.droite && y >= ou.haut && y < ou.bas)
        .map(|(quoi, _)| quoi.clone())
}

fn bouge(window: windows_sys::Win32::Foundation::HWND, (x, y): (f32, f32)) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
    };

    // Demandé à chaque passage : sans lui rien ne dit jamais qu'une main
    // est partie, et la dernière ligne survolée le resterait.
    let mut garde = TRACKMOUSEEVENT {
        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
        dwFlags: TME_LEAVE,
        hwndTrack: window,
        dwHoverTime: 0,
    };
    // SAFETY: une fenêtre à nous, et la structure qu'elle demande.
    unsafe { TrackMouseEvent(&mut garde) };

    let mut etat = ETAT.lock().expect("accueil");
    if let Some((ou, depuis)) = etat.tenu {
        let (contenu, voit, course) = etat.mesure(ou);
        if course > 0.0 {
            let de = (y - depuis) * (contenu - voit) / course;
            etat.defile_de(ou, de);
        }
        etat.tenu = Some((ou, y));
        drop(etat);
        invalide(window);
        return;
    }
    let dessous = sous(x, y);
    if etat.survol != dessous {
        etat.survol = dessous;
        drop(etat);
        invalide(window);
    }
}

fn quitte(window: windows_sys::Win32::Foundation::HWND) {
    let mut etat = ETAT.lock().expect("accueil");
    if etat.survol.is_none() {
        return;
    }
    etat.survol = None;
    drop(etat);
    invalide(window);
}

fn appuie(window: windows_sys::Win32::Foundation::HWND, (x, y): (f32, f32)) {
    let mut etat = ETAT.lock().expect("accueil");
    match sous(x, y) {
        Some(Quoi::Ascenseur(ou)) => etat.tenu = Some((ou, y)),
        dessous => etat.pressee = dessous,
    }
    drop(etat);
    invalide(window);
}

fn relache(window: windows_sys::Win32::Foundation::HWND, (x, y): (f32, f32)) {
    let mut etat = ETAT.lock().expect("accueil");
    etat.tenu = None;
    let pressee = etat.pressee.take();
    drop(etat);
    invalide(window);

    let Some(quoi) = pressee else {
        return;
    };
    if sous(x, y).as_ref() != Some(&quoi) {
        return;
    }
    if let Some(app) = programme() {
        fait(&app, quoi);
    }
}

fn roule(window: windows_sys::Win32::Foundation::HWND, crans: f32, travers: bool) {
    let mut etat = ETAT.lock().expect("accueil");
    let ou = match etat.ecran {
        Ecran::Accueil => Ou::Page,
        // Le texte du journal défile chez lui : c'est la seule chose qui
        // se lit dans ce dialogue, et le dialogue lui-même tient dans la
        // fenêtre.
        Ecran::Journal => Ou::Lignes,
        _ => Ou::Dialogue,
    };
    let de = -crans * echelle() * tenue::CRAN;
    if travers && ou == Ou::Lignes {
        // En travers : une ligne de journal ne se replie pas, et la lire
        // en entier demande de s'y déplacer.
        etat.defile_lignes.0 = (etat.defile_lignes.0 + de).max(0.0);
    } else {
        etat.defile_de(ou, de);
    }
    drop(etat);
    invalide(window);
}

/// Redemande une image depuis le fil qui dessine, où l'on est déjà.
fn invalide(window: windows_sys::Win32::Foundation::HWND) {
    use windows_sys::Win32::Graphics::Gdi::InvalidateRect;

    // SAFETY: une fenêtre à nous, sur le fil qui la possède.
    unsafe { InvalidateRect(window, std::ptr::null(), 0) };
}

/* ---- Le clavier --------------------------------------------------------- */

/// Ce que la toile fait d'une touche, et si elle l'a prise.
fn touche(
    window: windows_sys::Win32::Foundation::HWND,
    vk: u32,
    with: windows_sys::Win32::Foundation::LPARAM,
) -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        VK_BACK, VK_DELETE, VK_ESCAPE, VK_RETURN,
    };

    let ecoute = ETAT.lock().expect("accueil").ecoute;
    if let Some(doing) = ecoute {
        return la_combinaison(window, doing, vk, with);
    }

    let Some(app) = programme() else {
        return false;
    };
    let ecran = ETAT.lock().expect("accueil").ecran;
    match vk as u16 {
        VK_ESCAPE if ecran != Ecran::Accueil => {
            fait(&app, Quoi::Fermer);
            true
        }
        VK_RETURN if ecran == Ecran::Ajout => {
            fait(&app, Quoi::Connecter);
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
fn la_combinaison(
    window: windows_sys::Win32::Foundation::HWND,
    doing: Doing,
    vk: u32,
    with: windows_sys::Win32::Foundation::LPARAM,
) -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyState, VK_BACK, VK_CONTROL, VK_DELETE, VK_ESCAPE, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
    };

    let Some(app) = programme() else {
        return false;
    };
    match vk as u16 {
        VK_ESCAPE => {
            ETAT.lock().expect("accueil").ecoute = None;
            invalide(window);
            return true;
        }
        VK_BACK | VK_DELETE => {
            pose_la_combinaison(&app, doing, None);
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
    let tenue = unsafe {
        Held {
            ctrl: GetKeyState(i32::from(VK_CONTROL)) < 0,
            alt: GetKeyState(i32::from(VK_MENU)) < 0,
            shift: GetKeyState(i32::from(VK_SHIFT)) < 0,
            win: GetKeyState(i32::from(VK_LWIN)) < 0 || GetKeyState(i32::from(VK_RWIN)) < 0,
        }
    };
    pose_la_combinaison(
        &app,
        doing,
        Some(Combination {
            held: tenue,
            key: place.to_string(),
        }),
    );
    true
}

/// Écrit une combinaison, ou la retire, et relit les trois.
fn pose_la_combinaison(app: &App, doing: Doing, combination: Option<Combination>) {
    let mut etat = ETAT.lock().expect("accueil");
    etat.ecoute = None;
    etat.souci = None;
    if let Err(refus) = crate::shortcuts::bind(doing, combination) {
        etat.souci = Some(refus);
    }
    drop(etat);
    if let Some(vu) = VU.lock().expect("accueil").as_mut() {
        vu.raccourcis = crate::shortcuts::engraved();
    }
    redraw(app);
}

/* ---- Les trois champs de saisie ----------------------------------------- */

/// Un champ du dialogue d'ajout, dans l'ordre où on les remplit.
#[derive(Clone, Copy, PartialEq)]
enum Champ {
    Empreinte,
    Adresse,
    Nom,
}

impl Champ {
    const ALL: [Champ; 3] = [Champ::Empreinte, Champ::Adresse, Champ::Nom];

    fn rang(self) -> usize {
        match self {
            Champ::Empreinte => 0,
            Champ::Adresse => 1,
            Champ::Nom => 2,
        }
    }

    fn mot(self) -> &'static str {
        match self {
            Champ::Empreinte => "Empreinte",
            Champ::Adresse => "Adresse",
            Champ::Nom => "Nom (facultatif)",
        }
    }

    /// Le mot en filigrane, qui dit à quoi ressemble ce qu'on attend.
    fn exemple(self) -> &'static str {
        match self {
            Champ::Empreinte => "0829cc7ecb9e9ba5…",
            Champ::Adresse => "192.168.1.20",
            Champ::Nom => "PC du bureau",
        }
    }

    /// Ce que le champ a à redire, ou à expliquer, sous lui.
    fn dit(self) -> String {
        match self {
            Champ::Empreinte => {
                let combien = texte_du_champ(self).trim().chars().count();
                if combien == 0 || combien == TAILLE_EMPREINTE {
                    String::new()
                } else {
                    format!("{combien} caractères sur {TAILLE_EMPREINTE}")
                }
            }
            Champ::Adresse => "Seulement si vous voulez contrôler cet ordinateur depuis ici. Il \
                               restera alors sur l'accueil, et il n'y aura plus rien à ressaisir."
                .to_string(),
            Champ::Nom => String::new(),
        }
    }
}

/// Les trois champs, et la place que la dernière image leur a donnée.
static CHAMPS: Mutex<[isize; 3]> = Mutex::new([0; 3]);
static PLACES: Mutex<[Option<Cadre>; 3]> = Mutex::new([None; 3]);
/// La police des champs, faite une fois pour la taille de l'écran.
static POLICE: Mutex<isize> = Mutex::new(0);

/// Refait la police des champs à l'échelle de l'écran, et la leur pose.
///
/// Un champ est une fenêtre du système : il porte sa propre police, qui
/// ne suit pas ce que nous dessinons.
fn habille_les_champs() {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::Graphics::Gdi::{
        CLEARTYPE_QUALITY, CreateFontW, DEFAULT_CHARSET, DEFAULT_PITCH, DeleteObject, FF_DONTCARE,
        FW_NORMAL, OUT_DEFAULT_PRECIS,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_SETFONT};

    let famille = wide(FAMILLE_DES_CHAMPS);
    // SAFETY: le nom survit à l'appel, et la police qui revient est à
    // nous jusqu'à ce qu'on la rende.
    let police = unsafe {
        CreateFontW(
            -((design::CORPS * echelle()).round() as i32),
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
            famille.as_ptr(),
        )
    };
    if police.is_null() {
        return;
    }
    let mut avant = POLICE.lock().expect("accueil");
    for edit in CHAMPS.lock().expect("accueil").iter() {
        if *edit != 0 {
            // SAFETY: une fenêtre faite par nous, à qui l'on donne une
            // police qui lui survivra.
            unsafe { SendMessageW(*edit as HWND, WM_SETFONT, police as usize, 1) };
        }
    }
    if *avant != 0 {
        // SAFETY: la police d'avant, rendue une fois plus personne ne
        // l'emploie.
        unsafe { DeleteObject(*avant as _) };
    }
    *avant = police as isize;
}

/// La famille des champs : la même que celle du reste du dessin, autant
/// que le système la connaisse.
const FAMILLE_DES_CHAMPS: &str = "Segoe UI Variable Text";

/// Ouvre les trois vrais champs de Windows, vides.
fn ouvre_les_champs() {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::Controls::EM_SETCUEBANNER;
    use windows_sys::Win32::UI::Shell::SetWindowSubclass;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, ES_AUTOHSCROLL, SendMessageW, WS_CHILD, WS_VISIBLE,
    };

    let toile = ITS_WINDOW.load(Ordering::Relaxed) as HWND;
    if toile.is_null() {
        return;
    }
    let classe = wide("EDIT");
    let mut champs = CHAMPS.lock().expect("accueil");
    for champ in Champ::ALL {
        // SAFETY: une fenêtre du système, fille de la nôtre, sur le fil
        // qui possède les deux.
        let edit = unsafe {
            CreateWindowExW(
                0,
                classe.as_ptr(),
                std::ptr::null(),
                WS_CHILD | WS_VISIBLE | ES_AUTOHSCROLL as u32,
                0,
                0,
                0,
                0,
                toile,
                std::ptr::null_mut(),
                GetModuleHandleW(std::ptr::null()),
                std::ptr::null(),
            )
        };
        if edit.is_null() {
            continue;
        }
        let filigrane = wide(champ.exemple());
        // SAFETY: une fenêtre du système, à qui l'on donne un mot qui
        // survit à l'appel, puis un gardien qui lui survit : c'est une
        // simple fonction de ce programme.
        unsafe {
            SendMessageW(edit, EM_SETCUEBANNER, 1, filigrane.as_ptr() as isize);
            SetWindowSubclass(edit, Some(dans_un_champ), DANS_UN_CHAMP, champ.rang());
        }
        champs[champ.rang()] = edit as isize;
    }
    drop(champs);
    habille_les_champs();
}

/// Le nom sous lequel notre gardien est posé sur un champ.
const DANS_UN_CHAMP: usize = 3;

/// Ce que les touches d'un dialogue font dans un champ.
///
/// Un champ de Windows est une fenêtre à lui : la tabulation, Entrée et
/// Échap n'y arrivent jamais jusqu'à nous, et un dialogue où l'on ne
/// passe pas d'un champ au suivant n'est pas un dialogue. Elles sont donc
/// prises ici et rendues à qui de droit.
///
/// SAFETY: appelée par le système sur le fil qui possède ce champ, avec
/// les arguments qu'il documente.
unsafe extern "system" fn dans_un_champ(
    window: windows_sys::Win32::Foundation::HWND,
    message: u32,
    holding: windows_sys::Win32::Foundation::WPARAM,
    with: windows_sys::Win32::Foundation::LPARAM,
    _who: usize,
    rang: usize,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyState, SetFocus, VK_ESCAPE, VK_RETURN, VK_SHIFT, VK_TAB,
    };
    use windows_sys::Win32::UI::Shell::DefSubclassProc;
    use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CHAR, WM_KEYDOWN};

    let touche = holding as u16;
    let sienne = touche == VK_TAB || touche == VK_RETURN || touche == VK_ESCAPE;
    // Le signe qui suit la touche est avalé avec elle : sans ça le champ
    // sonne, la tabulation n'étant pas un signe qu'il accepte.
    if message == WM_CHAR && (holding == 9 || holding == 13 || holding == 27) {
        return 0;
    }
    if message == WM_KEYDOWN && sienne {
        match touche {
            VK_TAB => {
                // SAFETY: une question au système sur le clavier de ce
                // fil, puis le clavier donné à un champ à nous.
                let arriere = unsafe { GetKeyState(i32::from(VK_SHIFT)) } < 0;
                let combien = Champ::ALL.len();
                let suivant = if arriere {
                    (rang + combien - 1) % combien
                } else {
                    (rang + 1) % combien
                };
                let champs = *CHAMPS.lock().expect("accueil");
                if champs[suivant] != 0 {
                    // SAFETY: une fenêtre faite par nous, sur son fil.
                    unsafe { SetFocus(champs[suivant] as HWND) };
                }
            }
            // Posté et non fait tout de suite : les deux referment le
            // dialogue, donc détruisent le champ dans lequel on est en
            // train de répondre.
            autre => {
                let toile = ITS_WINDOW.load(Ordering::Relaxed) as HWND;
                if !toile.is_null() {
                    // SAFETY: une fenêtre à nous, à qui l'on poste un
                    // message qui n'appartient qu'à nous.
                    unsafe { PostMessageW(toile, AGIR, usize::from(autre == VK_RETURN), 0) };
                }
            }
        }
        return 0;
    }
    // SAFETY: les arguments que le système a donnés, rendus tels quels.
    unsafe { DefSubclassProc(window, message, holding, with) }
}

/// Referme les trois champs et rend leur place.
fn ferme_les_champs() {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::Graphics::Gdi::DeleteObject;
    use windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow;

    let mut champs = CHAMPS.lock().expect("accueil");
    for edit in champs.iter_mut() {
        if *edit != 0 {
            // SAFETY: une fenêtre faite par nous, détruite une fois.
            unsafe { DestroyWindow(*edit as HWND) };
            *edit = 0;
        }
    }
    *PLACES.lock().expect("accueil") = [None; 3];
    let mut police = POLICE.lock().expect("accueil");
    if *police != 0 {
        // SAFETY: une police faite par nous, rendue une fois.
        unsafe { DeleteObject(*police as _) };
        *police = 0;
    }
}

/// Note où le dessin veut ce champ. Il y sera posé une fois l'image
/// finie : déplacer une fenêtre pendant qu'on peint la sienne mêle deux
/// dessins.
fn pose_le_champ(champ: Champ, ou: Cadre) {
    PLACES.lock().expect("accueil")[champ.rang()] = Some(ou);
}

/// Pose les champs là où la dernière image les a voulus.
fn range_les_champs() {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos};

    let champs = *CHAMPS.lock().expect("accueil");
    let places = *PLACES.lock().expect("accueil");
    // Le texte respire dans son cadre comme la feuille de style le
    // demande : le vrai champ est posé dedans, jamais sur son trait.
    let dedans = design::PAS_3 * echelle();
    for (edit, place) in champs.iter().zip(places.iter()) {
        let (Some(place), true) = (place, *edit != 0) else {
            continue;
        };
        // SAFETY: une fenêtre faite par nous, déplacée sur le fil qui la
        // possède.
        unsafe {
            SetWindowPos(
                *edit as HWND,
                std::ptr::null_mut(),
                (place.gauche + dedans).round() as i32,
                (place.haut + dedans / 2.0).round() as i32,
                (place.droite - place.gauche - dedans * 2.0).round() as i32,
                (place.bas - place.haut - dedans).round() as i32,
                SWP_NOACTIVATE | SWP_NOZORDER,
            )
        };
    }
}

/// Ce qui est écrit dans un champ.
fn texte_du_champ(champ: Champ) -> String {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowTextLengthW, GetWindowTextW};

    let edit = CHAMPS.lock().expect("accueil")[champ.rang()];
    if edit == 0 {
        return String::new();
    }
    // SAFETY: une fenêtre faite par nous, dont le texte est lu dans un
    // tampon de la longueur qu'elle vient d'annoncer.
    unsafe {
        let combien = GetWindowTextLengthW(edit as HWND);
        if combien <= 0 {
            return String::new();
        }
        let mut lu = vec![0u16; combien as usize + 1];
        let lus = GetWindowTextW(edit as HWND, lu.as_mut_ptr(), combien + 1);
        String::from_utf16_lossy(&lu[..lus.max(0) as usize])
    }
}

/// De quelle couleur peindre le dedans d'un champ.
///
/// Le champ appartient au système, qui le dessine lui-même et demande
/// ici quelles couleurs employer : sans ça, un champ blanc trouerait une
/// fenêtre sombre.
fn teinte_du_champ(surface: windows_sys::Win32::Foundation::WPARAM) -> isize {
    use windows_sys::Win32::Graphics::Gdi::{
        CreateSolidBrush, DeleteObject, SetBkColor, SetTextColor,
    };

    let couleurs = palette();
    let (fond, encre) = (couleurs.surface_2, couleurs.texte);
    // SAFETY: la surface que le système vient de prêter, et un pinceau
    // qu'il rendra en même temps qu'il rendra celui d'avant.
    unsafe {
        SetTextColor(surface as _, rvb(encre));
        SetBkColor(surface as _, rvb(fond));
        let mut pinceau = PINCEAU.lock().expect("accueil");
        if *pinceau != 0 {
            DeleteObject(*pinceau as _);
        }
        *pinceau = CreateSolidBrush(rvb(fond)) as isize;
        *pinceau
    }
}

/// Le pinceau du fond des champs, gardé pour être rendu au suivant : le
/// système lit celui qu'on rend et ne le garde pas.
static PINCEAU: Mutex<isize> = Mutex::new(0);

/// Une couleur du système de design, dans le nombre que GDI attend.
fn rvb(couleur: Couleur) -> u32 {
    let part = |combien: f32| (combien.clamp(0.0, 1.0) * 255.0).round() as u32;
    part(couleur.red) | (part(couleur.green) << 8) | (part(couleur.blue) << 16)
}

/* ---- Ce qu'un clic fait -------------------------------------------------- */

/// Agit sur ce qui vient d'être cliqué.
///
/// Rien n'attend ici : ce qui demande au service part sur son propre fil
/// et redessine en revenant. Le fil qui dessine ne doit jamais attendre
/// une réponse qui traverse un tuyau.
fn fait(app: &App, quoi: Quoi) {
    match quoi {
        Quoi::OuvrirJournal => ouvre_le_journal(app, None),
        Quoi::JournalDe(rang) => {
            let voisin = VU
                .lock()
                .expect("accueil")
                .as_ref()
                .and_then(|vu| vu.voisins.get(rang).cloned());
            if let Some(voisin) = voisin {
                ouvre_le_journal(app, Some(voisin));
            }
        }
        Quoi::OuvrirReglages => {
            {
                let mut etat = ETAT.lock().expect("accueil");
                etat.ecran = Ecran::Reglages;
                etat.defile_dialogue = 0.0;
                etat.souci = None;
            }
            relis_les_reglages(app);
            redraw(app);
        }
        Quoi::CopierEmpreinte => {
            let empreinte = VU
                .lock()
                .expect("accueil")
                .as_ref()
                .and_then(|vu| vu.machine.as_ref().map(|dit| dit.fingerprint.clone()))
                .unwrap_or_default();
            copie(app, &empreinte, Quoi::CopierEmpreinte);
        }
        Quoi::CopierJournal => {
            let tout = ETAT.lock().expect("accueil").lignes.join("\n");
            copie(app, &tout, Quoi::CopierJournal);
        }
        Quoi::ARegler(rang) => remedie(app, rang),
        Quoi::Voisin(rang) => {
            let vise = VU
                .lock()
                .expect("accueil")
                .as_ref()
                .and_then(|vu| vu.voisins.get(rang).cloned());
            if let Some(voisin) = vise {
                lance(app, &voisin.address, &voisin.fingerprint, &voisin.name);
            }
        }
        Quoi::Ajouter => {
            {
                let mut etat = ETAT.lock().expect("accueil");
                etat.ecran = Ecran::Ajout;
                etat.defile_dialogue = 0.0;
            }
            // Vidés à chaque ouverture : rouverts pleins de la machine
            // précédente, ils laisseraient ajouter deux fois le même
            // ordinateur d'un simple double clic.
            ferme_les_champs();
            ouvre_les_champs();
            redraw(app);
        }
        Quoi::Fermer => {
            let mut etat = ETAT.lock().expect("accueil");
            etat.ecran = Ecran::Accueil;
            etat.ecoute = None;
            // Sur la fermeture et non sur son bouton : la touche Échap
            // ferme aussi, et laissait la confirmation de vidage armée
            // derrière un dialogue clos.
            etat.vidage = None;
            drop(etat);
            ferme_les_champs();
            redraw(app);
        }
        Quoi::Connecter => connecte(app),
        Quoi::Oublier(rang) => {
            let voisin = VU
                .lock()
                .expect("accueil")
                .as_ref()
                .and_then(|vu| vu.voisins.get(rang).cloned());
            if let Some(voisin) = voisin {
                oublie(app, voisin.fingerprint);
            }
        }
        Quoi::Interrupteur(bouton) => pousse(app, bouton),
        Quoi::Segment(quoi, rang) => choisis(app, quoi, rang),
        Quoi::Raccourci(doing) => {
            let mut etat = ETAT.lock().expect("accueil");
            etat.ecoute = if etat.ecoute == Some(doing) {
                None
            } else {
                Some(doing)
            };
            drop(etat);
            redraw(app);
        }
        Quoi::Avance => {
            let mut etat = ETAT.lock().expect("accueil");
            etat.avance = !etat.avance;
            drop(etat);
            redraw(app);
        }
        Quoi::Vider => vide_le_journal(app),
        Quoi::Actualiser => relis_le_journal(app),
        Quoi::OuvrirLesJournaux => ouvre_un_dossier(app, "logs"),
        Quoi::Ascenseur(_) => {}
    }
}

/// Ce que le bouton d'un bandeau « à faire » répare.
fn remedie(app: &App, rang: usize) {
    let manque = VU
        .lock()
        .expect("accueil")
        .as_ref()
        .map(ce_qui_manque)
        .and_then(|manques| manques.get(rang).map(|manque| manque.remede));
    match manque {
        Some(Remede::DemarrerLeService) => {
            let app = app.clone();
            crate::app::spawn(async move {
                if let Err(raison) = crate::desk::start_service().await {
                    annonce(&app, &raison, true);
                }
                relis(&app).await;
                redraw(&app);
            });
        }
        Some(Remede::MoteurHote) => ouvre_un_dossier(app, "host-engine"),
        Some(Remede::MoteurClient) => ouvre_un_dossier(app, "client-engine"),
        Some(Remede::VoirLeJournal) => ouvre_le_journal(app, None),
        None => {}
    }
}

fn ouvre_un_dossier(app: &App, lequel: &'static str) {
    if let Err(raison) = crate::folders::open_folder(lequel.to_string()) {
        annonce(app, &raison, true);
    }
}

/// Pousse un interrupteur, et le tient à sa nouvelle place le temps que
/// le service en prenne acte.
fn pousse(app: &App, bouton: Bouton) {
    let veut = {
        let rien = Vu::default();
        let vu = VU.lock().expect("accueil");
        let mut etat = ETAT.lock().expect("accueil");
        let veut = !bouton.allume(vu.as_ref().unwrap_or(&rien), &etat);
        etat.pousses.retain(|(quoi, _)| *quoi != bouton);
        etat.pousses.push((bouton, veut));
        etat.souci = None;
        etat.annonce = None;
        veut
    };
    redraw(app);

    let app = app.clone();
    crate::app::spawn(async move {
        let servir = || async {
            let dit = crate::desk::standing().await;
            crate::desk::set_serving(
                if bouton == Bouton::Cadence {
                    veut
                } else {
                    dit.steady_rate
                },
                dit.capture,
            )
            .await
        };
        let quoi = match bouton {
            Bouton::Acces => crate::desk::set_hosting(veut).await,
            Bouton::Confiance => crate::desk::set_trust(veut).await,
            Bouton::AuDemarrage => crate::desk::set_at_boot(veut).await,
            Bouton::Cadence => servir().await,
            Bouton::Son | Bouton::Stats => {
                ecrit_les_reglages(|chosen| {
                    if bouton == Bouton::Son {
                        chosen.mute_far_speakers = veut;
                    } else {
                        chosen.stats_overlay = veut;
                    }
                })
                .await
            }
        };
        if let Err(raison) = quoi {
            dit_le_souci(&app, &raison);
        }
        ETAT.lock()
            .expect("accueil")
            .pousses
            .retain(|(quoi, _)| *quoi != bouton);
        relis(&app).await;
        redraw(&app);
    });
}

/// Choisit un des côtés d'un choix segmenté.
fn choisis(app: &App, quoi: Choisi, rang: usize) {
    if quoi == Choisi::Theme {
        if let Some(choix) = Choix::ALL.get(rang) {
            crate::theme::choose(*choix);
            redraw(app);
        }
        return;
    }
    let Some(valeur) = quoi.valeurs().get(rang).copied() else {
        return;
    };
    let app = app.clone();
    crate::app::spawn(async move {
        // Celui-ci ne décrit pas ce qu'on demande aux autres mais ce que
        // cet ordinateur fait quand c'est lui qu'on regarde : il ne passe
        // pas par les mêmes réglages.
        let fait = if quoi == Choisi::Capture {
            let dit = crate::desk::standing().await;
            crate::desk::set_serving(dit.steady_rate, valeur.to_string()).await
        } else {
            ecrit_les_reglages(|chosen| match quoi {
                Choisi::Codec => chosen.codec = valeur.parse().unwrap_or(chosen.codec),
                Choisi::Affichage => chosen.display = valeur.parse().unwrap_or(chosen.display),
                Choisi::Souris => chosen.absolute_mouse = valeur == "desktop",
                _ => {}
            })
            .await
        };
        if let Err(raison) = fait {
            dit_le_souci(&app, &raison);
        }
        relis(&app).await;
        redraw(&app);
    });
}

/// Change un réglage de session, les autres restant ce qu'ils sont.
///
/// L'ensemble part au service pour qu'il n'ait jamais à deviner ce qui
/// est resté.
async fn ecrit_les_reglages(
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
fn connecte(app: &App) {
    let empreinte = texte_du_champ(Champ::Empreinte).trim().to_string();
    let adresse = texte_du_champ(Champ::Adresse).trim().to_string();
    let nom = texte_du_champ(Champ::Nom).trim().to_string();
    if empreinte.len() != TAILLE_EMPREINTE {
        return;
    }
    fait(app, Quoi::Fermer);

    let app = app.clone();
    crate::app::spawn(async move {
        let ecrit = crate::desk::authorize(
            empreinte.clone(),
            (!adresse.is_empty()).then(|| adresse.clone()),
            (!nom.is_empty()).then(|| nom.clone()),
        )
        .await;
        if let Err(raison) = ecrit {
            annonce(&app, &raison, true);
            return;
        }
        relis(&app).await;
        if adresse.is_empty() {
            // Autoriser ne se voit nulle part ailleurs : sans un mot, le
            // geste ferait exactement le même effet à l'écran que ne rien
            // faire.
            annonce(
                &app,
                "Cet ordinateur est autorisé à venir sur celui-ci.",
                false,
            );
            return;
        }
        redraw(&app);
        let vu_le_nom = VU
            .lock()
            .expect("accueil")
            .as_ref()
            .and_then(|vu| {
                vu.voisins
                    .iter()
                    .find(|voisin| voisin.fingerprint == empreinte)
                    .map(|voisin| voisin.name.clone())
            })
            .unwrap_or_else(|| adresse.clone());
        lance(&app, &adresse, &empreinte, &vu_le_nom);
    });
}

/// Oublie un ordinateur écrit à la main, des deux listes à la fois.
fn oublie(app: &App, empreinte: String) {
    let app = app.clone();
    crate::app::spawn(async move {
        if let Err(raison) = crate::desk::forget(empreinte).await {
            fait(&app, Quoi::Fermer);
            annonce(&app, &raison, true);
            return;
        }
        relis(&app).await;
        redraw(&app);
    });
}

/// Ouvre une session vers cet ordinateur.
fn lance(app: &App, adresse: &str, empreinte: &str, nom: &str) {
    {
        let vu = VU.lock().expect("accueil");
        let etat = ETAT.lock().expect("accueil");
        if vu.as_ref().is_some_and(|vu| vu.occupe(&etat)) {
            return;
        }
    }
    {
        let mut etat = ETAT.lock().expect("accueil");
        etat.annonce = None;
        etat.ouverture = Some(Ouverture {
            // Le nom plutôt que l'adresse : personne ne reconnaît son
            // ordinateur portable à ses quatre nombres.
            vers: nom.to_string(),
            detail: "Ouverture du tunnel…".to_string(),
            code: None,
            depuis: std::time::Instant::now(),
        });
    }
    redraw(app);

    let (app, adresse, empreinte) = (app.clone(), adresse.to_string(), empreinte.to_string());
    crate::app::spawn(async move {
        if let Err(raison) = crate::session::connect(app.clone(), adresse, empreinte).await {
            echoue(&app, &raison);
        }
    });
}

/* ---- Ce que la session raconte ------------------------------------------- */

/// Une étape de l'ouverture d'une session.
///
/// Appelée par ce qui conduit la session : la fenêtre est la seule à
/// pouvoir dire où en est ce qui n'a pas encore d'image.
pub fn etape(app: &App, detail: &str, code: Option<String>) {
    {
        let mut etat = ETAT.lock().expect("accueil");
        let Some(ouverture) = etat.ouverture.as_mut() else {
            return;
        };
        ouverture.detail = detail.to_string();
        ouverture.code = code;
    }
    redraw(app);
}

/// L'image se relance avec de nouveaux réglages : personne n'a cliqué
/// pour ouvrir celle-là, donc c'est ici que l'écran d'ouverture revient.
pub fn relance(app: &App) {
    {
        let mut etat = ETAT.lock().expect("accueil");
        let vers = etat
            .ouverture
            .as_ref()
            .map_or_else(String::new, |deja| deja.vers.clone());
        etat.ouverture = Some(Ouverture {
            vers,
            detail: "Nouveaux réglages, l'image se relance…".to_string(),
            code: None,
            depuis: std::time::Instant::now(),
        });
    }
    redraw(app);
}

/// La fenêtre n'a plus rien à raconter : ce qui se passe maintenant se
/// lit dans ce que tient le service.
pub fn range_l_ouverture(app: &App) {
    let app = app.clone();
    crate::app::spawn(async move {
        relis(&app).await;
        ETAT.lock().expect("accueil").ouverture = None;
        redraw(&app);
    });
}

/// Une session qui s'est mal terminée, ou qui n'a pas pu s'ouvrir.
pub fn echoue(app: &App, texte: &str) {
    annonce(app, texte, true);
    range_l_ouverture(app);
}

/// Le bandeau du haut.
fn annonce(app: &App, texte: &str, ennui: bool) {
    ETAT.lock().expect("accueil").annonce = Some(Annonce {
        texte: texte.to_string(),
        ennui,
        depuis: std::time::Instant::now(),
    });
    redraw(app);
}

/// Ce que les réglages ont à redire, qui vit dans leur dialogue.
fn dit_le_souci(app: &App, texte: &str) {
    ETAT.lock().expect("accueil").souci = Some(texte.to_string());
    redraw(app);
}

/* ---- Le journal ---------------------------------------------------------- */

fn ouvre_le_journal(app: &App, de: Option<Peer>) {
    {
        let mut etat = ETAT.lock().expect("accueil");
        etat.ecran = Ecran::Journal;
        etat.journal_de = de;
        etat.vidage = None;
        etat.defile_lignes = (0.0, 0.0);
        etat.lignes = vec!["Lecture…".to_string()];
    }
    redraw(app);
    relis_le_journal(app);
}

fn relis_le_journal(app: &App) {
    let de = ETAT.lock().expect("accueil").journal_de.clone();
    let app = app.clone();
    crate::app::spawn(async move {
        let texte = match &de {
            None => crate::journal::journal().await,
            Some(voisin) => {
                crate::journal::far_journal(voisin.address.clone(), voisin.fingerprint.clone())
                    .await
                    // Montré dans le journal lui-même : c'est là que
                    // regarde la personne qui vient de cliquer, et un
                    // ordinateur qui ne répond pas est déjà la moitié de
                    // la réponse.
                    .unwrap_or_else(|raison| raison)
            }
        };
        // Joindre une machine distante prend le temps qu'il faut : le
        // journal a pu être refermé, ou avoir changé d'ordinateur,
        // entre-temps. Ce qui arrive en retard n'écrase pas ce qui est à
        // l'écran.
        let mut etat = ETAT.lock().expect("accueil");
        if etat.journal_de != de || etat.ecran != Ecran::Journal {
            return;
        }
        etat.lignes = texte.lines().map(str::to_string).collect();
        // Le plus récent est en bas : c'est là que se trouve ce qui vient
        // d'arriver, et c'est ce qu'on ouvre le journal pour lire. Plus
        // bas que tout plutôt que d'une hauteur comptée : ce qui vient
        // d'être lu n'a pas encore été mesuré, et c'est le dessin qui
        // ramènera ce nombre à ce qu'il y a réellement à voir.
        etat.defile_lignes = (0.0, TOUT_EN_BAS);
        drop(etat);
        redraw(&app);
    });
}

/// Vider efface la seule trace de ce qui vient de se passer. Un deuxième
/// clic est demandé, et l'attente retombe d'elle-même.
fn vide_le_journal(app: &App) {
    let de = {
        let mut etat = ETAT.lock().expect("accueil");
        let arme = etat
            .vidage
            .is_some_and(|depuis| depuis.elapsed() < TEMPS_CONFIRMATION);
        if !arme {
            etat.vidage = Some(std::time::Instant::now());
            drop(etat);
            redraw(app);
            return;
        }
        etat.vidage = None;
        etat.lignes = vec!["Vidage…".to_string()];
        etat.journal_de.clone()
    };
    redraw(app);

    let app = app.clone();
    crate::app::spawn(async move {
        // Vidé là où il est écrit : celui de cette machine tout de suite,
        // celui d'en face en le lui demandant.
        let fait = match &de {
            None => crate::journal::clear_journal(),
            Some(voisin) => {
                crate::journal::clear_far_journal(
                    voisin.address.clone(),
                    voisin.fingerprint.clone(),
                )
                .await
            }
        };
        if let Err(raison) = fait {
            ETAT.lock().expect("accueil").lignes = raison.lines().map(str::to_string).collect();
            redraw(&app);
            return;
        }
        relis_le_journal(&app);
    });
}

/* ---- Le presse-papiers ---------------------------------------------------- */

/// Copie ce texte, et fait dire au bouton qu'il l'a fait.
///
/// Le presse-papiers peut refuser, et un bouton qui dit « Copié » sur un
/// refus enverrait quelqu'un coller du vide sur l'autre ordinateur.
fn copie(app: &App, texte: &str, quoi: Quoi) {
    if !mis_au_presse_papiers(texte) {
        annonce(app, "La copie a été refusée par Windows.", true);
        return;
    }
    ETAT.lock().expect("accueil").copie = Some((quoi, std::time::Instant::now()));
    redraw(app);

    let app = app.clone();
    crate::app::spawn(async move {
        tokio::time::sleep(TEMPS_COPIE).await;
        ETAT.lock().expect("accueil").copie = None;
        redraw(&app);
    });
}

fn mis_au_presse_papiers(texte: &str) -> bool {
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows_sys::Win32::System::Memory::{
        GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock,
    };
    use windows_sys::Win32::System::Ole::CF_UNICODETEXT;

    let mots = wide(texte);
    // SAFETY: le presse-papiers est ouvert et refermé ici même, et la
    // mémoire remise appartient au système dès qu'il l'a prise.
    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return false;
        }
        EmptyClipboard();
        let taille = std::mem::size_of_val(&mots[..]);
        let bloc = GlobalAlloc(GMEM_MOVEABLE, taille);
        if bloc.is_null() {
            CloseClipboard();
            return false;
        }
        let ou = GlobalLock(bloc);
        if ou.is_null() {
            CloseClipboard();
            return false;
        }
        std::ptr::copy_nonoverlapping(mots.as_ptr(), ou.cast::<u16>(), mots.len());
        GlobalUnlock(bloc);
        let pris = !SetClipboardData(CF_UNICODETEXT as u32, bloc).is_null();
        CloseClipboard();
        pris
    }
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
            let mut vu = VU.lock().expect("accueil");
            let neuf = vu.get_or_insert_with(Vu::default);
            neuf.version = crate::desk::build();
            neuf.dossier = crate::folders::logs_folder();
            neuf.raccourcis = crate::shortcuts::engraved();
        }
        loop {
            if relis(&app).await {
                redraw(&app);
            }
            tokio::time::sleep(RYTHME).await;
        }
    });
}

/// Redemande ce que le service dit, et dit si quelque chose a changé.
///
/// Ce qui n'a pas changé n'est pas redessiné : la fenêtre reste souvent
/// ouverte pendant une session, et repeindre une image identique trois
/// fois par minute serait du processeur pris à l'image de la session.
async fn relis(app: &App) -> bool {
    let machine = crate::desk::standing().await;
    let voisins = crate::desk::peers().await;
    let sessions = crate::session::sessions().await;
    let moteurs = crate::folders::engines();
    let reglages = crate::settings::settings(app.clone()).await;

    let mut vu = VU.lock().expect("accueil");
    let neuf = vu.get_or_insert_with(Vu::default);
    let avant = Vu {
        machine: neuf.machine.replace(machine),
        voisins: std::mem::replace(&mut neuf.voisins, voisins),
        sessions: std::mem::replace(&mut neuf.sessions, sessions),
        moteurs: neuf.moteurs.replace(moteurs),
        reglages: neuf.reglages.replace(reglages),
        raccourcis: neuf.raccourcis.clone(),
        version: neuf.version.clone(),
        dossier: neuf.dossier.clone(),
    };
    let mut change = avant != *neuf;
    drop(vu);

    // Une bonne nouvelle s'efface toute seule : restée à l'écran, elle
    // finit par se lire comme un état. Un ennui reste jusqu'au geste
    // suivant, puisqu'il attend qu'on y réponde.
    let mut etat = ETAT.lock().expect("accueil");
    if etat
        .annonce
        .as_ref()
        .is_some_and(|dit| !dit.ennui && dit.depuis.elapsed() > TEMPS_ANNONCE)
    {
        etat.annonce = None;
        change = true;
    }
    change
}

/// Relit ce que l'écran des réglages montre, et les trois raccourcis.
fn relis_les_reglages(app: &App) {
    let app = app.clone();
    crate::app::spawn(async move {
        let reglages = crate::settings::settings(app.clone()).await;
        if let Some(vu) = VU.lock().expect("accueil").as_mut() {
            vu.reglages = Some(reglages);
            vu.raccourcis = crate::shortcuts::engraved();
        }
        redraw(&app);
    });
}
