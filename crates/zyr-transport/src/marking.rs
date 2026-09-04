//! Whether the transport's packets carry a congestion mark on the wire.
//!
//! QUIC sets two bits of every packet's IP header, the ECN mark, and
//! reads them back in the acknowledgements to learn whether the path is
//! queueing. Nothing here uses that answer: the rate is the encoder's,
//! and the media controller ignores every congestion signal there is.
//! What the mark costs is unknown, and that is the point of this switch.
//! Some equipment between two homes treats marked packets differently
//! from unmarked ones, and the other product that crosses the same
//! equipment without a hiccup sends none. A session that dies with the
//! mark and lives without it names its culprit.
//!
//! The mark is removed at the socket, under the transport, which goes on
//! believing it marks its packets: the far end then acknowledges none of
//! the marks, the transport concludes the path does not carry them, and
//! stops asking. That is the ordinary road a path without ECN takes, and
//! the transport is built for it.

use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use quinn::udp::{RecvMeta, Transmit};
use quinn::{AsyncUdpSocket, UdpPoller};

/// What a socket does with the mark the transport puts on each packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Marking {
    /// Left as the transport set it, which is what QUIC does everywhere.
    #[default]
    Ecn,
    /// Taken off: the packet leaves as plain as any other UDP packet.
    None,
}

impl Marking {
    /// The socket as the transport is to use it: as it is, or behind
    /// something that takes the mark off.
    pub(crate) fn applied(self, socket: Arc<dyn AsyncUdpSocket>) -> Arc<dyn AsyncUdpSocket> {
        match self {
            Marking::Ecn => socket,
            Marking::None => Arc::new(Unmarked { inner: socket }),
        }
    }
}

/// A socket whose packets leave without a congestion mark.
#[derive(Debug)]
struct Unmarked {
    inner: Arc<dyn AsyncUdpSocket>,
}

impl AsyncUdpSocket for Unmarked {
    fn create_io_poller(self: Arc<Self>) -> Pin<Box<dyn UdpPoller>> {
        self.inner.clone().create_io_poller()
    }

    fn try_send(&self, transmit: &Transmit) -> io::Result<()> {
        self.inner.try_send(&Transmit {
            destination: transmit.destination,
            ecn: None,
            contents: transmit.contents,
            segment_size: transmit.segment_size,
            src_ip: transmit.src_ip,
        })
    }

    fn poll_recv(
        &self,
        cx: &mut Context,
        buffers: &mut [io::IoSliceMut<'_>],
        headers: &mut [RecvMeta],
    ) -> Poll<io::Result<usize>> {
        self.inner.poll_recv(cx, buffers, headers)
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }

    fn max_transmit_segments(&self) -> usize {
        self.inner.max_transmit_segments()
    }

    fn max_receive_segments(&self) -> usize {
        self.inner.max_receive_segments()
    }

    fn may_fragment(&self) -> bool {
        self.inner.may_fragment()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use quinn::udp::EcnCodepoint;

    /// A socket that writes down the mark of everything it is handed.
    #[derive(Debug)]
    struct Noting {
        marks: Mutex<Vec<Option<EcnCodepoint>>>,
    }

    impl AsyncUdpSocket for Noting {
        fn create_io_poller(self: Arc<Self>) -> Pin<Box<dyn UdpPoller>> {
            unreachable!("nothing here waits to write")
        }

        fn try_send(&self, transmit: &Transmit) -> io::Result<()> {
            self.marks.lock().unwrap().push(transmit.ecn);
            Ok(())
        }

        fn poll_recv(
            &self,
            _cx: &mut Context,
            _buffers: &mut [io::IoSliceMut<'_>],
            _headers: &mut [RecvMeta],
        ) -> Poll<io::Result<usize>> {
            Poll::Pending
        }

        fn local_addr(&self) -> io::Result<SocketAddr> {
            Ok("127.0.0.1:0".parse().unwrap())
        }
    }

    fn marked() -> Transmit<'static> {
        Transmit {
            destination: "127.0.0.1:47000".parse().unwrap(),
            ecn: Some(EcnCodepoint::Ect0),
            contents: b"paquet",
            segment_size: None,
            src_ip: None,
        }
    }

    #[test]
    fn the_mark_is_taken_off_when_asked_and_left_otherwise() {
        let wire = Arc::new(Noting {
            marks: Mutex::new(Vec::new()),
        });
        let as_is: Arc<dyn AsyncUdpSocket> = wire.clone();
        Marking::Ecn
            .applied(as_is.clone())
            .try_send(&marked())
            .unwrap();
        Marking::None.applied(as_is).try_send(&marked()).unwrap();
        assert_eq!(
            *wire.marks.lock().unwrap(),
            vec![Some(EcnCodepoint::Ect0), None]
        );
    }
}
