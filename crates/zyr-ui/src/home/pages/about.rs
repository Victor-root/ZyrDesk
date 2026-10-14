//! The « About » page: what ZyrDesk is, under which licence, and what it
//! is made with.
//!
//! Part of it is an obligation (`docs/COMPLIANCE.md`): the version, the
//! way to the source code, the credits, and the full text of every
//! licence, readable from here.

use zyr_draw::Rect;
use zyr_draw::design;
use zyr_draw::icons;
use zyr_i18n::say;

use crate::home::licences::{CREDITS, SOURCE, STUDIED};
use crate::home::pages::{Control, Head, line, page_title, section};
use crate::home::state::Target;
use crate::home::ui::{Button, Kind, Painter, middle, size};

/// The widest the page goes, like the settings.
const WIDEST: f32 = 860.0;

/// Draws the page within `at`, from its top, and gives back the height it
/// takes.
pub fn draw(painter: &mut Painter, at: Rect) -> f32 {
    let width = size(at).0.min(painter.px(WIDEST));
    let at = Rect {
        right: at.left + width,
        ..at
    };
    let between = painter.px(design::SPACE_5);
    let mut y = at.top;
    y += page_title(painter, at, &say!("about.title"), &say!("about.lead"));
    y += product(painter, Rect { top: y, ..at }) + between;
    y += credits(painter, Rect { top: y, ..at }) + between;
    y += studied(painter, Rect { top: y, ..at });
    y - at.top
}

/// ZyrDesk itself: its mark, its name, its build, its licence and the way
/// to its source.
fn product(painter: &mut Painter, at: Rect) -> f32 {
    let colours = painter.colours;
    let inside = painter.px(design::SPACE_6);
    let mark = painter.px(64.0);
    let height = mark + inside * 2.0;
    let card = Rect::at(at.left, at.top, size(at).0, height);
    painter.card(card);
    let (_, centre) = middle(card);
    painter.mark(Rect::at(
        card.left + inside,
        centre - mark / 2.0,
        mark,
        mark,
    ));
    let left = card.left + inside + mark + painter.px(design::SPACE_5);

    let source = say!("about.source");
    let licence = say!("about.licence");
    let button_height = painter.px(34.0);
    let licence_width = painter.button_width(&licence, Some(&icons::LICENSE));
    let source_width = painter.button_width(&source, Some(&icons::EXTERNAL));
    let buttons = licence_width + source_width + painter.px(design::SPACE_2);
    let side_by_side = size(card).0 - (left - card.left) - inside >= buttons + painter.px(240.0);
    let words_width = if side_by_side {
        card.right - inside - buttons - painter.px(design::SPACE_5) - left
    } else {
        card.right - inside - left
    };

    let title = painter.title();
    let caption = painter.caption();
    let (title_height, caption_height) = (painter.line(title), painter.line(caption));
    let words = title_height + painter.px(2.0) + caption_height * 2.0 + painter.px(2.0);
    let mut y = centre - words / 2.0;
    if !side_by_side {
        y -= (button_height + painter.px(design::SPACE_3)) / 2.0;
    }
    painter.text(
        "ZyrDesk",
        title,
        colours.text,
        Rect::at(left, y, words_width, title_height),
    );
    y += title_height + painter.px(2.0);
    painter.text(
        &painter.seen.version,
        caption.monospaced().ellipsized(),
        colours.text_soft,
        Rect::at(left, y, words_width, caption_height),
    );
    y += caption_height + painter.px(2.0);
    painter.text(
        &say!("about.what"),
        caption.ellipsized(),
        colours.text_soft,
        Rect::at(left, y, words_width, caption_height),
    );
    y += caption_height;

    let (mut x, top) = if side_by_side {
        (card.right - inside - buttons, centre - button_height / 2.0)
    } else {
        (left, y + painter.px(design::SPACE_3))
    };
    painter.button(
        Rect::at(x, top, source_width, button_height),
        Target::Link(SOURCE),
        Button::new(&source, Kind::Quiet).with_icon(&icons::EXTERNAL),
    );
    x += source_width + painter.px(design::SPACE_2);
    painter.button(
        Rect::at(x, top, licence_width, button_height),
        Target::Licence(0),
        Button::new(&licence, Kind::Ghost).with_icon(&icons::LICENSE),
    );
    height
}

/// What ZyrDesk is made with, each with its licence, its full text and
/// the way to it.
fn credits(painter: &mut Painter, at: Rect) -> f32 {
    let head = Head {
        icon: &icons::HEART,
        title: say!("about.credits"),
        lead: Some(say!("about.credits_lead")),
    };
    section(painter, at, &head, |painter, room| {
        let (left, width) = (room.left, size(room).0);
        let mut y = room.top;
        for (rank, credit) in CREDITS.iter().enumerate() {
            let caption = say!(
                "about.credit",
                role = zyr_i18n::text(credit.role),
                licence = credit.licence
            );
            y += line(
                painter,
                (left, y, width),
                None,
                credit.name,
                &caption,
                Control::Buttons(vec![
                    (
                        say!("about.read_licence"),
                        Kind::Ghost,
                        Target::Licence(rank + 1),
                        Some(&icons::LICENSE),
                    ),
                    (
                        say!("about.website"),
                        Kind::Quiet,
                        Target::Link(credit.link),
                        Some(&icons::EXTERNAL),
                    ),
                ]),
            );
        }
        y - room.top
    })
}

/// What the engine's design learned from.
fn studied(painter: &mut Painter, at: Rect) -> f32 {
    let head = Head {
        icon: &icons::CODE,
        title: say!("about.studied"),
        lead: Some(say!("about.studied_lead")),
    };
    section(painter, at, &head, |painter, room| {
        let (left, width) = (room.left, size(room).0);
        let mut y = room.top;
        for (name, link) in STUDIED {
            y += line(
                painter,
                (left, y, width),
                None,
                name,
                &say!("about.studied_caption"),
                Control::Buttons(vec![(
                    say!("about.website"),
                    Kind::Quiet,
                    Target::Link(link),
                    Some(&icons::EXTERNAL),
                )]),
            );
        }
        y - room.top
    })
}
