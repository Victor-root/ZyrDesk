//! What the home window is drawn with.
//!
//! One walk lays everything down and notes, on the way, what answers the
//! hand: drawing and knowing what is under the mouse are the same job,
//! and two walks would agree only until the day one of them changes. The
//! same walk measures, when asked to stay silent: what a dialog takes is
//! what its lines take, measured by drawing them nowhere.
//!
//! Lengths are written in page pixels, those of the design system, and
//! drawn in real ones: `px` turns one into the other.

pub mod fields;
mod widgets;

use std::hash::{Hash, Hasher};
use std::time::Duration;

use zyr_draw::design::{self, Colour, Palette, Shadow};
use zyr_draw::icons::Icon;
use zyr_draw::{Canvas, Pen, Picture, Rect, Scene};

use crate::home::seen::Seen;
use crate::home::state::{Hit, Kept, Scroller, State, Target};
use crate::shell::app::App;

pub use widgets::{Button, Kind, SIDES_HEIGHT, SWITCH as SWITCH_SIZE};

/// How long a hover takes to come and go: the design system's short
/// beat.
pub const QUICK: Duration = Duration::from_millis(design::DURATION_SHORT);
/// How long a change of state takes: a switch sliding, a dialog
/// settling.
pub const STEADY: Duration = Duration::from_millis(design::DURATION);

/// The size something arriving starts from, against its own.
const GROWS_FROM: f32 = 0.94;

/// The thickness of a line and of a border.
pub const HAIRLINE: f32 = 1.0;

/// What draws the home window, and what it learns on the way.
pub struct Painter<'a> {
    pub canvas: &'a Canvas,
    pub scale: f32,
    pub colours: Palette,
    pub light: bool,
    pub seen: &'a Seen,
    pub state: &'a mut State,
    pub app: &'a App,
    hits: Vec<Hit>,
    /// False under a dialog: what is behind no longer answers the hand.
    live: bool,
    /// True while only measuring: nothing is laid down.
    silent: bool,
}

impl<'a> Painter<'a> {
    pub fn new(
        canvas: &'a Canvas,
        scale: f32,
        light: bool,
        seen: &'a Seen,
        state: &'a mut State,
        app: &'a App,
    ) -> Self {
        Painter {
            canvas,
            scale,
            colours: design::palette(light),
            light,
            seen,
            state,
            app,
            hits: Vec::new(),
            live: true,
            silent: false,
        }
    }

    /// What the walk laid down that the hand can be on.
    pub fn into_hits(self) -> Vec<Hit> {
        self.hits
    }

    /* ---- Measures ------------------------------------------------------ */

    /// A length of the design system, in real pixels.
    pub fn px(&self, page: f32) -> f32 {
        page * self.scale
    }

    /// Draws `inside` this many times larger than the screen's scale asks:
    /// a page that has room to spare uses it.
    pub fn enlarged<R>(&mut self, factor: f32, inside: impl FnOnce(&mut Self) -> R) -> R {
        let scale = self.scale;
        self.scale *= factor;
        let made = inside(self);
        self.scale = scale;
        made
    }

    pub fn pen(&self, size: f32) -> Pen {
        Pen::of(self.px(size))
    }

    pub fn display(&self) -> Pen {
        self.pen(design::DISPLAY).in_bold()
    }

    pub fn title(&self) -> Pen {
        self.pen(design::TITLE).in_bold()
    }

    pub fn subtitle(&self) -> Pen {
        self.pen(design::SUBTITLE).in_bold()
    }

    pub fn body(&self) -> Pen {
        self.pen(design::BODY)
    }

    pub fn caption(&self) -> Pen {
        self.pen(design::CAPTION)
    }

    /// The height of one line written with this pen.
    pub fn line(&self, pen: Pen) -> f32 {
        self.canvas.line_height(pen)
    }

    pub fn width_of(&self, text: &str, pen: Pen) -> f32 {
        self.canvas.width_of(text, pen)
    }

