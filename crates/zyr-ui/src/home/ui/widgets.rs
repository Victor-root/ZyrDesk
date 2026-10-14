//! The pieces every page of the home window is made of.
//!
//! A card, a button, a switch, a row of sides, a tag, a dot: each drawn
//! here once, with its states, and nowhere else. Every one answers the
//! hand the same way, eases its hover in the same time, and shows the
//! keyboard's ring the same way, which is what makes a window feel like
//! one thing rather than a collection of screens.

use zyr_draw::design::{self, Colour};
use zyr_draw::icons::Icon;
use zyr_draw::{Align, Rect};

use super::{HAIRLINE, Painter, STEADY, middle, size, square};
use crate::home::state::{Scroller, Target};

/// What a button is.
#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    /// The main action of a dialog: the accent, full.
    Primary,
    /// The action a card offers, repeated from card to card: it rests
    /// as the design system's action and fills with the accent when the
    /// hand comes, whether to the button or to its card.
    Action,
    /// What goes along with the main action: outlined.
    Quiet,
    /// What hardly needs a frame: a word, and a background under the
    /// hand.
    Ghost,
    /// What asks for a second click before destroying.
    Warning,
}

/// How strongly a part that cannot be used yet is drawn.
pub const FADED: f32 = 0.45;

/// A button, as it is asked for.
pub struct Button<'b> {
    pub label: &'b str,
    pub icon: Option<&'b Icon>,
    pub kind: Kind,
    pub enabled: bool,
    /// How far a hand on what carries the button lights it, so that a
    /// card's action fills when the card is hovered and not only the
    /// button.
    pub lit: f32,
    /// Whether the icon follows the word, as a chevron that leads on
    /// does, rather than coming before it.
    pub icon_after: bool,
}

impl<'b> Button<'b> {
    pub fn new(label: &'b str, kind: Kind) -> Self {
        Button {
            label,
            icon: None,
            kind,
            enabled: true,
            lit: 0.0,
            icon_after: false,
        }
    }

    pub fn leading_on(self, icon: &'b Icon) -> Self {
        Button {
            icon: Some(icon),
            icon_after: true,
            ..self
        }
    }

    pub fn with_icon(self, icon: &'b Icon) -> Self {
        Button {
            icon: Some(icon),
            ..self
        }
    }

    pub fn enabled(self, enabled: bool) -> Self {
        Button { enabled, ..self }
    }

    pub fn lit(self, lit: f32) -> Self {
        Button { lit, ..self }
    }
}

