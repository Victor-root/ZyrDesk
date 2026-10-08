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
//!
//! A pointer that a program over there draws for itself has no shape to
//! be named after. Its picture comes with the engine's instead, and is
//! made here into a pointer of this computer, put on in place of the named
//! one.

// A session only exists on Windows, and this loop with it.
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use zyr_control::{Answer, Request, Service, WayId};
use zyr_player::PointerImage;
use zyr_proto::fact::Fact;
use zyr_proto::session::Pointer;

use crate::shell::app::App;

/// What this module's lines are filed under.
const TAG: &str = "pointer";

/// Writes a line under this module's tag.
fn note(what: &str) {
    crate::shell::journal::note_about(TAG, what);
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
        crate::session::video::the_pointer_changed();
    }
}

/// The pointer made of the far one's picture, and the picture it was made
/// of; nought where it could not be made.
///
/// Made once for each picture and not at every movement, which is twenty
/// times a second. One at most is ever kept, whatever the number of
/// sessions: it goes when the next one is made.
static MADE: Mutex<Option<(Arc<PointerImage>, isize)>> = Mutex::new(None);

/// Puts on the pointer made of the far one's picture; false when it could
/// not be made, for the named shape to be put on instead.
///
/// Called on the thread of the picture's window, which puts every pointer
/// on: the one it replaces is let go of there, once it is off.
#[cfg(windows)]
pub fn put_on_its_picture(image: Arc<PointerImage>) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyCursor, HCURSOR, SetCursor};

    let mut made = MADE.lock().expect("the pointer made here");
    let kept = made
        .as_ref()
        .filter(|(kept, _)| Arc::ptr_eq(kept, &image))
        .map(|(_, cursor)| *cursor as HCURSOR);
    let cursor = kept.unwrap_or_else(|| made_of(&image));
    if cursor.is_null() && kept.is_none() {
        note(&format!(
            "the far pointer's picture ({}x{}) could not be made a pointer here",
            image.width, image.height
        ));
    }
    // SAFETY: a pointer made here, put on from the window's thread, then
    // the one it replaces, made here as well and no longer on.
    unsafe {
        if !cursor.is_null() {
            SetCursor(cursor);
        }
        if kept.is_none()
            && let Some((_, before)) = made.replace((image, cursor as isize))
            && before != 0
        {
            DestroyCursor(before as HCURSOR);
        }
    }
    !cursor.is_null()
}

/// A pointer of this computer, of `image`; nought if it cannot be made.
#[cfg(windows)]
fn made_of(image: &PointerImage) -> windows_sys::Win32::UI::WindowsAndMessaging::HCURSOR {
    use windows_sys::Win32::Graphics::Gdi::{CreateBitmap, DeleteObject};
    use windows_sys::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, ICONINFO};

    let (width, height) = (image.width as i32, image.height as i32);
    // SAFETY: two bitmaps of the size and depth their bytes were made
    // for, copied by the system into the pointer, then given back.
    unsafe {
        let mask = CreateBitmap(width, height, 1, 1, image.mask.as_ptr().cast());
        let colour = CreateBitmap(width, height, 1, 32, image.colour.as_ptr().cast());
        let cursor = if mask.is_null() || colour.is_null() {
            std::ptr::null_mut()
        } else {
            CreateIconIndirect(&ICONINFO {
                fIcon: 0,
                xHotspot: image.hot_x,
                yHotspot: image.hot_y,
                hbmMask: mask,
                hbmColor: colour,
            })
        };
        for bitmap in [mask, colour] {
            if !bitmap.is_null() {
                DeleteObject(bitmap);
            }
        }
        cursor
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
    crate::shell::app::spawn(async move {
        let seen = keep_it_in_step(&app).await;
        // The next session starts from the ordinary pointer, and not
        // from whatever this one was left pointing at.
        keep(Pointer::Arrow);
        note(&format!("pointer shape: {seen}"));
        FOLLOWING.store(false, Ordering::SeqCst);
    });
}

/// The loop itself, and what it saw go by.
async fn keep_it_in_step(app: &App) -> Seen {
    let mut seen = Seen::default();
    let mut talking = None;
    let mut refused = 0;
    loop {
        tokio::time::sleep(ASK_EVERY).await;
        if !crate::session::floating::a_session_is_up(app) {
            seen.why = "the session is over";
            return seen;
        }
        // Read again at every turn: a picture that came back travels on
        // a way of its own.
        let Some(way) = crate::session::the_way() else {
            continue;
        };
        // In game mode, the game draws its own pointer and the one here
        // is hidden: asking for a shape nobody will show would be twenty
        // round trips a second for nothing. The loop stays alive, because
        // the person can come back to the desktop without closing.
        if crate::session::video::in_a_game() {
            continue;
        }
        match asked(&mut talking, way).await {
            Ok(shape) => {
                refused = 0;
                seen.saw(shape);
                keep(shape);
            }
            Err(reason) => {
                // The connection is thrown away: a refusal often comes
                // from a service that has restarted.
                talking = None;
                refused += 1;
                seen.first_refusal.get_or_insert(reason);
                if refused >= REFUSALS_BEFORE_GIVING_UP {
                    seen.why = "the far computer does not answer about its pointer";
                    return seen;
                }
            }
        }
    }
}

/// One question, on the connection being held, reopened when it has
/// given way.
async fn asked(talking: &mut Option<Service>, way: WayId) -> Result<Pointer, Fact> {
    if talking.is_none() {
        *talking = Some(Service::join().await.map_err(|e| e.fact())?);
    }
    let service = talking.as_mut().expect("a connection to the service");
    match service
        .ask(&Request::FarPointer { way })
        .await
        .map_err(|e| e.fact())?
    {
        Answer::Pointer(shape) => Ok(shape),
        Answer::Refused(reason) => Err(reason),
        other => Err(other.unexpected()),
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
    first_refusal: Option<Fact>,
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
        write!(f, "{} received", self.answers)?;
        if self.shapes.is_empty() {
            f.write_str(", no shape")?;
        } else {
            f.write_str(", shapes seen:")?;
            for shape in &self.shapes {
                write!(f, " {shape}")?;
            }
        }
        if let Some(refusal) = &self.first_refusal {
            write!(f, "; first refusal: {refusal}")?;
        }
        if !self.why.is_empty() {
            write!(f, "; stopped: {}", self.why)?;
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
            why: "the session is over",
            ..Default::default()
        };
        let said = nothing.to_string();
        assert!(said.contains("0 received"), "{said}");
        assert!(said.contains("no shape"), "{said}");

        let mut seen = Seen::default();
        seen.saw(Pointer::Arrow);
        seen.saw(Pointer::Text);
        seen.saw(Pointer::Arrow);
        let said = seen.to_string();
        assert!(said.contains("3 received"), "{said}");
        assert!(said.contains("arrow text"), "{said}");

        // And a refusal is told on a single line: the journal lines up
        // its lines, and a reason that wraps would break the column.
        let refused = Seen {
            first_refusal: Some(Fact::new("way.gone").with("way", 3)),
            ..Default::default()
        };
        let said = refused.to_string();
        assert!(said.contains("first refusal: way.gone way=3"), "{said}");
        assert_eq!(said.lines().count(), 1, "{said}");
    }
}
