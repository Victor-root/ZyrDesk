//! Asking a journal for the lines that are about one thing.
//!
//! A session writes four thousand lines. Six of them are about the
//! clipboard, and the difference between reading those six and reading
//! all four thousand is whether they can be asked for by name. Every
//! line the product writes carries a tag saying which part of it wrote
//! the line; this is the other half, the asking.
//!
//! # What can be written in the box
//!
//! A plain word is the name of a part of the product, and that is nine
//! askings out of ten: `clipboard` keeps the lines that part wrote.
//! Lines the engines write carry no name of their own, so the file they
//! are in stands in for one, and `session` is the client engine's whole
//! log.
//!
//! Several of them keep **one or the other**, because a line carries one
//! name and never two: `clipboard files` is both subjects at once, which
//! is the whole reason for typing two. Everything else piles up as
//! « and », so `clipboard -level:debug` is that part of the product with
//! its hunting lines left out.
//!
//! The rest is there for when a plain word is not enough:
//!
//! - `tag:clipboard` is the long way of writing `clipboard`, and means
//!   exactly the same thing.
//! - `"deux mots"` or `message:refusé` looks inside the line rather than
//!   at its name. Quotes are what tells one from the other: quoted is
//!   text to find, bare is a name.
//! - a word carrying a colon, like `192.168.1.5:47000`, is an address, a
//!   time or one of this product's own spellings, never the name of a
//!   part of it, so it too is looked for inside the line.
//! - `level:debug` keeps only what was written for a hunt, and
//!   `-level:debug` throws all of it away. There are two levels and no
//!   more: what the product says of itself, and what only a hunt wants.
//!   The second is written by a build made for hunting and by no other,
//!   so in an ordinary build there is none of it to ask for.
//! - a `-` in front of anything throws away what it names instead of
//!   keeping it.
//!
//! Nothing matches by case, since nobody remembers the case of a name
//! they read once.
//!
//! An empty box is not a filter that keeps nothing: it is no filter at
//! all, and the whole journal comes out, which is what it has always
//! done.

use std::fmt;

/// How wide a written timestamp is, which is where a tag starts.
///
/// Read from the shape `log` writes rather than counted here twice: the
/// two have to agree, and a test below is what says they do.
const AFTER_THE_DATE: usize = "2026-09-11 18:55:03".len();

/// What one thing written in the box asks of a line.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Term {
    /// Which part of the line it is asked of.
    of: Part,
    /// What is looked for there, already lowered.
    wanted: String,
    /// Whether finding it throws the line away rather than keeping it.
    against: bool,
}

/// Which part of a line a term is asked of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    /// What the line is filed under, or the file it is in when it is
    /// filed under nothing.
    Tag,
    /// Which of the two voices it was written in.
    Level,
    /// The line itself, tag and date and all.
    Anything,
}

/// What was asked of a journal, ready to be asked of each line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sifting {
    terms: Vec<Term>,
    /// What was written, kept word for word so that a journal can say
    /// what it was sifted through. A page of six lines that does not
    /// say it was sifted reads as a product that did nothing.
    said: String,
}

impl Sifting {
    /// Nothing asked, so everything kept.
    pub fn everything() -> Self {
        Self::default()
    }

    /// Reads what was written in the box.
    ///
    /// Nothing here can be wrong: what is not one of the shapes above is
    /// read as the name of a part of the product, which is the useful
    /// reading of a mistyped one, since a name nothing carries simply
    /// keeps nothing. A box that refused what was typed would be a box
    /// that argues instead of answering.
    pub fn of(said: &str) -> Self {
        let mut terms = Vec::new();
        for piece in pieces(said) {
            let (against, piece) = match piece.strip_prefix('-') {
                Some(rest) => (true, rest.to_string()),
                None => (false, piece),
            };
            let (key, after) = match piece.split_once(':') {
                Some((key, rest)) => (key.to_lowercase(), Some(rest)),
                None => (String::new(), None),
            };
            let (of, wanted) = match (key.as_str(), after) {
                ("tag", Some(wanted)) => (Part::Tag, wanted),
                ("level", Some(wanted)) => (Part::Level, wanted),
                ("message", Some(wanted)) => (Part::Anything, wanted),
                // A colon inside a word is an address, a time or a
                // spelling this product writes everywhere, and never the
                // name of one of its parts: looked for inside the line
                // as it stands rather than read as a key nobody meant.
                (_, Some(_)) => (Part::Anything, piece.as_str()),
                // Quotes say « these words, somewhere in the line », and
                // that is what makes them worth typing at all now that a
                // bare word means something else.
                (_, None) if piece.starts_with('"') => (Part::Anything, piece.as_str()),
                // And a plain word names a part of the product, which is
                // nine askings out of ten and therefore costs nothing.
                (_, None) => (Part::Tag, piece.as_str()),
            };
            let wanted = unquoted(wanted).to_lowercase();
            if !wanted.is_empty() {
                terms.push(Term {
                    of,
                    wanted,
                    against,
                });
            }
        }
        Self {
            terms,
            said: said.trim().to_string(),
        }
    }

