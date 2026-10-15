//! « Add a computer »: writing a computer down by hand, and forgetting
//! those written.
//!
//! Only for a computer that does not show up by itself, on a network that
//! drops the announcements. Each of the two has to know the other, so
//! this computer's own fingerprint and address come first, to be written
//! on the other one. Laid out as « See the details » is: a callout, then
//! groups under their heading, each in its frame.

use zyr_draw::Rect;
use zyr_draw::design;
use zyr_draw::icons;
use zyr_i18n::say;
use zyr_proto::fingerprint::{Fingerprint, WRITTEN_LENGTH};

use super::{
    Action, DISC, PADDING, Shape, callout, disc, fingerprint, framed, group_heading, trouble,
};
use crate::home::state::Target;
use crate::home::ui::fields::{self, Field};
use crate::home::ui::{Button, Kind, Painter, size};
use crate::shell::app::App;

pub fn shape(painter: &Painter) -> Shape {
    let towards = !fields::text(Field::Address).trim().is_empty();
    let busy = towards && painter.seen.busy(painter.state.opening.is_some());
    Shape {
        title: say!("adding.title"),
        lead: Some(say!("adding.lead")),
        width: 780.0,
        left: Vec::new(),
        right: vec![
            Action::new(say!("dialog.cancel"), Kind::Quiet, Target::CloseDialog),
            Action::new(
                if towards {
                    say!("computer.connect")
                } else {
                    say!("adding.allow")
                },
                Kind::Primary,
                Target::Confirm,
            )
            .enabled(typed().is_some() && !busy),
        ],
    }
}

pub fn draw(painter: &mut Painter, at: Rect) -> f32 {
    let seen = painter.seen;
    let colours = painter.colours;
    let (left, width) = (at.left, size(at).0);
    let mut y = at.top;

    let own = seen
        .standing
        .as_ref()
        .map(|said| said.fingerprint.clone())
        .unwrap_or_default();
    let addresses = seen
        .addresses
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    y += callout(
        painter,
        (left, y, width),
        colours.accent,
        |painter, (left, top, width)| {
            let colours = painter.colours;
            let mut y = top;
            let pen = painter.body().in_bold();
            let height = painter.line(pen);
            painter.text(
                &say!("home.this_computer"),
                pen,
                colours.text,
                Rect::at(left, y, width, height),
            );
            y += height + painter.px(2.0);
            y += painter.paragraph(
                &say!("adding.this_computer_lead"),
                painter.caption(),
                colours.text_soft,
                (left, y, width),
            );
            y += painter.px(design::SPACE_3);
            y += fingerprint(painter, (left, y, width), &own);
            if !addresses.is_empty() {
                y += painter.px(design::SPACE_2);
                y += painter.paragraph(
                    &say!("adding.this_address", addresses = addresses),
                    painter.caption(),
                    colours.text_soft,
                    (left, y, width),
                );
            }
            y - top
        },
    );

    y += painter.px(design::SPACE_6);
    y += group_heading(
        painter,
        (left, y, width),
        &icons::LAPTOP,
        &say!("adding.other"),
    );
    y += painter.px(design::SPACE_3);
    y += framed(painter, (left, y, width), the_other);

    // What was written by hand is forgotten here, as on its card.
    let written: Vec<(String, String, String)> = seen
        .peers
        .iter()
        .filter(|peer| peer.written)
        .map(|peer| {
            (
                peer.fingerprint.to_string(),
                peer.name.clone(),
                peer.host.clone(),
            )
        })
        .collect();
    if !written.is_empty() {
        y += painter.px(design::SPACE_6);
        y += group_heading(
            painter,
            (left, y, width),
            &icons::COMPUTERS,
            &say!("adding.written"),
        );
        y += painter.px(design::SPACE_3);
        y += framed(painter, (left, y, width), |painter, (left, top, width)| {
            let mut y = top;
            for (fingerprint, name, host) in &written {
                if y > top {
                    painter.rule(left, y, width);
                }
                y += written_row(painter, (left, y, width), fingerprint, name, host);
            }
            y - top
        });
    }
    y += trouble(painter, (left, y, width));
    y - at.top
}

/// The other computer's fields, in their frame: its fingerprint across,
/// its address and its name side by side when there is the room. Gives
/// back the height they take.
fn the_other(painter: &mut Painter, (left, top, width): (f32, f32, f32)) -> f32 {
    let colours = painter.colours;
    let padding = painter.px(PADDING);
    let gap = painter.px(design::SPACE_4);
    let (left, width) = (left + padding, width - padding * 2.0);
    let mut y = top + padding;
    let hint = fingerprint_hint();
    y += painter.field(
        (left, y, width),
        Field::Fingerprint,
        &say!("adding.fingerprint"),
        hint.as_deref().map(|hint| (hint, colours.warning)),
    );
    y += gap;
    let address_hint = say!("adding.address_hint");
    let address = |painter: &mut Painter, area| {
        painter.field(
            area,
            Field::Address,
            &say!("adding.address"),
            Some((&address_hint, colours.text_soft)),
        )
    };
    let name =
        |painter: &mut Painter, area| painter.field(area, Field::Name, &say!("adding.name"), None);
    if width >= painter.px(520.0) {
        let half = (width - gap) / 2.0;
        y += address(painter, (left, y, half)).max(name(painter, (left + half + gap, y, half)));
    } else {
        y += address(painter, (left, y, width));
        y += gap;
        y += name(painter, (left, y, width));
    }
    y + padding - top
}

