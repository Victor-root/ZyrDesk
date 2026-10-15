//! Asking the person at this computer whether a contact may come in.
//!
//! A contact's computer lent without « without asking me » waits at the
//! door, its session not yet opened, until someone at this computer says
//! yes or no. The window shows the question, and takes Ctrl+F1 for yes;
//! the service only holds it, and the answer when it comes. Nobody
//! answering within half a minute is a no: a computer nobody is in front
//! of lets in no one who had to be let in.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::oneshot;
use zyr_proto::fact::Fact;
use zyr_proto::rights::Rights;

/// How long a contact waits at the door for an answer.
pub const WITHIN: Duration = Duration::from_secs(30);

/// What the person at this computer said.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answered {
    Yes,
    No,
    /// Nobody said anything in time.
    Nothing,
}

/// The contacts waiting at the door, shared by the door and the window's
/// channel.
#[derive(Clone, Default)]
pub struct Accord(Arc<Mutex<Asking>>);

#[derive(Default)]
struct Asking {
    /// What the next knock is named by: never one that was used.
    next: u64,
    waiting: Vec<Waiting>,
}

struct Waiting {
    id: u64,
    who: String,
    rights: Rights,
    until: Instant,
    answer: oneshot::Sender<bool>,
}

/// Takes a knock off the list however its asking ends, its far end
/// leaving halfway included.
struct Gone<'a> {
    accord: &'a Accord,
    id: u64,
}

impl Drop for Gone<'_> {
    fn drop(&mut self) {
        self.accord
            .0
            .lock()
            .expect("knocks")
            .waiting
            .retain(|waiting| waiting.id != self.id);
    }
}

impl Accord {
    /// Asks whether `who`, who would be lent `rights`, may come in, and
    /// waits at most `within` for the answer.
    pub async fn ask(&self, who: &str, rights: Rights, within: Duration) -> Answered {
        let (answer, answered) = oneshot::channel();
        let id = {
            let mut asking = self.0.lock().expect("knocks");
            asking.next += 1;
            let id = asking.next;
            asking.waiting.push(Waiting {
                id,
                who: who.to_string(),
                rights,
                until: Instant::now() + within,
                answer,
            });
            id
        };
        let _gone = Gone { accord: self, id };
        match tokio::time::timeout(within, answered).await {
            Ok(Ok(true)) => Answered::Yes,
            Ok(Ok(false)) => Answered::No,
            _ => Answered::Nothing,
        }
    }

    /// The contacts waiting, the first come first.
    pub fn waiting(&self) -> Vec<zyr_control::Knock> {
        let now = Instant::now();
        self.0
            .lock()
            .expect("knocks")
            .waiting
            .iter()
            .map(|waiting| zyr_control::Knock {
                id: waiting.id,
                who: waiting.who.clone(),
                rights: waiting.rights,
                left: waiting.until.saturating_duration_since(now),
            })
            .collect()
    }

    /// Gives the answer to that knock. A knock no longer waiting is said:
    /// its contact left, or was turned away for want of an answer.
    pub fn answer(&self, knock: u64, yes: bool) -> Result<(), Fact> {
        let mut asking = self.0.lock().expect("knocks");
        let at = asking
            .waiting
            .iter()
            .position(|waiting| waiting.id == knock)
            .ok_or_else(|| Fact::new("accord.gone"))?;
        let waiting = asking.waiting.remove(at);
        let _ = waiting.answer.send(yes);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_contact_waits_until_someone_here_says_yes() {
        let accord = Accord::default();
        let asking = {
            let accord = accord.clone();
            tokio::spawn(async move { accord.ask("ami", Rights::WATCHING, WITHIN).await })
        };
        let knock = loop {
            if let Some(knock) = accord.waiting().first().cloned() {
                break knock;
            }
            tokio::task::yield_now().await;
        };
        assert_eq!(knock.who, "ami");
        assert!(knock.left <= WITHIN);
        accord.answer(knock.id, true).unwrap();
        assert_eq!(asking.await.unwrap(), Answered::Yes);
        // Answered, it waits no more, and a second answer finds nothing.
        assert!(accord.waiting().is_empty());
        assert_eq!(
            accord.answer(knock.id, false).unwrap_err().code(),
            "accord.gone"
        );
    }

    #[tokio::test]
    async fn nobody_answering_in_time_is_a_no() {
        let accord = Accord::default();
        assert_eq!(
            accord
                .ask("ami", Rights::WATCHING, Duration::from_millis(20))
                .await,
            Answered::Nothing
        );
        assert!(accord.waiting().is_empty());
    }

    #[tokio::test]
    async fn a_contact_that_leaves_while_waiting_is_off_the_list() {
        let accord = Accord::default();
        let asking = {
            let accord = accord.clone();
            tokio::spawn(async move { accord.ask("ami", Rights::WATCHING, WITHIN).await })
        };
        while accord.waiting().is_empty() {
            tokio::task::yield_now().await;
        }
        asking.abort();
        let _ = asking.await;
        assert!(accord.waiting().is_empty());
    }
}
