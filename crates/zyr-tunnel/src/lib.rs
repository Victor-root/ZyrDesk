//! Carrying one engine's local link to the other computer, and ZyrDesk's
//! own questions beside it, in a single connection.

pub mod aside;
pub mod channel;
pub mod frame;
pub mod pump;
pub mod queue;
pub mod service;
pub mod tunnel;

pub use aside::{Answers, Opening, Question, Told};
pub use channel::{DatagramChannel, StreamChannel};
pub use pump::{Counters, Reading, nudge};
pub use service::{ServiceEnd, ServiceSide, service_channel};
pub use tunnel::Tunnel;
