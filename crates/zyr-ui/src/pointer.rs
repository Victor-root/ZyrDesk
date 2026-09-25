//! The shape of the far computer's pointer, kept up to date during a
//! session.
//!
//! The pointer a hand follows is drawn here, with no network in between,
//! and that is the whole point: the far computer's pointer is its answer
//! to a movement that has crossed over twice. But a desktop says what a
//! click is going to do through the shape of the pointer and almost
//! nothing else, and that shape exists only over there.
//!
//! So it is asked for, several times a second while a session is on the
//! screen, and kept for the picture's window, which puts it on whenever
//! the system asks what pointer to draw over it. Only the last answer is
//! kept: a shape is worth nothing a moment later.
//!
//! A single connection to the service for the whole session, and not one
//! per question as everywhere else in this window: elsewhere it is a
//! question now and then, here it is twenty a second.

// A session only exists on Windows, and this loop with it.
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::time::Duration;

use zyr_control::{Answer, Request, Service, WayId};
use zyr_proto::session::Pointer;

use crate::app::App;

/// What this module's lines are filed under.
const TAG: &str = "pointer";

/// Writes a line under this module's tag.
fn note(what: &str) {
    crate::journal::note_about(TAG, what);
}

/// How many times a second the shape is asked for.
///
/// Twenty: a hand that enters a text field sees the bar appear within
/// fifty milliseconds, which is below what an eye can tell apart, and it
/// is twenty round trips a second on a channel already open rather than
/// sixty.
const ASK_EVERY: Duration = Duration::from_millis(50);

/// How many refusals in a row before giving up.
///
/// A single refusal is a way that has just closed or a service that is
/// restarting. Several in a row mean that computer does not know how to
/// answer, which is the case of a far machine older than this one: the
/// session carries on perfectly well without it, with the ordinary
/// arrow, and there is no reason to pester it.
const REFUSALS_BEFORE_GIVING_UP: u32 = 20;

/// True while the loop is running.
static FOLLOWING: AtomicBool = AtomicBool::new(false);

/// The far pointer's shape, by its rank in `Pointer::ALL`.
///
/// A number and not a lock: it is read by the picture's window every
/// time the pointer moves over it.
static SHAPE: AtomicU8 = AtomicU8::new(0);

/// The shape the pointer takes over the picture right now.
pub fn the_far_shape() -> Pointer {
    Pointer::ALL
        .get(usize::from(SHAPE.load(Ordering::Relaxed)))
        .copied()
        .unwrap_or_default()
}

/// Keeps that shape, and tells the picture when it changed.
fn keep(shape: Pointer) {
    let rank = Pointer::ALL
        .iter()
        .position(|each| *each == shape)
        .unwrap_or(0) as u8;
    if SHAPE.swap(rank, Ordering::Relaxed) != rank {
        crate::video::the_pointer_changed();
    }
}

/// Follows the shape of the far computer's pointer until the end of
/// the session.
///
/// Called at every turn of the watch: it does nothing while a loop is
/// already running, and starts a new one when the previous one has
/// stopped, which happens at the end of a session as well as when a
/// picture opens.
pub fn follow(app: &App) {
    if FOLLOWING.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    crate::app::spawn(async move {
        let seen = keep_it_in_step(&app).await;
        // The next session starts from the ordinary pointer, and not
        // from whatever this one was left pointing at.
        keep(Pointer::Arrow);
        note(&format!("forme du curseur : {seen}"));
        FOLLOWING.store(false, Ordering::SeqCst);
    });
}

