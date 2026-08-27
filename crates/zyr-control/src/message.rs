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
use std::time::Duration;

use zyr_proto::net::EnginePorts;
use zyr_proto::session::{Preferred, Serving};
use zyr_transport::{Fingerprint, MediaProfile};

/// Version of this dialect.
///
/// It only ever grows, and the service announces it: two halves of the
/// product installed at different times must be able to say so rather
/// than misunderstand each other quietly. A field that goes counts as
/// much as one that arrives, since the two halves would then no longer
/// be saying the same things to each other.
pub const PROTOCOL: u32 = 20;

/// Identifies one way out, for as long as it stays open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WayId(pub u64);

impl fmt::Display for WayId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// What stands between a computer meant to be reachable and being one.
///
/// Only ever read when it is not reachable. Without it, an engine that
/// is missing and an engine that is starting look exactly alike from a
/// window, and the second one never stops looking like it is about to
/// work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Holdup {
    /// Nothing is wrong: it is on its way up.
    #[default]
    Starting,
    /// The host engine is not on this machine.
    EngineMissing,
    /// It is there, and it will not stay up.
    EngineWontStand,
}

impl Holdup {
    fn spelled(self) -> &'static str {
        match self {
            Holdup::Starting => "starting",
            Holdup::EngineMissing => "engine-missing",
            Holdup::EngineWontStand => "engine-wont-stand",
        }
    }

    /// What a word means, the ordinary case standing in for anything
    /// this half of the product has never heard of.
    fn read(said: &str) -> Self {
        match said {
            "engine-missing" => Holdup::EngineMissing,
            "engine-wont-stand" => Holdup::EngineWontStand,
            _ => Holdup::Starting,
        }
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
    /// Hands the far computer, through an open way, the code its engine
    /// is waiting for.
    ///
    /// This is what spares the person a walk to the other computer. The
    /// two computers recognised each other by fingerprint before the way
    /// opened, so the code proves nothing they have not already proven,
    /// and nobody is ever shown one.
    Pair { way: WayId, pin: String },
    /// Asks the far computer to press Ctrl+Alt+Suppr on itself.
    ///
    /// Through the way and never through the engines. Windows keeps that
    /// combination for itself at both ends: the computer watching never
    /// sees it, and no engine may type it on the computer being watched.
    /// The far ZyrDesk presses it on its own machine, which is the one
    /// program there the system will let.
    SecureAttention { way: WayId },
    /// Asks the far computer to put its lock screen up.
    ///
    /// What stands in for Windows+L, which cannot travel: Windows keeps
    /// that combination where no program can reach it, at both ends of a
    /// session, and it keeps the raising of a lock screen for programs
    /// sitting at the desk being locked. So the ask goes round the same
    /// way Ctrl+Alt+Suppr does, and the far ZyrDesk does it from the one
    /// place its own Windows will take it from.
    LockScreen { way: WayId },
    /// Asks the far computer to silence its own speakers for the length
    /// of the session, or to let them play again.
    ///
    /// A choice made by whoever is watching, which is why it travels at
    /// all: the person taking control of a machine in another room is the
    /// only one who knows that the room should go quiet, and a setting on
    /// that machine would have to be walked over to.
    Hush { way: WayId, quiet: bool },
    /// Asks the far computer to resend a still screen at full rate while
    /// this session watches it, or to stop doing it.
    ///
    /// The same reasoning as the hush: what it costs is paid over there,
    /// and the only person who can tell whether the picture feels smooth
    /// is the one looking at it. Its engine reads this when it starts, so
    /// it is asked at the opening of a session and never inside one.
    SteadyFar { way: WayId, rate: bool },
    /// Ties an open way to the process using it: the way closes on its
    /// own once that process is gone, whatever became of whoever asked.
    Hold { way: WayId, process: u32 },
    /// Closes a way.
    Release { way: WayId },
    /// The ZyrDesk computers seen on the local network.
    Peers,
    /// The sessions this computer is holding towards others.
    ///
    /// What lets a window that was closed, updated or killed find the
    /// session again instead of opening on an empty home screen.
    Sessions,
    /// Decides whether this computer accepts being controlled.
    ///
    /// A decision and not a state: it survives a restart, since a
    /// computer that let itself be reached again the next morning
    /// would be honouring nobody's wish.
    SetHosting { on: bool },
    /// Decides whether the ZyrDesk of the local network are let in
    /// without anyone having to recognise them one by one.
    SetTrust { on: bool },
    /// Changes how this computer makes the pictures it serves.
    ///
    /// A host setting and not a session one: it changes nothing about a
    /// session opened from here, and everything about one opened towards
    /// here. The engine reads both at its own start, so this restarts it.
    ServeLike { serving: Serving },
    /// Writes a computer down.
    ///
    /// What is left when the network announces nothing: on a network
    /// that drops the announcements, recognising the other machine by
    /// hand is the only way in, and it has to be doable from a window
    /// like everything else.
    ///
    /// The fingerprint alone lets that computer come in. With an address
    /// it is also kept on the home screen, so that reaching it again
    /// never costs anybody a second copying.
    Authorize {
        peer: Fingerprint,
        host: Option<String>,
        name: Option<String>,
    },
    /// Takes a computer written down off both lists: it no longer shows,
    /// and it no longer comes in.
    Forget { peer: Fingerprint },
    /// Decides whether this computer is reachable from the moment it
    /// powers on, before anybody has signed in.
    ///
    /// The service registers itself with Windows either way; this is the
    /// difference between Windows starting it on its own and it waiting
    /// to be asked.
    SetAtBoot { on: bool },
    /// Asks the service to stop.
    ///
    /// What « quit » means, from an interface: closing the window has to
    /// be able to take everything with it. Stopping a service through
    /// Windows asks for administrator rights every single time, so the
    /// service stops itself instead, on a channel it already answers.
    Stop,
    /// What a session opened from this computer is set to.
    Settings,
    /// Changes it, for this session and all the ones after.
    Choose { preferred: Preferred },
}

