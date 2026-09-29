//! This computer's own counters, read from Windows a second at a time.
//!
//! How busy the processors are and which programs keep them busy, what
//! the system counted of UDP and IP, and what the network card that
//! reaches the far computer has carried and lost. What they gained
//! between two readings is worked out and worded in `counted.rs`.

use std::net::IpAddr;
use std::time::Instant;

use windows_sys::Win32::Foundation::{FILETIME, INVALID_HANDLE_VALUE};
use windows_sys::Win32::NetworkManagement::IpHelper::{
    GetBestInterfaceEx, GetIfEntry2, GetIpStatisticsEx, GetUdpStatisticsEx, MIB_IF_ROW2,
    MIB_IPSTATS_LH, MIB_UDPSTATS,
};
use windows_sys::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows_sys::Win32::Networking::WinSock::{
    AF_INET, AF_INET6, SOCKADDR, SOCKADDR_IN, SOCKADDR_IN6,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentThread, GetProcessTimes, GetSystemTimes, OpenProcess,
    PROCESS_QUERY_LIMITED_INFORMATION, SetThreadPriority, THREAD_PRIORITY_LOWEST,
};
use zyr_win32::{Handle, read_wide};

use crate::counted::{Card, Carried, Cpu, Ip, Kind, Program, Raw, Udp};

/// What a card says its speed is when it does not know.
const UNKNOWN_SPEED: u64 = u64::MAX;

/// The computer's own counters, read a second at a time.
pub struct Vitals {
    before: Option<Raw>,
}

impl Vitals {
    /// Starts reading.
    ///
    /// Meant to be called on the thread that will read, which is put at
    /// the lowest priority: going through every program on the computer
    /// takes a few milliseconds, and they must never be taken from a
    /// picture.
    pub fn start() -> Option<Self> {
        // SAFETY: the pseudo-handle of the calling thread, which needs no
        // closing.
        unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_LOWEST) };
        Some(Self { before: None })
    }

    /// What the counters gained since the last reading, told for the
    /// journal. Nothing the first time, which only takes the reading the
    /// next ones are compared with.
    ///
    /// `peer` is where the session's packets go: the card that reaches it
    /// is the one followed.
    pub fn second(&mut self, peer: Option<IpAddr>) -> Option<String> {
        let now = Raw {
            at: Instant::now(),
            cpu: cpu(),
            programs: programs(),
            udp: udp(),
            ip: ip(),
            card: peer.and_then(card_towards),
        };
        let told = self
            .before
            .as_ref()
            .map(|before| now.since(before).to_string());
        self.before = Some(now);
        told
    }
}

/// A time of Windows, in hundreds of nanoseconds.
fn filetime(time: FILETIME) -> u64 {
    (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)
}

fn cpu() -> Option<Cpu> {
    let (mut idle, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    // SAFETY: three slots of ours.
    let read = unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) };
    (read != 0).then(|| Cpu {
        idle: filetime(idle),
        kernel: filetime(kernel),
        user: filetime(user),
    })
}

/// Every program that could be asked, and the processor time it has used.
fn programs() -> Vec<Program> {
    // SAFETY: a snapshot of the running programs, closed by its guard.
    let snapshot = Handle(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) });
    if snapshot.0 == INVALID_HANDLE_VALUE {
        return Vec::new();
    }
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..PROCESSENTRY32W::default()
    };
    let mut programs = Vec::new();
    // SAFETY: an entry of ours, with the size of it said.
    let mut more = unsafe { Process32FirstW(snapshot.0, &mut entry) } != 0;
    while more {
        if let Some(time) = processor_time(entry.th32ProcessID) {
            programs.push(Program {
                id: entry.th32ProcessID,
                name: read_wide(&entry.szExeFile),
                time,
            });
        }
        // SAFETY: the same entry, still ours.
        more = unsafe { Process32NextW(snapshot.0, &mut entry) } != 0;
    }
    programs
}

/// The processor time a program has used, when it lets itself be asked.
fn processor_time(id: u32) -> Option<u64> {
    if id == 0 {
        return None;
    }
    // SAFETY: a program that refuses or is gone gives a null handle, which
    // its guard leaves alone; a real one is closed by it.
    let program = Handle(unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, id) });
    if program.0.is_null() {
        return None;
    }
    let (mut created, mut exited, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    // SAFETY: the handle is live and the four slots are ours.
    let read =
        unsafe { GetProcessTimes(program.0, &mut created, &mut exited, &mut kernel, &mut user) };
    (read != 0).then(|| filetime(kernel) + filetime(user))
}

