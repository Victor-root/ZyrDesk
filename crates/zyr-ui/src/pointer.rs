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
//! screen, and written into the file the player follows. Nothing is kept
//! from one question to the next: a shape is worth nothing a moment
//! later.
//!
//! A single connection to the service for the whole session, and not one
//! per question as everywhere else in this window: elsewhere it is a
//! question now and then, here it is twenty a second.

// A session only exists on Windows, and this loop with it.
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::atomic::{AtomicBool, Ordering};
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
        zyr_session::point_like_nothing();
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
    let mut in_a_game = false;
    loop {
        tokio::time::sleep(ASK_EVERY).await;
        if !crate::floating::a_session_is_up(app) {
            seen.why = "la session est terminée";
            return seen;
        }
        // The way is looked for at every turn for as long as it is
        // missing, and not once at the start. The service only knows
        // about a session once the player has been handed over: this
        // loop starts well before that, and giving up there meant asking
        // nothing during the six seconds a session takes to be believed.
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
        //
        // The ordinary arrow is set on the way out, and never the last
        // shape received. The player keeps whichever one it is left with,
        // and one of the thirteen is not a shape at all: "theirs" is an
        // empty shape, for the moments when the far computer draws its
        // pointer itself. Left there, it makes invisible any pointer this
        // player would show afterwards.
        if crate::floating::in_game_mouse(app) {
            if !in_a_game {
                in_a_game = true;
                zyr_session::point_like_nothing();
            }
            continue;
        }
        in_a_game = false;
        match asked(&mut talking, asking).await {
            Ok(shape) => {
                refused = 0;
                seen.saw(shape);
                if let Err(reason) = zyr_session::point_like(shape) {
                    seen.why = "la forme n'a pas pu être écrite pour le lecteur";
                    note(&format!("forme du curseur non écrite : {reason}"));
                    return seen;
                }
            }
            Err(reason) => {
                // The connection is thrown away, and the way forgotten:
                // a refusal often comes from a service that has
                // restarted or a picture that was relaunched, and the
                // way is then a different one.
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
