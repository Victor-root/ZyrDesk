//! What the service and the programs driving it say to each other.
//!
//! One message per line: a verb, then `key=value` fields separated by
//! spaces. The same shape as the files the product writes, and for the
//! same reason: a channel that can be read with the eyes is a channel
//! that can be diagnosed.
//!
//! Unknown fields are ignored rather than refused. A newer program
//! talking to an older service loses what the old one cannot do, instead
//! of failing outright, and an unknown verb says plainly which side is
//! behind.

use std::fmt;
use std::net::IpAddr;
use std::str::FromStr;

use zyr_proto::net::EnginePorts;
use zyr_transport::{Fingerprint, MediaProfile};

/// Version of this dialect.
///
/// It only ever grows, and the service announces it: two halves of the
/// product installed at different times must be able to say so rather
/// than misunderstand each other quietly.
pub const PROTOCOL: u32 = 1;

/// Identifies one way out, for as long as it stays open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WayId(pub u64);

impl fmt::Display for WayId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A line that did not say what it was supposed to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Malformed(pub String);

impl fmt::Display for Malformed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Malformed {}

/// What a program asks the service to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// What this computer is doing, and who it is.
    Standing,
    /// Opens a way to a remote computer, and keeps it open.
    Reach {
        host: String,
        peer: Fingerprint,
        media: MediaProfile,
    },
    /// Ties an open way to the process using it: the way closes on its
    /// own once that process is gone, whatever became of whoever asked.
    Hold { way: WayId, process: u32 },
    /// Closes a way.
    Release { way: WayId },
    /// The ZyrDesk computers seen on the local network.
    Peers,
}

impl Request {
    pub fn parse(line: &str) -> Result<Self, Malformed> {
        let (verb, rest) = split_verb(line);
        let fields = Fields(rest);
        match verb {
            "standing" => Ok(Request::Standing),
            "reach" => Ok(Request::Reach {
                host: fields.text("host")?.to_string(),
                peer: fields.parsed("peer")?,
                media: MediaProfile {
                    bits_per_second: u64::from(fields.parsed::<u32>("bitrate")?) * 1000,
                    frames_per_second: fields.parsed("fps")?,
                },
            }),
            "hold" => Ok(Request::Hold {
                way: WayId(fields.parsed("way")?),
                process: fields.parsed("process")?,
            }),
            "release" => Ok(Request::Release {
                way: WayId(fields.parsed("way")?),
            }),
            "peers" => Ok(Request::Peers),
            other => Err(Malformed(format!("verbe inconnu « {other} »"))),
        }
    }
}

impl fmt::Display for Request {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Request::Standing => f.write_str("standing"),
            Request::Reach { host, peer, media } => write!(
                f,
                "reach host={host} peer={peer} bitrate={} fps={}",
                media.bits_per_second / 1000,
                media.frames_per_second
            ),
            Request::Hold { way, process } => write!(f, "hold way={way} process={process}"),
            Request::Release { way } => write!(f, "release way={way}"),
            Request::Peers => f.write_str("peers"),
        }
    }
}

/// What this computer is doing, and who it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Standing {
    /// Dialect the service speaks.
    pub protocol: u32,
    /// What other computers pin to reach this one.
    pub fingerprint: Fingerprint,
    /// Whether this computer can be reached right now.
    pub hosting: bool,
    /// Ways out currently open.
    pub ways: usize,
}

/// Where a remote engine now appears, on this computer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reached {
    pub way: WayId,
    /// Local address standing in for the remote computer.
    pub address: IpAddr,
    /// Ports of the remote engine, mirrored on that address.
    pub engine: EnginePorts,
    /// Largest packet the path takes in one piece.
    pub packet: u16,
}

/// A ZyrDesk seen on the local network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Peer {
    /// Name its owner knows it by.
    pub name: String,
    pub fingerprint: Fingerprint,
    pub address: IpAddr,
    pub port: u16,
}

/// What the service answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    Standing(Standing),
    Reached(Reached),
    /// One computer of a list. The list ends on `Done`.
    Peer(Peer),
    /// Done, with nothing to report.
    Done,
    /// Not done, and why. The text is meant for the person, not the
    /// program: it is shown as it is.
    Refused(String),
}

