//! The port a computer is reached on, and how long it may stay silent
//! before a session is given up.

use std::time::Duration;

/// The one port a computer opens to be reachable.
///
/// Everything a session needs travels through it: the engine's picture,
/// sound and control, and ZyrDesk's own questions beside them, all
/// multiplexed inside a single encrypted connection. That is what makes
/// one firewall rule enough.
pub const TUNNEL_PORT: u16 = 47000;

/// How long a computer may go completely unheard before the session it
/// carries is given up.
///
/// Written once, here, because more than one part of a session holds that
/// patience and the shortest of them decides for all: the tunnel gives a
/// connection up after it, and the host engine lets go of every key and
/// button still held once its player has been silent that long. When each
/// part chose its own, every hiccup longer than the shortest ended a
/// session the tunnel would have carried through (D138).
///
/// Half a minute is what a road between two homes takes to come back
/// from a box dropping its translations, and it is not a wait anybody
/// sits through by accident: a session that is really over is closed
/// from the window, and the picture says it is frozen long before.
pub const UNHEARD_LIMIT: Duration = Duration::from_secs(30);
