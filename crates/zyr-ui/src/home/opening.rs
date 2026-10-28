//! What the window shows while a session opens, or comes back.
//!
//! The whole window, because it is the only thing going on, and because
//! it is the last thing read before the session lays its own picture on
//! top: between the two there must never be a moment where one wonders
//! whether it is working. The session says each step; the window is the
//! only one that can say how far along something is that has no picture
//! yet.

use std::time::{Duration, Instant};

use zyr_draw::design;
use zyr_draw::{Align, Rect};
use zyr_i18n::say;
use zyr_proto::fact::Fact;

use crate::home::state::Opening;
use crate::home::ui::{Painter, STEADY};
use crate::shell::app::App;

/// The mark above, and the thread that goes back and forth: its size, and
/// the share of it the moving piece covers.
const MARK: f32 = 64.0;
const THREAD: (f32, f32) = (280.0, 6.0);
const PIECE: f32 = 0.38;

/// How long the piece takes to go across, one way.
const ACROSS: Duration = Duration::from_millis(1400);

/// Draws the opening over a window of that size.
pub fn draw(painter: &mut Painter, width: f32, height: f32) {
    let Some((towards, detail, since)) = painter.state.opening.as_ref().map(|opening| {
        (
            opening.towards.clone(),
            opening.detail.clone(),
            opening.since,
        )
    }) else {
        return;
    };
    let colours = painter.colours;
    let shown = painter.arriving("opening", STEADY);
    let whole = Rect::at(0.0, 0.0, width, height);
    painter.fill(whole, 0.0, colours.background.faded(shown));
    painter.absorbs(whole);

    let mark = painter.px(MARK);
    let title_pen = painter.title().aligned(Align::Centre);
    let title = painter.line(title_pen);
    let words_width = (width - painter.px(design::SPACE_6) * 2.0).min(painter.px(440.0));
    let body = painter.body().aligned(Align::Centre);
    let caption = painter.caption().aligned(Align::Centre);
    let towards_height = painter.height_of(&towards, body, words_width);
    let detail_height = painter
        .height_of(&detail, caption, words_width)
        .max(painter.line(caption));
    let (thread_width, thread_height) =
        (painter.px(THREAD.0).min(width * 0.6), painter.px(THREAD.1));
    let gap = painter.px(design::SPACE_4);
    let content = mark
        + gap
        + title
        + painter.px(design::SPACE_1)
        + towards_height
        + gap * 1.5
        + thread_height
        + gap
        + detail_height;

    let centre = width / 2.0;
    let mut y = (height - content) / 2.0 + painter.px(10.0) * (1.0 - shown);
    painter.mark(Rect::at(centre - mark / 2.0, y, mark, mark));
    y += mark + gap;
    painter.text(
        &say!("opening.title"),
        title_pen,
        colours.text.faded(shown),
        Rect::at(0.0, y, width, title),
    );
    y += title + painter.px(design::SPACE_1);
    painter.text(
        &towards,
        body,
        colours.text_soft.faded(shown),
        Rect::at(centre - words_width / 2.0, y, words_width, towards_height),
    );
    y += towards_height + gap * 1.5;

    // A thread the piece goes along, back and forth. It measures nothing:
    // what is waited for here does not cut into percentages, and a bar
    // claiming it does would be lying.
    let track = Rect::at(centre - thread_width / 2.0, y, thread_width, thread_height);
    painter.fill(track, thread_height / 2.0, colours.surface_3.faded(shown));
    let part = since.elapsed().as_secs_f32() / ACROSS.as_secs_f32();
    let along = part.fract() * (1.0 + PIECE * 2.0) - PIECE;
    // Shortened at the ends of the thread rather than cut off there, so
    // that it stays rounded at both ends all the way across.
    let left = (track.left + along * thread_width).max(track.left);
    let right = (track.left + (along + PIECE) * thread_width).min(track.right);
    if right > left {
        let piece = Rect::at(left, y, right - left, thread_height);
        let radius = (thread_height / 2.0).min((right - left) / 2.0);
        painter.fill(piece, radius, colours.accent.faded(shown));
    }
    painter.state.motion.keep_going();
    y += thread_height + gap;

    painter.text(
        &detail,
        caption,
        colours.text_faint.faded(shown),
        Rect::at(centre - words_width / 2.0, y, words_width, detail_height),
    );
}

/// A session starts opening towards that computer, named as its owner
/// knows it rather than by its address: nobody recognises their laptop
/// by four numbers.
pub fn begin(app: &App, towards: &str) {
    {
        let mut state = crate::home::state();
        state.notice = None;
        state.menu = None;
        state.opening = Some(Opening {
            towards: towards.to_string(),
            detail: say!("opening.first_step"),
            since: Instant::now(),
        });
    }
    crate::home::redraw(app);
}

/// One more step of the opening.
pub fn step(app: &App, detail: &Fact) {
    {
        let mut state = crate::home::state();
        let Some(opening) = state.opening.as_mut() else {
            return;
        };
        opening.detail = zyr_i18n::fact(detail);
    }
    crate::home::redraw(app);
}

/// The session dropped by itself, and its picture is coming back.
///
/// The same screen as the opening, since it says the same thing: there
/// is nothing to watch, and something is being done. The attempt is
/// counted from the second on: the first is the ordinary case, where a
/// third says what the thread never will, that it is not going well.
pub fn coming_back(app: &App, attempt: u32) {
    {
        let mut state = crate::home::state();
        let towards = state
            .opening
            .as_ref()
            .map_or_else(String::new, |already| already.towards.clone());
        state.opening = Some(Opening {
            towards,
            detail: if attempt > 1 {
                say!("opening.coming_back_again", attempt = attempt)
            } else {
                say!("opening.coming_back")
            },
            since: Instant::now(),
        });
    }
    crate::home::redraw(app);
}

/// The window has nothing more to tell: what happens now can be read in
/// what the service holds, asked first so that the page under the
/// opening is already up to date when it shows.
pub fn put_the_opening_away(app: &App) {
    let app = app.clone();
    crate::shell::app::spawn(async move {
        crate::home::seen::again(&app).await;
        crate::home::state().opening = None;
        crate::home::redraw(&app);
    });
}

/// A session that ended badly, or that could not open.
pub fn failed(app: &App, why: &Fact) {
    crate::home::state().tell_trouble(why);
    put_the_opening_away(app);
}
