//! The shape a picture has while it sits on a clipboard.
//!
//! Windows carries a picture there as a « packed bitmap »: a header
//! saying how wide, how high and how many bits to a pixel, then whatever
//! the header announced, then the pixels. Nothing in it is a file and
//! nothing in it is compressed, which is why a screenshot of a large
//! screen weighs eight million bytes there and a few hundred thousand
//! once it is a PNG.
//!
//! Only the arithmetic lives here, and it is the arithmetic that is worth
//! testing: where the pixels begin in what a clipboard hands over, and
//! what a header has to say for a picture to go back on it. Reading and
//! writing the clipboard itself is Windows', and the imaging that turns
//! one shape into the other is Windows' too.

// Outside Windows nothing calls this: there is no clipboard to reach
// there. It stays compiled and tested all the same, having nothing
// platform-specific about it, and it is the half where a mistake would
// be a picture of coloured noise rather than a refusal.
#![cfg_attr(not(windows), allow(dead_code))]

/// What a header says when the three colour channels are named by masks
/// rather than taken in the usual order.
///
/// Its own name because it decides where the pixels begin: a header of
/// the old size that says this has three more numbers after it.
const MASKED: u32 = 3;

/// The size of the oldest header a clipboard still hands over.
const OLDEST_HEADER: usize = 40;

/// How many bytes a colour takes in the table some headers carry.
const A_COLOUR: usize = 4;

/// Where the pixels begin in a packed bitmap, whatever shape its header
/// has.
///
/// Three things can sit between the header and the pixels, and any of
/// them being missed puts every colour off by a few bytes, which is a
/// picture of coloured noise rather than a picture: the three masks that
/// follow an old header naming its channels, and the table of colours a
/// picture of 256 shades or fewer carries.
///
/// `None` is something that does not describe a picture at all, which is
/// left alone rather than guessed at.
pub fn where_the_pixels_start(dib: &[u8]) -> Option<usize> {
    let header = a_number(dib, 0)? as usize;
    if header < OLDEST_HEADER || header > dib.len() {
        return None;
    }
    let bits = u16::from_le_bytes(dib.get(14..16)?.try_into().ok()?);
    let compression = a_number(dib, 16)?;
    let used = a_number(dib, 32)? as usize;

    // A picture of few enough shades carries the shades themselves. The
    // header says how many when it is not all of them, and nought means
    // all of them.
    let colours = if used > 0 {
        used
    } else if bits <= 8 {
        1usize << bits
    } else {
        0
    };
    // The masks sit after the header only when the header is too old to
    // hold them; the later ones carry their own.
    let masks = if compression == MASKED && header == OLDEST_HEADER {
        12
    } else {
        0
    };
    let start = header.checked_add(masks)?.checked_add(colours * A_COLOUR)?;
    (start < dib.len()).then_some(start)
}

fn a_number(dib: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(dib.get(at..at + 4)?.try_into().ok()?))
}

/// How many bytes of the head of a packed bitmap this writes.
const NEWEST_HEADER: usize = 124;

// Where each of the things a header says sits in it. The newest of the
// headers is written here, because it is the only one that names the
// see-through channel: the older ones carry the same four bytes a pixel
// and say nothing at all about the fourth, so every program has to guess,
// and most guess opaque. Windows works those older shapes out from this
// one by itself, so putting this one on a clipboard puts all of them
// there.
const SIZE: usize = 0;
const WIDTH: usize = 4;
const HEIGHT: usize = 8;
const PLANES: usize = 12;
const BITS: usize = 14;
const COMPRESSION: usize = 16;
const IMAGE_SIZE: usize = 20;
const RED: usize = 40;
const GREEN: usize = 44;
const BLUE: usize = 48;
const ALPHA: usize = 52;
const COLOUR_SPACE: usize = 56;
const INTENT: usize = 108;

/// The colour space every screen and every PNG already means: sRGB, named
/// the way a header names one, by its four letters.
const SRGB: u32 = 0x7352_4742;

/// What the picture is for, out of the four a header may say: pictures,
/// as opposed to charts, proofs and screens of text.
const FOR_PICTURES: u32 = 4;

/// A packed bitmap of those pixels, ready to be put on a clipboard.
///
/// The pixels come in as four bytes each, blue, green, red and
/// see-through, the first row at the top, which is how the imaging hands
/// them over. They go out the other way up: a bitmap counts its rows from
/// the bottom, and a picture pasted upside down is the oldest mistake
/// there is to make here.
pub fn a_packed_bitmap(wide: u32, high: u32, rows: &[u8]) -> Vec<u8> {
    let stride = wide as usize * 4;
    let mut out = vec![0u8; NEWEST_HEADER];
    put(&mut out, SIZE, NEWEST_HEADER as u32);
    put(&mut out, WIDTH, wide);
    put(&mut out, HEIGHT, high);
    out[PLANES..PLANES + 2].copy_from_slice(&1u16.to_le_bytes());
    out[BITS..BITS + 2].copy_from_slice(&32u16.to_le_bytes());
    put(&mut out, COMPRESSION, MASKED);
    put(&mut out, IMAGE_SIZE, (stride * high as usize) as u32);
    put(&mut out, RED, 0x00FF_0000);
    put(&mut out, GREEN, 0x0000_FF00);
    put(&mut out, BLUE, 0x0000_00FF);
    put(&mut out, ALPHA, 0xFF00_0000);
    put(&mut out, COLOUR_SPACE, SRGB);
    put(&mut out, INTENT, FOR_PICTURES);

    out.reserve(stride * high as usize);
    for row in rows.chunks_exact(stride).rev() {
        out.extend_from_slice(row);
    }
    out
}

