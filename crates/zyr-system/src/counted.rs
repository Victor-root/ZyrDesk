//! What a computer's counters gained between two readings, and how it is
//! said.
//!
//! A session going wrong leaves the same trace whether the fault is the
//! network, the far computer or this one, and what tells them apart is
//! what this computer was doing at that second: busy with something
//! else, throwing away datagrams the system had no room for, a card
//! discarding packets. Counters only ever grow, so what is worth writing
//! is what they gained; the arithmetic and the words are here, and can be
//! tried on any system, while reading the counters is Windows' own
//! (`reading.rs`).

use std::fmt;
use std::time::{Duration, Instant};

/// The programs named at most: a computer where more than a handful use
/// the processor is one where the list is not what matters.
const MOST_PROGRAMS: usize = 4;

/// A program using less of one processor than this is not worth naming.
const LEAST_PERCENT: f32 = 2.0;

/// The types Windows gives an Ethernet card, a Wi-Fi card and the
/// loopback.
const ETHERNET: u32 = 6;
const WIFI: u32 = 71;
const LOOPBACK: u32 = 24;

/// Everything counted at one moment.
#[derive(Debug, Clone)]
pub struct Raw {
    pub at: Instant,
    pub cpu: Option<Cpu>,
    pub programs: Vec<Program>,
    pub udp: Option<Udp>,
    pub ip: Option<Ip>,
    pub card: Option<Card>,
}

/// What the processors have spent since the computer started, in hundreds
/// of nanoseconds, all of them together. The kernel's share includes the
/// idle time, as Windows counts it.
#[derive(Debug, Clone, Copy)]
pub struct Cpu {
    pub idle: u64,
    pub kernel: u64,
    pub user: u64,
}

/// A program and the processor time it has used since it started, in
/// hundreds of nanoseconds.
#[derive(Debug, Clone)]
pub struct Program {
    pub id: u32,
    pub name: String,
    pub time: u64,
}

/// What the system counted of UDP, both IP versions together. Counters of
/// 32 bits, which wrap.
#[derive(Debug, Clone, Copy)]
pub struct Udp {
    pub received: u32,
    /// Received and not delivered, for want of room in the socket among
    /// other things.
    pub failed: u32,
    /// Received for a port nobody listens on.
    pub unclaimed: u32,
    pub sent: u32,
}

/// What the system counted of IP, both versions together. Counters of 32
/// bits, which wrap.
#[derive(Debug, Clone, Copy)]
pub struct Ip {
    pub received: u32,
    /// Packets thrown away though nothing was wrong with them.
    pub discarded_in: u32,
    pub discarded_out: u32,
    /// Fragmented packets that could not be put back together.
    pub not_reassembled: u32,
}

/// A network card and what it has carried since it came up.
#[derive(Debug, Clone)]
pub struct Card {
    pub name: String,
    pub kind: Kind,
    /// The speed the link negotiated, in bits a second, when it says.
    pub speed: Option<u64>,
    pub up: bool,
    pub received: Carried,
    pub sent: Carried,
}

/// What a card has carried one way.
#[derive(Debug, Clone, Copy)]
pub struct Carried {
    pub bytes: u64,
    pub packets: u64,
    pub discarded: u64,
    pub errors: u64,
}

/// What sort of card it is, as far as it matters here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Ethernet,
    WiFi,
    Loopback,
    /// Anything else, by the number Windows gives its type.
    Other(u32),
}

impl Kind {
    /// The kind of a card, from the number Windows gives its type.
    pub fn of_type(number: u32) -> Self {
        match number {
            ETHERNET => Kind::Ethernet,
            WIFI => Kind::WiFi,
            LOOPBACK => Kind::Loopback,
            other => Kind::Other(other),
        }
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Kind::Ethernet => f.write_str("Ethernet"),
            Kind::WiFi => f.write_str("Wi-Fi"),
            Kind::Loopback => f.write_str("loopback"),
            Kind::Other(number) => write!(f, "type {number}"),
        }
    }
}

impl Raw {
    /// What the counters gained since `before`.
    pub fn since(&self, before: &Raw) -> Report {
        let elapsed = self.at.saturating_duration_since(before.at);
        Report {
            elapsed,
            busy: self
                .cpu
                .zip(before.cpu)
                .and_then(|(now, before)| busy(now, before)),
            busiest: busiest(&self.programs, &before.programs, elapsed),
            udp: self.udp.zip(before.udp).map(|(now, before)| UdpGained {
                received: now.received.wrapping_sub(before.received),
                failed: now.failed.wrapping_sub(before.failed),
                unclaimed: now.unclaimed.wrapping_sub(before.unclaimed),
                sent: now.sent.wrapping_sub(before.sent),
            }),
            ip: self.ip.zip(before.ip).map(|(now, before)| IpGained {
                received: now.received.wrapping_sub(before.received),
                discarded_in: now.discarded_in.wrapping_sub(before.discarded_in),
                discarded_out: now.discarded_out.wrapping_sub(before.discarded_out),
                not_reassembled: now.not_reassembled.wrapping_sub(before.not_reassembled),
            }),
            // The same card, or nothing to compare: a session that moved
            // to another card between two readings has no gain to tell.
            card: self
                .card
                .as_ref()
                .zip(before.card.as_ref())
                .filter(|(now, before)| now.name == before.name)
                .map(|(now, before)| CardGained {
                    name: now.name.clone(),
                    kind: now.kind,
                    speed: now.speed,
                    up: now.up,
                    received: now.received.gained_since(&before.received),
                    sent: now.sent.gained_since(&before.sent),
                }),
        }
    }
}