impl Request {
    pub fn parse(line: &str) -> Result<Self, Malformed> {
        let (verb, rest) = split_verb(line);
        let fields = Fields(rest);
        match verb {
            "standing" => Ok(Request::Standing),
            "reach" => Ok(Request::Reach {
                host: unpacked(fields.text("host")?),
                peer: fields.parsed("peer")?,
                media: MediaProfile {
                    bits_per_second: u64::from(fields.parsed::<u32>("bitrate")?) * 1000,
                    frames_per_second: fields.parsed("fps")?,
                },
            }),
            "pair" => Ok(Request::Pair {
                way: WayId(fields.parsed("way")?),
                pin: fields.text("pin")?.to_string(),
            }),
            "sas" => Ok(Request::SecureAttention {
                way: WayId(fields.parsed("way")?),
            }),
            "lock" => Ok(Request::LockScreen {
                way: WayId(fields.parsed("way")?),
            }),
            "hush" => Ok(Request::Hush {
                way: WayId(fields.parsed("way")?),
                quiet: fields.text("quiet")? == "yes",
            }),
            "steady" => Ok(Request::SteadyFar {
                way: WayId(fields.parsed("way")?),
                rate: fields.text("rate")? == "yes",
            }),
            "hold" => Ok(Request::Hold {
                way: WayId(fields.parsed("way")?),
                process: fields.parsed("process")?,
            }),
            "release" => Ok(Request::Release {
                way: WayId(fields.parsed("way")?),
            }),
            "peers" => Ok(Request::Peers),
            "sessions" => Ok(Request::Sessions),
            "hosting" => Ok(Request::SetHosting {
                on: fields.text("on")? == "yes",
            }),
            "trusting" => Ok(Request::SetTrust {
                on: fields.text("on")? == "yes",
            }),
            "serving" => Ok(Request::ServeLike {
                serving: Serving {
                    steady_rate: fields.text("steady")? == "yes",
                    capture: fields
                        .parsed("capture")
                        .unwrap_or(Serving::default().capture),
                },
            }),
            "authorize" => Ok(Request::Authorize {
                peer: fields.parsed("peer")?,
                host: fields.text("host").ok().map(unpacked),
                name: fields.text("name").ok().map(unpacked),
            }),
            "forget" => Ok(Request::Forget {
                peer: fields.parsed("peer")?,
            }),
            "at-boot" => Ok(Request::SetAtBoot {
                on: fields.text("on")? == "yes",
            }),
            "stop" => Ok(Request::Stop),
            "settings" => Ok(Request::Settings),
            "choose" => Ok(Request::Choose {
                preferred: fields.preferred(),
            }),
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
                "reach host={} peer={peer} bitrate={} fps={}",
                packed(host),
                media.bits_per_second / 1000,
                media.frames_per_second
            ),
            Request::Pair { way, pin } => write!(f, "pair way={way} pin={pin}"),
            Request::SecureAttention { way } => write!(f, "sas way={way}"),
            Request::LockScreen { way } => write!(f, "lock way={way}"),
            Request::SteadyFar { way, rate } => {
                write!(f, "steady way={way} rate={}", said(*rate))
            }
            Request::Hush { way, quiet } => write!(f, "hush way={way} quiet={}", said(*quiet)),
            Request::Hold { way, process } => write!(f, "hold way={way} process={process}"),
            Request::Release { way } => write!(f, "release way={way}"),
            Request::Peers => f.write_str("peers"),
            Request::Sessions => f.write_str("sessions"),
            Request::SetHosting { on } => write!(f, "hosting on={}", said(*on)),
            Request::SetTrust { on } => write!(f, "trusting on={}", said(*on)),
            Request::ServeLike { serving } => write!(
                f,
                "serving steady={} capture={}",
                said(serving.steady_rate),
                serving.capture
            ),
            Request::Authorize { peer, host, name } => {
                write!(f, "authorize peer={peer}")?;
                if let Some(host) = host {
                    write!(f, " host={}", packed(host))?;
                }
                if let Some(name) = name {
                    write!(f, " name={}", packed(name))?;
                }
                Ok(())
            }
            Request::Forget { peer } => write!(f, "forget peer={peer}"),
            Request::SetAtBoot { on } => write!(f, "at-boot on={}", said(*on)),
            Request::Stop => f.write_str("stop"),
            Request::Settings => f.write_str("settings"),
            Request::Choose { preferred } => write!(f, "choose {}", spelled(preferred)),
        }
    }
}

