//! What one clipboard hands to the other.
//!
//! A clipboard holds one thing at a time, and the product carries two
//! kinds of it: what somebody selected in a text, and what somebody
//! copied of a picture. Nothing else crosses. Files are a transfer and
//! not a clipboard, and they will be their own thing when they come.
//!
//! Both computers are Windows, so a picture travels as the one shape
//! every program there already agrees on: PNG. It is what a browser and
//! most editors put on the clipboard to begin with, in which case it is
//! carried exactly as it was found; and it is what the system's own
//! imaging turns a screenshot into, a screenshot being handed over as
//! several million bytes of uncompressed pixels that nothing would want
//! to send down a session.
//!
//! Every clip carries a stamp, which is a digest of what it holds. It is
//! what lets the two ends say « the same one » without moving a single
//! byte of it: a clipboard is read many times a second and changes a few
//! times an hour, so almost every exchange is two stamps that match and
//! nothing else.

use std::fmt;
use std::str::FromStr;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use sha2::{Digest, Sha256};

/// The most a clip may weigh.
///
/// A screenshot of a large screen comes to a few hundred thousand bytes
/// once it is a PNG, and a photograph to a few million. Past this, what
/// somebody copied is not a clipboard's business any more: it would hold
/// up the picture of the session for seconds on a link that has to carry
/// both, to paste something they are far more likely to have on the other
/// machine already. What does not cross is said in the journal rather
/// than silently dropped.
pub const LARGEST: usize = 5 * 1024 * 1024;

/// What kind of thing a clipboard holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Text, as its own bytes, which are UTF-8 here and UTF-16 on a
    /// Windows clipboard.
    Text,
    /// A picture, as PNG.
    Picture,
}

impl Kind {
    fn spelled(self) -> &'static str {
        match self {
            Kind::Text => "text",
            Kind::Picture => "picture",
        }
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.spelled())
    }
}

impl FromStr for Kind {
    type Err = Unreadable;

    fn from_str(said: &str) -> Result<Self, Self::Err> {
        match said.trim() {
            "text" => Ok(Kind::Text),
            "picture" => Ok(Kind::Picture),
            _ => Err(Unreadable),
        }
    }
}

/// The short name of what a clipboard holds.
///
/// A digest and not a counter, because the two computers have to agree on
/// it without either being the one who decides: a counter would say « the
/// third thing I copied », which means nothing at the other end, whereas
/// a digest of the contents means the same thing on both machines and
/// nowhere needs a conversation to stay in step.
///
/// It is also what stops a clip from bouncing. What arrives is put on
/// this computer's clipboard and immediately reads back with the very
/// stamp it came with, so the next turn has nothing to say.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Stamp([u8; 32]);

impl fmt::Display for Stamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl FromStr for Stamp {
    type Err = Unreadable;

    fn from_str(said: &str) -> Result<Self, Self::Err> {
        let said = said.trim();
        if said.len() != 64 {
            return Err(Unreadable);
        }
        let mut bytes = [0u8; 32];
        let (pairs, _) = said.as_bytes().as_chunks::<2>();
        for (slot, pair) in bytes.iter_mut().zip(pairs) {
            let pair = std::str::from_utf8(pair).map_err(|_| Unreadable)?;
            *slot = u8::from_str_radix(pair, 16).map_err(|_| Unreadable)?;
        }
        Ok(Self(bytes))
    }
}

/// Something that does not say what a clipboard holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unreadable;

impl fmt::Display for Unreadable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ce n'est pas ce qu'un presse-papiers porte")
    }
}

impl std::error::Error for Unreadable {}

/// What a clipboard holds, and the short name of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clip {
    kind: Kind,
    bytes: Vec<u8>,
    stamp: Stamp,
}