    /// Whether nothing was asked, so that nothing has to be read line by
    /// line.
    pub fn takes_everything(&self) -> bool {
        self.terms.is_empty()
    }

    /// Whether that line answers what was asked of it.
    ///
    /// `within` is the file the line is in, and it counts as a name of
    /// its own beside the one the line carries. Two reasons, and both
    /// matter: the engines write their own journals in their own shape
    /// and carry no name at all, so the file is the only one theirs has;
    /// and asking for one whole file is a thing somebody wants often
    /// enough that it should not need a second word for it.
    ///
    /// The names asked for are weighed together as « one or the other »,
    /// and everything else as « and ». A line carries one name: asking
    /// for two and keeping what answers both would keep nothing at all,
    /// which is the opposite of what somebody typing two of them wants.
    pub fn keeps(&self, line: &str, within: &str) -> bool {
        let lowered = line.to_lowercase();
        let (level, tag) = about(line).unwrap_or((SOMEBODY_ELSE, ""));
        let tag = tag.to_lowercase();
        let within = within.to_lowercase();
        let found = |term: &Term| match term.of {
            Part::Tag => tag.contains(&term.wanted) || within.contains(&term.wanted),
            Part::Level => level.starts_with(&term.wanted),
            Part::Anything => lowered.contains(&term.wanted),
        };

        let a_name_asked_for = |term: &&Term| term.of == Part::Tag && !term.against;
        let mut asked = self.terms.iter().filter(a_name_asked_for).peekable();
        if asked.peek().is_some() && !asked.any(&found) {
            return false;
        }
        self.terms
            .iter()
            .filter(|term| !a_name_asked_for(term))
            .all(|term| found(term) != term.against)
    }

    /// What was asked, as it was written.
    pub fn said(&self) -> &str {
        &self.said
    }
}

impl fmt::Display for Sifting {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.said)
    }
}

/// What a line says of itself: the voice it was written in and what it
/// is filed under.
///
/// Read at the one place they can be rather than hunted for: both sit
/// straight after the date, in that order, so a letter or a bracket
/// anywhere in the message is part of the message and nothing else.
///
/// Nothing at all for a line the engines wrote, which carries neither.
pub(crate) fn about(line: &str) -> Option<(&'static str, &str)> {
    let rest = line.get(AFTER_THE_DATE..)?.strip_prefix(' ')?;
    let (voice, rest) = rest.split_at_checked(1)?;
    let rest = rest.strip_prefix(" [")?;
    let (tag, _) = rest.split_once(']')?;
    if tag.is_empty() || tag.contains(' ') {
        return None;
    }
    Some((named(voice), tag))
}

/// What a voice is called, in the word somebody would type.
///
/// Lines the engines wrote are neither, and are called so: asking for
/// one level or the other must not quietly hand over a third kind.
const SOMEBODY_ELSE: &str = "engine";

fn named(voice: &str) -> &'static str {
    // Read from the letters the journal writes rather than spelled out
    // again here: two lists of the same letters drift apart the first
    // time one of them is touched, and nothing would say so.
    match voice.chars().next() {
        Some(crate::log::HUNTS) => "debug",
        Some(crate::log::SAYS) => "info",
        _ => SOMEBODY_ELSE,
    }
}

/// What was written, cut into things, quotes holding their spaces.
fn pieces(said: &str) -> Vec<String> {
    let mut pieces = Vec::new();
    let mut piece = String::new();
    let mut quoted = false;
    for letter in said.chars() {
        match letter {
            '"' => {
                quoted = !quoted;
                piece.push(letter);
            }
            letter if letter.is_whitespace() && !quoted => {
                if !piece.is_empty() {
                    pieces.push(std::mem::take(&mut piece));
                }
            }
            letter => piece.push(letter),
        }
    }
    if !piece.is_empty() {
        pieces.push(piece);
    }
    pieces
}

