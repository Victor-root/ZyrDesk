//! Trying several addresses at once, and keeping whichever answers first.
//!
//! A computer often has several addresses, and they are not worth the
//! same at all: one is the cable between two machines, another belongs to
//! a virtual adapter or a VPN that wraps the traffic up and sends it
//! somewhere far away before bringing it back; a relay's name leads to an
//! IPv6 address a machine with broken IPv6 can never reach, beside the
//! IPv4 one it can. Nothing tells them apart by looking: an address is a
//! few numbers, and which of them leads nowhere or the long way round is
//! written nowhere. So they are all tried at once and the first to answer
//! wins, which is the same answer arrived at by measuring instead of
//! guessing. Taking them in turn would cost the whole patience of the
//! transport for every address that leads nowhere.

use std::future::Future;
use std::net::SocketAddr;

/// Tries every candidate at once, and hands back the first that answers.
///
/// The others are dropped where they stand the moment there is a winner.
/// Each refusal is handed to `refused` as it comes, so that what was
/// tried and why it failed can be said; nothing at all is a candidate
/// list where every attempt was refused, or an empty one.
pub async fn first_to_answer<T, E, F, Attempt>(
    candidates: impl IntoIterator<Item = SocketAddr>,
    attempt: F,
    mut refused: impl FnMut(SocketAddr, E),
) -> Option<(SocketAddr, T)>
where
    F: Fn(SocketAddr) -> Attempt,
    Attempt: Future<Output = Result<T, E>> + Send + 'static,
    T: Send + 'static,
    E: Send + 'static,
{
    let mut running = tokio::task::JoinSet::new();
    for address in candidates {
        let trying = attempt(address);
        running.spawn(async move { (address, trying.await) });
    }
    while let Some(finished) = running.join_next().await {
        let Ok((address, outcome)) = finished else {
            continue;
        };
        match outcome {
            Ok(answered) => return Some((address, answered)),
            Err(e) => refused(address, e),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::time::Duration;

    fn at(port: u16) -> SocketAddr {
        SocketAddr::from(([192, 168, 1, 20], port))
    }

    /// An attempt that answers after that long, or refuses.
    async fn after(wait: Duration, answer: Result<u16, &'static str>) -> Result<u16, &'static str> {
        tokio::time::sleep(wait).await;
        answer
    }

    #[tokio::test]
    async fn the_fastest_to_answer_wins_and_the_refusals_are_said() {
        let mut said = Vec::new();
        let won = first_to_answer(
            [at(1), at(2), at(3)],
            |address| match address.port() {
                // The one somebody named first, and the slowest.
                1 => after(Duration::from_secs(60), Ok(1)),
                2 => after(Duration::from_millis(20), Ok(2)),
                _ => after(Duration::ZERO, Err("unreachable")),
            },
            |address, e| said.push(format!("{address}: {e}")),
        )
        .await;
        assert_eq!(won, Some((at(2), 2)));
        assert_eq!(said, ["192.168.1.20:3: unreachable"]);
    }

    #[tokio::test]
    async fn nothing_answering_is_every_refusal_and_no_winner() {
        let mut said = 0;
        let won = first_to_answer(
            [at(1), at(2)],
            |_| after(Duration::ZERO, Err("unreachable")),
            |_, _| said += 1,
        )
        .await;
        assert_eq!(won, None);
        assert_eq!(said, 2);
        // And nothing to try is nobody answering, rather than a wait.
        let won = first_to_answer(
            [],
            |_| after(Duration::ZERO, Ok(1)),
            |_, _: &str| panic!("nothing was tried"),
        )
        .await;
        assert_eq!(won, None);
    }
}
