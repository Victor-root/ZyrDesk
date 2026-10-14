//! « Add a computer »: writing a computer down by hand, and forgetting
//! those written.
//!
//! Only for a computer that does not show up by itself, on a network that
//! drops the announcements. Each of the two has to know the other, so
//! this computer's own fingerprint and address come first, to be written
//! on the other one.

use zyr_draw::Rect;
use zyr_draw::design;
use zyr_i18n::say;
use zyr_proto::fingerprint::{Fingerprint, WRITTEN_LENGTH};

use super::{Action, Shape, callout, fingerprint, trouble};
use crate::home::pages::{Control, line, subheading};
use crate::home::state::Target;
use crate::home::ui::fields::{self, Field};
use crate::home::ui::{Kind, Painter, size};
use crate::shell::app::App;

pub fn shape(painter: &Painter) -> Shape {
    let towards = !fields::text(Field::Address).trim().is_empty();
    let busy = towards && painter.seen.busy(painter.state.opening.is_some());
    Shape {
        title: say!("adding.title"),
        lead: Some(say!("adding.lead")),
        width: 580.0,
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
    let gap = painter.px(design::SPACE_4);
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
    y += gap;

    let hint = fingerprint_hint();
    y += painter.field(
        (left, y, width),
        Field::Fingerprint,
        &say!("adding.fingerprint"),
        hint.as_deref().map(|hint| (hint, colours.warning)),
    );
    y += gap;
    y += painter.field(
        (left, y, width),
        Field::Address,
        &say!("adding.address"),
        Some((&say!("adding.address_hint"), colours.text_soft)),
    );
    y += gap;
    y += painter.field((left, y, width), Field::Name, &say!("adding.name"), None);

    // What was written by hand is forgotten where it was written: a card
    // of the home page is a button as a whole, and its menu is the other
    // way to it.
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
        y += painter.px(design::SPACE_5);
        y += subheading(painter, (left, y, width), &say!("adding.written"));
        for (fingerprint, name, host) in written {
            let forget = Target::Forget(fingerprint);
            let armed = painter.state.is_armed(&forget);
            y += line(
                painter,
                (left, y, width),
                None,
                &name,
                &host,
                Control::Buttons(vec![(
                    if armed {
                        say!("dialog.confirm")
                    } else {
                        say!("computer.forget")
                    },
                    if armed { Kind::Warning } else { Kind::Quiet },
                    forget,
                    None,
                )]),
            );
        }
    }
    y += trouble(painter, (left, y, width));
    y - at.top
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
