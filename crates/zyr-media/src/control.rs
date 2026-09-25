//! The control stream between the player and the engine.
//!
//! Reliable and ordered, carried by one stream of the tunnel. Each
//! message is framed on the byte stream as a u16 length (of what
//! follows), a u8 kind and the body. The length says how much to pass
//! over, so a kind this build does not know is skipped rather than
//! refused: a newer player can say more to an older engine without
//! breaking the conversation.
//!
//! Player to engine:
//!
//! ```text
//! 1  Hello    version u16, wanted, decodable u8 (codec set)
//! 2  Change   wanted
//! 3  Recover  stream u16, frame u32 (the last frame delivered)
//! 4  Key      scan code u8, flags u8 (bit0 extended, bit1 down)
//! 5  PointerAt  x u16, y u16
//! 6  PointerBy  dx i16, dy i16
//! 7  Button   button u8, down u8
//! 8  Wheel    vertical i16, horizontal i16
//! 9  ReleaseAll
//! 10 Ping     sent_us u64
//! 11 Bye
//! wanted: width u16, height u16, fps u16, bitrate_kbps u32, codec u8,
//!         flags u8 (bit0 draw pointer, bit1 audio, bit2 steady)
//! ```
//!
//! Engine to player:
//!
//! ```text
//! 1  Welcome    version u16, encodable u8, display width u16, display height u16
//! 2  Streaming  stream u16, codec u8, width u16, height u16, fps u16
//! 3  Pong       sent_us u64 (echoed), host_us u64
//! 4  Notice     kind u8, text (UTF-8, the rest of the body)
//! 5  Bye        reason u8
//! 6  Still      stream u16, frame u32 (the last frame sent)
//! ```

use std::marker::PhantomData;

use crate::MEDIA_VERSION;
use crate::codec::{CodecChoice, CodecSet, VideoCodec};
use crate::input::{Button, InputEvent};
use crate::wire::{Reader, WireError, clipped};

/// Longest message on the stream, kind and body, as its length says.
pub const MAX_CONTROL_MESSAGE: usize = 16 * 1024;

/// What the person asked for, as the player tells the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wanted {
    pub width: u16,
    pub height: u16,
    pub fps: u16,
    pub bitrate_kbps: u32,
    pub codec: CodecChoice,
    /// The host draws its pointer into the picture.
    pub draw_pointer: bool,
    pub audio: bool,
    /// A still screen is sent again at the full rate.
    pub steady: bool,
}

const DRAW_POINTER: u8 = 1;
const AUDIO: u8 = 2;
const STEADY: u8 = 4;

impl Wanted {
    fn write(&self, out: &mut Vec<u8>) {
        let flags = (u8::from(self.draw_pointer) * DRAW_POINTER)
            | (u8::from(self.audio) * AUDIO)
            | (u8::from(self.steady) * STEADY);
        out.extend_from_slice(&self.width.to_le_bytes());
        out.extend_from_slice(&self.height.to_le_bytes());
        out.extend_from_slice(&self.fps.to_le_bytes());
        out.extend_from_slice(&self.bitrate_kbps.to_le_bytes());
        out.push(self.codec.wire());
        out.push(flags);
    }

    fn read(reader: &mut Reader<'_>) -> Result<Self, WireError> {
        let width = reader.u16()?;
        let height = reader.u16()?;
        let fps = reader.u16()?;
        let bitrate_kbps = reader.u32()?;
        let codec = CodecChoice::from_wire(reader.u8()?).ok_or(WireError::Invalid("codec"))?;
        let flags = reader.u8()?;
        Ok(Self {
            width,
            height,
            fps,
            bitrate_kbps,
            codec,
            draw_pointer: flags & DRAW_POINTER != 0,
            audio: flags & AUDIO != 0,
            steady: flags & STEADY != 0,
        })
    }
}

