//! Who came in through the door, and what they may do on this computer.
//!
//! The computers of its account, and those it was told to trust, written
//! down by hand or met on a network it trusts, may do everything. A
//! contact's computer came in under a share, and may do what that share
//! says: watch and hear always, the keyboard and the mouse if they were
//! lent with it. Read again every second for as long as it is here, so a
//! switch turned at the owner's window takes hold within the second, and
//! a share taken back sends it away.
//!
//! What a contact may not send to the engine is held back on its way
//! there, in the tunnel, by [`HeldBack`]: the engine is told nothing and
//! knows nothing of contacts.

use std::io;
use std::time::Duration;

use tokio::sync::watch;
use zyr_broker::Grant;
use zyr_media::control::{Frames, Moves, ToEngine};
use zyr_media::input::InputEvent;
use zyr_proto::fingerprint::Fingerprint;
use zyr_proto::paths;
use zyr_proto::rights::Rights;
use zyr_transport::{Bytes, authorized};
use zyr_tunnel::Sieve;

use crate::account::Account;
use crate::machine::Machine;

/// How often what a contact may do is read again.
const READ_AGAIN: Duration = Duration::from_secs(1);

/// Who came in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Visitor {
    /// A computer of the account, written down here, or met on a network
    /// this computer trusts.
    Trusted,
    /// A contact's computer, let in under that share.
    Contact { share: String },
}

/// The computers announcing themselves on this local network, when this
/// computer trusts it.
pub fn met_on_a_trusted_network(machine: &Machine) -> Vec<Fingerprint> {
    if !machine.remembered.trust_local_network() {
        return Vec::new();
    }
    machine
        .neighbours
        .peers()
        .into_iter()
        .map(|peer| peer.fingerprint)
        .collect()
}

/// Who that computer is to this one, once let in.
///
/// What this computer was told to trust comes first: a contact's
/// computer written down by hand here was trusted by hand. Nothing when
/// none of the ways in names it any more, which only happens when the
/// list moved between the knock and this question.
pub fn recognised(machine: &Machine, peer: Fingerprint) -> Option<Visitor> {
    let written = authorized::read(&paths::authorized_devices()).unwrap_or_default();
    if written.contains(&peer) || met_on_a_trusted_network(machine).contains(&peer) {
        return Some(Visitor::Trusted);
    }
    match machine.account.admitted_under(peer)? {
        Grant::Owner => Some(Visitor::Trusted),
        Grant::Share { id } => Some(Visitor::Contact { share: id }),
    }
}

/// What a visitor may do right now.
#[derive(Clone)]
pub struct Allowed {
    /// Whose computer it is, for a contact; nothing for one trusted.
    contact: Option<String>,
    rights: watch::Receiver<Rights>,
}

impl Allowed {
    /// Everything, for good: a trusted computer.
    pub fn everything() -> Self {
        let (_, rights) = watch::channel(Rights::ALL);
        Self {
            contact: None,
            rights,
        }
    }

    /// Whose computer it is, for a contact.
    pub fn contact(&self) -> Option<&str> {
        self.contact.as_deref()
    }

    /// The contact who has to be let in by someone at this computer,
    /// when it is one that was not lent this computer « without asking ».
    pub fn waits_for_an_accord(&self) -> Option<&str> {
        self.contact().filter(|_| !self.now().unattended)
    }

    pub fn now(&self) -> Rights {
        *self.rights.borrow()
    }

    /// Refuses what only a trusted computer may ask: this computer's
    /// journal, its picture on the desk, what it has measured.
    pub fn trusted_only(&self) -> Result<(), String> {
        if self.contact.is_some() {
            return Err("not handed to a contact".to_string());
        }
        Ok(())
    }

    /// Refuses what writes into this computer, its clipboard, its files,
    /// its keys, to a contact not lent the keyboard.
    pub fn with_the_keyboard(&self) -> Result<(), String> {
        if !self.now().keyboard {
            return Err("this contact was not lent the keyboard".to_string());
        }
        Ok(())
    }

    /// What holds back, on the engine's stream, what this visitor may
    /// not send. Nothing to hold back for a trusted computer.
    pub fn sieve(&self) -> Option<Box<dyn Sieve>> {
        self.contact.is_some().then(|| {
            Box::new(HeldBack {
                rights: self.rights.clone(),
                frames: Frames::new(),
                had: self.now(),
            }) as Box<dyn Sieve>
        })
    }
}

/// What a contact under that share may do, and what keeps it up to date:
/// a task that ends once the share is gone, which is the moment the
/// contact has to go.
pub fn lent_to_a_contact(
    account: &Account,
    share: &str,
) -> Option<(Allowed, impl Future<Output = ()> + use<>)> {
    let (with, first) = account.lent_under(share)?;
    let (telling, rights) = watch::channel(first);
    let account = account.clone();
    let share = share.to_string();
    let kept_up = async move {
        loop {
            tokio::time::sleep(READ_AGAIN).await;
            match account.lent_under(&share) {
                Some((_, now)) => {
                    telling.send_if_modified(|was| {
                        let moved = *was != now;
                        *was = now;
                        moved
                    });
                }
                None => return,
            }
        }
    };
    Some((
        Allowed {
            contact: Some(with),
            rights,
        },
        kept_up,
    ))
}

