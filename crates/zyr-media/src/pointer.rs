//! What the far pointer looks like, when a program draws it for itself.
//!
//! The computer watching draws its own pointer, where the hand is, in the
//! shape the far one has: the service names that shape, and the system
//! there draws it at its own size. A pointer that a program draws for
//! itself (a page that plays a finger, a brush, a sight) has no name, so
//! the engine that films it carries its picture instead, as Windows hands
//! it over. Only then: a pointer that has a name keeps travelling as its
//! name, and the engine only says that it has one.
//!
//! A picture may be larger than a control message, so it travels in
//! pieces, one after the other on the ordered stream.

use crate::control::{MAX_CONTROL_MESSAGE, ToPlayer};
use crate::wire::{Reader, WireError};

/// The longest side of a pointer picture. Windows draws none larger, and
/// a picture said to be larger is not a pointer.
pub const LONGEST_SIDE: u16 = 512;

/// The three forms Windows hands a pointer in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerKind {
    /// One bit a pixel: an AND mask, then an XOR mask.
    Monochrome,
    /// Blue, green, red and alpha.
    Color,
    /// Blue, green, red, and an alpha that is either 0 (paint the colour)
    /// or 255 (flip the screen's bits with it).
    MaskedColor,
}

impl PointerKind {
    fn wire(self) -> u8 {
        match self {
            PointerKind::Monochrome => 1,
            PointerKind::Color => 2,
            PointerKind::MaskedColor => 3,
        }
    }

    fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(PointerKind::Monochrome),
            2 => Some(PointerKind::Color),
            3 => Some(PointerKind::MaskedColor),
            _ => None,
        }
    }
}

/// A pointer as Windows hands it over, its rows one after the other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PointerPicture {
    pub kind: PointerKind,
    pub width: u16,
    /// Rows of the pointer as it shows. A black and white one carries
    /// twice as many: its AND mask, then its XOR mask.
    pub height: u16,
    /// The pixel that points, from the top left corner.
    pub hot_x: u16,
    pub hot_y: u16,
    /// How wide the screen it points at is, in the same pixels: how large
    /// the computer watching draws it follows from that.
    pub screen_width: u16,
    /// [`PointerPicture::row_bytes`] a row, with nothing between rows.
    pub bytes: Vec<u8>,
}

impl PointerPicture {
    /// The picture of a pointer of `kind`, `width` by `height` as Windows
    /// says them (a black and white one counting both its masks), its rows
    /// `pitch` bytes apart in `bytes`, pointing at `hot`, on a screen
    /// `screen_width` wide. Nothing if the bytes cannot hold it, or if it
    /// is larger than a pointer is.
    pub fn from_rows(
        kind: PointerKind,
        width: u32,
        height: u32,
        pitch: u32,
        hot: (u32, u32),
        screen_width: u32,
        bytes: &[u8],
    ) -> Option<Self> {
        let shown = match kind {
            PointerKind::Monochrome => height / 2,
            PointerKind::Color | PointerKind::MaskedColor => height,
        };
        let side = |value: u32| {
            u16::try_from(value)
                .ok()
                .filter(|side| (1..=LONGEST_SIDE).contains(side))
        };
        let mut picture = Self {
            kind,
            width: side(width)?,
            height: side(shown)?,
            hot_x: u16::try_from(hot.0).unwrap_or(u16::MAX),
            hot_y: u16::try_from(hot.1).unwrap_or(u16::MAX),
            screen_width: u16::try_from(screen_width).unwrap_or(u16::MAX),
            bytes: Vec::new(),
        };
        let (row, rows, pitch) = (picture.row_bytes(), picture.rows_carried(), pitch as usize);
        if pitch < row || bytes.len() < pitch * (rows - 1) + row {
            return None;
        }
        picture.bytes = (0..rows)
            .flat_map(|y| &bytes[y * pitch..y * pitch + row])
            .copied()
            .collect();
        Some(picture)
    }

    /// Bytes in a row.
    pub fn row_bytes(&self) -> usize {
        let width = usize::from(self.width);
        match self.kind {
            PointerKind::Monochrome => width.div_ceil(8),
            PointerKind::Color | PointerKind::MaskedColor => width * 4,
        }
    }

    /// Rows carried: twice those shown for a black and white pointer.
    fn rows_carried(&self) -> usize {
        let rows = usize::from(self.height);
        match self.kind {
            PointerKind::Monochrome => rows * 2,
            PointerKind::Color | PointerKind::MaskedColor => rows,
        }
    }

    fn write(&self, out: &mut Vec<u8>) {
        out.push(self.kind.wire());
        for field in [
            self.width,
            self.height,
            self.hot_x,
            self.hot_y,
            self.screen_width,
        ] {
            out.extend_from_slice(&field.to_le_bytes());
        }
        out.extend_from_slice(&self.bytes);
    }