/// How a yes-or-no travels.
fn said(yes: bool) -> &'static str {
    if yes { "yes" } else { "no" }
}

/// The fields a set of preferences travels as, shared by the question
/// and the answer so the two can never drift apart.
fn spelled(preferred: &Preferred) -> String {
    format!(
        "asked={} bitrate={} codec={} display={} mouse={} stats={} hush={} keys={} steady={}",
        preferred.asked,
        preferred.bitrate_kbps,
        preferred.codec,
        preferred.display_mode,
        if preferred.absolute_mouse {
            "desktop"
        } else {
            "game"
        },
        said(preferred.stats_overlay),
        said(preferred.mute_far_speakers),
        said(preferred.system_keys),
        said(preferred.steady_far_rate)
    )
}

/// What this computer is doing, and who it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Standing {
    /// Dialect the service speaks.
    pub protocol: u32,
    /// The code the service was built from.
    ///
    /// The window shows it beside its own. Two halves of the product
    /// built at different times is the one fault nobody thinks to check
    /// for, and the one that wastes the most time.
    pub build: String,
    /// What other computers pin to reach this one.
    pub fingerprint: Fingerprint,
    /// Whether this computer can be reached right now.
    pub hosting: bool,
    /// What is in the way when it is not, which is meaningless when it
    /// is.
    pub holdup: Holdup,
    /// Whether it is meant to be. The two differ while the engine is
    /// starting, and after it has given up.
    pub wanted: bool,
    /// Whether the ZyrDesk of the local network are let in without
    /// anyone recognising them one by one.
    pub trusting: bool,
    /// Whether Windows starts the service on its own, so that this
    /// computer answers before anybody has signed in.
    pub at_boot: bool,
    /// How this computer makes the pictures it serves.
    pub serving: Serving,
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

/// A ZyrDesk this computer can show on its home screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Peer {
    /// Name its owner knows it by.
    pub name: String,
    pub fingerprint: Fingerprint,
    /// Where to reach it, as it was announced or as it was written.
    ///
    /// Text and not an address: a computer written down by hand is
    /// reached at whatever was typed, which is not always something this
    /// program gets to interpret.
    pub host: String,
    pub port: u16,
    /// Whether it is announcing itself right now.
    ///
    /// A computer written down by hand shows on a network that carries
    /// no announcement at all, and there it is never seen. Saying so is
    /// the difference between a machine that is off and one this network
    /// simply cannot hear.
    pub seen: bool,
    /// Whether somebody wrote it down by hand.
    ///
    /// Only those can be taken off again: what the network announces
    /// comes back the moment it is removed.
    pub written: bool,
}