impl Painter<'_> {
    /// A card: its shadow, its surface and its edge.
    pub fn card(&self, at: Rect) {
        let radius = self.px(design::RADIUS);
        self.shadow(at, radius, self.colours.shadow_1);
        self.fill(at, radius, self.colours.surface_1);
        self.stroke(at, radius, self.colours.border);
    }

    /// A card the hand can be on: its edge warms as the hand comes.
    pub fn card_lit(&self, at: Rect, lit: f32) {
        self.card(at);
        if lit > 0.0 {
            let radius = self.px(design::RADIUS);
            self.stroke(at, radius, self.colours.accent.faded(0.45 * lit));
        }
    }

    /// The ring around what the keyboard is on.
    pub fn focus_ring(&self, at: Rect, radius: f32) {
        let gap = self.px(3.0);
        self.stroke_of(
            at.grown(gap),
            radius + gap,
            self.px(2.0),
            self.colours.accent,
        );
    }

    /// How wide a button with this word and this icon is.
    pub fn button_width(&self, label: &str, icon: Option<&Icon>) -> f32 {
        let pen = self.body().in_bold();
        self.width_of(label, pen)
            + icon.map_or(0.0, |_| self.px(ICON + design::SPACE_2))
            + self.px(design::SPACE_4) * 2.0
    }

    /// A button, laid over `at`.
    pub fn button(&mut self, at: Rect, target: Target, button: Button) {
        let Button {
            label,
            icon,
            kind,
            enabled,
            lit,
            icon_after,
        } = button;
        let colours = self.colours;
        let radius = self.px(design::RADIUS_SMALL);
        let hover = if enabled { self.hover_of(&target) } else { 0.0 };
        let warm = hover.max(lit);
        let (background, ink, edge) = if !enabled {
            (
                Some(colours.surface_3),
                colours.text_faint,
                matches!(kind, Kind::Quiet).then_some(colours.border),
            )
        } else {
            match kind {
                Kind::Primary => (
                    Some(colours.accent_bright.mixed_with(colours.accent, hover)),
                    colours.on_accent,
                    None,
                ),
                Kind::Action => (
                    Some(colours.accent.mixed_with(colours.action, warm)),
                    colours.on_accent.mixed_with(colours.on_action, warm),
                    None,
                ),
                Kind::Quiet => (
                    Some(colours.surface_2.mixed_with(colours.surface_1, hover)),
                    colours.text,
                    Some(colours.border_strong),
                ),
                Kind::Ghost => (
                    (hover > 0.0).then(|| colours.surface_3.faded(hover)),
                    colours.text.mixed_with(colours.text_soft, hover),
                    None,
                ),
                Kind::Warning => (
                    (hover > 0.0).then(|| colours.warning.faded(0.1 * hover)),
                    colours.warning,
                    Some(colours.warning.faded(0.55)),
                ),
            }
        };
        if let Some(background) = background {
            self.fill(at, radius, background);
        }
        if self.pressed(&target) {
            self.fill(at, radius, colours.text.faded(0.06));
        }
        if let Some(edge) = edge {
            self.stroke(at, radius, edge);
        }
        let pen = self.body().in_bold();
        let words = self.width_of(label, pen);
        let room = icon.map_or(0.0, |_| self.px(ICON + design::SPACE_2));
        let (centre_x, centre_y) = middle(at);
        let left = centre_x - (words + room) / 2.0;
        let side = self.px(ICON);
        let (words_left, icon_left) = if icon_after {
            (left, left + words + self.px(design::SPACE_2))
        } else {
            (left + room, left)
        };
        if let Some(icon) = icon {
            self.icon(
                icon,
                Rect::at(icon_left, centre_y - side / 2.0, side, side),
                ink,
            );
        }
        self.text(
            label,
            pen,
            ink,
            Rect::at(words_left, at.top, words + self.px(2.0), at.bottom - at.top),
        );
        if self.focused(&target) {
            self.focus_ring(at, radius);
        }
        if enabled {
            self.answers(target, at);
        }
    }

    /// A button that carries only an icon, with a bubble that names it.
    pub fn icon_button(&mut self, at: Rect, icon: &Icon, target: Target, tip: String) {
        let colours = self.colours;
        let radius = self.px(design::RADIUS_SMALL);
        let hover = self.hover_of(&target);
        if hover > 0.0 {
            self.fill(at, radius, colours.surface_3.faded(hover));
        }
        let side = self.px(ICON);
        self.icon(
            icon,
            square(middle(at), side),
            colours.text.mixed_with(colours.text_soft, hover),
        );
        if self.focused(&target) {
            self.focus_ring(at, radius);
        }
        self.answers_with_tip(target, at, tip);
    }

    /// The switch: its rail, and the thumb that slides in it. `at` is its
    /// top left corner.
    pub fn switch(
        &mut self,
        (left, top): (f32, f32),
        on: bool,
        enabled: bool,
        target: Target,
    ) -> Rect {
        let colours = self.colours;
        let at = Rect::at(left, top, self.px(SWITCH.0), self.px(SWITCH.1));
        let radius = (at.bottom - at.top) / 2.0;
        let along = self.ease(&target, 1, if on { 1.0 } else { 0.0 }, STEADY);
        let hover = if enabled { self.hover_of(&target) } else { 0.0 };
        let opacity = if enabled { 1.0 } else { FADED };
        let rail = colours
            .accent
            .mixed_with(colours.surface_3, along)
            .faded(opacity);
        self.fill(at, radius, rail);
        if along < 1.0 {
            self.stroke(
                at,
                radius,
                colours.border_strong.faded((1.0 - along) * opacity),
            );
        }
        let slack = self.px(3.0);
        let side = (at.bottom - at.top) - slack * 2.0;
        let x = at.left + slack + (size(at).0 - slack * 2.0 - side) * along;
        let thumb = Rect::at(x, at.top + slack, side, side);
        if hover > 0.0 {
            self.fill(
                thumb.grown(self.px(3.0) * hover),
                side / 2.0 + self.px(3.0) * hover,
                colours.accent.faded(0.16 * hover),
            );
        }
        self.fill(
            thumb,
            side / 2.0,
            colours
                .on_accent
                .mixed_with(colours.text_soft, along)
                .faded(opacity),
        );
        if self.focused(&target) {
            self.focus_ring(at, radius);
        }
        if enabled {
            self.answers(target, at);
        }
        at
    }

    /// How wide a row of sides with these words is.
    pub fn sides_width(&self, words: &[String]) -> f32 {
        let pen = self.caption().in_bold();
        words
            .iter()
            .map(|word| self.width_of(word, pen) + self.px(design::SPACE_3) * 2.0)
            .sum::<f32>()
            + self.px(SIDES_INSET) * 2.0
    }

    /// A row of sides, only one of which is chosen, laid from its right
    /// edge and centred on `centre_y`. The chosen side's background
    /// slides from one side to the next.
    pub fn sides(
        &mut self,
        (right, centre_y): (f32, f32),
        words: &[String],
        chosen: Option<usize>,
        enabled: bool,
        target_of: impl Fn(usize) -> Target,
    ) -> Rect {
        let colours = self.colours;
        let pen = self.caption().in_bold();
        let inset = self.px(SIDES_INSET);
        let width = self.sides_width(words);
        let height = self.px(SIDES_HEIGHT);
        let at = Rect::at(right - width, centre_y - height / 2.0, width, height);
        let radius = self.px(design::RADIUS_SMALL);
        let opacity = if enabled { 1.0 } else { FADED };
        self.fill(at, radius, colours.surface_2.faded(opacity));
        self.stroke(at, radius, colours.border.faded(opacity));

        let mut places = Vec::with_capacity(words.len());
        let mut x = at.left + inset;
        for word in words {
            let wide = self.width_of(word, pen) + self.px(design::SPACE_3) * 2.0;
            places.push(Rect::at(x, at.top + inset, wide, height - inset * 2.0));
            x += wide;
        }
        if let Some(chosen) = chosen.and_then(|rank| places.get(rank).copied()) {
            let first = target_of(0);
            let left = self.ease(&first, 2, chosen.left, STEADY);
            let wide = self.ease(&first, 3, size(chosen).0, STEADY);
            let thumb = Rect::at(left, chosen.top, wide, size(chosen).1);
            let round = radius - inset;
            self.shadow(thumb, round, colours.shadow_1);
            self.fill(thumb, round, colours.surface_1.faded(opacity));
        }
        for (rank, (word, place)) in words.iter().zip(&places).enumerate() {
            let target = target_of(rank);
            let hover = if enabled { self.hover_of(&target) } else { 0.0 };
            let ink = if Some(rank) == chosen {
                colours.text
            } else {
                colours.text_soft.mixed_with(colours.text_faint, hover)
            };
            self.text(word, pen.aligned(Align::Centre), ink.faded(opacity), *place);
            if self.focused(&target) {
                self.focus_ring(*place, radius - inset);
            }
            if enabled {
                self.answers(target, *place);
            }
        }
        at
    }

    /// How wide a tag with this word and this icon is.
    pub fn tag_width(&self, label: &str, icon: Option<&Icon>) -> f32 {
        let room = icon.map_or(0.0, |_| self.px(TAG_ICON + design::SPACE_1 + 2.0));
        self.width_of(label, self.caption().in_bold()) + room + self.px(design::SPACE_2) * 2.0
    }

    /// A tag: a word, and an icon before it, tinted with a role. `at` is
    /// its top left corner; gives back where it was laid.
    pub fn tag(
        &self,
        (left, top): (f32, f32),
        label: &str,
        icon: Option<&Icon>,
        tint: Colour,
    ) -> Rect {
        let pen = self.caption().in_bold();
        let height = self.px(TAG);
        let room = icon.map_or(0.0, |_| self.px(TAG_ICON + design::SPACE_1 + 2.0));
        let width = self.tag_width(label, icon);
        let at = Rect::at(left, top, width, height);
        self.fill(at, self.px(6.0), tint.faded(0.13));
        let mut x = left + self.px(design::SPACE_2);
        if let Some(icon) = icon {
            let side = self.px(TAG_ICON);
            self.icon(
                icon,
                Rect::at(x, top + (height - side) / 2.0, side, side),
                tint,
            );
            x += room;
        }
        self.text(label, pen, tint, Rect::at(x, top, width, height));
        at
    }

    /// A presence dot, with a soft ring around one that is live.
    pub fn dot(&self, centre: (f32, f32), diameter: f32, ink: Colour, live: bool) {
        if live {
            let ring = self.px(3.0);
            self.fill(
                square(centre, diameter + ring * 2.0),
                diameter / 2.0 + ring,
                ink.faded(0.2),
            );
        }
        self.fill(square(centre, diameter), diameter / 2.0, ink);
    }

    /// A person's initials in a disc.
    pub fn avatar(&self, at: Rect, initials: &str) {
        let (width, _) = size(at);
        self.fill(at, width / 2.0, self.colours.accent_muted);
        self.text(
            initials,
            self.pen(width * 0.36 / self.scale)
                .in_bold()
                .aligned(Align::Centre),
            self.colours.accent,
            at,
        );
    }

    /// A key of the keyboard, as a combination shows it.
    pub fn keycap(&self, at: Rect, text: &str, ink: Colour) {
        let radius = self.px(6.0);
        self.fill(at, radius, self.colours.surface_2);
        self.stroke(at, radius, self.colours.border);
        self.text(
            text,
            self.pen(design::CAPTION - 1.0).aligned(Align::Centre),
            ink,
            at,
        );
    }

    /// A line across, the width of `at`, at its top.
    pub fn rule(&self, left: f32, top: f32, width: f32) {
        self.fill(
            Rect::at(left, top, width, self.px(HAIRLINE)),
            0.0,
            self.colours.border,
        );
    }

    /// A line down, `height` long, from its top.
    pub fn upright(&self, left: f32, top: f32, height: f32, ink: Colour) {
        self.fill(Rect::at(left, top, self.px(HAIRLINE), height), 0.0, ink);
    }

    /// A grey bar standing in for a line of text that will come, in
    /// what is drawn and not working yet.
    pub fn stand_in(&self, at: Rect) {
        self.fill(at, size(at).1 / 2.0, self.colours.surface_3);
    }

    /// A section's heading, and how many it holds when that is worth
    /// saying. Gives back the height it takes.
    pub fn heading(&self, (left, top): (f32, f32), text: &str, count: Option<usize>) -> f32 {
        let pen = self.subtitle();
        let height = self.line(pen);
        let width = self.width_of(text, pen);
        self.text(
            text,
            pen,
            self.colours.text,
            Rect::at(left, top, width + self.px(2.0), height),
        );
        if let Some(count) = count {
            let said = count.to_string();
            let small = self.caption().in_bold();
            let side = self.px(22.0);
            let wide = (self.width_of(&said, small) + self.px(design::SPACE_2) * 2.0).max(side);
            let at = Rect::at(
                left + width + self.px(design::SPACE_2),
                top + (height - self.px(20.0)) / 2.0,
                wide,
                self.px(20.0),
            );
            self.fill(at, self.px(10.0), self.colours.surface_3);
            self.text(
                &said,
                small.aligned(Align::Centre),
                self.colours.text_soft,
                at,
            );
        }
        height
    }

    /// The scrollbar of something that scrolls, when there is more to see
    /// than room. `corner` is the rounding of what scrolls: the thumb
    /// stops where the corner begins, rather than sticking out of it.
    pub fn scrollbar(&mut self, which: Scroller, at: Rect, content: f32, corner: f32) {
        let room = size(at).1;
        let scrolled = self.state.scrolled(which);
        scrolled.content = content;
        scrolled.room = room;
        // A page that became shorter is not left scrolled past its end.
        scrolled.go(scrolled.goal);
        let position = scrolled.now;
        if content <= room + 1.0 {
            self.state.scrolled(which).travel = 0.0;
            return;
        }
        let target = Target::Scrollbar(which);
        let hover = self.hover_of(&target);
        let held = self.state.held.is_some_and(|(held, _)| held == which);
        let wide = self.px(4.0 + 3.0 * hover.max(f32::from(u8::from(held))));
        let rail = (room - corner * 2.0 - self.px(design::SPACE_2)).max(0.0);
        let thumb = (rail * room / content)
            .max(self.px(design::SPACE_6))
            .min(rail);
        let travel = rail - thumb;
        self.state.scrolled(which).travel = travel;
        let part = (position / (content - room)).clamp(0.0, 1.0);
        let thumb = Rect::at(
            at.right - wide - self.px(3.0),
            at.top + corner + self.px(design::SPACE_1) + travel * part,
            wide,
            thumb,
        );
        let ink = self
            .colours
            .text_faint
            .mixed_with(self.colours.border_strong, hover);
        self.fill(thumb, wide / 2.0, ink);
        let reach = Rect {
            left: thumb.left - self.px(6.0),
            ..thumb
        };
        self.answers(target, reach);
    }
}

/// The size of an icon beside a word.
pub const ICON: f32 = 18.0;

/// The switch, and the height of a row of sides.
pub const SWITCH: (f32, f32) = (44.0, 24.0);
pub const SIDES_HEIGHT: f32 = 32.0;
const SIDES_INSET: f32 = 3.0;

/// A tag's height and the icon inside it.
const TAG: f32 = 22.0;
const TAG_ICON: f32 = 14.0;
