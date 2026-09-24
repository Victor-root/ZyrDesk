//! What the service and the engines say to each other over the local
//! link.
//!
//! One message per link frame: a u8 kind, then the body. Strings are
//! UTF-8 behind their u16 length. Both ends are the same program, so a
//! kind one of them does not know is an error, not something to skip.
//!
//! Service to host engine:
//!
//! ```text
//! 1 Setup        datagram_budget u16 (the first message)
//! 2 Film         display string ("" for the main screen)
//! 3 Rate         kbps u32
//! 4 Steady       on u8
//! 5 DrawPointer  on u8
//! 6 Stop
//! ```
//!
//! Host engine to service:
//!
//! ```text
//! 1 Ready     encodable u8, encoders string, displays
//! 2 Filming   display string, width u32, height u32
//! 3 Displays  displays
//! 4 Trouble   text string
//! displays: count u16, then each: id string, main u8, width u32,
//!           height u32, name string
//! ```
//!
//! Service to player, on the client:
//!
//! ```text
//! 1 Tunnel  rtt_us u32, relayed u8
//! ```

use crate::codec::CodecSet;
use crate::wire::{Reader, WireError, put_text};

/// A screen the host engine can film.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Display {
    /// The monitor's device path, stable across restarts, or its GDI
    /// device name when Windows gives none.
    pub id: String,
    pub main: bool,
    pub width: u32,
    pub height: u32,
    /// What the person calls it.
    pub name: String,
}

/// What the service tells the host engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToEngine {
    /// The largest datagram the tunnel carries after its channel byte.
    Setup {
        datagram_budget: u16,
    },
    /// Which screen to film: a [`Display::id`], or "" for the main one.
    Film {
        display: String,
    },
    /// The live ceiling on the bitrate.
    Rate {
        kbps: u32,
    },
    /// Whether a still screen is sent again at the full rate.
    Steady {
        on: bool,
    },
    /// Whether the host draws its pointer into the picture.
    DrawPointer {
        on: bool,
    },
    Stop,
}

/// What the host engine tells the service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToService {
    /// Sent once the encoders are probed.
    Ready {
        encodable: CodecSet,
        /// The encoders found, by name, separated by spaces.
        encoders: String,
        displays: Vec<Display>,
    },
    /// Sent each time the capture is aimed at a screen.
    Filming {
        display: String,
        width: u32,
        height: u32,
    },
    /// Sent when the screens change.
    Displays(Vec<Display>),
    /// A sentence in French, for the journal.
    Trouble { text: String },
}

/// What the service tells the player on the client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToPlayer {
    /// How the tunnel stands, every second.
    Tunnel { rtt_us: u32, relayed: bool },
}

const SETUP: u8 = 1;
const FILM: u8 = 2;
const RATE: u8 = 3;
const STEADY: u8 = 4;
const DRAW_POINTER: u8 = 5;
const STOP: u8 = 6;

impl ToEngine {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        match self {
            ToEngine::Setup { datagram_budget } => {
                out.push(SETUP);
                out.extend_from_slice(&datagram_budget.to_le_bytes());
            }
            ToEngine::Film { display } => {
                out.push(FILM);
                put_text(&mut out, display);
            }
            ToEngine::Rate { kbps } => {
                out.push(RATE);
                out.extend_from_slice(&kbps.to_le_bytes());
            }
            ToEngine::Steady { on } => out.extend_from_slice(&[STEADY, u8::from(*on)]),
            ToEngine::DrawPointer { on } => out.extend_from_slice(&[DRAW_POINTER, u8::from(*on)]),
            ToEngine::Stop => out.push(STOP),
        }
        out
    }

    pub fn decode(frame: &[u8]) -> Result<Self, WireError> {
        let mut reader = Reader::new(frame);
        let message = match reader.u8()? {
            SETUP => ToEngine::Setup {
                datagram_budget: reader.u16()?,
            },
            FILM => ToEngine::Film {
                display: reader.text("display")?,
            },
            RATE => ToEngine::Rate {
                kbps: reader.u32()?,
            },
            STEADY => ToEngine::Steady {
                on: reader.flag("on")?,
            },
            DRAW_POINTER => ToEngine::DrawPointer {
                on: reader.flag("on")?,
            },
            STOP => ToEngine::Stop,
            other => return Err(WireError::Kind(other)),
        };
        reader.finish()?;
        Ok(message)
    }
}

