//! The page of activity: the sessions that took place, day by day; and
//! the line of one session, which the home page's recent sessions use
//! as well.

use zyr_control::Past;
use zyr_draw::Rect;
use zyr_draw::design;
use zyr_draw::icons;
use zyr_i18n::say;

use crate::home::pages::computers::empty_place;
use crate::home::pages::{page_title, subheading};
use crate::home::seen::Seen;
use crate::home::ui::{Painter, middle, size, square};
use crate::home::words;

/// The widest the page goes, like the settings.
const WIDEST: f32 = 860.0;

/// The height of one session's line.
pub const LINE: f32 = 60.0;

/// Draws the page within `at`, from its top, and gives back the height it
/// takes.
pub fn draw(painter: &mut Painter, at: Rect) -> f32 {
    let seen = painter.seen;
    let width = size(at).0.min(painter.px(WIDEST));
    let at = Rect {
        right: at.left + width,
        ..at
    };
    let mut y = at.top + page_title(painter, at, &say!("activity.title"), &say!("activity.lead"));
    if seen.history.is_empty() {
        y += nothing_yet(painter, Rect::at(at.left, y, width, painter.px(240.0)));
        return y - at.top;
    }
    // Day by day, the latest first: a heading for each day, then its
    // sessions on one card.
    let mut rank = 0;
    while rank < seen.history.len() {
        let day = words::day_of(seen.history[rank].began);
        let count = seen.history[rank..]
            .iter()
            .take_while(|past| words::day_of(past.began) == day)
            .count();
        y += subheading(painter, (at.left, y, width), &day);
        let card = Rect::at(at.left, y, width, painter.px(LINE) * count as f32);
        painter.card(card);
        for (within, past) in seen.history[rank..rank + count].iter().enumerate() {
            let line = Rect::at(
                card.left,
                card.top + painter.px(LINE) * within as f32,
                width,
                painter.px(LINE),
            );
            if within > 0 {
                let inside = painter.px(design::SPACE_5);
                painter.rule(line.left + inside, line.top, width - inside * 2.0);
            }
            past_line(painter, seen, past, line, false);
        }
        y = card.bottom + painter.px(design::SPACE_5);
        rank += count;
    }
    y - at.top
}

/// What the page says before any session has ended.
fn nothing_yet(painter: &mut Painter, at: Rect) -> f32 {
    empty_place(
        painter,
        at,
        (
            &icons::ACTIVITY,
            &say!("activity.none"),
            &say!("activity.none_text"),
        ),
        None,
    )
}

/// One session that took place, across `at`: which way it went, the
/// computer, when, and how long it lasted. With `day`, the day is said
/// beside the hour, for a list that is not under the day's heading.
pub fn past_line(painter: &mut Painter, seen: &Seen, past: &Past, at: Rect, day: bool) {
    let colours = painter.colours;
    let inside = painter.px(design::SPACE_5);
    let (_, centre) = middle(at);
    let disc = painter.px(34.0);
    let disc_at = Rect::at(at.left + inside, centre - disc / 2.0, disc, disc);
    let (icon, tint, direction, tip) = if past.outgoing {
        (
            &icons::ARROW_RIGHT,
            colours.accent,
            say!("activity.outgoing"),
            say!("activity.outgoing_tip"),
        )
    } else {
        (
            &icons::BACK,
            colours.road_internet,
            say!("activity.incoming"),
            say!("activity.incoming_tip"),
        )
    };
    painter.fill(disc_at, disc / 2.0, tint.faded(0.13));
    painter.icon(icon, square(middle(disc_at), painter.px(18.0)), tint);
    painter.tells(disc_at, tip);

    // On the right, how long it lasted.
    let lasted = words::duration(past.lasted.as_secs());
    let caption = painter.caption();
    let lasted_width = painter.width_of(&lasted, caption) + painter.px(2.0);
    let right = at.right - inside;
    painter.text(
        &lasted,
        caption.in_bold(),
        colours.text_soft,
        Rect::at(right - lasted_width, at.top, lasted_width, size(at).1),
    );

    let left = disc_at.right + painter.px(design::SPACE_3);
    let width = (right - lasted_width - painter.px(design::SPACE_4) - left).max(0.0);
    let name_pen = painter.body().in_bold().ellipsized();
    let (name_height, caption_height) = (painter.line(name_pen), painter.line(caption));
    let top = centre - (name_height + caption_height) / 2.0;
    painter.text(
        &seen.name_of(&past.peer, &past.address),
        name_pen,
        colours.text,
        Rect::at(left, top, width, name_height),
    );
    let when = if day {
        words::when(past.began)
    } else {
        words::hour_of(past.began)
    };
    painter.text(
        &say!("activity.said", direction = direction, when = when),
        caption.ellipsized(),
        colours.text_soft,
        Rect::at(left, top + name_height, width, caption_height),
    );
}