/// What the player says to the engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToEngine {
    /// The first message. One of another version is read as
    /// [`WireError::Version`], its body being another format.
    Hello {
        version: u16,
        wanted: Wanted,
        decodable: CodecSet,
    },
    /// What the person asks for changed, mid-session.
    Change {
        wanted: Wanted,
    },
    /// Asks for a key frame: `frame` is the last frame delivered.
    Recover {
        stream: u16,
        frame: u32,
    },
    Input(InputEvent),
    /// `sent_us` on the player's clock.
    Ping {
        sent_us: u64,
    },
    Bye,
}

/// Why the engine ends a session.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByeReason {
    Asked = 1,
    ServiceStop = 2,
    Fatal = 3,
}

impl ByeReason {
    /// The value written on the wire.
    pub fn wire(self) -> u8 {
        self as u8
    }

    pub fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(ByeReason::Asked),
            2 => Some(ByeReason::ServiceStop),
            3 => Some(ByeReason::Fatal),
            _ => None,
        }
    }
}

/// What a notice is about, for the client to file it.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    NoEncoder = 1,
    CaptureTrouble = 2,
    EncoderTrouble = 3,
    AudioTrouble = 4,
    DisplayChanged = 5,
}

impl NoticeKind {
    /// The value written on the wire.
    pub fn wire(self) -> u8 {
        self as u8
    }

    pub fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(NoticeKind::NoEncoder),
            2 => Some(NoticeKind::CaptureTrouble),
            3 => Some(NoticeKind::EncoderTrouble),
            4 => Some(NoticeKind::AudioTrouble),
            5 => Some(NoticeKind::DisplayChanged),
            _ => None,
        }
    }
}

/// What the engine says to the player.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToPlayer {
    /// The answer to Hello. One of another version is read as
    /// [`WireError::Version`].
    Welcome {
        version: u16,
        encodable: CodecSet,
        display_width: u16,
        display_height: u16,
    },
    /// Sent before the first packet of each new stream; the picture keeps
    /// the display's shape, so it may be smaller than asked.
    Streaming {
        stream: u16,
        codec: VideoCodec,
        width: u16,
        height: u16,
        fps: u16,
    },
    /// `sent_us` echoed from the ping, `host_us` on the engine's clock
    /// when it answered.
    Pong {
        sent_us: u64,
        host_us: u64,
    },
    /// A sentence in French, for the person or the journal. One too long
    /// for a message is cut at a character.
    Notice {
        kind: NoticeKind,
        text: String,
    },
    Bye {
        reason: ByeReason,
    },
    /// The screen has not changed since that frame, the last one sent,
    /// and nothing is sent again while it stays still.
    Still {
        stream: u16,
        frame: u32,
    },
}

/// A message a [`ControlReader`] can read.
pub trait ControlMessage: Sized {
    /// The message of that kind with that body, or `None` for a kind this
    /// build does not know.
    fn read(kind: u8, body: &[u8]) -> Result<Option<Self>, WireError>;
}

const HELLO: u8 = 1;
const CHANGE: u8 = 2;
const RECOVER: u8 = 3;
const KEY: u8 = 4;
const POINTER_AT: u8 = 5;
const POINTER_BY: u8 = 6;
const BUTTON: u8 = 7;
const WHEEL: u8 = 8;
const RELEASE_ALL: u8 = 9;
const PING: u8 = 10;
const ENGINE_BYE: u8 = 11;

const KEY_EXTENDED: u8 = 1;
const KEY_DOWN: u8 = 2;

impl ToEngine {
    /// Appends the message, framed, to what `out` holds.
    pub fn write(&self, out: &mut Vec<u8>) {
        match self {
            ToEngine::Hello {
                version,
                wanted,
                decodable,
            } => framed(out, HELLO, |out| {
                out.extend_from_slice(&version.to_le_bytes());
                wanted.write(out);
                out.push(decodable.wire());
            }),
            ToEngine::Change { wanted } => framed(out, CHANGE, |out| wanted.write(out)),
            ToEngine::Recover { stream, frame } => framed(out, RECOVER, |out| {
                out.extend_from_slice(&stream.to_le_bytes());
                out.extend_from_slice(&frame.to_le_bytes());
            }),
            ToEngine::Input(event) => write_input(event, out),
            ToEngine::Ping { sent_us } => framed(out, PING, |out| {
                out.extend_from_slice(&sent_us.to_le_bytes())
            }),
            ToEngine::Bye => framed(out, ENGINE_BYE, |_| {}),
        }
    }
}