    fn read(bytes: &[u8]) -> Result<Self, WireError> {
        let mut reader = Reader::new(bytes);
        let kind = PointerKind::from_wire(reader.u8()?).ok_or(WireError::Invalid("pointer"))?;
        let mut side = || {
            reader.u16().and_then(|side| match side {
                1..=LONGEST_SIDE => Ok(side),
                _ => Err(WireError::Invalid("pointer size")),
            })
        };
        let (width, height) = (side()?, side()?);
        let mut picture = Self {
            kind,
            width,
            height,
            hot_x: reader.u16()?,
            hot_y: reader.u16()?,
            screen_width: reader.u16()?,
            bytes: Vec::new(),
        };
        let rest = reader.rest();
        let expected = picture.row_bytes() * picture.rows_carried();
        if rest.len() != expected {
            return Err(if rest.len() < expected {
                WireError::Truncated
            } else {
                WireError::TooLong
            });
        }
        picture.bytes = rest.to_vec();
        Ok(picture)
    }
}

/// What the far pointer looks like.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PointerLook {
    /// One of the system's own shapes, which the computer watching draws
    /// itself, by the name the service carries.
    Named,
    /// A shape a program drew for itself, which nothing but its picture
    /// says.
    Drawn(PointerPicture),
}

/// Most bytes of a picture a piece carries: what a control message holds
/// once its kind, the total and the offset are in.
const PIECE: usize = MAX_CONTROL_MESSAGE - 9;

/// The most bytes a whole picture takes: its fields, then the largest
/// pointer of four bytes a pixel.
const LARGEST_PICTURE: usize = 11 + LONGEST_SIDE as usize * LONGEST_SIDE as usize * 4;

impl PointerLook {
    /// The control messages that say it, in order.
    pub fn messages(&self) -> Vec<ToPlayer> {
        match self {
            PointerLook::Named => vec![ToPlayer::PointerNamed],
            PointerLook::Drawn(picture) => {
                let mut whole = Vec::new();
                picture.write(&mut whole);
                let total = whole.len() as u32;
                whole
                    .chunks(PIECE)
                    .enumerate()
                    .map(|(index, piece)| ToPlayer::PointerPiece {
                        total,
                        offset: (index * PIECE) as u32,
                        bytes: piece.to_vec(),
                    })
                    .collect()
            }
        }
    }
}

/// The pieces of a picture, gathered as they arrive.
///
/// The stream is ordered and a picture's pieces leave one after the
/// other: a piece at offset 0 starts a new picture, whatever was being
/// gathered, and any other piece follows the one before it or is refused.
#[derive(Debug, Default)]
pub struct Gathering {
    bytes: Vec<u8>,
}