impl Clip {
    /// A clip, with its stamp worked out once here.
    ///
    /// Once and not on demand: the stamp is asked for several times a
    /// second by both ends, and digesting a few million bytes that often
    /// would cost more than everything else this feature does put
    /// together.
    pub fn new(kind: Kind, bytes: Vec<u8>) -> Self {
        let mut digest = Sha256::new();
        // The kind by its name and not by its rank in the list above: two
        // computers built at different dates have to work out the same
        // stamp for the same thing, and a rank is whatever the order of
        // an enumeration happens to be that day.
        digest.update(kind.spelled().as_bytes());
        digest.update(&bytes);
        let stamp = Stamp(digest.finalize().into());
        Self { kind, bytes, stamp }
    }

    pub fn text(said: &str) -> Self {
        Self::new(Kind::Text, said.as_bytes().to_vec())
    }

    pub fn picture(png: Vec<u8>) -> Self {
        Self::new(Kind::Picture, png)
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn stamp(&self) -> Stamp {
        self.stamp
    }

    /// The text of it, when it is text at all.
    pub fn said(&self) -> Option<&str> {
        match self.kind {
            Kind::Text => std::str::from_utf8(&self.bytes).ok(),
            Kind::Picture => None,
        }
    }

    /// Whether it is more than a clipboard carries.
    pub fn too_large(&self) -> bool {
        self.bytes.len() > LARGEST
    }

    /// What it is, in the words the service writes its journal in.
    pub fn in_words(&self) -> String {
        match self.kind {
            Kind::Text => format!("{} bytes of text", self.bytes.len()),
            Kind::Picture => format!("a picture of {} bytes", self.bytes.len()),
        }
    }

    /// The line that names a clip without carrying it: its kind and its
    /// stamp.
    ///
    /// The head of both shapes below, written once here. It is the whole
    /// of what a file needs beside its bytes, and the whole of what the
    /// tunnel says before them.
    pub fn named(&self) -> String {
        format!("{} {}", self.kind, self.stamp)
    }

    /// A clip as it travels on ZyrDesk's own channel.
    ///
    /// The channel carries text, and a PNG is not text: the bytes go in
    /// base64, which costs a third of their weight and keeps the message
    /// one message. The head stays readable, so a line caught in a trace
    /// still says what it was and how large.
    pub fn on_the_wire(&self) -> String {
        format!("{} {}", self.named(), BASE64.encode(&self.bytes))
    }

    /// Reads a clip that travelled that way.
    ///
    /// The stamp is worked out again from the bytes and compared to the
    /// one that came: what names a clip on this computer has to be what
    /// this computer computed, or the two ends would agree on a name for
    /// two different things and stop exchanging anything at all.
    pub fn from_the_wire(said: &str) -> Result<Self, Unreadable> {
        let mut pieces = said.trim().splitn(3, char::is_whitespace);
        let kind: Kind = pieces.next().ok_or(Unreadable)?.parse()?;
        let stamp: Stamp = pieces.next().ok_or(Unreadable)?.parse()?;
        let bytes = BASE64
            .decode(pieces.next().unwrap_or("").trim())
            .map_err(|_| Unreadable)?;
        let clip = Self::new(kind, bytes);
        (clip.stamp == stamp).then_some(clip).ok_or(Unreadable)
    }
}

/// What names a clip: its kind and its stamp, with nothing of it.
///
/// Read on its own wherever the bytes are somewhere else: the file this
/// computer's helper writes is two of them, a line that names and a file
/// that holds, so that a service looking three times a second reads a
/// line and not a picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Head {
    pub kind: Kind,
    pub stamp: Stamp,
}

impl Head {
    /// Reads the two words at the head of something, whatever follows.
    pub fn at_the_head(said: &str) -> Result<Self, Unreadable> {
        let mut words = said.split_whitespace();
        let kind = words.next().ok_or(Unreadable)?.parse()?;
        let stamp = words.next().ok_or(Unreadable)?.parse()?;
        Ok(Self { kind, stamp })
    }

