//! What one clipboard hands to the other.
//!
//! A clipboard holds one thing at a time, and the product carries three
//! kinds of it: what somebody selected in a text, what somebody copied of
//! a picture, and files.
//!
//! Files are not like the other two, and the difference is the whole of
//! how they work. Copying a file has never put the file on a clipboard,
//! on any Windows that ever shipped: it puts the names of files that live
//! on that machine's disks. So what crosses here is those names and how
//! heavy each one is, which weighs nothing at all however many gigabytes
//! they stand for. The bytes follow later, and only if somebody pastes.
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
    /// Files, as the list of them and not one byte of what is in them.
    ///
    /// The one kind whose clip is not the thing itself. Copying a file
    /// never put the file on a clipboard, on any Windows that ever
    /// shipped: it puts the names of files that live on that machine's
    /// own disks. What crosses here is those names and how heavy each
    /// one is, which weighs nothing however many gigabytes they stand
    /// for; the bytes follow later, and only if somebody pastes.
    Files,
}

impl Kind {
    fn spelled(self) -> &'static str {
        match self {
            Kind::Text => "text",
            Kind::Picture => "picture",
            Kind::Files => "files",
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
            "files" => Ok(Kind::Files),
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

    /// A clip that names files without carrying any of them.
    pub fn files(listed: &Listing) -> Self {
        Self::new(Kind::Files, listed.written().into_bytes())
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
            Kind::Picture | Kind::Files => None,
        }
    }

