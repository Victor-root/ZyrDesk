//! What the tests share: the access their links are made with.

use crate::Access;

/// Every account signed in, whatever the way: Windows' "Authenticated
/// Users".
const SIGNED_IN: &str = "S-1-5-11";

/// Access every test link is made with: the system, and every account
/// signed in.
///
/// A test runs under an ordinary account, never the system's: a link
/// made for the system alone, as the engine's is, refuses it on Windows,
/// and the test then waits for a connection that never comes. Every
/// account signed in rather than those at the machine, so a test reaches
/// its own link when run at the machine as well as by a service or over
/// the network, where nobody counts as logged in at it.
pub fn in_tests() -> Access {
    Access::SystemAnd {
        user_sid: SIGNED_IN.to_string(),
    }
}