/// The same, without the quotes that were holding its spaces together.
fn unquoted(said: &str) -> &str {
    said.strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(said)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Une ligne comme le journal les écrit, dans la voix ordinaire.
    fn ligne(tag: &str, message: &str) -> String {
        format!("2026-09-11 18:55:03 I [{tag}] {message}")
    }

    /// La même, dans celle que seule une chasse veut.
    fn ligne_de_chasse(tag: &str, message: &str) -> String {
        format!("2026-09-11 18:55:03 D [{tag}] {message}")
    }

    #[test]
    fn une_boite_vide_ne_trie_rien() {
        // C'est ce qui fait que le bouton Copier continue de tout copier
        // tant qu'on ne lui a rien demandé.
        let tamis = Sifting::of("   ");
        assert!(tamis.takes_everything());
        assert!(tamis.keeps(&ligne("clipboard", "peu importe"), "service"));
    }

    #[test]
    fn une_etiquette_demandee_ecarte_tout_le_reste() {
        // Le mot seul et la forme longue disent la même chose : le
        // second est ce qui a été tapé pendant des semaines, et rien de
        // ce qui a été appris ne doit cesser de marcher.
        for said in ["clipboard", "tag:clipboard"] {
            let tamis = Sifting::of(said);
            assert!(!tamis.takes_everything(), "{said}");
            assert!(
                tamis.keeps(
                    &ligne("clipboard", "ce que tient cet ordinateur"),
                    "service"
                ),
                "{said}"
            );
            assert!(
                !tamis.keeps(&ligne("ways", "voie 1 ouverte"), "service"),
                "{said}"
            );
        }
    }

    #[test]
    fn plusieurs_etiquettes_gardent_l_une_ou_l_autre() {
        // Une ligne ne porte qu'une étiquette : les exiger toutes ne
        // garderait jamais rien, ce qui est le contraire de ce que veut
        // celui qui en tape deux.
        let tamis = Sifting::of("clipboard files");
        assert!(tamis.keeps(
            &ligne("clipboard", "ce que tient cet ordinateur"),
            "service"
        ));
        assert!(tamis.keeps(&ligne("files", "1 fichier de 4,7 Go arrive"), "service"));
        assert!(!tamis.keeps(&ligne("way", "voie 1 ouverte"), "service"));
    }

    #[test]
    fn une_etiquette_et_un_refus_se_cumulent() {
        // Les étiquettes entre elles font « l'une ou l'autre », tout le
        // reste fait « et » : c'est ce qui rend « le sujet, sans le
        // bruit connu » possible en deux mots.
        let tamis = Sifting::of("clipboard files -level:debug");
        assert!(tamis.keeps(&ligne("files", "1 fichier arrive"), "service"));
        assert!(!tamis.keeps(&ligne_de_chasse("files", "morceau 12 demandé"), "service"));
        assert!(!tamis.keeps(&ligne("way", "voie 1 ouverte"), "service"));
    }

    #[test]
    fn la_casse_ne_compte_pas() {
        // Personne ne se souvient de la casse d'une étiquette lue une
        // fois.
        let tamis = Sifting::of("TAG:ClipBoard");
        assert!(tamis.keeps(&ligne("clipboard", "quoi que ce soit"), "service"));
    }

    #[test]
    fn le_nom_du_fichier_compte_comme_une_etiquette() {
        // Les moteurs écrivent leur journal à leur façon et ne portent
        // aucune étiquette : sans ça, le leur ne se demanderait pas du
        // tout. Et demander un fichier entier est une chose assez
        // courante pour ne pas mériter un second mot.
        let tamis = Sifting::of("tag:session");
        let moteur = "00:00:03 - SDL Info (0): IDR frame request sent";
        assert!(tamis.keeps(moteur, "session.log"));
        assert!(!tamis.keeps(moteur, "service.log"));
        // Et une ligne étiquetée reste dans son fichier : les deux noms
        // vont ensemble plutôt que l'un à la place de l'autre.
        let notre = ligne("clipboard", "ce que tient cet ordinateur");
        assert!(tamis.keeps(&notre, "session.log"));
        assert!(Sifting::of("tag:clipboard").keeps(&notre, "service.log"));
    }

    #[test]
    fn un_mot_dans_la_ligne_se_demande_entre_guillemets_ou_par_son_nom() {
        // Les deux formes de la recherche de texte, maintenant qu'un mot
        // nu nomme une partie du produit.
        for tamis in [
            Sifting::of("\"DataObject\""),
            Sifting::of("message:DataObject"),
        ] {
            assert!(tamis.keeps(&ligne("clipboard", "il tient DataObject"), "service"));
            assert!(!tamis.keeps(&ligne("clipboard", "il tient du texte"), "service"));
        }
    }

    #[test]
    fn une_etiquette_et_un_mot_ecarte_se_cumulent() {
        // C'est ce qui rend une paire comme celle-ci utile : le nom pour
        // le sujet, le moins pour le bruit connu.
        let tamis = Sifting::of("clipboard -\"DataObject\"");
        assert!(tamis.keeps(&ligne("clipboard", "15997 octets de texte"), "service"));
        assert!(!tamis.keeps(&ligne("clipboard", "il tient DataObject"), "service"));
        assert!(!tamis.keeps(&ligne("ways", "15997 octets de texte"), "service"));
    }

    #[test]
    fn ce_qui_est_entre_guillemets_est_une_seule_chose() {
        let tamis = Sifting::of("\"deux mots\"");
        assert!(tamis.keeps(&ligne("ways", "voici deux mots ici"), "service"));
        assert!(!tamis.keeps(&ligne("ways", "deux, puis mots"), "service"));
    }

    #[test]
    fn un_deux_points_au_milieu_d_un_mot_reste_dans_le_mot() {
        // Ce produit écrit des adresses et des heures partout : les lire
        // comme une clé inconnue ne rendrait jamais rien.
        let tamis = Sifting::of("192.168.1.5:57577");
        assert!(tamis.keeps(&ligne("ways", "card 192.168.1.5:57577 sondée"), "service"));
    }

    #[test]
    fn l_etiquette_se_lit_la_ou_le_journal_l_ecrit() {
        // Et nulle part ailleurs : un crochet dans le message est un
        // crochet dans le message.
        let written = ligne("clipboard", "[pas une étiquette] la suite");
        assert_eq!(about(&written), Some(("info", "clipboard")));
        assert_eq!(about("00:00:03 - SDL Info (0): [hevc @ 0x1] rien"), None);
    }

    #[test]
    fn une_voix_se_demande_par_son_nom_ou_par_sa_lettre() {
        // Ce qui n'est là que pour une chasse noie tout le reste : on
        // doit pouvoir ne garder que ça, ou tout sauf ça.
        let chasse = ligne_de_chasse("way", "pas un paquet depuis 1098 ms");
        let dit = ligne("way", "voie 1 ouverte vers PC-SAV");

        assert!(Sifting::of("level:debug").keeps(&chasse, "service.log"));
        assert!(!Sifting::of("level:debug").keeps(&dit, "service.log"));
        assert!(Sifting::of("-level:debug").keeps(&dit, "service.log"));
        // La lettre suffit, et la casse ne compte pas.
        assert!(Sifting::of("level:D").keeps(&chasse, "service.log"));

        // Et ce que les moteurs écrivent n'est ni l'un ni l'autre :
        // demander une voix ne doit pas rendre en douce une troisième
        // sorte de ligne.
        let moteur = "00:00:03 - SDL Info (0): IDR frame request sent";
        assert!(!Sifting::of("level:debug").keeps(moteur, "session.log"));
        assert!(!Sifting::of("level:info").keeps(moteur, "session.log"));
        assert!(Sifting::of("level:engine").keeps(moteur, "session.log"));
    }

    #[test]
    fn ce_qui_a_ete_ecrit_se_relit_tel_quel() {
        // La page dira sous quel tri elle a été prise : une page de six
        // lignes qui ne le dit pas se lit comme un produit muet.
        let tamis = Sifting::of("  tag:clipboard -DataObject  ");
        assert_eq!(tamis.said(), "tag:clipboard -DataObject");
        assert_eq!(tamis.to_string(), "tag:clipboard -DataObject");
    }

    #[test]
    fn l_etiquette_se_retrouve_la_ou_le_journal_l_a_vraiment_ecrite() {
        // Les deux moitiés se tiennent l'une l'autre : si le journal
        // change la forme de sa date, l'étiquette cesse d'être là où on
        // la cherche, et plus rien ne se trie. Écrit pour de vrai et
        // relu, plutôt que recopié de tête ici.
        let folder = std::env::temp_dir().join(format!(
            "zyrdesk-tamis-{}",
            crate::random::alphanumeric_string(8)
        ));
        let path = folder.join("service.log");
        let log = crate::log::Log::open(&path).unwrap();
        log.about("clipboard").write("ce que tient cet ordinateur");

        let written = std::fs::read_to_string(&path).unwrap();
        let line = written.lines().next().unwrap();
        assert_eq!(about(line), Some(("info", "clipboard")), "{line}");
        assert!(Sifting::of("tag:clipboard").keeps(line, "service"));

        std::fs::remove_dir_all(&folder).ok();
    }
}