const READY: u8 = 1;
const FILMING: u8 = 2;
const DISPLAYS: u8 = 3;
const TROUBLE: u8 = 4;

impl ToService {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        match self {
            ToService::Ready {
                encodable,
                encoders,
                displays,
            } => {
                out.extend_from_slice(&[READY, encodable.wire()]);
                put_text(&mut out, encoders);
                put_displays(&mut out, displays);
            }
            ToService::Filming {
                display,
                width,
                height,
            } => {
                out.push(FILMING);
                put_text(&mut out, display);
                out.extend_from_slice(&width.to_le_bytes());
                out.extend_from_slice(&height.to_le_bytes());
            }
            ToService::Displays(displays) => {
                out.push(DISPLAYS);
                put_displays(&mut out, displays);
            }
            ToService::Trouble { text } => {
                out.push(TROUBLE);
                put_text(&mut out, text);
            }
        }
        out
    }

    pub fn decode(frame: &[u8]) -> Result<Self, WireError> {
        let mut reader = Reader::new(frame);
        let message = match reader.u8()? {
            READY => ToService::Ready {
                encodable: CodecSet::from_wire(reader.u8()?),
                encoders: reader.text("encoders")?,
                displays: read_displays(&mut reader)?,
            },
            FILMING => ToService::Filming {
                display: reader.text("display")?,
                width: reader.u32()?,
                height: reader.u32()?,
            },
            DISPLAYS => ToService::Displays(read_displays(&mut reader)?),
            TROUBLE => ToService::Trouble {
                text: reader.text("text")?,
            },
            other => return Err(WireError::Kind(other)),
        };
        reader.finish()?;
        Ok(message)
    }
}

const TUNNEL: u8 = 1;

impl ToPlayer {
    pub fn encode(&self) -> Vec<u8> {
        match self {
            ToPlayer::Tunnel { rtt_us, relayed } => {
                let mut out = vec![TUNNEL];
                out.extend_from_slice(&rtt_us.to_le_bytes());
                out.push(u8::from(*relayed));
                out
            }
        }
    }

    pub fn decode(frame: &[u8]) -> Result<Self, WireError> {
        let mut reader = Reader::new(frame);
        let message = match reader.u8()? {
            TUNNEL => ToPlayer::Tunnel {
                rtt_us: reader.u32()?,
                relayed: reader.flag("relayed")?,
            },
            other => return Err(WireError::Kind(other)),
        };
        reader.finish()?;
        Ok(message)
    }
}

/// Writes a list of screens; one past the 65 535th would not be counted,
/// and is left out.
fn put_displays(out: &mut Vec<u8>, displays: &[Display]) {
    let count = u16::try_from(displays.len()).unwrap_or(u16::MAX);
    out.extend_from_slice(&count.to_le_bytes());
    for display in &displays[..usize::from(count)] {
        put_text(out, &display.id);
        out.push(u8::from(display.main));
        out.extend_from_slice(&display.width.to_le_bytes());
        out.extend_from_slice(&display.height.to_le_bytes());
        put_text(out, &display.name);
    }
}