/// What the system counted of UDP, both IP versions together.
fn udp() -> Option<Udp> {
    let mut sum = Udp {
        received: 0,
        failed: 0,
        unclaimed: 0,
        sent: 0,
    };
    let mut read_any = false;
    for family in [AF_INET, AF_INET6] {
        let mut counted = MIB_UDPSTATS::default();
        // SAFETY: a structure of ours.
        if unsafe { GetUdpStatisticsEx(&mut counted, u32::from(family)) } == 0 {
            read_any = true;
            sum.received = sum.received.wrapping_add(counted.dwInDatagrams);
            sum.failed = sum.failed.wrapping_add(counted.dwInErrors);
            sum.unclaimed = sum.unclaimed.wrapping_add(counted.dwNoPorts);
            sum.sent = sum.sent.wrapping_add(counted.dwOutDatagrams);
        }
    }
    read_any.then_some(sum)
}

/// What the system counted of IP, both versions together.
fn ip() -> Option<Ip> {
    let mut sum = Ip {
        received: 0,
        discarded_in: 0,
        discarded_out: 0,
        not_reassembled: 0,
    };
    let mut read_any = false;
    for family in [AF_INET, AF_INET6] {
        let mut counted = MIB_IPSTATS_LH::default();
        // SAFETY: a structure of ours.
        if unsafe { GetIpStatisticsEx(&mut counted, u32::from(family)) } == 0 {
            read_any = true;
            sum.received = sum.received.wrapping_add(counted.dwInReceives);
            sum.discarded_in = sum.discarded_in.wrapping_add(counted.dwInDiscards);
            sum.discarded_out = sum.discarded_out.wrapping_add(counted.dwOutDiscards);
            sum.not_reassembled = sum.not_reassembled.wrapping_add(counted.dwReasmFails);
        }
    }
    read_any.then_some(sum)
}

/// The card the routing table sends packets for that address through, and
/// what it has carried.
fn card_towards(peer: IpAddr) -> Option<Card> {
    let mut index = 0u32;
    let asked = match peer {
        IpAddr::V4(address) => {
            let mut to = SOCKADDR_IN {
                sin_family: AF_INET,
                ..SOCKADDR_IN::default()
            };
            to.sin_addr.S_un.S_addr = u32::from_ne_bytes(address.octets());
            // SAFETY: an address of ours, of the size its family needs,
            // and a slot of ours for the answer.
            unsafe { GetBestInterfaceEx((&raw const to).cast::<SOCKADDR>(), &mut index) }
        }
        IpAddr::V6(address) => {
            let mut to = SOCKADDR_IN6 {
                sin6_family: AF_INET6,
                ..SOCKADDR_IN6::default()
            };
            to.sin6_addr.u.Byte = address.octets();
            // SAFETY: as above.
            unsafe { GetBestInterfaceEx((&raw const to).cast::<SOCKADDR>(), &mut index) }
        }
    };
    if asked != 0 {
        return None;
    }
    let mut row = MIB_IF_ROW2 {
        InterfaceIndex: index,
        ..MIB_IF_ROW2::default()
    };
    // SAFETY: a row of ours, whose index says which card is meant.
    if unsafe { GetIfEntry2(&mut row) } != 0 {
        return None;
    }
    Some(Card {
        name: read_wide(&row.Alias),
        kind: Kind::of_type(row.Type),
        speed: Some(row.ReceiveLinkSpeed).filter(|speed| *speed != UNKNOWN_SPEED && *speed != 0),
        up: row.OperStatus == IfOperStatusUp,
        received: Carried {
            bytes: row.InOctets,
            packets: row.InUcastPkts + row.InNUcastPkts,
            discarded: row.InDiscards,
            errors: row.InErrors,
        },
        sent: Carried {
            bytes: row.OutOctets,
            packets: row.OutUcastPkts + row.OutNUcastPkts,
            discarded: row.OutDiscards,
            errors: row.OutErrors,
        },
    })
}

#[cfg(test)]
mod tests {
    use std::net::UdpSocket;
    use std::time::Duration;

    use super::*;

    /// Burns a processor for that long, on a thread of its own.
    fn busy_for(how_long: Duration) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            let until = Instant::now() + how_long;
            let mut sum = 0u64;
            while Instant::now() < until {
                sum = sum.wrapping_add(1);
            }
            std::hint::black_box(sum);
        })
    }

    #[test]
    fn the_first_reading_tells_nothing_and_the_second_tells_what_the_computer_did() {
        let mut vitals = Vitals::start().expect("Windows counts");
        let loopback = Some(IpAddr::from([127, 0, 0, 1]));
        assert_eq!(vitals.second(loopback), None);

        // A datagram to ourselves, and a processor kept busy meanwhile.
        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
        let burner = busy_for(Duration::from_millis(400));
        sender
            .send_to(&[0u8; 100], receiver.local_addr().unwrap())
            .unwrap();
        std::thread::sleep(Duration::from_millis(450));
        burner.join().unwrap();

        let told = vitals.second(loopback).expect("a second reading tells");
        assert!(told.starts_with("over "), "{told}");
        assert!(told.contains("the computer "), "{told}");
        assert!(told.contains("; UDP "), "{told}");
        assert!(told.contains("; IP "), "{told}");
        assert!(told.contains("; card "), "{told}");
    }
}