fn write_input(event: &InputEvent, out: &mut Vec<u8>) {
    match *event {
        InputEvent::Key {
            scancode,
            extended,
            down,
        } => framed(out, KEY, |out| {
            let flags = (u8::from(extended) * KEY_EXTENDED) | (u8::from(down) * KEY_DOWN);
            out.extend_from_slice(&[scancode, flags]);
        }),
        InputEvent::PointerAt { x, y } => framed(out, POINTER_AT, |out| {
            out.extend_from_slice(&x.to_le_bytes());
            out.extend_from_slice(&y.to_le_bytes());
        }),
        InputEvent::PointerBy { dx, dy } => framed(out, POINTER_BY, |out| {
            out.extend_from_slice(&dx.to_le_bytes());
            out.extend_from_slice(&dy.to_le_bytes());
        }),
        InputEvent::Button { button, down } => framed(out, BUTTON, |out| {
            out.extend_from_slice(&[button.wire(), u8::from(down)]);
        }),
        InputEvent::Wheel {
            vertical,
            horizontal,
        } => framed(out, WHEEL, |out| {
            out.extend_from_slice(&vertical.to_le_bytes());
            out.extend_from_slice(&horizontal.to_le_bytes());
        }),
        InputEvent::ReleaseAll => framed(out, RELEASE_ALL, |_| {}),
    }
}

impl ControlMessage for ToEngine {
    fn read(kind: u8, body: &[u8]) -> Result<Option<Self>, WireError> {
        let mut reader = Reader::new(body);
        let message = match kind {
            HELLO => {
                let version = reader.u16()?;
                if version != MEDIA_VERSION {
                    return Err(WireError::Version(version));
                }
                ToEngine::Hello {
                    version,
                    wanted: Wanted::read(&mut reader)?,
                    decodable: CodecSet::from_wire(reader.u8()?),
                }
            }
            CHANGE => ToEngine::Change {
                wanted: Wanted::read(&mut reader)?,
            },
            RECOVER => ToEngine::Recover {
                stream: reader.u16()?,
                frame: reader.u32()?,
            },
            KEY => {
                let scancode = reader.u8()?;
                let flags = reader.u8()?;
                ToEngine::Input(InputEvent::Key {
                    scancode,
                    extended: flags & KEY_EXTENDED != 0,
                    down: flags & KEY_DOWN != 0,
                })
            }
            POINTER_AT => ToEngine::Input(InputEvent::PointerAt {
                x: reader.u16()?,
                y: reader.u16()?,
            }),
            POINTER_BY => ToEngine::Input(InputEvent::PointerBy {
                dx: reader.i16()?,
                dy: reader.i16()?,
            }),
            BUTTON => ToEngine::Input(InputEvent::Button {
                button: Button::from_wire(reader.u8()?).ok_or(WireError::Invalid("button"))?,
                down: reader.flag("down")?,
            }),
            WHEEL => ToEngine::Input(InputEvent::Wheel {
                vertical: reader.i16()?,
                horizontal: reader.i16()?,
            }),
            RELEASE_ALL => ToEngine::Input(InputEvent::ReleaseAll),
            PING => ToEngine::Ping {
                sent_us: reader.u64()?,
            },
            ENGINE_BYE => ToEngine::Bye,
            _ => return Ok(None),
        };
        reader.finish()?;
        Ok(Some(message))
    }
}

const WELCOME: u8 = 1;
const STREAMING: u8 = 2;
const PONG: u8 = 3;
const NOTICE: u8 = 4;
const PLAYER_BYE: u8 = 5;
const STILL: u8 = 6;