impl Answer {
    pub fn parse(line: &str) -> Result<Self, Malformed> {
        let (verb, rest) = split_verb(line);
        let fields = Fields(rest);
        match verb {
            "standing" => Ok(Answer::Standing(Standing {
                protocol: fields.parsed("protocol")?,
                fingerprint: fields.parsed("fingerprint")?,
                hosting: fields.text("hosting")? == "yes",
                ways: fields.parsed("ways")?,
            })),
            "reached" => Ok(Answer::Reached(Reached {
                way: WayId(fields.parsed("way")?),
                address: fields.parsed("address")?,
                engine: EnginePorts::new(fields.parsed("base")?)
                    .map_err(|e| Malformed(e.to_string()))?,
                packet: fields.parsed("packet")?,
            })),
            "peer" => Ok(Answer::Peer(Peer {
                name: unpacked(fields.text("name")?),
                fingerprint: fields.parsed("fingerprint")?,
                address: fields.parsed("address")?,
                port: fields.parsed("port")?,
            })),
            "done" => Ok(Answer::Done),
            "no" => Ok(Answer::Refused(unfolded(rest.trim()))),
            other => Err(Malformed(format!("réponse inconnue « {other} »"))),
        }
    }
}

impl fmt::Display for Answer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Answer::Standing(standing) => write!(
                f,
                "standing protocol={} fingerprint={} hosting={} ways={}",
                standing.protocol,
                standing.fingerprint,
                if standing.hosting { "yes" } else { "no" },
                standing.ways
            ),
            Answer::Reached(reached) => write!(
                f,
                "reached way={} address={} base={} packet={}",
                reached.way,
                reached.address,
                reached.engine.base(),
                reached.packet
            ),
            Answer::Peer(peer) => write!(
                f,
                "peer name={} fingerprint={} address={} port={}",
                packed(&peer.name),
                peer.fingerprint,
                peer.address,
                peer.port
            ),
            Answer::Done => f.write_str("done"),
            // The reason travels on one line: a newline would be read as
            // the start of another message.
            Answer::Refused(reason) => write!(f, "no {}", folded(reason)),
        }
    }
}

/// Verb of a line, and everything after it.
fn split_verb(line: &str) -> (&str, &str) {
    let line = line.trim();
    match line.split_once(char::is_whitespace) {
        Some((verb, rest)) => (verb, rest),
        None => (line, ""),
    }
}

/// Packs a value so it survives inside a `key=value` field.
///
/// Spaces are what separate one field from the next, so a computer
/// called « PC de Victor » would otherwise be read as three fields and
/// lose everything after the first word.
fn packed(text: &str) -> String {
    text.replace('\\', r"\\")
        .replace(' ', r"\s")
        .replace('\n', r"\n")
}