fn read_displays(reader: &mut Reader<'_>) -> Result<Vec<Display>, WireError> {
    let count = reader.u16()?;
    // Nothing reserved from the count: one the frame cannot hold stops
    // at the first screen missing.
    let mut displays = Vec::new();
    for _ in 0..count {
        displays.push(Display {
            id: reader.text("id")?,
            main: reader.flag("main")?,
            width: reader.u32()?,
            height: reader.u32()?,
            name: reader.text("name")?,
        });
    }
    Ok(displays)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::VideoCodec;
    use crate::testing::Noise;

    fn displays() -> Vec<Display> {
        vec![
            Display {
                id: r"MONITOR\GSM5B7F\{4d36e96e-e325-11ce-bfc1-08002be10318}\0003".to_owned(),
                main: true,
                width: 3840,
                height: 2160,
                name: "LG ULTRAFINE".to_owned(),
            },
            Display {
                id: r"\\.\DISPLAY3".to_owned(),
                main: false,
                width: 1920,
                height: 1080,
                name: "Écran virtuel".to_owned(),
            },
        ]
    }

    fn to_engine() -> Vec<ToEngine> {
        vec![
            ToEngine::Setup {
                datagram_budget: 1161,
            },
            ToEngine::Film {
                display: String::new(),
            },
            ToEngine::Film {
                display: displays()[1].id.clone(),
            },
            ToEngine::Rate { kbps: 150_000 },
            ToEngine::Steady { on: true },
            ToEngine::Steady { on: false },
            ToEngine::DrawPointer { on: true },
            ToEngine::Stop,
        ]
    }

    fn to_service() -> Vec<ToService> {
        vec![
            ToService::Ready {
                encodable: CodecSet::empty()
                    .with(VideoCodec::H264)
                    .with(VideoCodec::Hevc),
                encoders: "h264_nvenc hevc_nvenc".to_owned(),
                displays: displays(),
            },
            ToService::Ready {
                encodable: CodecSet::empty(),
                encoders: String::new(),
                displays: Vec::new(),
            },
            ToService::Filming {
                display: displays()[0].id.clone(),
                width: 3840,
                height: 2160,
            },
            ToService::Displays(displays()),
            ToService::Trouble {
                text: "La capture de l'écran a échoué".to_owned(),
            },
        ]
    }

    fn to_player() -> Vec<ToPlayer> {
        vec![
            ToPlayer::Tunnel {
                rtt_us: 12_345,
                relayed: true,
            },
            ToPlayer::Tunnel {
                rtt_us: u32::MAX,
                relayed: false,
            },
        ]
    }

    #[test]
    fn every_message_makes_the_round_trip() {
        for message in to_engine() {
            assert_eq!(ToEngine::decode(&message.encode()).unwrap(), message);
        }
        for message in to_service() {
            assert_eq!(ToService::decode(&message.encode()).unwrap(), message);
        }
        for message in to_player() {
            assert_eq!(ToPlayer::decode(&message.encode()).unwrap(), message);
        }
    }

    fn refuses_every_other_length<M>(
        messages: &[M],
        encode: fn(&M) -> Vec<u8>,
        decode: fn(&[u8]) -> Result<M, WireError>,
    ) {
        for message in messages {
            let frame = encode(message);
            for len in 0..frame.len() {
                assert!(decode(&frame[..len]).is_err(), "{frame:?} cut at {len}");
            }
            let mut longer = frame.clone();
            longer.push(0);
            assert_eq!(decode(&longer).err(), Some(WireError::TooLong));
        }
    }

    #[test]
    fn every_shortened_or_lengthened_message_is_refused() {
        refuses_every_other_length(&to_engine(), ToEngine::encode, ToEngine::decode);
        refuses_every_other_length(&to_service(), ToService::encode, ToService::decode);
        refuses_every_other_length(&to_player(), ToPlayer::encode, ToPlayer::decode);
    }

    #[test]
    fn unknown_kinds_and_impossible_values_are_named() {
        assert_eq!(ToEngine::decode(&[99]), Err(WireError::Kind(99)));
        assert_eq!(ToService::decode(&[0]), Err(WireError::Kind(0)));
        assert_eq!(ToPlayer::decode(&[2]), Err(WireError::Kind(2)));
        assert_eq!(
            ToEngine::decode(&[STEADY, 2]),
            Err(WireError::Invalid("on"))
        );
        assert_eq!(
            ToService::decode(&[TROUBLE, 2, 0, 0xff, 0xfe]),
            Err(WireError::Invalid("text"))
        );
        assert_eq!(
            ToService::decode(&[DISPLAYS, 0xff, 0xff]),
            Err(WireError::Truncated)
        );
    }

    #[test]
    fn garbage_never_panics() {
        let mut noise = Noise::new(40);
        let valid: Vec<Vec<u8>> = to_engine()
            .iter()
            .map(ToEngine::encode)
            .chain(to_service().iter().map(ToService::encode))
            .chain(to_player().iter().map(ToPlayer::encode))
            .collect();
        for _ in 0..100_000 {
            let bytes = noise.garbage(&valid);
            let _ = ToEngine::decode(&bytes);
            let _ = ToService::decode(&bytes);
            let _ = ToPlayer::decode(&bytes);
        }
    }
}
