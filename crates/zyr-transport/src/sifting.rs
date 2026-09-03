//! Sorting a batch of datagrams a socket has just received.
//!
//! Two sockets of this transport carry datagrams of ours beside the
//! transport's own: the junction, where probes and echoes travel beside
//! the tunnel, and the doorway a server puts its relay on, where the
//! mirror answers questions on the relay's port. Both have the same work
//! to do on every batch, and it is done here once: what is ours is
//! answered and taken out, and what is left is moved to the front, so
//! the transport never sees anything but its own.
//!
//! A batch entry is not one datagram. The system hands several of them
//! over in a single buffer when it can, `stride` bytes apart, so the
//! sorting goes segment by segment inside each entry; an entry left
//! empty is not handed over at all.
//!
//! Writing back to such a socket is here too, for the one thing it has
//! that an ordinary socket does not: an address has to be handed over in
//! the form the socket speaks.

use std::io;
use std::net::{IpAddr, SocketAddr};

use quinn::udp::RecvMeta;

use crate::probe;

/// An address as a socket wants it: one that speaks IPv6 is handed every
/// IPv4 address in its mapped form, which is what the transport does
/// too.
pub(crate) fn outward(address: SocketAddr, ipv6: bool) -> SocketAddr {
    match address {
        SocketAddr::V4(v4) if ipv6 => {
            SocketAddr::new(IpAddr::V6(v4.ip().to_ipv6_mapped()), v4.port())
        }
        other => other,
    }
}

/// Sorts one batch in place.
///
/// `ours` is given every datagram of ours, with the address it came
/// from, and says what to answer and where; `instead` gives the address
/// to show the transport in place of the real one, for whoever renames
/// what it receives. Answers are handed back rather than sent, because
/// sending them belongs to the caller and its socket.
pub(crate) fn sift<A>(
    bufs: &mut [io::IoSliceMut<'_>],
    meta: &mut [RecvMeta],
    count: usize,
    mut ours: impl FnMut(SocketAddr, &[u8]) -> Option<(A, Vec<u8>)>,
    mut instead: impl FnMut(SocketAddr) -> Option<SocketAddr>,
) -> (usize, Vec<(A, Vec<u8>)>) {
    let mut answers = Vec::new();
    let mut kept = 0;
    for read in 0..count {
        let from = meta[read].addr;
        let canonical = SocketAddr::new(from.ip().to_canonical(), from.port());
        let len = meta[read].len;
        let stride = meta[read].stride.clamp(1, len.max(1));
        let mut write_at = 0;
        let mut read_at = 0;
        while read_at < len {
            let segment = stride.min(len - read_at);
            let buf = &mut bufs[read][..len];
            if probe::is_ours(&buf[read_at..read_at + segment]) {
                if let Some(answer) = ours(canonical, &buf[read_at..read_at + segment]) {
                    answers.push(answer);
                }
            } else {
                if write_at != read_at {
                    buf.copy_within(read_at..read_at + segment, write_at);
                }
                write_at += segment;
            }
            read_at += segment;
        }
        if write_at == 0 {
            continue;
        }
        let mut entry = meta[read];
        entry.len = write_at;
        if let Some(renamed) = instead(canonical) {
            entry.addr = renamed;
        }
        if kept != read {
            let (before, after) = bufs.split_at_mut(read);
            before[kept][..write_at].copy_from_slice(&after[0][..write_at]);
        }
        meta[kept] = entry;
        kept += 1;
    }
    (kept, answers)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn somewhere() -> SocketAddr {
        "192.0.2.7:47000".parse().unwrap()
    }

    /// One batch entry, holding those datagrams end to end.
    fn batch(datagrams: &[&[u8]]) -> (Vec<u8>, RecvMeta) {
        let stride = datagrams.iter().map(|one| one.len()).max().unwrap_or(1);
        let mut buffer = vec![0u8; stride * datagrams.len().max(1)];
        for (at, datagram) in datagrams.iter().enumerate() {
            buffer[at * stride..at * stride + datagram.len()].copy_from_slice(datagram);
        }
        let len = buffer.len();
        (
            buffer,
            RecvMeta {
                addr: somewhere(),
                len,
                stride,
                ecn: None,
                dst_ip: None,
            },
        )
    }

    #[test]
    fn what_is_ours_is_answered_and_taken_out_of_the_batch() {
        // Un paquet du transport, un des nôtres, un du transport : ce
        // qui reste doit être les deux paquets du transport collés, et
        // rien du nôtre.
        let mine = probe::who_am_i([1, 2, 3, 4, 5, 6, 7, 8]);
        let mut padded = mine.clone();
        padded.resize(8, 0);
        let (mut buffer, entry) = batch(&[b"AAAAAAAA", &padded, b"BBBBBBBB"]);
        let mut meta = [entry];
        let mut bufs = [io::IoSliceMut::new(&mut buffer)];

        let (kept, answers) = sift(
            &mut bufs,
            &mut meta,
            1,
            |from, datagram| {
                assert_eq!(from, somewhere());
                assert!(probe::is_ours(datagram));
                Some((from, b"reponse".to_vec()))
            },
            |_| None,
        );
        assert_eq!(kept, 1);
        assert_eq!(answers.len(), 1);
        assert_eq!(meta[0].len, 16);
        assert_eq!(&bufs[0][..16], b"AAAAAAAABBBBBBBB");
    }

    #[test]
    fn an_entry_holding_nothing_but_ours_is_not_handed_over() {
        let mine = probe::who_am_i([9; 8]);
        let (mut buffer, entry) = batch(&[&mine]);
        let mut meta = [entry];
        let mut bufs = [io::IoSliceMut::new(&mut buffer)];
        let (kept, answers) = sift(
            &mut bufs,
            &mut meta,
            1,
            |_, _| None::<(SocketAddr, Vec<u8>)>,
            |_| None,
        );
        assert_eq!(kept, 0);
        assert!(answers.is_empty());
    }

    #[test]
    fn a_datagram_of_the_transport_may_be_renamed() {
        let card: SocketAddr = "240.1.2.3:47000".parse().unwrap();
        let (mut buffer, entry) = batch(&[b"paquet"]);
        let mut meta = [entry];
        let mut bufs = [io::IoSliceMut::new(&mut buffer)];
        let (kept, _) = sift(
            &mut bufs,
            &mut meta,
            1,
            |_, _| None::<(SocketAddr, Vec<u8>)>,
            |_| Some(card),
        );
        assert_eq!(kept, 1);
        assert_eq!(meta[0].addr, card);
    }
}
