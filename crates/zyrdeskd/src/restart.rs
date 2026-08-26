//! What to do when the host engine stops.
//!
//! A service that blindly restarts an engine dying at every start spins
//! in a loop and hides the fault: instead of a clear message, the user
//! sees a computer that never gets there. A service that restarts
//! nothing, on the other hand, abandons a machine a passing incident
//! would have been enough to put back on its feet.
//!
//! The rule tells four cases apart. An engine that asked to be left where
//! it is is not an incident. An engine taken away with the session it
//! lived in is not one either, and is the one that has to be handled with
//! care. An engine falling after holding for a long time deserves an
//! immediate restart. An engine falling the moment it starts, several
//! times over, will not be saved by one more attempt.
//!
//! This module knows neither Windows nor processes: it decides, from
//! what it is told.

use std::time::Duration;

/// Code the engine returns when it asks to be left where it is.
///
/// It is the system's own, `ERROR_SHUTDOWN_IN_PROGRESS`. The upstream
/// engine returns it from its own tray, whose « quit » means quit rather
/// than start again, and the upstream service leans on it for that.
///
/// It is not what a machine going down looks like from here, and reading
/// it as such was a mistake that cost a screen. This product runs its
/// engine with no tray at all, so nothing on a ZyrDesk computer has ever
/// returned this. It stays because the engine may return it one day and
/// because leaving it alone would still be the right answer.
pub const ENGINE_ASKED_TO_BE_LEFT: i32 = 1115;

/// Code Windows leaves on a process it took away itself.
///
/// `DBG_TERMINATE_PROCESS`. Neither a fault nor a fall: it is what the
/// session a process lives in leaves behind when it takes that process
/// with it. Three things do that, and nothing about the code tells them
/// apart: somebody signing out, somebody switching user, and the machine
/// going down.
///
/// The third is why this matters more than it looks. What the host's
/// screens were before a session is written down by the engine and
/// nowhere else, and it is spent by the next engine that manages to put
/// them back. An engine started into a machine already halfway out of the
/// door spends that record on a machine about to stop having screens at
/// all, and the morning after there is nothing left to put anything back
/// from. That is exactly what happened on the bench: two engines started
/// in the five seconds a computer took to go, and its screen never came
/// home.
pub const TAKEN_WITH_ITS_SESSION: i32 = 0x4001_0004;

/// How long the machine is left to itself the first time the session
/// takes the engine.
///
/// Long enough to outlast a computer going down, which is the case that
/// must start no engine at all: the machine goes during the wait, and
/// nothing of ours runs again to spend what the next start will need.
/// Short enough that somebody merely switching user waits a few seconds
/// for a computer nobody is asking for yet. The upstream service holds
/// the same shape for the same reason, waiting before every start of its
/// engine rather than only after a fall.
const SESSION_TOOK_IT: Duration = Duration::from_secs(10);

/// Past this, the engine counts as having held: the failure counter
/// goes back to zero.
const HEALTHY_LIFE: Duration = Duration::from_secs(60);

/// Delay before the first restart after a failure.
const INITIAL_DELAY: Duration = Duration::from_secs(2);

/// Ceiling on the restart delay. Beyond it, the wait would be more
/// painful than the fault.
const MAXIMUM_DELAY: Duration = Duration::from_secs(60);

/// Closely spaced failures allowed before giving up.
const MAX_FAILURES: u32 = 5;

/// What ought to happen after the engine stops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Next {
    /// Restart after this delay.
    Restart(Duration),
    /// Give up and say so: the engine will not stand.
    GiveUp,
    /// Do nothing: the stop was wanted.
    Finish,
}

/// Decides what comes next, remembering recent failures.
#[derive(Debug, Default)]
pub struct Policy {
    failures: u32,
    /// Times in a row the session has taken the engine away.
    ///
    /// Once is somebody switching user. Twice in a row is a computer on
    /// its way out, taking every engine started behind the last, and each
    /// of those spends the one record of what the screens were before the
    /// session. So the wait grows, and on a slow machine it outlasts the
    /// shutdown instead of feeding it.
    takings: u32,
}