/// Gives a packed value its spaces back.
fn unpacked(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pieces = text.chars();
    while let Some(c) = pieces.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match pieces.next() {
            Some('s') => out.push(' '),
            Some('n') => out.push('\n'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Folds a reason onto a single line.
///
/// Refusals are written for the person reading them, over several lines
/// where that helps. They still have to travel as one message, and come
/// out with their shape intact.
fn folded(text: &str) -> String {
    text.replace('\\', r"\\").replace('\n', r"\n")
}

/// Gives a folded reason its shape back.
fn unfolded(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pieces = text.chars();
    while let Some(c) = pieces.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match pieces.next() {
            Some('n') => out.push('\n'),
            Some('\\') => out.push('\\'),
            // Anything else was never folded by us: kept as it came,
            // since a reason is worth more slightly wrong than lost.
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Reads the `key=value` fields of a line.
struct Fields<'a>(&'a str);

impl<'a> Fields<'a> {
    fn text(&self, key: &str) -> Result<&'a str, Malformed> {
        self.0
            .split_whitespace()
            .filter_map(|piece| piece.split_once('='))
            .find(|(name, _)| *name == key)
            .map(|(_, value)| value)
            .ok_or_else(|| Malformed(format!("champ « {key} » absent")))
    }

    fn parsed<T: FromStr>(&self, key: &str) -> Result<T, Malformed> {
        self.text(key)?
            .parse()
            .map_err(|_| Malformed(format!("champ « {key} » illisible")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fingerprint() -> Fingerprint {
        "0829cc7ecb9e9ba53cd36e6f342268ddf3c8ef05a49d1d7944ac6332c89cf237"
            .parse()
            .unwrap()
    }

    fn every_request() -> Vec<Request> {
        vec![
            Request::Standing,
            Request::Reach {
                host: "192.168.1.20".to_string(),
                peer: fingerprint(),
                media: MediaProfile {
                    bits_per_second: 20_000_000,
                    frames_per_second: 60,
                },
            },
            Request::Hold {
                way: WayId(3),
                process: 11248,
            },
            Request::Release { way: WayId(3) },
            Request::Peers,
        ]
    }

    fn every_answer() -> Vec<Answer> {
        vec![
            Answer::Standing(Standing {
                protocol: PROTOCOL,
                fingerprint: fingerprint(),
                hosting: true,
                ways: 2,
            }),
            Answer::Reached(Reached {
                way: WayId(3),
                address: "127.77.0.1".parse().unwrap(),
                engine: EnginePorts::new(42000).unwrap(),
                packet: 1353,
            }),
            Answer::Peer(Peer {
                // Un nom d'ordinateur contient des espaces bien plus
                // souvent qu'on ne le croit.
                name: "PC de Victor".to_string(),
                fingerprint: fingerprint(),
                address: "192.168.1.20".parse().unwrap(),
                port: 47000,
            }),
            Answer::Done,
            Answer::Refused("cet ordinateur a refusé l'accès".to_string()),
        ]
    }

    #[test]
    fn every_request_survives_the_round_trip() {
        for request in every_request() {
            let line = request.to_string();
            assert_eq!(Request::parse(&line), Ok(request), "sur « {line} »");
        }
    }

    #[test]
    fn every_answer_survives_the_round_trip() {
        for answer in every_answer() {
            let line = answer.to_string();
            assert_eq!(Answer::parse(&line), Ok(answer), "sur « {line} »");
        }
    }

    #[test]
    fn no_message_ever_carries_a_newline() {
        // Two messages on what should be one line is the one mistake
        // this format cannot recover from.
        for line in every_request()
            .iter()
            .map(ToString::to_string)
            .chain(every_answer().iter().map(ToString::to_string))
        {
            assert!(!line.contains('\n'), "« {line} »");
        }
        let onto_one_line = Answer::Refused("deux\nlignes".to_string()).to_string();
        assert_eq!(onto_one_line, r"no deux\nlignes");
    }

    #[test]
    fn a_refusal_keeps_its_shape_across_the_channel() {
        // Refusals are written to be read: the hint on the second line
        // is what tells the person what to do about it.
        for reason in [
            "192.168.1.20 a refusé cet ordinateur.\n  Sur 192.168.1.20 : zyr-cli host authorize 0829cc",
            r"un chemin C:\ZyrDesk\data introuvable",
            "une barre à la fin \\",
            "",
        ] {
            let sent = Answer::Refused(reason.to_string()).to_string();
            assert!(!sent.contains('\n'), "« {sent} »");
            assert_eq!(
                Answer::parse(&sent),
                Ok(Answer::Refused(reason.to_string())),
                "sur « {sent} »"
            );
        }
    }

    #[test]
    fn a_field_added_later_does_not_upset_an_older_reader() {
        let line = "reach host=192.168.1.20 peer=0829cc7ecb9e9ba53cd36e6f342268ddf3c8ef05a49d1d7944ac6332c89cf237 bitrate=20000 fps=60 codec=av1";
        assert!(matches!(Request::parse(line), Ok(Request::Reach { .. })));
    }

    #[test]
    fn a_name_with_spaces_arrives_whole() {
        for name in [
            "PC de Victor",
            "  PC  ",
            r"un nom\avec une barre",
            "ordinateur",
        ] {
            let sent = Answer::Peer(Peer {
                name: name.to_string(),
                fingerprint: fingerprint(),
                address: "192.168.1.20".parse().unwrap(),
                port: 47000,
            })
            .to_string();
            let Ok(Answer::Peer(read)) = Answer::parse(&sent) else {
                panic!("« {sent} » n'est pas relu comme un ordinateur");
            };
            assert_eq!(read.name, name, "sur « {sent} »");
        }
    }

    #[test]
    fn an_unknown_verb_names_itself() {
        let refusal = Request::parse("teleport way=1").unwrap_err();
        assert!(refusal.to_string().contains("teleport"), "{refusal}");
    }

    #[test]
    fn a_missing_field_names_itself() {
        let refusal = Request::parse("release").unwrap_err();
        assert!(refusal.to_string().contains("way"), "{refusal}");
    }
}