impl ToPlayer {
    /// Appends the message, framed, to what `out` holds.
    pub fn write(&self, out: &mut Vec<u8>) {
        match self {
            ToPlayer::Welcome {
                version,
                encodable,
                display_width,
                display_height,
            } => framed(out, WELCOME, |out| {
                out.extend_from_slice(&version.to_le_bytes());
                out.push(encodable.wire());
                out.extend_from_slice(&display_width.to_le_bytes());
                out.extend_from_slice(&display_height.to_le_bytes());
            }),
            ToPlayer::Streaming {
                stream,
                codec,
                width,
                height,
                fps,
            } => framed(out, STREAMING, |out| {
                out.extend_from_slice(&stream.to_le_bytes());
                out.push(codec.wire());
                out.extend_from_slice(&width.to_le_bytes());
                out.extend_from_slice(&height.to_le_bytes());
                out.extend_from_slice(&fps.to_le_bytes());
            }),
            ToPlayer::Pong { sent_us, host_us } => framed(out, PONG, |out| {
                out.extend_from_slice(&sent_us.to_le_bytes());
                out.extend_from_slice(&host_us.to_le_bytes());
            }),
            ToPlayer::Notice { kind, text } => framed(out, NOTICE, |out| {
                out.push(kind.wire());
                // What the length leaves once kind and notice kind are in.
                out.extend_from_slice(clipped(text, MAX_CONTROL_MESSAGE - 2).as_bytes());
            }),
            ToPlayer::Bye { reason } => framed(out, PLAYER_BYE, |out| out.push(reason.wire())),
            ToPlayer::Still { stream, frame } => framed(out, STILL, |out| {
                out.extend_from_slice(&stream.to_le_bytes());
                out.extend_from_slice(&frame.to_le_bytes());
            }),
        }
    }
}

impl ControlMessage for ToPlayer {
    fn read(kind: u8, body: &[u8]) -> Result<Option<Self>, WireError> {
        let mut reader = Reader::new(body);
        let message = match kind {
            WELCOME => {
                let version = reader.u16()?;
                if version != MEDIA_VERSION {
                    return Err(WireError::Version(version));
                }
                ToPlayer::Welcome {
                    version,
                    encodable: CodecSet::from_wire(reader.u8()?),
                    display_width: reader.u16()?,
                    display_height: reader.u16()?,
                }
            }
            STREAMING => ToPlayer::Streaming {
                stream: reader.u16()?,
                codec: VideoCodec::from_wire(reader.u8()?).ok_or(WireError::Invalid("codec"))?,
                width: reader.u16()?,
                height: reader.u16()?,
                fps: reader.u16()?,
            },
            PONG => ToPlayer::Pong {
                sent_us: reader.u64()?,
                host_us: reader.u64()?,
            },
            NOTICE => {
                let kind =
                    NoticeKind::from_wire(reader.u8()?).ok_or(WireError::Invalid("notice"))?;
                let text = std::str::from_utf8(reader.rest())
                    .map_err(|_| WireError::Invalid("text"))?
                    .to_owned();
                return Ok(Some(ToPlayer::Notice { kind, text }));
            }
            PLAYER_BYE => ToPlayer::Bye {
                reason: ByeReason::from_wire(reader.u8()?).ok_or(WireError::Invalid("reason"))?,
            },
            STILL => ToPlayer::Still {
                stream: reader.u16()?,
                frame: reader.u32()?,
            },
            _ => return Ok(None),
        };
        reader.finish()?;
        Ok(Some(message))
    }
}

/// Appends one message: its length, its kind, and the body `body` writes,
/// which is never longer than a message may be (the only body that could,
/// a notice's, is cut to fit).
fn framed(out: &mut Vec<u8>, kind: u8, body: impl FnOnce(&mut Vec<u8>)) {
    let start = out.len();
    out.extend_from_slice(&[0, 0, kind]);
    body(out);
    let len = (out.len() - start - 2) as u16;
    out[start..start + 2].copy_from_slice(&len.to_le_bytes());
}