impl Policy {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of closely spaced failures piled up.
    pub fn failures(&self) -> u32 {
        self.failures
    }

    /// Decides what comes next after the engine stops.
    ///
    /// `code` is the exit code, absent when the engine was interrupted.
    /// `lifetime` is how long it held.
    pub fn after_stop(&mut self, code: Option<i32>, lifetime: Duration) -> Next {
        if code == Some(ENGINE_ASKED_TO_BE_LEFT) {
            return Next::Finish;
        }

        // Taken with its session, which is a thing that happens to a
        // healthy engine and says nothing against it: the failure count
        // is wiped like after any other good life. What it does not get
        // is the immediate restart below, and that is the whole point.
        if code == Some(TAKEN_WITH_ITS_SESSION) {
            self.failures = 0;
            self.takings += 1;
            return Next::Restart(after_a_taking(self.takings));
        }
        self.takings = 0;

        // An engine that held its time is not at fault: whatever just
        // felled it is isolated, and the slate is wiped.
        if lifetime >= HEALTHY_LIFE {
            self.failures = 0;
            return Next::Restart(Duration::ZERO);
        }

        self.failures += 1;
        if self.failures > MAX_FAILURES {
            return Next::GiveUp;
        }
        Next::Restart(delay(self.failures))
    }
}

/// Wait after the session has taken the engine `takings` times running.
///
/// Doubling, like the one below and for a kindred reason: what makes it
/// happen again is the very thing that made it happen, and a wait that
/// did not grow would keep handing engines to a machine that is going.
fn after_a_taking(takings: u32) -> Duration {
    let factor = 1u32 << takings.saturating_sub(1).min(16);
    SESSION_TOOK_IT.saturating_mul(factor).min(MAXIMUM_DELAY)
}