impl Carried {
    fn gained_since(&self, before: &Carried) -> Carried {
        Carried {
            bytes: self.bytes.saturating_sub(before.bytes),
            packets: self.packets.saturating_sub(before.packets),
            discarded: self.discarded.saturating_sub(before.discarded),
            errors: self.errors.saturating_sub(before.errors),
        }
    }
}

/// How much of all the processors was used, as a percentage.
fn busy(now: Cpu, before: Cpu) -> Option<f32> {
    let total = (now.kernel + now.user).checked_sub(before.kernel + before.user)?;
    let idle = now.idle.checked_sub(before.idle)?;
    (total > 0).then(|| 100.0 * total.saturating_sub(idle) as f32 / total as f32)
}

/// The programs that used the most of one processor, biggest first.
fn busiest(now: &[Program], before: &[Program], elapsed: Duration) -> Vec<Busy> {
    // In hundreds of nanoseconds, like the times.
    let wall = elapsed.as_nanos() as f32 / 100.0;
    if wall <= 0.0 {
        return Vec::new();
    }
    let mut busy: Vec<Busy> = now
        .iter()
        .filter_map(|program| {
            let earlier = before.iter().find(|earlier| earlier.id == program.id)?;
            // A program that was replaced by another under the same
            // number has less time than the one before it.
            let used = program.time.checked_sub(earlier.time)?;
            Some(Busy {
                id: program.id,
                name: program.name.clone(),
                percent: 100.0 * used as f32 / wall,
            })
        })
        .filter(|busy| busy.percent >= LEAST_PERCENT)
        .collect();
    busy.sort_by(|a, b| b.percent.total_cmp(&a.percent));
    busy.truncate(MOST_PROGRAMS);
    busy
}

/// What the counters gained between two readings.
#[derive(Debug, Clone)]
pub struct Report {
    pub elapsed: Duration,
    /// How much of all the processors was used, as a percentage.
    pub busy: Option<f32>,
    pub busiest: Vec<Busy>,
    pub udp: Option<UdpGained>,
    pub ip: Option<IpGained>,
    pub card: Option<CardGained>,
}

