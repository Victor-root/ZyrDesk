//! The link of a device to an account, and everything that goes through
//! it.
//!
//! One crate touches the network towards a server, and it is this one:
//! attaching the device, the requests of the account, and the live
//! channel that carries presence and the rendezvous of a session. The
//! service holds it when a link exists and nothing at all when none
//! does; a product without a link contains no road to any server.
//!
//! Every connection is TLS, and only TLS. A server that a public
//! authority vouches for is taken as any browser would; a server nobody
//! vouches for is taken on the fingerprint of its key, once a person has
//! compared it, and on nothing else afterwards. That trust lives in the
//! transport, where the server borrows it to check itself.

pub mod address;
pub mod attach;
pub mod link;
pub mod live;
pub mod rest;

pub use address::{BadAddress, normalized};
pub use attach::{AttachError, Credentials, Registering, attach};
pub use link::Link;
pub use live::{Event, Live, Snapshot, Start};
pub use rest::{Failure, Rest};
pub use zyr_transport::trust::{Trust, Untrusted};