/// A session this computer is holding towards another.
///
/// Described from the service's side, which is the only side that
/// survives everything: it holds the way whether or not the program that
/// asked for it is still there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub way: WayId,
    /// Remote computer, as the person named it when asking.
    pub towards: String,
    /// Fingerprint it was recognised by. What matches a session to a
    /// computer on screen, since an address can change and a name is
    /// not unique.
    pub peer: Fingerprint,
    /// Number the system knows the player by. What lets a window find
    /// the session's own window among all the others.
    pub process: u32,
    /// Local address the client engine reaches that computer at, through
    /// the tunnel. What anything else driving the same engine has to be
    /// given: from outside, the far computer only exists there.
    pub at: String,
    /// How long the picture has been up.
    pub since: Duration,
}

/// What the service answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    Standing(Standing),
    Reached(Reached),
    /// One computer of a list. The list ends on `Done`.
    Peer(Peer),
    /// One session of a list. The list ends on `Done`.
    Session(Session),
    /// What a session opened from this computer is set to.
    Settings(Preferred),
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
                build: unpacked(fields.text("build").unwrap_or_default()),
                fingerprint: fields.parsed("fingerprint")?,
                hosting: fields.text("hosting")? == "yes",
                holdup: Holdup::read(fields.text("holdup").unwrap_or_default()),
                wanted: fields.text("wanted")? == "yes",
                trusting: fields.flag("trusting", false),
                at_boot: fields.flag("at-boot", true),
                serving: Serving {
                    steady_rate: fields.flag("steady", Serving::default().steady_rate),
                    capture: fields
                        .parsed("capture")
                        .unwrap_or(Serving::default().capture),
                },
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
                host: unpacked(fields.text("address")?),
                port: fields.parsed("port")?,
                seen: fields.flag("seen", true),
                written: fields.flag("written", false),
            })),
            "session" => Ok(Answer::Session(Session {
                way: WayId(fields.parsed("way")?),
                towards: unpacked(fields.text("towards")?),
                peer: fields.parsed("peer")?,
                process: fields.parsed("process")?,
                at: fields.text("at")?.to_string(),
                since: Duration::from_secs(fields.parsed("since")?),
            })),
            "settings" => Ok(Answer::Settings(fields.preferred())),
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
                "standing protocol={} build={} fingerprint={} hosting={} holdup={} wanted={} trusting={} at-boot={} steady={} capture={} ways={}",
                standing.protocol,
                packed(&standing.build),
                standing.fingerprint,
                said(standing.hosting),
                standing.holdup.spelled(),
                said(standing.wanted),
                said(standing.trusting),
                said(standing.at_boot),
                said(standing.serving.steady_rate),
                standing.serving.capture,
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
                "peer name={} fingerprint={} address={} port={} seen={} written={}",
                packed(&peer.name),
                peer.fingerprint,
                packed(&peer.host),
                peer.port,
                said(peer.seen),
                said(peer.written)
            ),
            Answer::Session(session) => write!(
                f,
                "session way={} towards={} peer={} process={} at={} since={}",
                session.way,
                packed(&session.towards),
                session.peer,
                session.process,
                session.at,
                session.since.as_secs()
            ),
            Answer::Settings(preferred) => write!(f, "settings {}", spelled(preferred)),
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

    /// Reads a yes-or-no, falling back when it is absent.
    ///
    /// How every field added after the fact is read: an older half of
    /// the product then costs the newer one that one thing instead of
    /// the whole exchange. Only a plain « yes » says yes, so a value
    /// nobody understands decides nothing either.
    fn flag(&self, key: &str, fallback: bool) -> bool {
        match self.text(key) {
            Ok(value) => value == "yes",
            Err(_) => fallback,
        }
    }

    /// Reads a whole set of preferences.
    ///
    /// A missing field falls back to what the product does by default
    /// rather than refusing the message: an older half of the product
    /// then loses one setting instead of the whole exchange.
    fn preferred(&self) -> Preferred {
        let fallback = Preferred::default();
        Preferred {
            asked: self.parsed("asked").unwrap_or(fallback.asked),
            bitrate_kbps: self.parsed("bitrate").unwrap_or(fallback.bitrate_kbps),
            codec: self.parsed("codec").unwrap_or(fallback.codec),
            display_mode: self.parsed("display").unwrap_or(fallback.display_mode),
            // Only a plain word turns a setting away from what the
            // product does by default, here as in the settings file: a
            // value nobody understands must not decide anything.
            absolute_mouse: match self.text("mouse") {
                Ok(mouse) => mouse != "game",
                Err(_) => fallback.absolute_mouse,
            },
            stats_overlay: self.flag("stats", fallback.stats_overlay),
            mute_far_speakers: self.flag("hush", fallback.mute_far_speakers),
            system_keys: self.flag("keys", fallback.system_keys),
            steady_far_rate: self.flag("steady", fallback.steady_far_rate),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zyr_proto::session::Capture;

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
            Request::Reach {
                // Une adresse écrite à la main peut porter une espace :
                // elle traverse le même champ que les autres textes.
                host: "pc de victor.local".to_string(),
                peer: fingerprint(),
                media: MediaProfile {
                    bits_per_second: 20_000_000,
                    frames_per_second: 60,
                },
            },
            Request::Pair {
                way: WayId(3),
                pin: "0429".to_string(),
            },
            Request::Hold {
                way: WayId(3),
                process: 11248,
            },
            Request::Release { way: WayId(3) },
            Request::SecureAttention { way: WayId(3) },
            Request::LockScreen { way: WayId(3) },
            Request::SteadyFar {
                way: WayId(3),
                rate: true,
            },
            Request::SteadyFar {
                way: WayId(3),
                rate: false,
            },
            Request::Hush {
                way: WayId(3),
                quiet: true,
            },
            Request::Hush {
                way: WayId(3),
                quiet: false,
            },
            Request::Peers,
            Request::Sessions,
            Request::SetHosting { on: true },
            Request::SetHosting { on: false },
            Request::SetTrust { on: true },
            Request::SetTrust { on: false },
            // Les trois réglages d'hôte voyagent ensemble dans un seul
            // message : un champ qui ne fait pas l'aller-retour remet
            // silencieusement les deux autres à ce qu'ils étaient.
            Request::ServeLike {
                serving: Serving::default(),
            },
            Request::ServeLike {
                serving: Serving {
                    steady_rate: false,
                    capture: Capture::Windows,
                },
            },
            Request::Authorize {
                peer: fingerprint(),
                host: None,
                name: None,
            },
            Request::Authorize {
                peer: fingerprint(),
                host: Some("192.168.1.20".to_string()),
                name: Some("PC de Victor".to_string()),
            },
            Request::Forget {
                peer: fingerprint(),
            },
            Request::SetAtBoot { on: true },
            Request::SetAtBoot { on: false },
            Request::Stop,
            Request::Settings,
            Request::Choose {
                preferred: preferred(),
            },
            Request::Choose {
                preferred: Preferred::default(),
            },
        ]
    }

    fn preferred() -> Preferred {
        use zyr_proto::session::{Asked, Codec, DisplayMode};
        Preferred {
            asked: Asked::Fixed(2560, 1440),
            bitrate_kbps: 15_000,
            codec: Codec::Hevc,
            display_mode: DisplayMode::Windowed,
            absolute_mouse: false,
            stats_overlay: true,
            mute_far_speakers: true,
            system_keys: false,
            steady_far_rate: false,
        }
    }

    fn every_answer() -> Vec<Answer> {
        vec![
            Answer::Standing(Standing {
                protocol: PROTOCOL,
                // Une empreinte de compilation porte une espace : elle
                // traverse le même champ « clé=valeur » que le reste.
                build: "599c1c4 2026-08-18".to_string(),
                fingerprint: fingerprint(),
                hosting: true,
                holdup: Holdup::Starting,
                wanted: true,
                trusting: true,
                at_boot: true,
                serving: Serving::default(),
                ways: 2,
            }),
            Answer::Standing(Standing {
                protocol: PROTOCOL,
                build: String::new(),
                fingerprint: fingerprint(),
                hosting: false,
                holdup: Holdup::EngineMissing,
                wanted: true,
                trusting: false,
                at_boot: false,
                serving: Serving {
                    steady_rate: false,
                    capture: Capture::Windows,
                },
                ways: 0,
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
                host: "192.168.1.20".to_string(),
                port: 47000,
                seen: true,
                written: false,
            }),
            Answer::Peer(Peer {
                name: "PC fixe".to_string(),
                fingerprint: fingerprint(),
                host: "192.168.1.20".to_string(),
                port: 47000,
                seen: false,
                written: true,
            }),
            Answer::Session(Session {
                way: WayId(3),
                towards: "192.168.1.20".to_string(),
                peer: fingerprint(),
                process: 11248,
                at: "127.77.0.1:47989".to_string(),
                since: Duration::from_secs(742),
            }),
            Answer::Settings(preferred()),
            Answer::Settings(Preferred::default()),
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
    fn an_older_service_is_still_understood_minus_what_it_cannot_do() {
        // Le service d'avant ne connaît ni sa propre empreinte de
        // compilation ni la confiance réseau. La fenêtre doit perdre ces
        // deux choses-là, pas la conversation.
        let line = format!(
            "standing protocol=6 fingerprint={} hosting=yes wanted=yes ways=0",
            fingerprint()
        );
        let Ok(Answer::Standing(standing)) = Answer::parse(&line) else {
            panic!("« {line} » n'est pas relu comme un état");
        };
        assert_eq!(standing.protocol, 6);
        assert!(standing.build.is_empty());
        assert!(!standing.trusting);
        assert_eq!(standing.holdup, Holdup::Starting);
        assert!(standing.hosting);
    }

    #[test]
    fn a_holdup_nobody_understands_reads_as_the_ordinary_case() {
        // Une moitié plus ancienne du produit ne doit pas afficher un
        // empêchement inventé : elle montre « démarrage », qui est vrai
        // le temps qu'on la mette à jour.
        assert_eq!(Holdup::read("un-empechement-inedit"), Holdup::Starting);
        assert_eq!(Holdup::read(""), Holdup::Starting);
        for holdup in [
            Holdup::Starting,
            Holdup::EngineMissing,
            Holdup::EngineWontStand,
        ] {
            assert_eq!(Holdup::read(holdup.spelled()), holdup);
        }
    }

    #[test]
    fn a_setting_the_other_half_never_heard_of_falls_back() {
        // Une moitié du produit plus ancienne que l'autre perd le
        // réglage qu'elle ne connaît pas, pas la conversation.
        let Ok(Answer::Settings(read)) = Answer::parse("settings asked=2560x1440") else {
            panic!("« settings asked=2560x1440 » n'est pas relu comme des réglages");
        };
        assert_eq!(read.asked, zyr_proto::session::Asked::Fixed(2560, 1440));
        assert_eq!(read.bitrate_kbps, Preferred::default().bitrate_kbps);
        assert_eq!(read.codec, Preferred::default().codec);
        assert_eq!(read.absolute_mouse, Preferred::default().absolute_mouse);

        // Et une valeur que personne ne comprend ne vaut pas mieux
        // qu'une absente : le défaut, et la session s'ouvre quand même.
        let Ok(Answer::Settings(read)) = Answer::parse("settings asked=ultra bitrate=beaucoup")
        else {
            panic!("« settings asked=ultra » n'est pas relu comme des réglages");
        };
        assert_eq!(read.asked, Preferred::default().asked);
        assert_eq!(read.bitrate_kbps, Preferred::default().bitrate_kbps);
    }

    #[test]
    fn a_name_with_spaces_arrives_whole() {
        // Le nom d'un ordinateur et l'adresse tapée pour le joindre
        // portent tous deux du texte libre : les deux traversent le même
        // champ `clé=valeur` et doivent en ressortir entiers.
        for name in [
            "PC de Victor",
            "  PC  ",
            r"un nom\avec une barre",
            "ordinateur",
        ] {
            let sent = Answer::Peer(Peer {
                name: name.to_string(),
                fingerprint: fingerprint(),
                host: "192.168.1.20".to_string(),
                port: 47000,
                seen: true,
                written: false,
            })
            .to_string();
            let Ok(Answer::Peer(read)) = Answer::parse(&sent) else {
                panic!("« {sent} » n'est pas relu comme un ordinateur");
            };
            assert_eq!(read.name, name, "sur « {sent} »");

            let sent = Answer::Session(Session {
                way: WayId(1),
                towards: name.to_string(),
                peer: fingerprint(),
                process: 11248,
                at: "127.77.0.1:47989".to_string(),
                since: Duration::from_secs(0),
            })
            .to_string();
            let Ok(Answer::Session(read)) = Answer::parse(&sent) else {
                panic!("« {sent} » n'est pas relu comme une session");
            };
            assert_eq!(read.towards, name, "sur « {sent} »");
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