/// The loop itself, and what it saw go by.
async fn keep_it_in_step(app: &App) -> Seen {
    let mut seen = Seen::default();
    let mut way = None;
    let mut talking = None;
    let mut refused = 0;
    loop {
        tokio::time::sleep(ASK_EVERY).await;
        if !crate::floating::a_session_is_up(app) {
            seen.why = "la session est terminée";
            return seen;
        }
        // The way is looked for at every turn for as long as it is
        // missing, and not once at the start. The service only lists a
        // session once its first picture is up, and this loop can start
        // just before.
        let asking = match way {
            Some(known) => known,
            None => match crate::session::the_way_in_use().await {
                Some(found) => {
                    way = Some(found);
                    found
                }
                None => continue,
            },
        };
        // In game mode, the game draws its own pointer and the one here
        // is hidden: asking for a shape nobody will show would be twenty
        // round trips a second for nothing. The loop stays alive, because
        // the person can come back to the desktop without closing.
        if crate::video::in_a_game() {
            continue;
        }
        match asked(&mut talking, asking).await {
            Ok(shape) => {
                refused = 0;
                seen.saw(shape);
                keep(shape);
            }
            Err(reason) => {
                // The connection is thrown away, and the way forgotten:
                // a refusal often comes from a service that has
                // restarted or a picture that came back, and the way is
                // then a different one.
                talking = None;
                way = None;
                refused += 1;
                seen.first_refusal.get_or_insert(reason);
                if refused >= REFUSALS_BEFORE_GIVING_UP {
                    seen.why = "l'ordinateur distant ne répond pas sur la forme de son curseur";
                    return seen;
                }
            }
        }
    }
}

/// One question, on the connection being held, reopened when it has
/// given way.
async fn asked(talking: &mut Option<Service>, way: WayId) -> Result<Pointer, String> {
    if talking.is_none() {
        *talking = Some(Service::join().await.map_err(|e| e.to_string())?);
    }
    let service = talking.as_mut().expect("une connexion au service");
    match service
        .ask(&Request::FarPointer { way })
        .await
        .map_err(|e| e.to_string())?
    {
        Answer::Pointer(shape) => Ok(shape),
        Answer::Refused(reason) => Err(reason),
        other => Err(crate::service::unexpected(other)),
    }
}

/// What the loop saw, for the journal.
///
/// One line per session, and it answers the two questions a pointer
/// stuck as an arrow raises: was anything received at all, and did
/// anything other than an arrow come through. A line per shape received
/// would make twenty a second and would answer neither.
#[derive(Default)]
struct Seen {
    answers: u64,
    shapes: Vec<Pointer>,
    first_refusal: Option<String>,
    why: &'static str,
}

impl Seen {
    fn saw(&mut self, shape: Pointer) {
        self.answers += 1;
        if !self.shapes.contains(&shape) {
            self.shapes.push(shape);
        }
    }
}

impl std::fmt::Display for Seen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} reçues", self.answers)?;
        if self.shapes.is_empty() {
            f.write_str(", aucune forme")?;
        } else {
            f.write_str(", formes vues :")?;
            for shape in &self.shapes {
                write!(f, " {shape}")?;
            }
        }
        if let Some(refusal) = &self.first_refusal {
            write!(f, " ; premier refus : {}", refusal.replace('\n', " "))?;
        }
        if !self.why.is_empty() {
            write!(f, " ; arrêt : {}", self.why)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_journal_line_says_what_is_missing() {
        // It is there for a pointer stuck as an arrow: it must
        // tell "nothing arrived" apart from "everything arrived
        // and it was all arrows".
        let nothing = Seen {
            why: "la session est terminée",
            ..Default::default()
        };
        let said = nothing.to_string();
        assert!(said.contains("0 reçues"), "{said}");
        assert!(said.contains("aucune forme"), "{said}");

        let mut seen = Seen::default();
        seen.saw(Pointer::Arrow);
        seen.saw(Pointer::Text);
        seen.saw(Pointer::Arrow);
        let said = seen.to_string();
        assert!(said.contains("3 reçues"), "{said}");
        assert!(said.contains("arrow text"), "{said}");

        // And a refusal is told on a single line: the journal lines up
        // its lines, and a reason that wraps would break the column.
        let refused = Seen {
            first_refusal: Some("la voie 3\n  n'existe plus".to_string()),
            ..Default::default()
        };
        let said = refused.to_string();
        assert!(
            said.contains("premier refus : la voie 3   n'existe plus"),
            "{said}"
        );
        assert_eq!(said.lines().count(), 1, "{said}");
    }
}