impl Gathering {
    /// Takes a piece in, and gives the picture once it is whole.
    pub fn take(
        &mut self,
        total: u32,
        offset: u32,
        piece: &[u8],
    ) -> Result<Option<PointerPicture>, WireError> {
        if offset == 0 {
            self.bytes.clear();
        }
        let total = total as usize;
        if offset as usize != self.bytes.len()
            || total > LARGEST_PICTURE
            || self.bytes.len() + piece.len() > total
        {
            self.bytes.clear();
            return Err(WireError::Invalid("pointer piece"));
        }
        self.bytes.extend_from_slice(piece);
        if self.bytes.len() < total {
            return Ok(None);
        }
        let whole = std::mem::take(&mut self.bytes);
        PointerPicture::read(&whole).map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::ControlReader;

    /// A colour pointer of that size, every pixel told apart.
    fn coloured(width: u16, height: u16) -> PointerPicture {
        let bytes = (0..usize::from(width) * usize::from(height) * 4)
            .map(|at| (at % 251) as u8)
            .collect();
        PointerPicture {
            kind: PointerKind::Color,
            width,
            height,
            hot_x: 3,
            hot_y: 5,
            screen_width: 1920,
            bytes,
        }
    }

    /// What a player reads of `look`, sent on a control stream.
    fn told(look: &PointerLook) -> Vec<Option<PointerPicture>> {
        let mut stream = Vec::new();
        for message in look.messages() {
            message.write(&mut stream);
        }
        let mut reader = ControlReader::<ToPlayer>::new();
        reader.feed(&stream);
        let mut gathering = Gathering::default();
        let mut read = Vec::new();
        for message in reader {
            match message.unwrap() {
                ToPlayer::PointerNamed => read.push(None),
                ToPlayer::PointerPiece {
                    total,
                    offset,
                    bytes,
                } => {
                    if let Some(picture) = gathering.take(total, offset, &bytes).unwrap() {
                        read.push(Some(picture));
                    }
                }
                other => panic!("not a pointer: {other:?}"),
            }
        }
        read
    }

    #[test]
    fn a_picture_larger_than_a_message_arrives_whole_in_pieces() {
        let picture = coloured(128, 128);
        let look = PointerLook::Drawn(picture.clone());
        assert!(look.messages().len() > 1);
        assert_eq!(told(&look), vec![Some(picture)]);
        assert_eq!(told(&PointerLook::Named), vec![None]);
    }

    #[test]
    fn the_largest_pointer_fits_and_one_larger_is_refused() {
        let largest = coloured(LONGEST_SIDE, LONGEST_SIDE);
        let look = PointerLook::Drawn(largest.clone());
        assert_eq!(told(&look), vec![Some(largest)]);

        let rows = |height: usize| vec![0; 4 * 4 * height];
        let fits = PointerPicture::from_rows(PointerKind::Color, 4, 512, 16, (0, 0), 0, &rows(512));
        assert!(fits.is_some());
        let tall = PointerPicture::from_rows(PointerKind::Color, 4, 513, 16, (0, 0), 0, &rows(513));
        assert_eq!(tall, None);
    }

    #[test]
    fn rows_are_taken_without_what_pads_them() {
        // Two pixels a row, padded to twelve bytes.
        let bytes = [
            1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 0, 9, 10, 11, 12, 13, 14, 15, 16,
        ];
        let picture =
            PointerPicture::from_rows(PointerKind::Color, 2, 2, 12, (1, 1), 800, &bytes).unwrap();
        assert_eq!(picture.bytes, (1..=16).collect::<Vec<u8>>());
        assert_eq!(
            (picture.hot_x, picture.hot_y, picture.screen_width),
            (1, 1, 800)
        );
    }

    #[test]
    fn a_black_and_white_pointer_shows_half_the_rows_it_carries() {
        // 8 pixels wide, AND mask then XOR mask, two rows each.
        let bytes = [0xf0, 0x0f, 0xaa, 0x55];
        let picture =
            PointerPicture::from_rows(PointerKind::Monochrome, 8, 4, 1, (0, 0), 0, &bytes).unwrap();
        assert_eq!(picture.height, 2);
        assert_eq!(picture.bytes, bytes);
        let look = PointerLook::Drawn(picture.clone());
        assert_eq!(told(&look), vec![Some(picture)]);
    }

    #[test]
    fn bytes_that_cannot_hold_the_pointer_are_refused() {
        let from = |kind, width, height, pitch, len| {
            PointerPicture::from_rows(kind, width, height, pitch, (0, 0), 0, &vec![0; len])
        };
        assert_eq!(from(PointerKind::Color, 4, 4, 16, 63), None);
        assert_eq!(from(PointerKind::Color, 4, 4, 8, 64), None);
        assert_eq!(from(PointerKind::Monochrome, 8, 4, 1, 3), None);
        assert_eq!(from(PointerKind::Monochrome, 0, 4, 1, 4), None);
        assert_eq!(from(PointerKind::Monochrome, 8, 1, 1, 4), None);
        assert!(from(PointerKind::Monochrome, 8, 4, 1, 4).is_some());
        // The last row needs no padding after it.
        assert!(from(PointerKind::Color, 4, 2, 20, 36).is_some());
    }

    #[test]
    fn a_piece_out_of_turn_is_refused_and_the_next_picture_still_read() {
        let picture = coloured(128, 128);
        let messages = PointerLook::Drawn(picture.clone()).messages();
        let piece = |message: &ToPlayer| match message {
            ToPlayer::PointerPiece {
                total,
                offset,
                bytes,
            } => (*total, *offset, bytes.clone()),
            other => panic!("not a piece: {other:?}"),
        };
        let mut gathering = Gathering::default();
        let (total, offset, bytes) = piece(&messages[1]);
        assert_eq!(
            gathering.take(total, offset, &bytes),
            Err(WireError::Invalid("pointer piece"))
        );
        // A picture left halfway is given up for the one that starts.
        let (total, offset, bytes) = piece(&messages[0]);
        assert_eq!(gathering.take(total, offset, &bytes), Ok(None));
        let mut whole = None;
        for message in &messages {
            let (total, offset, bytes) = piece(message);
            whole = gathering.take(total, offset, &bytes).unwrap();
        }
        assert_eq!(whole, Some(picture));
        // More than a pointer can weigh is never gathered.
        assert_eq!(
            gathering.take(LARGEST_PICTURE as u32 + 1, 0, &[0]),
            Err(WireError::Invalid("pointer piece"))
        );
    }

    #[test]
    fn a_picture_that_says_more_or_less_than_it_carries_is_refused() {
        let mut whole = Vec::new();
        coloured(2, 2).write(&mut whole);
        assert!(PointerPicture::read(&whole).is_ok());
        assert_eq!(
            PointerPicture::read(&whole[..whole.len() - 1]),
            Err(WireError::Truncated)
        );
        whole.push(0);
        assert_eq!(PointerPicture::read(&whole), Err(WireError::TooLong));
        whole[0] = 9;
        assert_eq!(
            PointerPicture::read(&whole),
            Err(WireError::Invalid("pointer"))
        );
    }
}
