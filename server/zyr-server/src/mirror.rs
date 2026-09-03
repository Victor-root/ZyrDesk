//! The mirror: the UDP port that tells a device where it is seen from.
//!
//! A device behind a box does not know the address the box gives it.
//! It sends a small question here, from the very socket its tunnel will
//! speak on, and the answer carries the address and port the question
//! came from. That is all: nothing is kept, nothing is signed, and a
//! wrong answer is only a candidate no probe will ever confirm.
//!
//! A server with a relay answers it on the relay's own port, where the
//! same words are said by the doorway the relay stands on. This is the
//! other case, and the one that keeps a server without a relay useful:
//! the mirror is what makes a direct path possible at all.

use std::io;
use std::net::SocketAddr;
use std::sync::Arc;

use tokio::net::UdpSocket;
use tokio::task::JoinHandle;
use zyr_transport::junction::bind_socket;
use zyr_transport::probe;

use crate::journal;

/// The mirror, answering for as long as it is held.
pub struct Mirror {
    address: SocketAddr,
    answering: JoinHandle<()>,
}

impl Mirror {
    /// Binds the port and starts answering.
    ///
    /// To be called from inside the runtime, which the socket registers
    /// with as it is taken over.
    pub fn open(listen: SocketAddr) -> io::Result<Self> {
        let socket = Arc::new(UdpSocket::from_std(bind_socket(listen)?)?);
        let address = socket.local_addr()?;
        let answering = tokio::spawn(answer(socket));
        Ok(Self { address, answering })
    }

    /// Where it listens: what the configuration said or, for a port left
    /// at nought, what the system gave.
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    pub fn stop(&self) {
        self.answering.abort();
    }
}

impl Drop for Mirror {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Answers every question with where it came from, and nothing else to
/// anything else.
async fn answer(socket: Arc<UdpSocket>) {
    let mut buf = [0u8; 1500];
    loop {
        let (count, from) = match socket.recv_from(&mut buf).await {
            Ok(received) => received,
            // A port unreachable answer from an earlier send, or the
            // like: not the end of the mirror.
            Err(e) if e.kind() == io::ErrorKind::ConnectionReset => continue,
            Err(e) => {
                journal::say(format!("mirror stopped: {e}"));
                return;
            }
        };
        if let Some(said) = probe::what_the_mirror_answers(&buf[..count], from) {
            let _ = socket.send_to(&said, from).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn the_mirror_says_where_a_question_came_from() {
        let mirror = Mirror::open("127.0.0.1:0".parse().unwrap()).unwrap();
        let asking = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let nonce = [9, 8, 7, 6, 5, 4, 3, 2];
        asking
            .send_to(&probe::who_am_i(nonce), mirror.address())
            .await
            .unwrap();
        let mut buf = [0u8; 1500];
        let (count, from) =
            tokio::time::timeout(Duration::from_secs(2), asking.recv_from(&mut buf))
                .await
                .unwrap()
                .unwrap();
        assert_eq!(from, mirror.address());
        let Some(probe::Heard::SeenAs {
            nonce: answered,
            seen,
        }) = probe::heard(&buf[..count])
        else {
            panic!("pas une réponse du miroir");
        };
        assert_eq!(answered, nonce);
        assert_eq!(seen, asking.local_addr().unwrap());

        // Tout autre datagramme est ignoré, sans réponse.
        asking.send_to(b"bonjour", mirror.address()).await.unwrap();
        let silence =
            tokio::time::timeout(Duration::from_millis(300), asking.recv_from(&mut buf)).await;
        assert!(silence.is_err(), "le miroir a répondu à autre chose");
    }
}
