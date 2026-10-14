//! The pages of the home window, and what they are laid out with.
//!
//! The home page and the computers have their own shape; the others are
//! titled pages of cards, each card a section of lines, each line what it is about on
//! the left and what decides it on the right. That shape is written here
//! once, for all of them.

pub mod about;
pub mod activity;
pub mod computers;
pub mod home;
pub mod settings;

use zyr_draw::design;
use zyr_draw::icons::Icon;
use zyr_draw::{Align, Rect};
use zyr_i18n::say;

use crate::home::pages::home::{toggle_enabled, toggle_is_on};
use crate::home::state::{Target, Toggle};
use crate::home::ui::{Button, Kind, Painter, SWITCH_SIZE, middle, size, square};
use crate::shell::shortcuts::Doing;

/// A page's title and the line under it. Gives back what they take.
pub fn page_title(painter: &Painter, at: Rect, title: &str, lead: &str) -> f32 {
    let colours = painter.colours;
    let title_pen = painter.title();
    let height = painter.line(title_pen);
    painter.text(
        title,
        title_pen,
        colours.text,
        Rect::at(at.left, at.top, size(at).0, height),
    );
    let mut taken = height + painter.px(design::SPACE_1);
    taken += painter.paragraph(
        lead,
        painter.body(),
        colours.text_soft,
        (at.left, at.top + taken, size(at).0),
    );
    taken + painter.px(design::SPACE_5)
}

/// The head of a section: its icon, its title, and what it is about.
pub struct Head<'h> {
    pub icon: &'h Icon,
    pub title: String,
    pub lead: Option<String>,
}

/// A card: the head of its section, then what `inside` draws from the top
/// it is given, across the width inside the card, which gives back the
/// height it took. Measured first and then drawn, by the same walk, since
/// a card has to be laid before what is in it. Gives back the card's
/// height.
pub fn section(
    painter: &mut Painter,
    at: Rect,
    head: &Head,
    inside: impl Fn(&mut Painter, Rect) -> f32,
) -> f32 {
    let edge = painter.px(design::SPACE_5);
    let head_height = painter.measured(|painter| draw_head(painter, at, head));
    let room = Rect::at(
        at.left + edge,
        at.top + head_height,
        size(at).0 - edge * 2.0,
        0.0,
    );
    let inside_height = painter.measured(|painter| inside(painter, room));
    let card = Rect::at(
        at.left,
        at.top,
        size(at).0,
        head_height + inside_height + edge,
    );
    painter.card(card);
    draw_head(painter, card, head);
    inside(painter, room);
    size(card).1
}

/// Draws a section's head at the top of its card, and gives back what it
/// takes, the space under it included.
fn draw_head(painter: &mut Painter, at: Rect, head: &Head) -> f32 {
    let colours = painter.colours;
    let edge = painter.px(design::SPACE_5);
    let disc = painter.px(36.0);
    let badge = Rect::at(at.left + edge, at.top + edge, disc, disc);
    painter.fill(badge, painter.px(10.0), colours.accent_tint);
    painter.icon(
        head.icon,
        square(middle(badge), painter.px(20.0)),
        colours.accent,
    );
    let left = badge.right + painter.px(design::SPACE_3);
    let width = (at.right - edge - left).max(0.0);
    let title_pen = painter.subtitle();
    let title_height = painter.line(title_pen);
    let lead_height = head.lead.as_deref().map_or(0.0, |lead| {
        painter.px(2.0) + painter.height_of(lead, painter.caption(), width)
    });
    let words = title_height + lead_height;
    let top = badge.top + ((disc - words) / 2.0).max(0.0);
    painter.text(
        &head.title,
        title_pen,
        colours.text,
        Rect::at(left, top, width, title_height),
    );
    if let Some(lead) = head.lead.as_deref() {
        painter.paragraph(
            lead,
            painter.caption(),
            colours.text_soft,
            (left, top + title_height + painter.px(2.0), width),
        );
    }
    edge + words.max(disc) + painter.px(design::SPACE_3)
}