    /// The files it names, when it names any.
    pub fn listing(&self) -> Option<Listing> {
        match self.kind {
            Kind::Files => Listing::read(std::str::from_utf8(&self.bytes).ok()?).ok(),
            Kind::Text | Kind::Picture => None,
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
            // What the names stand for and never what they weigh: a list
            // of a hundred gigabytes is a few hundred bytes here, and
            // saying those bytes would say nothing anybody wants.
            Kind::Files => match self.listing() {
                Some(listed) => listed.in_words(),
                None => "a list of files that says nothing".to_string(),
            },
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

/// One file of a listing: where it goes, and how heavy it is.
///
/// The path is relative and never the one it had on the machine it was
/// copied from: `D:\\Photos\\2026\\lac.jpg` copied with its folder
/// travels as `2026/lac.jpg`, and the other computer decides for itself
/// where that lands. An absolute path would name a disk that may not
/// exist over there, and a path climbing out of its folder would name a
/// place nobody asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    path: String,
    bytes: u64,
}

impl Listed {
    /// A file of a listing, or nothing when its path is not one this
    /// product will write.
    ///
    /// Refused rather than mended: a path that climbs out of its folder,
    /// or names a disk, is either a mistake or an attempt, and the two
    /// are answered the same way. It is the far computer that hands this
    /// over, and the far computer is where a name is chosen.
    pub fn new(path: &str, bytes: u64) -> Option<Self> {
        let path = path.replace('\\', "/");
        let clean = !path.is_empty()
            && !path.starts_with('/')
            && !path.contains(':')
            && !path.split('/').any(|piece| {
                piece.is_empty() || piece == "." || piece == ".." || piece.ends_with(' ')
            });
        clean.then_some(Self { path, bytes })
    }

    /// Where it goes, under whatever folder the far computer chose, with
    /// `/` between its pieces.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Its name alone, without the folders above it.
    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    pub fn bytes(&self) -> u64 {
        self.bytes
    }
}

/// The files a clipboard names, in the order they were copied.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Listing(Vec<Listed>);

impl Listing {
    pub fn of(files: Vec<Listed>) -> Self {
        Self(files)
    }

    pub fn files(&self) -> &[Listed] {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// What the whole of it weighs.
    pub fn whole(&self) -> u64 {
        self.0.iter().map(|file| file.bytes).sum()
    }

    /// One file by its rank in the list, which is how the two computers
    /// name a file to each other.
    ///
    /// By rank and never by path: a rank is a number that cannot be made
    /// to mean another file, where a path handed back by the far computer
    /// would be a path this one then has to check all over again.
    pub fn at(&self, rank: usize) -> Option<&Listed> {
        self.0.get(rank)
    }

    /// The listing as it travels and as it is written down: one file to a
    /// line, its weight first because that is the fixed-width half, then
    /// the path, which takes the whole of what is left and may hold
    /// spaces.
    pub fn written(&self) -> String {
        let mut out = String::new();
        for file in &self.0 {
            out.push_str(&format!("{} {}\n", file.bytes, file.path));
        }
        out
    }

    /// Reads what the line above wrote.
    ///
    /// A line that does not say what it is supposed to costs the whole
    /// listing and not just that line: half a folder pasted as though it
    /// were the whole of it is worse than nothing pasted at all.
    pub fn read(said: &str) -> Result<Self, Unreadable> {
        let mut files = Vec::new();
        for line in said.lines().filter(|line| !line.trim().is_empty()) {
            let (weight, path) = line.split_once(' ').ok_or(Unreadable)?;
            let bytes = weight.parse().map_err(|_| Unreadable)?;
            files.push(Listed::new(path, bytes).ok_or(Unreadable)?);
        }
        Ok(Self(files))
    }

    /// What it is, in the words the service writes its journal in.
    pub fn in_words(&self) -> String {
        format!(
            "{} file{}, {}",
            self.0.len(),
            if self.0.len() == 1 { "" } else { "s" },
            weighed(self.whole())
        )
    }
}

/// A weight in the largest unit that leaves it above one, which is how a
/// person reads one.
///
/// In the units Windows itself shows, powers of a thousand and not of
/// 1024: a product that says a file is smaller than the Explorer says it
/// is has an argument with the Explorer that it cannot win.
pub fn weighed(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["o", "ko", "Mo", "Go", "To"];
    let mut left = bytes as f64;
    let mut unit = 0;
    while left >= 1000.0 && unit + 1 < UNITS.len() {
        left /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        return format!("{bytes} o");
    }
    format!("{left:.1} {}", UNITS[unit])
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

    fn listee(path: &str, bytes: u64) -> Listed {
        Listed::new(path, bytes).unwrap()
    }

    #[test]
    fn une_liste_de_fichiers_fait_l_aller_retour() {
        let listing = Listing::of(vec![
            listee("lac.jpg", 2_400_000),
            listee("2026/été au bord de l'eau.png", 940),
            listee("un dossier/un fichier avec des espaces.txt", 0),
        ]);
        let clip = Clip::files(&listing);
        assert_eq!(clip.kind(), Kind::Files);
        assert_eq!(clip.listing().unwrap(), listing);
        // Et il traverse le tunnel comme les deux autres espèces.
        assert_eq!(Clip::from_the_wire(&clip.on_the_wire()).unwrap(), clip);
    }

    #[test]
    fn une_liste_pese_ce_que_ses_fichiers_pesent_et_non_ce_qu_elle_pese() {
        // C'est tout l'intérêt : cent gigaoctets nommés tiennent en
        // quelques centaines d'octets, et rien ne bouge tant que
        // personne ne colle.
        let listing = Listing::of(vec![
            listee("gros.iso", 80_000_000_000),
            listee("encore.iso", 20_000_000_000),
        ]);
        assert_eq!(listing.whole(), 100_000_000_000);
        assert!(Clip::files(&listing).bytes().len() < 100);
        assert_eq!(listing.in_words(), "2 files, 100.0 Go");
        assert_eq!(
            Listing::of(vec![listee("seul.txt", 3)]).in_words(),
            "1 file, 3 o"
        );
    }

    #[test]
    fn un_chemin_qui_sort_de_son_dossier_est_refuse() {
        // C'est la machine d'en face qui remet ces noms, et un nom est
        // une chose qu'on choisit : sans ça, coller un dossier pourrait
        // écrire n'importe où sur ce disque-ci.
        assert!(Listed::new("../ailleurs.txt", 1).is_none());
        assert!(Listed::new("dossier/../../ailleurs.txt", 1).is_none());
        assert!(Listed::new("/racine.txt", 1).is_none());
        assert!(Listed::new("C:/Windows/System32/rien.dll", 1).is_none());
        assert!(Listed::new("", 1).is_none());
        assert!(Listed::new("dossier//vide.txt", 1).is_none());
        // Et une liste entière tombe avec une seule de ses lignes : une
        // moitié de dossier collée comme si c'était le tout est pire que
        // rien du tout.
        assert!(Listing::read("3 ../ailleurs.txt").is_err());
        assert!(Listing::read("pas-un-nombre fichier.txt").is_err());
        assert!(Listing::read("3").is_err());
    }

    #[test]
    fn un_chemin_arrive_avec_des_barres_obliques_dans_un_seul_sens() {
        // Windows écrit ses chemins avec l'autre barre, et les deux
        // ordinateurs doivent nommer le même fichier pareil.
        let file = listee(r"2026\lac.jpg", 12);
        assert_eq!(file.path(), "2026/lac.jpg");
        assert_eq!(file.name(), "lac.jpg");
    }

    #[test]
    fn un_poids_se_lit_dans_l_unite_ou_il_veut_dire_quelque_chose() {
        assert_eq!(weighed(0), "0 o");
        assert_eq!(weighed(999), "999 o");
        assert_eq!(weighed(1_000), "1.0 ko");
        assert_eq!(weighed(94_000), "94.0 ko");
        assert_eq!(weighed(1_500_000), "1.5 Mo");
        assert_eq!(weighed(4_700_000_000), "4.7 Go");
    }

    #[test]
    fn un_texte_se_relit_comme_du_texte_et_une_image_non() {
        assert_eq!(Clip::text("bonjour").said(), Some("bonjour"));
        assert_eq!(Clip::picture(vec![0xff, 0xfe]).said(), None);
    }
}
