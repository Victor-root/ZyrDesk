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
//! The same handful of things as the one every developer already knows
//! from a phone's own log, and deliberately no more:
//!
//! - `tag:clipboard` keeps the lines that part of the product wrote.
//!   Lines the engines write carry no tag of their own, so the name of
//!   the file they are in stands in for one: `tag:session` is the client
//!   engine's whole log.
//! - `message:refusé`, or the same word written on its own, keeps the
//!   lines that hold it anywhere.
//! - `-tag:card` throws away what it names instead of keeping it.
//! - `"deux mots"` is one thing and not two.
//!
//! Several of them together keep what answers all of them, which is what
//! makes a pair like `tag:clipboard -message:DataObject` worth typing.
//! Nothing matches by case, since nobody remembers the case of a tag
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
    /// a word to look for in the line, which is the useful reading of a
    /// mistyped one. A box that refused what was typed would be a box
    /// that argues instead of answering.
    pub fn of(said: &str) -> Self {
        let mut terms = Vec::new();
        for piece in pieces(said) {
            let (against, piece) = match piece.strip_prefix('-') {
                Some(rest) => (true, rest.to_string()),
                None => (false, piece),
            };
            let key = piece
                .split_once(':')
                .map(|(key, _)| key.to_lowercase())
                .unwrap_or_default();
            let after = piece.split_once(':').map(|(_, rest)| rest);
            let (of, wanted) = match (key.as_str(), after) {
                ("tag", Some(wanted)) => (Part::Tag, wanted),
                ("message", Some(wanted)) => (Part::Anything, wanted),
                // A colon inside a word is an address, a time or a
                // spelling this product uses everywhere: looked for as
                // it stands rather than read as a key nobody meant.
                _ => (Part::Anything, piece.as_str()),
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

    /// Whether that line answers everything that was asked of it.
    ///
    /// `within` is the file the line is in, and it counts as a tag of
    /// its own beside the one the line carries. Two reasons, and both
    /// matter: the engines write their own journals in their own shape
    /// and carry no tag at all, so the file is the only name theirs has;
    /// and asking for one whole file is a thing somebody wants often
    /// enough that it should not need a second word for it.
    pub fn keeps(&self, line: &str, within: &str) -> bool {
        let lowered = line.to_lowercase();
        let tag = tag_of(line).unwrap_or_default().to_lowercase();
        let within = within.to_lowercase();
        self.terms.iter().all(|term| {
            let found = match term.of {
                Part::Tag => tag.contains(&term.wanted) || within.contains(&term.wanted),
                Part::Anything => lowered.contains(&term.wanted),
            };
            found != term.against
        })
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

/// What a line is filed under, when it carries one.
///
/// Read at the one place it can be rather than hunted for: the tag sits
/// in brackets straight after the date, so a bracket anywhere in the
/// message is a bracket in the message and nothing else.
fn tag_of(line: &str) -> Option<&str> {
    let rest = line.get(AFTER_THE_DATE..)?.strip_prefix(" [")?;
    let (tag, _) = rest.split_once(']')?;
    (!tag.is_empty() && !tag.contains(' ')).then_some(tag)
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

    /// Une ligne comme le journal les écrit.
    fn ligne(tag: &str, message: &str) -> String {
        format!("2026-09-11 18:55:03 [{tag}] {message}")
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
        let tamis = Sifting::of("tag:clipboard");
        assert!(!tamis.takes_everything());
        assert!(tamis.keeps(
            &ligne("clipboard", "ce que tient cet ordinateur"),
            "service"
        ));
        assert!(!tamis.keeps(&ligne("ways", "voie 1 ouverte"), "service"));
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
    fn un_mot_seul_se_cherche_dans_toute_la_ligne() {
        let tamis = Sifting::of("DataObject");
        assert!(tamis.keeps(&ligne("clipboard", "il tient DataObject"), "service"));
        assert!(!tamis.keeps(&ligne("clipboard", "il tient du texte"), "service"));
    }

    #[test]
    fn plusieurs_choses_demandees_se_cumulent() {
        // C'est ce qui rend une paire comme celle-ci utile : l'étiquette
        // pour le sujet, le moins pour le bruit connu.
        let tamis = Sifting::of("tag:clipboard -DataObject");
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
        assert_eq!(tag_of(&written), Some("clipboard"));
        assert_eq!(tag_of("00:00:03 - SDL Info (0): [hevc @ 0x1] rien"), None);
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
        assert_eq!(tag_of(line), Some("clipboard"), "{line}");
        assert!(Sifting::of("tag:clipboard").keeps(line, "service"));

        std::fs::remove_dir_all(&folder).ok();
    }
}
