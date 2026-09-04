//! ZyrDesk transport.
//!
//! The tunnel that joins the two computers, and the computation of what
//! can be sent through it in one piece.

pub mod authorized;
pub mod congestion;
pub mod endpoint;
pub mod identity;
pub mod junction;
pub mod mtu;
pub mod path;
pub mod probe;
pub mod relay;
mod sifting;
pub mod trust;

pub use congestion::{FASTEST, Media, MediaController, MediaProfile};
pub use endpoint::{
    Bytes, Carrying, Connection, DatagramError, EndpointError, Knocking, RecvStream, SendStream,
    TunnelEndpoint,
};
pub use identity::{AllowedPeers, Fingerprint, Identity, signed_by};
pub use junction::{Junction, Road, card_of, is_card};
pub use mtu::{PacketSize, packet_size};
pub use path::Path;
pub use relay::{Branch, Doorway, RelayError, Wanted};