    /// Whether those bytes are the ones this head names.
    ///
    /// Asked of every clip read from a file, because the two halves are
    /// written one after the other and a reader can arrive between them.
    /// A head that does not match its bytes is not a fault, it is a
    /// reading taken a moment too early: it is skipped, and the turn
    /// after has both halves.
    pub fn matches(&self, bytes: &[u8]) -> bool {
        Clip::new(self.kind, bytes.to_vec()).stamp == self.stamp
    }
}

impl fmt::Display for Head {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.kind, self.stamp)
    }
}

impl FromStr for Head {
    type Err = Unreadable;

    fn from_str(said: &str) -> Result<Self, Self::Err> {
        Self::at_the_head(said)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deux_fois_la_meme_chose_porte_le_meme_nom() {
        // C'est tout ce qui fait tenir la fonction : sans ça, chaque tour
        // renverrait le presse-papiers à l'autre ordinateur pour rien, et
        // ce qui arrive repartirait aussitôt d'où il vient.
        assert_eq!(Clip::text("bonjour").stamp(), Clip::text("bonjour").stamp());
        assert_ne!(Clip::text("bonjour").stamp(), Clip::text("bonsoir").stamp());
    }

    #[test]
    fn un_texte_et_une_image_des_memes_octets_ne_sont_pas_la_meme_chose() {
        // L'espèce entre dans l'empreinte, sans quoi une image qui se
        // trouve avoir les octets d'un texte serait collée comme un
        // texte.
        let octets = b"PNG".to_vec();
        assert_ne!(
            Clip::new(Kind::Text, octets.clone()).stamp(),
            Clip::new(Kind::Picture, octets).stamp()
        );
    }

    #[test]
    fn un_clip_fait_l_aller_retour_par_le_tunnel() {
        for clip in [
            Clip::text("deux mots"),
            Clip::text(""),
            Clip::picture(vec![
                0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0xff, 0x00,
            ]),
        ] {
            let ecrit = clip.on_the_wire();
            assert_eq!(Clip::from_the_wire(&ecrit).unwrap(), clip);
        }
    }

    #[test]
    fn une_empreinte_s_ecrit_et_se_relit() {
        let stamp = Clip::text("bonjour").stamp();
        assert_eq!(stamp.to_string().parse::<Stamp>().unwrap(), stamp);
        assert_eq!(stamp.to_string().len(), 64);
    }

    #[test]
    fn la_tete_nomme_sans_porter() {
        let clip = Clip::picture(vec![1, 2, 3]);
        let head: Head = clip.named().parse().unwrap();
        assert_eq!(head.kind, Kind::Picture);
        assert_eq!(head.stamp, clip.stamp());
        assert!(head.matches(clip.bytes()));
        // Une tête lue entre les deux écritures ne correspond pas aux
        // octets encore en place : c'est ce qui la fait sauter ce tour-là
        // au lieu de coller la moitié de deux choses.
        assert!(!head.matches(&[1, 2]));
    }

    #[test]
    fn ce_qui_ne_dit_pas_ce_qu_il_porte_est_refuse() {
        assert!(Head::at_the_head("").is_err());
        assert!(Head::at_the_head("picture").is_err());
        assert!(Head::at_the_head("son 00").is_err());
        assert!(Clip::from_the_wire("text").is_err());
        assert!(
            Clip::from_the_wire(&format!("text {} pas-du-base64!", Clip::text("").stamp()))
                .is_err()
        );
    }

    #[test]
    fn le_plafond_se_lit_sur_le_clip() {
        assert!(!Clip::picture(vec![0; LARGEST]).too_large());
        assert!(Clip::picture(vec![0; LARGEST + 1]).too_large());
    }

    #[test]
    fn un_texte_se_relit_comme_du_texte_et_une_image_non() {
        assert_eq!(Clip::text("bonjour").said(), Some("bonjour"));
        assert_eq!(Clip::picture(vec![0xff, 0xfe]).said(), None);
    }
}
