//! The service's own channel on the local link.
//!
//! Beside the engine's control stream and its datagrams, the link carries
//! what the service and the engine, or the service and the player, say to
//! each other. The tunnel does not read any of it: it hands what arrives
//! to the service and writes what the service gives it, and the words
//! themselves belong to `zyr_media::service`.

use tokio::sync::mpsc;
use zyr_transport::Bytes;

/// Service messages that may wait in either direction.
///
/// They are rare, a handful a session and one a second at most, so a
/// queue this deep only fills when whoever should be reading it has
/// stopped: what arrives then is counted and dropped rather than let to
/// hold up the picture on the same link.
pub const WAITING: usize = 64;

/// The tunnel's half: what it writes onto the link, and where it puts
/// what it reads off it.
pub struct ServiceSide {
    pub(crate) outgoing: mpsc::Receiver<Vec<u8>>,
    pub(crate) incoming: mpsc::Sender<Bytes>,
}

/// The service's half.
pub struct ServiceEnd {
    /// What the service has to say, one message per frame.
    pub to_link: mpsc::Sender<Vec<u8>>,
    /// What the engine or the player said, one message per frame. Ends
    /// once the tunnel has gone.
    pub from_link: mpsc::Receiver<Bytes>,
}

/// A service channel, both halves.
pub fn service_channel() -> (ServiceSide, ServiceEnd) {
    let (to_link, outgoing) = mpsc::channel(WAITING);
    let (incoming, from_link) = mpsc::channel(WAITING);
    (
        ServiceSide { outgoing, incoming },
        ServiceEnd { to_link, from_link },
    )
}