/// Delay before restart number `failure`, doubling each time.
fn delay(failure: u32) -> Duration {
    let factor = 1u32 << (failure.saturating_sub(1)).min(16);
    INITIAL_DELAY.saturating_mul(factor).min(MAXIMUM_DELAY)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seconds(count: u64) -> Duration {
        Duration::from_secs(count)
    }

    #[test]
    fn an_engine_asking_to_be_left_where_it_is_gets_its_wish() {
        let mut policy = Policy::new();
        // Même tombé tout de suite : il a demandé, on ne le relance pas.
        assert_eq!(
            policy.after_stop(Some(ENGINE_ASKED_TO_BE_LEFT), Duration::ZERO),
            Next::Finish
        );
        assert_eq!(policy.failures(), 0);
    }

    #[test]
    fn an_engine_taken_with_its_session_is_never_started_again_on_the_spot() {
        // C'est le défaut qui a coûté un écran. Un ordinateur qui s'éteint
        // emporte le moteur, et ce qu'il savait de l'écran d'avant la
        // session ne vit que dans ce qu'il a écrit : un moteur relancé
        // dans la seconde le dépense sur une machine qui n'aura bientôt
        // plus d'écrans du tout, et le lendemain il n'y a plus rien à
        // remettre. L'attente doit durer plus longtemps qu'une extinction.
        let mut policy = Policy::new();
        let Next::Restart(wait) = policy.after_stop(Some(TAKEN_WITH_ITS_SESSION), HEALTHY_LIFE)
        else {
            panic!("emporté avec sa session n'est pas une panne");
        };
        assert!(wait >= seconds(5), "{wait:?} ne couvre pas une extinction");
        // Et ce n'est pas une faute : le compteur ne bouge pas, sinon une
        // machine où l'on change d'utilisateur finirait par renoncer.
        assert_eq!(policy.failures(), 0);

        // Y compris quand le moteur venait tout juste de démarrer, ce qui
        // est le cas du deuxième et du troisième pendant une extinction.
        let mut policy = Policy::new();
        assert_eq!(
            policy.after_stop(Some(TAKEN_WITH_ITS_SESSION), seconds(2)),
            Next::Restart(wait)
        );
        assert_eq!(policy.failures(), 0);
    }

    #[test]
    fn a_computer_that_keeps_taking_the_engine_is_waited_out_longer_each_time() {
        // Une extinction emporte aussi tous les moteurs démarrés derrière
        // le premier. Une attente qui ne grandirait pas continuerait à en
        // fournir à une machine qui s'en va, et chacun dépense ce dont le
        // démarrage suivant aura besoin.
        let mut policy = Policy::new();
        let mut previous = Duration::ZERO;
        for _ in 0..4 {
            let Next::Restart(wait) = policy.after_stop(Some(TAKEN_WITH_ITS_SESSION), seconds(2))
            else {
                panic!("emporté avec sa session n'est jamais un abandon");
            };
            assert!(wait > previous || wait == MAXIMUM_DELAY, "{wait:?}");
            assert!(wait <= MAXIMUM_DELAY);
            previous = wait;
        }
        assert!(previous > SESSION_TOOK_IT, "l'attente n'a jamais grandi");

        // Un moteur qui tient sa vie remet tout à zéro : un changement
        // d'utilisateur des semaines plus tard repart d'une attente courte.
        policy.after_stop(Some(1), HEALTHY_LIFE);
        assert_eq!(
            policy.after_stop(Some(TAKEN_WITH_ITS_SESSION), seconds(2)),
            Next::Restart(SESSION_TOOK_IT)
        );
    }

    #[test]
    fn an_engine_that_held_starts_again_at_once() {
        let mut policy = Policy::new();
        assert_eq!(
            policy.after_stop(Some(1), HEALTHY_LIFE),
            Next::Restart(Duration::ZERO)
        );
    }

    #[test]
    fn closely_spaced_failures_space_the_restarts_out() {
        let mut policy = Policy::new();
        let mut previous = Duration::ZERO;
        for _ in 0..MAX_FAILURES {
            let Next::Restart(delay) = policy.after_stop(Some(1), seconds(1)) else {
                panic!("the engine should still have its chance");
            };
            assert!(
                delay >= previous,
                "{delay:?} after {previous:?}: the wait has to grow"
            );
            assert!(delay <= MAXIMUM_DELAY);
            previous = delay;
        }
        assert!(previous > INITIAL_DELAY, "the wait never grew at all");
    }

    #[test]
    fn an_engine_that_never_holds_is_eventually_left_alone() {
        let mut policy = Policy::new();
        for _ in 0..MAX_FAILURES {
            assert!(matches!(
                policy.after_stop(Some(1), seconds(1)),
                Next::Restart(_)
            ));
        }
        assert_eq!(policy.after_stop(Some(1), seconds(1)), Next::GiveUp);
    }

    #[test]
    fn one_success_wipes_the_earlier_failures() {
        let mut policy = Policy::new();
        for _ in 0..MAX_FAILURES {
            policy.after_stop(Some(1), seconds(1));
        }
        assert_eq!(policy.failures(), MAX_FAILURES);

        // The engine starts again, holds its time, then falls once more:
        // it has to get all of its chances back, or a machine left on for
        // weeks would end up restarting nothing.
        policy.after_stop(Some(1), HEALTHY_LIFE);
        assert_eq!(policy.failures(), 0);
        assert!(matches!(
            policy.after_stop(Some(1), seconds(1)),
            Next::Restart(_)
        ));
    }

    #[test]
    fn an_engine_interrupted_without_a_code_counts_as_a_failure() {
        let mut policy = Policy::new();
        assert!(matches!(
            policy.after_stop(None, seconds(1)),
            Next::Restart(_)
        ));
        assert_eq!(policy.failures(), 1);
    }

    #[test]
    fn the_delay_never_overflows() {
        // The shift that doubles the delay must not run wild on an
        // absurd value.
        for failure in [0u32, 1, 5, 100, u32::MAX] {
            assert!(delay(failure) <= MAXIMUM_DELAY, "failure {failure}");
        }
    }
}