/// What decides a line, on its right.
pub enum Control {
    Nothing,
    Switch(Toggle),
    /// Sides: their words, the one chosen, whether they can be changed,
    /// and what each one is.
    Sides(Vec<String>, Option<usize>, bool, Vec<Target>),
    Key(Doing),
    /// Buttons, laid from the right in the order given.
    Buttons(Vec<(String, Kind, Target, Option<&'static Icon>)>),
}

/// A line of settings: a rule above, its icon and what it is about on the
/// left, what decides it on the right. Gives back its height.
pub fn line(
    painter: &mut Painter,
    (left, top, width): (f32, f32, f32),
    icon: Option<&Icon>,
    label: &str,
    caption: &str,
    control: Control,
) -> f32 {
    let colours = painter.colours;
    painter.rule(left, top, width);
    let right = left + width;
    let badge = painter.px(36.0);
    let left = icon.map_or(left, |_| left + badge + painter.px(design::SPACE_4));
    let width = right - left;
    let control_width = control_width(painter, &control);
    let gap = if control_width > 0.0 {
        painter.px(design::SPACE_5)
    } else {
        0.0
    };
    let words_width = (width - control_width - gap).max(painter.px(120.0));
    let body = painter.body();
    let caption_pen = painter.caption();
    let label_height = painter.line(body);
    let caption_height = painter.height_of(caption, caption_pen, words_width);
    let between = if caption_height > 0.0 {
        painter.px(2.0)
    } else {
        0.0
    };
    let words = label_height + between + caption_height;
    let inside = painter.px(design::SPACE_4);
    let height = words.max(control_height(painter, &control)) + inside * 2.0;
    let centre = top + height / 2.0;
    let words_top = centre - words / 2.0;
    if let Some(icon) = icon {
        let at = Rect::at(
            left - badge - painter.px(design::SPACE_4),
            centre - badge / 2.0,
            badge,
            badge,
        );
        painter.fill(at, painter.px(10.0), colours.surface_2);
        painter.icon(
            icon,
            square(middle(at), painter.px(20.0)),
            colours.text_soft,
        );
    }
    painter.text(
        label,
        body,
        colours.text,
        Rect::at(left, words_top, words_width, label_height),
    );
    painter.paragraph(
        caption,
        caption_pen,
        colours.text_soft,
        (left, words_top + label_height + between, words_width),
    );
    match control {
        Control::Nothing => {}
        Control::Switch(toggle) => {
            let on = toggle_is_on(painter.seen, painter.state, toggle);
            let enabled = toggle_enabled(painter.seen, painter.state, toggle);
            painter.switch(
                (
                    right - painter.px(SWITCH_SIZE.0),
                    centre - painter.px(SWITCH_SIZE.1) / 2.0,
                ),
                on,
                enabled,
                Target::Switch(toggle),
            );
        }
        Control::Sides(words, chosen, enabled, targets) => {
            painter.sides((right, centre), &words, chosen, enabled, |rank| {
                targets[rank].clone()
            });
        }
        Control::Key(doing) => key(
            painter,
            Rect::at(
                right - control_width,
                centre - painter.px(17.0),
                control_width,
                painter.px(34.0),
            ),
            doing,
        ),
        Control::Buttons(buttons) => {
            let mut x = right;
            let height = painter.px(34.0);
            for (label, kind, target, icon) in buttons {
                let width = painter.button_width(&label, icon);
                x -= width;
                let mut button = Button::new(&label, kind);
                if let Some(icon) = icon {
                    button = button.with_icon(icon);
                }
                painter.button(
                    Rect::at(x, centre - height / 2.0, width, height),
                    target,
                    button,
                );
                x -= painter.px(design::SPACE_2);
            }
        }
    }
    height
}

/// A heading inside a section, small and spaced out, with a rule above
/// it. Gives back what it takes, the space under it included.
pub fn subheading(painter: &Painter, (left, top, width): (f32, f32, f32), text: &str) -> f32 {
    let pen = painter.caption().in_bold().spaced(0.06);
    let height = painter.line(pen);
    let gap = painter.px(design::SPACE_4);
    painter.rule(left, top, width);
    painter.text(
        &text.to_uppercase(),
        pen,
        painter.colours.text_faint,
        Rect::at(left, top + gap, width, height),
    );
    gap + height + painter.px(design::SPACE_2)
}

fn control_width(painter: &Painter, control: &Control) -> f32 {
    match control {
        Control::Nothing => 0.0,
        Control::Switch(_) => painter.px(SWITCH_SIZE.0),
        Control::Sides(words, ..) => painter.sides_width(words),
        Control::Key(doing) => key_width(painter, *doing),
        Control::Buttons(buttons) => buttons
            .iter()
            .map(|(label, _, _, icon)| {
                painter.button_width(label, *icon) + painter.px(design::SPACE_2)
            })
            .sum::<f32>(),
    }
}

fn control_height(painter: &Painter, control: &Control) -> f32 {
    match control {
        Control::Nothing => 0.0,
        Control::Switch(_) => painter.px(SWITCH_SIZE.1),
        Control::Sides(..) | Control::Key(_) | Control::Buttons(_) => painter.px(34.0),
    }
}

/// What a combination reads, or what it is waiting for.
fn key_words(painter: &Painter, doing: Doing) -> (String, bool) {
    if painter.state.listening == Some(doing) {
        return (say!("settings.key_waiting"), true);
    }
    match painter
        .seen
        .shortcuts
        .iter()
        .find(|(other, _)| *other == doing)
        .and_then(|(_, said)| said.clone())
    {
        Some(said) => (said, false),
        None => (say!("settings.key_none"), false),
    }
}

fn key_width(painter: &Painter, doing: Doing) -> f32 {
    let (said, _) = key_words(painter, doing);
    (painter.width_of(&said, painter.caption().monospaced()) + painter.px(design::SPACE_4) * 2.0)
        .max(painter.px(150.0))
}

/// A combination, which reads as keys and is clicked to be changed.
fn key(painter: &mut Painter, at: Rect, doing: Doing) {
    let colours = painter.colours;
    let target = Target::Shortcut(doing);
    let (said, waiting) = key_words(painter, doing);
    let hover = painter.hover_of(&target);
    let radius = painter.px(design::RADIUS_SMALL);
    painter.fill(at, radius, colours.surface_2);
    painter.stroke(
        at,
        radius,
        if waiting {
            colours.accent
        } else {
            colours
                .border_strong
                .mixed_with(colours.accent, 1.0 - hover * 0.6)
        },
    );
    let ink = if waiting {
        colours.accent
    } else if painter
        .seen
        .shortcuts
        .iter()
        .any(|(other, said)| *other == doing && said.is_some())
    {
        colours.text
    } else {
        colours.text_faint
    };
    painter.text(
        &said,
        painter.caption().monospaced().aligned(Align::Centre),
        ink,
        at,
    );
    if painter.focused(&target) {
        painter.focus_ring(at, radius);
    }
    painter.answers(target, at);
}