/// Gathers the bytes of the stream, in whatever pieces they come, and
/// gives back whole messages.
///
/// Feed what the stream gave, then take messages until `None`. An error
/// for one message leaves the others readable, except
/// [`WireError::Framing`]: a length out of bounds means the stream no
/// longer says where messages start, nothing more is read from it, and
/// it has to be closed.
pub struct ControlReader<M> {
    buffer: Vec<u8>,
    /// Where the bytes not yet read start in `buffer`.
    start: usize,
    broken: bool,
    skipped: u64,
    message: PhantomData<fn() -> M>,
}

impl<M: ControlMessage> Default for ControlReader<M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M: ControlMessage> ControlReader<M> {
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            start: 0,
            broken: false,
            skipped: 0,
            message: PhantomData,
        }
    }

    pub fn feed(&mut self, chunk: &[u8]) {
        if self.broken {
            return;
        }
        self.buffer.drain(..self.start);
        self.start = 0;
        self.buffer.extend_from_slice(chunk);
    }

    /// Messages of a kind this build does not know, passed over.
    pub fn skipped(&self) -> u64 {
        self.skipped
    }
}

impl<M: ControlMessage> Iterator for ControlReader<M> {
    type Item = Result<M, WireError>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.broken {
                return None;
            }
            let at = self.start;
            let length = self.buffer.get(at..at + 2)?;
            let len = usize::from(u16::from_le_bytes([length[0], length[1]]));
            if len == 0 || len > MAX_CONTROL_MESSAGE {
                self.broken = true;
                self.buffer = Vec::new();
                self.start = 0;
                return Some(Err(WireError::Framing));
            }
            let end = at + 2 + len;
            if self.buffer.len() < end {
                return None;
            }
            self.start = end;
            match M::read(self.buffer[at + 2], &self.buffer[at + 3..end]) {
                Ok(Some(message)) => return Some(Ok(message)),
                Ok(None) => self.skipped += 1,
                Err(e) => return Some(Err(e)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::Noise;

    fn wanted() -> Wanted {
        Wanted {
            width: 2560,
            height: 1440,
            fps: 144,
            bitrate_kbps: 80_000,
            codec: CodecChoice::Hevc,
            draw_pointer: false,
            audio: true,
            steady: true,
        }
    }

    fn to_engine() -> Vec<ToEngine> {
        let decodable = CodecSet::empty()
            .with(VideoCodec::H264)
            .with(VideoCodec::Hevc);
        vec![
            ToEngine::Hello {
                version: MEDIA_VERSION,
                wanted: wanted(),
                decodable,
            },
            ToEngine::Change {
                wanted: Wanted {
                    draw_pointer: true,
                    audio: false,
                    steady: false,
                    codec: CodecChoice::Auto,
                    ..wanted()
                },
            },
            ToEngine::Recover {
                stream: 3,
                frame: u32::MAX,
            },
            ToEngine::Input(InputEvent::Key {
                scancode: 0x1d,
                extended: true,
                down: true,
            }),
            ToEngine::Input(InputEvent::Key {
                scancode: 0x1d,
                extended: false,
                down: false,
            }),
            ToEngine::Input(InputEvent::PointerAt { x: 65535, y: 0 }),
            ToEngine::Input(InputEvent::PointerBy {
                dx: -32768,
                dy: 32767,
            }),
            ToEngine::Input(InputEvent::Button {
                button: Button::X2,
                down: true,
            }),
            ToEngine::Input(InputEvent::Wheel {
                vertical: -120,
                horizontal: 240,
            }),
            ToEngine::Input(InputEvent::ReleaseAll),
            ToEngine::Ping {
                sent_us: u64::MAX - 7,
            },
            ToEngine::Bye,
        ]
    }

    fn to_player() -> Vec<ToPlayer> {
        vec![
            ToPlayer::Welcome {
                version: MEDIA_VERSION,
                encodable: CodecSet::empty().with(VideoCodec::Av1),
                display_width: 3840,
                display_height: 2160,
            },
            ToPlayer::Streaming {
                stream: 9,
                codec: VideoCodec::Av1,
                width: 1920,
                height: 1080,
                fps: 60,
            },
            ToPlayer::Pong {
                sent_us: 1,
                host_us: 2,
            },
            ToPlayer::Notice {
                kind: NoticeKind::DisplayChanged,
                text: "L'écran a changé de définition".to_owned(),
            },
            ToPlayer::Notice {
                kind: NoticeKind::NoEncoder,
                text: String::new(),
            },
            ToPlayer::Bye {
                reason: ByeReason::Fatal,
            },
            ToPlayer::Still {
                stream: 7,
                frame: u32::MAX,
            },
        ]
    }

    fn stream<M>(messages: &[M], write: fn(&M, &mut Vec<u8>)) -> Vec<u8> {
        let mut out = Vec::new();
        for message in messages {
            write(message, &mut out);
        }
        out
    }

    fn read_all<M: ControlMessage>(reader: &mut ControlReader<M>) -> Vec<M> {
        reader.map(Result::unwrap).collect()
    }

    #[test]
    fn every_message_makes_the_round_trip() {
        let messages = to_engine();
        let mut reader = ControlReader::<ToEngine>::new();
        reader.feed(&stream(&messages, ToEngine::write));
        assert_eq!(read_all(&mut reader), messages);

        let messages = to_player();
        let mut reader = ControlReader::<ToPlayer>::new();
        reader.feed(&stream(&messages, ToPlayer::write));
        assert_eq!(read_all(&mut reader), messages);
    }

    #[test]
    fn wire_values_name_the_right_notice_and_reason() {
        for kind in [
            NoticeKind::NoEncoder,
            NoticeKind::CaptureTrouble,
            NoticeKind::EncoderTrouble,
            NoticeKind::AudioTrouble,
            NoticeKind::DisplayChanged,
        ] {
            assert_eq!(NoticeKind::from_wire(kind.wire()), Some(kind));
        }
        for reason in [ByeReason::Asked, ByeReason::ServiceStop, ByeReason::Fatal] {
            assert_eq!(ByeReason::from_wire(reason.wire()), Some(reason));
        }
        assert_eq!(NoticeKind::from_wire(0), None);
        assert_eq!(ByeReason::from_wire(4), None);
    }

    #[test]
    fn every_input_event_has_its_own_kind() {
        let mut kinds: Vec<u8> = to_engine()
            .iter()
            .filter(|m| matches!(m, ToEngine::Input(_)))
            .map(|m| {
                let mut out = Vec::new();
                m.write(&mut out);
                out[2]
            })
            .collect();
        kinds.dedup();
        assert_eq!(kinds, vec![4, 5, 6, 7, 8, 9]);
    }

    #[test]
    fn messages_split_anywhere_come_out_whole() {
        let messages = to_engine();
        let bytes = stream(&messages, ToEngine::write);
        for split in 0..=bytes.len() {
            let mut reader = ControlReader::<ToEngine>::new();
            let mut read = Vec::new();
            reader.feed(&bytes[..split]);
            read.extend(read_all(&mut reader));
            reader.feed(&bytes[split..]);
            read.extend(read_all(&mut reader));
            assert_eq!(read, messages, "split at {split}");
        }

        let messages = to_player();
        let bytes = stream(&messages, ToPlayer::write);
        let mut reader = ControlReader::<ToPlayer>::new();
        let mut read = Vec::new();
        for byte in &bytes {
            reader.feed(std::slice::from_ref(byte));
            read.extend(read_all(&mut reader));
        }
        assert_eq!(read, messages);
        assert!(reader.buffer.is_empty() || reader.start == reader.buffer.len());
    }

    #[test]
    fn an_unknown_kind_is_skipped() {
        let mut bytes = Vec::new();
        framed(&mut bytes, 200, |out| out.extend_from_slice(&[1, 2, 3]));
        ToEngine::Bye.write(&mut bytes);
        let mut reader = ControlReader::<ToEngine>::new();
        reader.feed(&bytes);
        assert_eq!(read_all(&mut reader), vec![ToEngine::Bye]);
        assert_eq!(reader.skipped(), 1);
    }

    #[test]
    fn a_bad_message_is_refused_and_the_next_still_read() {
        let mut bytes = Vec::new();
        framed(&mut bytes, BUTTON, |out| out.extend_from_slice(&[9, 1]));
        framed(&mut bytes, BUTTON, |out| out.extend_from_slice(&[1, 2]));
        framed(&mut bytes, PING, |out| out.extend_from_slice(&[1, 2, 3]));
        framed(&mut bytes, ENGINE_BYE, |out| out.push(0));
        ToEngine::Bye.write(&mut bytes);
        let mut reader = ControlReader::<ToEngine>::new();
        reader.feed(&bytes);
        let read: Vec<_> = reader.collect();
        assert_eq!(
            read,
            vec![
                Err(WireError::Invalid("button")),
                Err(WireError::Invalid("down")),
                Err(WireError::Truncated),
                Err(WireError::TooLong),
                Ok(ToEngine::Bye),
            ]
        );
    }

    #[test]
    fn another_version_says_so() {
        let (mut hello, mut welcome) = (Vec::new(), Vec::new());
        framed(&mut hello, HELLO, |out| {
            out.extend_from_slice(&[2, 0, 7, 7])
        });
        framed(&mut welcome, WELCOME, |out| out.extend_from_slice(&[9, 0]));
        let mut engine = ControlReader::<ToEngine>::new();
        engine.feed(&hello);
        assert_eq!(engine.next(), Some(Err(WireError::Version(2))));
        let mut player = ControlReader::<ToPlayer>::new();
        player.feed(&welcome);
        assert_eq!(player.next(), Some(Err(WireError::Version(9))));
    }

    #[test]
    fn a_length_out_of_bounds_breaks_the_stream_for_good() {
        for length in [0u16, (MAX_CONTROL_MESSAGE + 1) as u16] {
            let mut reader = ControlReader::<ToEngine>::new();
            reader.feed(&length.to_le_bytes());
            assert_eq!(reader.next(), Some(Err(WireError::Framing)));
            let mut bye = Vec::new();
            ToEngine::Bye.write(&mut bye);
            reader.feed(&bye);
            assert_eq!(reader.next(), None);
        }
    }

    #[test]
    fn a_notice_too_long_is_cut_to_fit() {
        let text = "é".repeat(MAX_CONTROL_MESSAGE);
        let mut bytes = Vec::new();
        ToPlayer::Notice {
            kind: NoticeKind::CaptureTrouble,
            text: text.clone(),
        }
        .write(&mut bytes);
        assert!(bytes.len() <= MAX_CONTROL_MESSAGE + 2);
        let mut reader = ControlReader::<ToPlayer>::new();
        reader.feed(&bytes);
        let Some(Ok(ToPlayer::Notice { text: read, .. })) = reader.next() else {
            panic!("no notice");
        };
        assert!(text.starts_with(&read) && read.len() >= MAX_CONTROL_MESSAGE - 3);
    }

    #[test]
    fn garbage_never_panics() {
        let mut noise = Noise::new(30);
        let valid: Vec<Vec<u8>> = to_engine()
            .iter()
            .map(|m| stream(std::slice::from_ref(m), ToEngine::write))
            .chain(
                to_player()
                    .iter()
                    .map(|m| stream(std::slice::from_ref(m), ToPlayer::write)),
            )
            .collect();
        for _ in 0..2_000 {
            let mut engine = ControlReader::<ToEngine>::new();
            let mut player = ControlReader::<ToPlayer>::new();
            for _ in 0..20 {
                let chunk = noise.garbage(&valid);
                engine.feed(&chunk);
                player.feed(&chunk);
                engine.by_ref().for_each(drop);
                player.by_ref().for_each(drop);
            }
        }
        for kind in 0..=u8::MAX {
            for _ in 0..50 {
                let len = noise.below(40);
                let body = noise.bytes(len);
                let _ = ToEngine::read(kind, &body);
                let _ = ToPlayer::read(kind, &body);
            }
        }
    }
}