/// Holds back, on its way to the engine, what a contact may not send.
///
/// The stream is cut into whole messages and each goes on untouched or
/// not at all, by its kind: the keys without the keyboard, the pointer,
/// the buttons and the wheel without the mouse. When a right is taken
/// back mid-session, everything held down is let go first: a key held
/// at that moment would otherwise stay down on this computer.
struct HeldBack {
    rights: watch::Receiver<Rights>,
    frames: Frames,
    /// What was allowed for the last piece.
    had: Rights,
}

impl Sieve for HeldBack {
    fn sifted(&mut self, piece: Bytes) -> io::Result<Bytes> {
        let now = *self.rights.borrow();
        let mut kept = Vec::with_capacity(piece.len());
        if (self.had.keyboard && !now.keyboard) || (self.had.mouse && !now.mouse) {
            ToEngine::Input(InputEvent::ReleaseAll).write(&mut kept);
        }
        self.had = now;
        self.frames.feed(&piece);
        while let Some(whole) = self.frames.next_whole() {
            let (kind, whole) = whole.map_err(|e| {
                io::Error::other(format!("the player's stream cannot be read any more: {e}"))
            })?;
            let allowed = match ToEngine::moves(kind) {
                Some(Moves::Keyboard) => now.keyboard,
                Some(Moves::Mouse) => now.mouse,
                None => true,
            };
            if allowed {
                kept.extend_from_slice(whole);
            }
        }
        Ok(Bytes::from(kept))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use zyr_media::control::ControlReader;
    use zyr_media::input::Button;

    fn written(messages: &[ToEngine]) -> Bytes {
        let mut bytes = Vec::new();
        for message in messages {
            message.write(&mut bytes);
        }
        Bytes::from(bytes)
    }

    fn read(bytes: &[u8]) -> Vec<ToEngine> {
        let mut reader = ControlReader::<ToEngine>::new();
        reader.feed(bytes);
        reader.map(Result::unwrap).collect()
    }

    fn key() -> ToEngine {
        ToEngine::Input(InputEvent::Key {
            scancode: 30,
            extended: false,
            down: true,
        })
    }

    fn click() -> ToEngine {
        ToEngine::Input(InputEvent::Button {
            button: Button::Left,
            down: true,
        })
    }

    fn held_back(rights: Rights) -> (watch::Sender<Rights>, HeldBack) {
        let (telling, receiving) = watch::channel(rights);
        (
            telling,
            HeldBack {
                rights: receiving,
                frames: Frames::new(),
                had: rights,
            },
        )
    }

    #[test]
    fn a_contact_only_watching_sends_neither_keys_nor_clicks() {
        let (_telling, mut sieve) = held_back(Rights::WATCHING);
        let ping = ToEngine::Ping { sent_us: 7 };
        let passed = sieve
            .sifted(written(&[key(), click(), ping.clone()]))
            .unwrap();
        // What is neither keyboard nor mouse goes on.
        assert_eq!(read(&passed), [ping]);
    }

    #[test]
    fn what_was_lent_passes_whole_even_cut_anywhere() {
        let (_telling, mut sieve) = held_back(Rights {
            keyboard: true,
            mouse: false,
            unattended: false,
        });
        let bytes = written(&[key(), click(), key()]);
        let mut passed = Vec::new();
        for byte in bytes.iter() {
            passed.extend_from_slice(&sieve.sifted(Bytes::copy_from_slice(&[*byte])).unwrap());
        }
        assert_eq!(read(&passed), [key(), key()]);
    }

    #[test]
    fn a_right_taken_back_lets_go_of_what_is_held_and_stops_there() {
        let (telling, mut sieve) = held_back(Rights {
            keyboard: true,
            mouse: true,
            unattended: false,
        });
        assert_eq!(read(&sieve.sifted(written(&[key()])).unwrap()), [key()]);
        telling.send_replace(Rights {
            keyboard: false,
            mouse: true,
            unattended: false,
        });
        assert_eq!(
            read(&sieve.sifted(written(&[key(), click()])).unwrap()),
            [ToEngine::Input(InputEvent::ReleaseAll), click()]
        );
        // Once let go, not again.
        assert_eq!(read(&sieve.sifted(written(&[click()])).unwrap()), [click()]);
    }

    #[test]
    fn a_contact_is_handed_nothing_of_this_computer_and_writes_only_with_the_keyboard() {
        let (telling, rights) = watch::channel(Rights::WATCHING);
        let contact = Allowed {
            contact: Some("ami".to_string()),
            rights,
        };
        assert_eq!(contact.waits_for_an_accord(), Some("ami"));
        assert!(contact.trusted_only().is_err());
        assert!(contact.with_the_keyboard().is_err());
        assert!(contact.sieve().is_some());
        telling.send_replace(Rights::WATCHING.toggled(zyr_proto::rights::Right::Keyboard));
        assert!(contact.with_the_keyboard().is_ok());
        assert!(contact.trusted_only().is_err());
        telling.send_replace(Rights::ALL);
        assert_eq!(contact.waits_for_an_accord(), None);

        // A computer trusted may do all of it, and nothing is held back.
        let trusted = Allowed::everything();
        assert_eq!(trusted.waits_for_an_accord(), None);
        assert!(trusted.trusted_only().is_ok());
        assert!(trusted.with_the_keyboard().is_ok());
        assert!(trusted.sieve().is_none());
    }

    #[test]
    fn a_stream_that_no_longer_says_where_messages_start_ends() {
        let (_telling, mut sieve) = held_back(Rights::ALL);
        assert!(sieve.sifted(Bytes::from_static(&[0, 0, 1])).is_err());
    }
}