/// A program and how much of one processor it used, as a percentage.
#[derive(Debug, Clone, PartialEq)]
pub struct Busy {
    pub id: u32,
    pub name: String,
    pub percent: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UdpGained {
    pub received: u32,
    pub failed: u32,
    pub unclaimed: u32,
    pub sent: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IpGained {
    pub received: u32,
    pub discarded_in: u32,
    pub discarded_out: u32,
    pub not_reassembled: u32,
}

#[derive(Debug, Clone)]
pub struct CardGained {
    pub name: String,
    pub kind: Kind,
    pub speed: Option<u64>,
    pub up: bool,
    pub received: Carried,
    pub sent: Carried,
}

/// `over 1.0 s: the computer 23 % busy, most of a processor used by
/// zyrdeskd.exe (2210) 14 %, ...; UDP ...; IP ...; card ...`: one line,
/// and only what could be read.
impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "over {:.1} s", self.elapsed.as_secs_f32())?;
        if let Some(busy) = self.busy {
            write!(f, ", the computer {busy:.0} % busy")?;
        }
        if !self.busiest.is_empty() {
            let programs: Vec<String> = self
                .busiest
                .iter()
                .map(|busy| format!("{} ({}) {:.0} %", busy.name, busy.id, busy.percent))
                .collect();
            write!(f, ", of one processor {}", programs.join(", "))?;
        }
        if let Some(udp) = self.udp {
            write!(
                f,
                "; UDP {} datagrams in, {} of them failed and {} unclaimed, {} out",
                udp.received, udp.failed, udp.unclaimed, udp.sent
            )?;
        }
        if let Some(ip) = self.ip {
            write!(
                f,
                "; IP {} packets in, {} discarded in and {} out, {} not reassembled",
                ip.received, ip.discarded_in, ip.discarded_out, ip.not_reassembled
            )?;
        }
        if let Some(card) = &self.card {
            write!(
                f,
                "; card {} ({}, {}, {}): in {} KB, {} packets, {} discarded, {} errors; out {} KB, \
                 {} packets, {} discarded, {} errors",
                card.name,
                card.kind,
                match card.speed {
                    Some(bits) => format!("{} Mb/s", bits / 1_000_000),
                    None => "speed unknown".to_string(),
                },
                if card.up { "up" } else { "down" },
                card.received.bytes / 1024,
                card.received.packets,
                card.received.discarded,
                card.received.errors,
                card.sent.bytes / 1024,
                card.sent.packets,
                card.sent.discarded,
                card.sent.errors,
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(name: &str, received: u64, discarded: u64) -> Card {
        Card {
            name: name.to_string(),
            kind: Kind::Ethernet,
            speed: Some(1_000_000_000),
            up: true,
            received: Carried {
                bytes: received * 1000,
                packets: received,
                discarded,
                errors: 0,
            },
            sent: Carried {
                bytes: 0,
                packets: 0,
                discarded: 0,
                errors: 0,
            },
        }
    }

    fn reading(at: Instant, seconds: u64) -> Raw {
        Raw {
            at: at + Duration::from_secs(seconds),
            cpu: Some(Cpu {
                idle: 700 * seconds * 10_000_000,
                kernel: 800 * seconds * 10_000_000,
                user: 200 * seconds * 10_000_000,
            }),
            programs: vec![
                Program {
                    id: 10,
                    name: "zyrdeskd.exe".to_string(),
                    time: 15_000_000 * seconds,
                },
                Program {
                    id: 20,
                    name: "chrome.exe".to_string(),
                    time: 3_000_000 * seconds,
                },
                Program {
                    id: 30,
                    name: "idle.exe".to_string(),
                    time: 100_000 * seconds,
                },
            ],
            udp: Some(Udp {
                received: (u32::MAX - 5).wrapping_add(2318 * seconds as u32),
                failed: 7 * seconds as u32,
                unclaimed: 0,
                sent: 55 * seconds as u32,
            }),
            ip: Some(Ip {
                received: 2400 * seconds as u32,
                discarded_in: 0,
                discarded_out: 0,
                not_reassembled: 0,
            }),
            card: Some(card("Ethernet", 2318 * seconds, 3 * seconds)),
        }
    }

    #[test]
    fn a_second_of_counters_is_told_by_what_they_gained() {
        let at = Instant::now();
        let report = reading(at, 2).since(&reading(at, 1));
        assert_eq!(
            report.to_string(),
            "over 1.0 s, the computer 30 % busy, of one processor zyrdeskd.exe (10) 150 %, \
             chrome.exe (20) 30 %; UDP 2318 datagrams in, 7 of them failed and 0 unclaimed, 55 \
             out; IP 2400 packets in, 0 discarded in and 0 out, 0 not reassembled; card Ethernet \
             (Ethernet, 1000 Mb/s, up): in 2263 KB, 2318 packets, 3 discarded, 0 errors; out 0 \
             KB, 0 packets, 0 discarded, 0 errors"
        );
    }

    #[test]
    fn counters_of_thirty_two_bits_that_wrap_still_gain_what_they_gained() {
        let at = Instant::now();
        // The datagram counter started five short of its end, and wrapped
        // in the second that followed.
        let report = reading(at, 1).since(&reading(at, 0));
        assert_eq!(report.udp.map(|udp| udp.received), Some(2318));
    }

    #[test]
    fn a_program_too_idle_to_matter_is_not_named() {
        let at = Instant::now();
        let report = reading(at, 2).since(&reading(at, 1));
        assert!(report.busiest.iter().all(|busy| busy.name != "idle.exe"));
        assert_eq!(report.busiest.len(), 2);
    }

    #[test]
    fn a_program_replaced_under_the_same_number_is_not_counted() {
        let at = Instant::now();
        let before = reading(at, 5);
        let mut now = reading(at, 6);
        // A new program came under number 10 with almost no time used.
        now.programs[0].time = 1_000;
        let report = now.since(&before);
        assert!(report.busiest.iter().all(|busy| busy.id != 10));
    }

    #[test]
    fn a_session_that_changed_card_has_no_card_to_compare() {
        let at = Instant::now();
        let before = reading(at, 1);
        let mut now = reading(at, 2);
        now.card = Some(card("Wi-Fi", 10, 0));
        let report = now.since(&before);
        assert!(report.card.is_none());
        assert!(!report.to_string().contains("; card "));
    }

    #[test]
    fn what_could_not_be_read_is_left_out_of_the_line() {
        let at = Instant::now();
        let bare = |seconds| Raw {
            at: at + Duration::from_secs(seconds),
            cpu: None,
            programs: Vec::new(),
            udp: None,
            ip: None,
            card: None,
        };
        assert_eq!(bare(1).since(&bare(0)).to_string(), "over 1.0 s");
    }

    #[test]
    fn a_card_is_named_after_the_type_windows_gives_it() {
        assert_eq!(Kind::of_type(6).to_string(), "Ethernet");
        assert_eq!(Kind::of_type(71).to_string(), "Wi-Fi");
        assert_eq!(Kind::of_type(24).to_string(), "loopback");
        assert_eq!(Kind::of_type(131), Kind::Other(131));
        assert_eq!(Kind::of_type(131).to_string(), "type 131");
    }

    #[test]
    fn a_link_that_says_no_speed_says_so() {
        let at = Instant::now();
        let mut now = reading(at, 2);
        now.card.as_mut().unwrap().speed = None;
        now.card.as_mut().unwrap().up = false;
        let line = now.since(&reading(at, 1)).to_string();
        assert!(line.contains("(Ethernet, speed unknown, down)"), "{line}");
    }
}