fn put(out: &mut [u8], at: usize, number: u32) {
    out[at..at + 4].copy_from_slice(&number.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A header of the old size, said in the order a bitmap says it.
    fn old_header(bits: u16, compression: u32, used: u32) -> Vec<u8> {
        let mut head = vec![0u8; OLDEST_HEADER];
        head[0..4].copy_from_slice(&(OLDEST_HEADER as u32).to_le_bytes());
        head[4..8].copy_from_slice(&4i32.to_le_bytes());
        head[8..12].copy_from_slice(&4i32.to_le_bytes());
        head[12..14].copy_from_slice(&1u16.to_le_bytes());
        head[14..16].copy_from_slice(&bits.to_le_bytes());
        head[16..20].copy_from_slice(&compression.to_le_bytes());
        head[32..36].copy_from_slice(&used.to_le_bytes());
        head
    }

    fn with_pixels(mut head: Vec<u8>, how_many: usize) -> Vec<u8> {
        head.extend(std::iter::repeat_n(0u8, how_many));
        head
    }

    #[test]
    fn une_capture_ordinaire_commence_juste_apres_son_entete() {
        // Le cas de tous les jours : trente-deux bits par pixel, pas de
        // table de couleurs, pas de masques.
        let dib = with_pixels(old_header(32, 0, 0), 64);
        assert_eq!(where_the_pixels_start(&dib), Some(OLDEST_HEADER));
    }

    #[test]
    fn les_masques_repoussent_les_pixels_de_douze_octets() {
        // Sans ça, chaque couleur est décalée de trois pixels et l'image
        // devient du bruit coloré.
        let dib = with_pixels(old_header(32, MASKED, 0), 64);
        assert_eq!(where_the_pixels_start(&dib), Some(OLDEST_HEADER + 12));
    }

    #[test]
    fn une_table_de_couleurs_les_repousse_d_autant() {
        // Une image de 256 nuances porte ses 256 nuances entre l'entête
        // et les pixels, même quand l'entête n'en compte aucune.
        let dib = with_pixels(old_header(8, 0, 0), 256 * A_COLOUR + 16);
        assert_eq!(where_the_pixels_start(&dib), Some(OLDEST_HEADER + 256 * 4));
        let dib = with_pixels(old_header(8, 0, 16), 16 * A_COLOUR + 16);
        assert_eq!(where_the_pixels_start(&dib), Some(OLDEST_HEADER + 16 * 4));
    }

    #[test]
    fn un_entete_recent_porte_ses_masques_en_lui() {
        // Le même dessin qu'au-dessus mais avec l'entête le plus récent :
        // les douze octets ne sont plus derrière lui, ils sont dedans.
        let mut head = vec![0u8; NEWEST_HEADER];
        head[0..4].copy_from_slice(&(NEWEST_HEADER as u32).to_le_bytes());
        head[14..16].copy_from_slice(&32u16.to_le_bytes());
        head[16..20].copy_from_slice(&MASKED.to_le_bytes());
        let dib = with_pixels(head, 64);
        assert_eq!(where_the_pixels_start(&dib), Some(NEWEST_HEADER));
    }

    #[test]
    fn ce_qui_ne_decrit_pas_une_image_est_laisse_tranquille() {
        assert_eq!(where_the_pixels_start(&[]), None);
        // Un entête plus court qu'aucun entête connu.
        assert_eq!(
            where_the_pixels_start(&with_pixels(vec![8, 0, 0, 0], 40)),
            None
        );
        // Un entête plus long que ce qui a été remis.
        let mut tronque = old_header(32, 0, 0);
        tronque.truncate(20);
        assert_eq!(where_the_pixels_start(&tronque), None);
        // Un entête qui annonce une table de couleurs plus grande que
        // tout ce qui a été remis : il ne reste aucun pixel derrière.
        assert_eq!(
            where_the_pixels_start(&with_pixels(old_header(8, 0, 4096), 16)),
            None
        );
    }

    #[test]
    fn une_image_rendue_au_presse_papiers_est_retournee() {
        // Deux rangées d'un pixel, la première en haut : elles doivent
        // ressortir dans l'autre sens, un bitmap comptant ses rangées
        // depuis le bas. C'est l'erreur classique de cet endroit.
        let haut = [1, 2, 3, 255];
        let bas = [4, 5, 6, 255];
        let packed = a_packed_bitmap(1, 2, &[haut, bas].concat());
        assert_eq!(&packed[NEWEST_HEADER..NEWEST_HEADER + 4], &bas);
        assert_eq!(&packed[NEWEST_HEADER + 4..NEWEST_HEADER + 8], &haut);
    }

    #[test]
    fn ce_qui_est_ecrit_se_relit_par_la_lecture_d_a_cote() {
        // Les deux moitiés de ce fichier doivent se répondre : ce qu'on
        // pose au presse-papiers doit se relire comme on lit ce qu'on y
        // trouve.
        let packed = a_packed_bitmap(2, 2, &[0u8; 16]);
        assert_eq!(where_the_pixels_start(&packed), Some(NEWEST_HEADER));
        assert_eq!(packed.len(), NEWEST_HEADER + 16);
    }
}