    /// The height of a paragraph wrapped at this width.
    pub fn height_of(&self, text: &str, pen: Pen, width: f32) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        self.canvas.height_of(text, pen, width.max(1.0))
    }

    /* ---- What answers the hand ---------------------------------------- */

    /// Notes that this place answers a click with that target.
    ///
    /// Never while measuring, nor under a dialog: what is not laid down,
    /// or no longer current, cannot be clicked.
    pub fn answers(&mut self, target: Target, at: Rect) {
        self.hit(Some(target), at, None);
    }

    /// The same, with a bubble that names it.
    pub fn answers_with_tip(&mut self, target: Target, at: Rect, tip: String) {
        self.hit(Some(target), at, Some(tip));
    }

    /// Notes that a bubble opens over this place, which answers nothing:
    /// what is shown and not working yet says so here.
    pub fn tells(&mut self, at: Rect, tip: String) {
        self.hit(None, at, Some(tip));
    }

    /// Notes that this place stops the hand from reaching what is under
    /// it, as a veil does.
    pub fn absorbs(&mut self, at: Rect) {
        self.hit(None, at, None);
    }

    fn hit(&mut self, target: Option<Target>, at: Rect, tip: Option<String>) {
        if self.live && !self.silent {
            self.hits.push(Hit { target, at, tip });
        }
    }

    /// Whether the hand is on that target.
    pub fn hovered(&self, target: &Target) -> bool {
        self.live && self.state.hovered() == Some(target)
    }

    /// Whether that target is pressed and not yet let go.
    pub fn pressed(&self, target: &Target) -> bool {
        self.live && self.state.pressed.as_ref() == Some(target) && self.hovered(target)
    }

    /// Whether the keyboard is on that target, after a key was used to
    /// get there.
    pub fn focused(&self, target: &Target) -> bool {
        self.live && self.state.focus_shown && self.state.focus.as_ref() == Some(target)
    }

    /// Draws what `inside` draws, answering the hand or not.
    pub fn living(&mut self, live: bool, inside: impl FnOnce(&mut Self)) {
        let before = self.live;
        self.live = live;
        inside(self);
        self.live = before;
    }

    /// Draws what lies under a cover, a dialog or the opening of a
    /// session: once, then laid down again as it was at every frame, as
    /// long as the cover stays and nothing it was drawn from has changed
    /// (what is shown, the theme, the scale and the size). Under a cover
    /// nothing answers the hand, so nothing there moves by itself.
    pub fn under_a_cover(
        &mut self,
        covered: bool,
        size: (f32, f32),
        inside: impl FnOnce(&mut Self),
    ) {
        let drawn_from = Kept {
            generation: self.seen.generation,
            light: self.light,
            scale: self.scale.to_bits(),
            size: (size.0.to_bits(), size.1.to_bits()),
        };
        if covered && self.state.kept == Some(drawn_from) && self.canvas.lay_kept() {
            return;
        }
        inside(self);
        self.state.kept = (covered && self.canvas.keep()).then_some(drawn_from);
    }

    /// What `inside` would take, without laying anything down.
    pub fn measured(&mut self, inside: impl FnOnce(&mut Self) -> f32) -> f32 {
        let before = self.silent;
        self.silent = true;
        let taken = inside(self);
        self.silent = before;
        taken
    }

    /// Whether the walk only measures.
    pub fn is_silent(&self) -> bool {
        self.silent
    }

    /* ---- What moves ---------------------------------------------------- */

    /// How far that aspect of a target has got on its way to `goal`.
    pub fn ease(&mut self, target: &Target, aspect: u8, goal: f32, over: Duration) -> f32 {
        if self.silent {
            return goal;
        }
        self.state
            .motion
            .follow((key_of(target), aspect), goal, over)
    }

    /// How far a thing has got since it came into being, from nought to
    /// one.
    pub fn arriving(&mut self, name: &str, over: Duration) -> f32 {
        if self.silent {
            return 1.0;
        }
        self.state.motion.appear((key_of(&name), 0), 0.0, 1.0, over)
    }

    /// Where a scrolling thing is shown this frame: easing towards where
    /// it was asked to go, or right there while a hand holds its bar,
    /// which has to follow the hand without lagging.
    pub fn scrolled_to(&mut self, which: Scroller) -> f32 {
        let goal = self.state.scrolled(which).goal;
        let held = self.state.held.is_some_and(|(held, _)| held == which);
        let now = if held {
            goal
        } else {
            self.ease(&Target::Scrollbar(which), 4, goal, QUICK)
        };
        self.state.scrolled(which).now = now;
        now
    }

    /// How far the hand being on a target has come, from nought to one.
    pub fn hover_of(&mut self, target: &Target) -> f32 {
        let on = self.hovered(target);
        self.ease(target, 0, if on { 1.0 } else { 0.0 }, QUICK)
    }

    /* ---- What draws, and what stays silent while measuring ------------ */

    pub fn fill(&self, at: Rect, radius: f32, ink: Colour) {
        if !self.silent {
            self.canvas.fill(at, radius, ink);
        }
    }

    /// A border, held inside its rectangle.
    pub fn stroke(&self, at: Rect, radius: f32, ink: Colour) {
        self.stroke_of(at, radius, self.px(HAIRLINE), ink);
    }

    pub fn stroke_of(&self, at: Rect, radius: f32, thickness: f32, ink: Colour) {
        if !self.silent {
            self.canvas.stroke_inside(at, radius, thickness, ink);
        }
    }

    pub fn dashed(&self, at: Rect, radius: f32, ink: Colour) {
        if !self.silent {
            let thickness = self.px(HAIRLINE);
            self.canvas
                .stroke_dashed(at.grown(-thickness / 2.0), radius, thickness, ink);
        }
    }

    pub fn shadow(&self, at: Rect, radius: f32, shadow: Shadow) {
        if !self.silent {
            self.canvas.shadow(at, radius, shadow, self.scale);
        }
    }

    pub fn text(&self, text: &str, pen: Pen, ink: Colour, at: Rect) {
        if !self.silent {
            self.canvas.draw_text(text, pen, ink, at);
        }
    }

    /// Writes a paragraph within this width from this top, and gives back
    /// the height it took.
    pub fn paragraph(
        &self,
        text: &str,
        pen: Pen,
        ink: Colour,
        (left, top, width): (f32, f32, f32),
    ) -> f32 {
        let height = self.height_of(text, pen, width);
        if height > 0.0 {
            self.text(text, pen, ink, Rect::at(left, top, width, height));
        }
        height
    }

    pub fn icon(&self, icon: &Icon, at: Rect, ink: Colour) {
        if !self.silent {
            self.canvas.icon(icon, at, ink);
        }
    }

    /// ZyrDesk's own picture, its ridges drawn from `seed`, with words on
    /// its left or not.
    pub fn art(&self, at: Rect, radius: f32, seed: u32) {
        if !self.silent {
            self.canvas.art(
                &Scene {
                    light: self.light,
                    seed,
                },
                at,
                radius,
                self.scale,
            );
        }
    }

    /// The picture the window lies on, within `at`, under the light
    /// frost that lets what is written on it read; the plain background
    /// where it could not be read.
    pub fn backdrop(&self, at: Rect) {
        if self.silent {
            return;
        }
        match zyr_draw::backdrop::backdrop(self.light) {
            Some(picture) => {
                self.canvas
                    .frosted(picture, at, 0.0, self.px(design::FROST_BLUR));
                self.fill(at, 0.0, self.colours.frost);
            }
            None => self.fill(at, 0.0, self.colours.background),
        }
    }

    /// The deeper glass of a panel in the rounded `at`: the picture the
    /// window lies on, blurred further, under the glass's tint.
    pub fn glass(&self, at: Rect, radius: f32) {
        if self.silent {
            return;
        }
        if let Some(picture) = zyr_draw::backdrop::backdrop(self.light) {
            self.canvas
                .frosted(picture, at, radius, self.px(design::GLASS_BLUR));
        }
        self.fill(at, radius, self.colours.glass);
    }

    pub fn picture(&self, picture: &Picture, at: Rect, radius: f32) {
        if !self.silent {
            self.canvas.picture(picture, at, radius, 1.0);
        }
    }

    /// The ZyrDesk mark.
    pub fn mark(&self, at: Rect) {
        if !self.silent {
            zyr_draw::mark::lay(self.canvas, at, 1.0, false);
        }
    }

    /// Draws what `inside` draws without letting anything out of `at`.
    pub fn clipped(&mut self, at: Rect, inside: impl FnOnce(&mut Self)) {
        if self.silent {
            inside(self);
            return;
        }
        let canvas = self.canvas;
        canvas.clipped(at, || inside(self));
    }

    /// Draws `inside` as something arriving over `at`, from a little
    /// smaller and faint to its place, as `shown` goes from nothing to one.
    pub fn grown_in(&mut self, at: Rect, shown: f32, inside: impl FnOnce(&mut Self)) {
        if self.silent {
            inside(self);
            return;
        }
        let factor = GROWS_FROM + (1.0 - GROWS_FROM) * shown;
        let canvas = self.canvas;
        canvas.growing(at, factor, shown, || inside(self));
    }
}

/// What a moving value of this thing is known by.
fn key_of(thing: &impl Hash) -> u64 {
    let mut hasher = std::hash::DefaultHasher::new();
    thing.hash(&mut hasher);
    hasher.finish()
}

/// A rectangle's middle.
pub fn middle(at: Rect) -> (f32, f32) {
    ((at.left + at.right) / 2.0, (at.top + at.bottom) / 2.0)
}

/// A rectangle's width and height.
pub fn size(at: Rect) -> (f32, f32) {
    (at.right - at.left, at.bottom - at.top)
}

/// A square of this side, centred on this point.
pub fn square(centre: (f32, f32), side: f32) -> Rect {
    Rect::at(centre.0 - side / 2.0, centre.1 - side / 2.0, side, side)
}