/// A computer written down by hand: its icon, its name and its address,
/// and the button that forgets it. Gives back its height.
fn written_row(
    painter: &mut Painter,
    (left, top, width): (f32, f32, f32),
    fingerprint: &str,
    name: &str,
    host: &str,
) -> f32 {
    let colours = painter.colours;
    let padding = painter.px(PADDING);
    let side = painter.px(DISC);
    let height = side + padding * 2.0;
    let centre = top + height / 2.0;
    disc(
        painter,
        Rect::at(left + padding, centre - side / 2.0, side, side),
        &icons::COMPUTERS,
    );

    let forget = Target::Forget(fingerprint.to_string());
    let armed = painter.state.is_armed(&forget);
    let label = if armed {
        say!("dialog.confirm")
    } else {
        say!("computer.forget")
    };
    let button_width = painter.button_width(&label, None);
    let button_height = painter.px(34.0);
    let button = Rect::at(
        left + width - padding - button_width,
        centre - button_height / 2.0,
        button_width,
        button_height,
    );
    painter.button(
        button,
        forget,
        Button::new(&label, if armed { Kind::Warning } else { Kind::Quiet }),
    );

    let words = left + padding + side + painter.px(design::SPACE_3);
    let room = button.left - painter.px(design::SPACE_4) - words;
    let name_pen = painter.body().in_bold().ellipsized();
    let caption = painter.caption().ellipsized();
    let (name_height, host_height) = (painter.line(name_pen), painter.line(caption));
    let words_top = centre - (name_height + painter.px(2.0) + host_height) / 2.0;
    painter.text(
        name,
        name_pen,
        colours.text,
        Rect::at(words, words_top, room, name_height),
    );
    painter.text(
        host,
        caption,
        colours.text_soft,
        Rect::at(
            words,
            words_top + name_height + painter.px(2.0),
            room,
            host_height,
        ),
    );
    height
}

/// The fingerprint written in its field, if it is one. Spaces are let
/// through: a fingerprint read out loud, or copied from where it is
/// shown, comes in groups.
fn typed() -> Option<Fingerprint> {
    without_spaces(&fields::text(Field::Fingerprint))
        .parse()
        .ok()
}

fn without_spaces(text: &str) -> String {
    text.split_whitespace().collect()
}

/// What the fingerprint field says below it while what it holds is not
/// a fingerprint yet: how far along it is, or that it is not one.
fn fingerprint_hint() -> Option<String> {
    let written = without_spaces(&fields::text(Field::Fingerprint));
    let count = written.chars().count();
    if count == 0 || typed().is_some() {
        None
    } else if count == WRITTEN_LENGTH {
        Some(say!("adding.not_a_fingerprint"))
    } else {
        Some(say!(
            "adding.characters",
            count = count,
            total = WRITTEN_LENGTH
        ))
    }
}

/// Writes the computer down and, when it has an address, goes to it.
///
/// The fingerprint works both ways: it lets that computer in, and it is
/// what this one recognises it by on the way there. Without the first,
/// the far computer would be turned away on arrival, and only half the
/// way would have been made.
pub fn confirm(app: &App) {
    let Some(fingerprint) = typed() else {
        return;
    };
    let address = fields::text(Field::Address).trim().to_string();
    let name = fields::text(Field::Name).trim().to_string();
    let busy = crate::home::seen::with(|seen| seen.busy(crate::home::state().opening.is_some()));
    if !address.is_empty() && busy {
        return;
    }
    super::close(app);

    let app = app.clone();
    crate::shell::app::spawn(async move {
        let fingerprint = fingerprint.to_string();
        let written =
            crate::shell::desk::authorize(fingerprint.clone(), Some(address.clone()), Some(name))
                .await;
        if let Err(reason) = written {
            crate::home::state().tell_trouble(&reason);
            crate::home::redraw(&app);
            return;
        }
        crate::home::seen::again(&app).await;
        if address.is_empty() {
            // Letting a computer in shows nowhere else: without a word,
            // the gesture would look exactly like doing nothing.
            crate::home::state().tell_news(say!("adding.allowed"));
            crate::home::redraw(&app);
            return;
        }
        let named =
            crate::home::seen::with(|seen| seen.peer(&fingerprint).map(|peer| peer.name.clone()))
                .unwrap_or_else(|| address.clone());
        // A computer just written down is reached by the best road:
        // keeping to this network is asked for on a card, for a computer
        // this network announces.
        crate::home::act::launch(&app, &address, &fingerprint, &named, false);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fingerprint_read_in_groups_is_written_whole() {
        assert_eq!(
            without_spaces(" 0829cc7e cb9e9ba5\t42 "),
            "0829cc7ecb9e9ba542"
        );
    }
}
