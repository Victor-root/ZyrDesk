//! Drawing the card, and handing the picture to its window.

use super::*;

/// How much a colour tints the background when it serves as a hover:
/// twelve per cent of it, over what is behind.
const VEIL: f32 = 0.12;

/// Draws the card and hands it to the window.
///
/// The window follows what the card asks for. It can change size without
/// anything flickering: the picture and the size are handed to Windows in
/// the same move, so there is no moment when the window is large without
/// being painted. That is what a web view cannot do, and it is what lets
/// the card here be measured on what it really holds rather than on what
/// it might hold one day.
pub(super) fn repaint(window: windows_sys::Win32::Foundation::HWND) {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect;

    let scale = scale();
    let colours = palette();
    let radius = design::RADIUS * scale;
    let hover = *HOVER.lock().expect("menu's hover");
    let is_open = *PANEL.lock().expect("menu's panel");

    CANVAS.with_borrow_mut(|canvas| {
        // The room needed, measured on the canvas that is there: measuring
        // text does not need the right size of canvas, only a canvas.
        if canvas.is_none() {
            *canvas = Canvas::new(1, 1);
        }
        let Some(measure) = canvas.as_ref() else {
            return;
        };
        measure_the_card(measure, scale);
        let (width, height) = size(measure);
        if width <= 0 || height <= 0 {
            return;
        }
        WIDTH.store(width as u32, Ordering::Relaxed);
        HEIGHT.store(height as u32, Ordering::Relaxed);
        // Made again as soon as it is no longer the right size, which is
        // also the case of the one-pixel canvas that has just served for
        // measuring.
        if measure.size() != (width, height) {
            *canvas = Canvas::new(width, height);
        }
        let Some(canvas) = canvas.as_ref() else {
            return;
        };

        let card = card(scale);
        canvas.begin(Colour::TRANSPARENT);
        canvas.shadow(card, radius, colours.shadow_2, scale);
        canvas.fill(card, radius, colours.surface_1);
        canvas.stroke_inside(
            card,
            radius,
            layout::HAIRLINE * scale,
            colours.border_strong,
        );

        let painter = Painter {
            canvas,
            scale,
            colours,
        };
        for (rank, line, at) in walk(scale) {
            let under_the_hand = hover.filter(|target| target.line() == Some(rank));
            let side = match under_the_hand {
                Some(Target::Side(_, side)) => Some(side),
                _ => None,
            };
            match line {
                Line::Measures => painter.measures(at),
                Line::Separator => painter.separator(at),
                Line::Refusal => painter.refusal(at),
                Line::Entry(entry) => painter.entry(at, entry, under_the_hand.is_some()),
                Line::Toggle(toggle) => painter.sides(
                    at,
                    &Sides {
                        icon: toggle.icon,
                        label: toggle.label,
                        words: &toggle.words(),
                        current_side: toggle.current_side(),
                        struck: &[],
                    },
                    side,
                ),
                Line::Choice(choice) => {
                    if let (Some(words), Some((current_side, struck))) =
                        (choice.words(), choice.current())
                    {
                        painter.sides(
                            at,
                            &Sides {
                                icon: choice.icon,
                                label: choice.label,
                                words: &words,
                                current_side,
                                struck: &struck,
                            },
                            side,
                        );
                    }
                }
                Line::Slider(slider) => painter.slider(at, slider),
                Line::List(list) => painter.list(at, list, under_the_hand.is_some()),
            }
        }

        if let Some(setting) = is_open {
            painter.panel(setting, hover);
        }
        if !canvas.finish() {
            return;
        }

        let mut place = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        // SAFETY: a window of ours, whose rectangle is read into ours.
        if unsafe { GetWindowRect(window, &mut place) } == 0 {
            return;
        }
        // Hung by the edge the menu opens from, and by the one it sets
        // out from vertically: those are the only two nobody must see
        // move when the window changes size. They are also the ones
        // `lay` works out, so the two agree by themselves.
        let x = if RIGHTWARD.load(Ordering::Relaxed) {
            place.left
        } else {
            place.right - width
        };
        let y = if UPWARD.load(Ordering::Relaxed) {
            place.bottom - height
        } else {
            place.top
        };
        canvas.lay_on(window as isize, x, y);
    });
}

/// What does not change while a card is being drawn: something to draw
/// with, how much a page pixel counts for, and the theme.
///
/// Carried together rather than passed three times to every line, and
/// the menu now has seven kinds of them.
struct Painter<'a> {
    pub(super) canvas: &'a Canvas,
    pub(super) scale: f32,
    colours: Palette,
}

/// What a line with sides shows: its head, its words, the one in place,
/// and the ones the far machine cannot do.
///
/// Carried together because it is drawn together, and because a switch
/// and a line of buttons are described no differently.
struct Sides<'a> {
    pub(super) icon: &'a Icon,
    pub(super) label: &'a str,
    pub(super) words: &'a [String],
    current_side: usize,
    struck: &'a [bool],
}

impl Painter<'_> {
    /// The start of a line, which is the same for all of them: its icon
    /// in its place, and its word after it, said from the key of its
    /// label.
    fn head(&self, at: Rect, icon: &Icon, label: &str, ink: Colour) {
        let (canvas, scale) = (self.canvas, self.scale);
        let side = layout::ICON * scale;
        canvas.icon(
            icon,
            Rect::at(
                at.left + design::SPACE_2 * scale,
                at.top + (at.bottom - at.top - side) / 2.0,
                side,
                side,
            ),
            ink,
        );
        canvas.draw_text(
            &zyr_i18n::text(label),
            Pen::of(design::BODY * scale),
            ink,
            Rect {
                left: at.left + (design::SPACE_2 + layout::ICON + design::SPACE_3) * scale,
                ..at
            },
        );
    }

    /// The background a line takes under the hand.
    fn hover(&self, at: Rect, tint: Option<Colour>) {
        if let Some(tint) = tint {
            self.canvas
                .fill(at, design::RADIUS_SMALL * self.scale, tint);
        }
    }

    /// What is written to the right of a line, in the colour of things
    /// one reads without looking for them.
    fn on_the_right(&self, at: Rect, label: &str, size: f32, ink: Colour) {
        if label.is_empty() {
            return;
        }
        self.canvas.draw_text(
            label,
            Pen::of(size).aligned(Align::Right),
            ink,
            Rect {
                right: at.right - design::SPACE_2 * self.scale,
                ..at
            },
        );
    }

    /// An entry: its icon, its word, what is written to its right, and
    /// the background the hover gives it.
    fn entry(&self, at: Rect, entry: &Entry, under_the_hand: bool) {
        let colours = self.colours;
        let ink = if entry.destructive {
            colours.error
        } else {
            colours.text
        };
        // The line that cuts the session off lights up in its own colour
        // rather than the grey of the others: it is not one more hover,
        // it is the one to be wary of.
        self.hover(
            at,
            under_the_hand.then(|| {
                if entry.destructive {
                    colours.error.faded(VEIL)
                } else {
                    colours.surface_3
                }
            }),
        );
        self.head(at, entry.icon, entry.label, ink);
        self.on_the_right(
            at,
            &entry.trailing.text(),
            design::CAPTION * self.scale,
            colours.text_faint,
        );
    }

    /// A line that opens a list: its value in place, then the chevron that
    /// says it leads elsewhere.
    fn list(&self, at: Rect, list: &List, under_the_hand: bool) {
        let (canvas, scale, colours) = (self.canvas, self.scale, self.colours);
        self.hover(at, under_the_hand.then_some(colours.surface_3));
        self.head(at, list.icon, list.label, colours.text);

        let brand = layout::BRAND * scale;
        let edge = design::SPACE_2 * scale;
        let open = *PANEL.lock().expect("menu's panel") == Some(list.setting);
        canvas.icon(
            // The chevron says which way the list opens, so it turns
            // round when the list is open: the list appears on the left
            // as a rule, and it points towards it; on the right when the
            // card itself is stuck to the window's left edge, it points
            // towards it by simply pointing where it already pointed
            // while closed.
            if open && !RIGHTWARD.load(Ordering::Relaxed) {
                &icons::BACK
            } else {
                &icons::CHEVRON
            },
            Rect::at(
                at.right - edge - brand,
                at.top + (at.bottom - at.top - brand) / 2.0,
                brand,
                brand,
            ),
            colours.text_faint,
        );
        let value = SESSION_MENU
            .lock()
            .expect("menu's settings")
            .as_ref()
            .map_or_else(String::new, |menu| list.setting.summary(menu));
        self.on_the_right(
            Rect {
                right: at.right - brand - edge,
                ..at
            },
            &value,
            design::CAPTION * scale,
            colours.text_faint,
        );
    }

    /// A line with sides: a switch or a row of buttons, only one of which
    /// is filled.
    ///
    /// Both are drawn here because they are drawn the same way. What sets
    /// them apart is what they do, not what they show: one flips the
    /// session at once, the other writes a choice that the session takes
    /// up where it stands.
    fn sides(&self, at: Rect, spec: &Sides, under_the_hand: Option<usize>) {
        let (canvas, scale, colours) = (self.canvas, self.scale, self.colours);
        let words = spec.words;
        self.head(at, spec.icon, spec.label, colours.text);

        let sides = sides_of(canvas, at, words, scale);
        let Some(whole) = sides.first().map(|first| Rect {
            left: first.left,
            ..*sides.last().unwrap_or(first)
        }) else {
            return;
        };
        let radius = design::RADIUS_SMALL * scale;
        for (rank, place) in sides.iter().enumerate() {
            let bar = spec.struck.get(rank).copied().unwrap_or(false);
            let (background, ink) = if rank == spec.current_side {
                (Some(colours.accent_bright), colours.on_accent)
            } else if bar {
                (None, colours.text_faint)
            } else if under_the_hand == Some(rank) {
                (Some(colours.surface_3), colours.text)
            } else {
                (None, colours.text_faint)
            };
            if let Some(background) = background {
                // The background of the whole object, seen through that
                // side: the sides make up a single one, rounded on the
                // outside and straight where they touch, which no rounded
                // rectangle can be on its own.
                canvas.clipped(*place, || canvas.fill(whole, radius, background));
            }
            canvas.draw_text(
                &words[rank],
                Pen::of(design::CAPTION * scale).aligned(Align::Centre),
                ink,
                *place,
            );
            if bar {
                // What the far machine cannot do keeps its place: an
                // option that disappears from one computer to the next
                // suggests a menu that changes its mind, when it is the
                // machine being looked at that does not have the same
                // graphics card. Struck through, then, and not wiped.
                let middle = (place.top + place.bottom) / 2.0;
                let half_label =
                    canvas.width_of(&words[rank], Pen::of(design::CAPTION * scale)) / 2.0;
                let centre = (place.left + place.right) / 2.0;
                canvas.fill(
                    Rect::at(
                        centre - half_label,
                        middle,
                        half_label * 2.0,
                        layout::HAIRLINE * scale,
                    ),
                    0.0,
                    colours.text_faint,
                );
            }
        }
        canvas.stroke_inside(whole, radius, layout::HAIRLINE * scale, colours.border);
    }

    /// A slider line: its head, its value, and the bar below.
    fn slider(&self, at: Rect, slider: &Slider) {
        let (canvas, scale, colours) = (self.canvas, self.scale, self.colours);
        // Its head fits in the height of a body line, the bar taking
        // the rest.
        let head = Rect {
            bottom: at.top + design::SPACE_2 * scale * 2.0 + load(&BODY_HEIGHT),
            ..at
        };
        self.head(head, slider.icon, slider.label, colours.text);
        // A setting's value is read where the shortcuts are read, but it is
        // not one: it is what the line is worth, so it reads like the rest
        // of the line and not toned down.
        self.on_the_right(head, &slider.value(), design::BODY * scale, colours.text);

        let Some((notch, how_many)) = slider.notch() else {
            return;
        };
        let bar = slider_bar(at, scale);
        let radius = layout::BAR * scale / 2.0;
        canvas.fill(bar, radius, colours.border);
        let part = if how_many > 1 {
            notch as f32 / (how_many - 1) as f32
        } else {
            0.0
        };
        let thumb = layout::THUMB * scale;
        // The thumb stays whole inside the bar at both its ends: placed
        // by its share alone, it would spill over by half of itself.
        let thumb_x = bar.left + thumb / 2.0 + (bar.right - bar.left - thumb) * part;
        let middle = (bar.top + bar.bottom) / 2.0;
        canvas.fill(
            Rect::at(bar.left, bar.top, thumb_x - bar.left, radius * 2.0),
            radius,
            colours.accent_bright,
        );
        canvas.fill(
            Rect::at(thumb_x - thumb / 2.0, middle - thumb / 2.0, thumb, thumb),
            thumb / 2.0,
            colours.accent_bright,
        );
    }

    /// The stroke between two groups, centred in the room it takes.
    ///
    /// Brought in by one step on each side: a stroke that runs from one
    /// edge to the other cuts the card in two instead of separating two
    /// groups of lines.
    fn separator(&self, at: Rect) {
        let edge = design::SPACE_2 * self.scale;
        self.canvas.fill(
            Rect::at(
                at.left + edge,
                at.top + edge,
                at.right - at.left - edge * 2.0,
                layout::HAIRLINE * self.scale,
            ),
            0.0,
            self.colours.border,
        );
    }

    /// What the menu has just refused to do, spelled out.
    ///
    /// Wrapped to the card's width: what a refusal has to say is what
    /// needs doing elsewhere, and cutting that short would come down to
    /// saying nothing at all.
    fn refusal(&self, at: Rect) {
        let Some(said) = refusal_to_say() else {
            return;
        };
        let edge = design::SPACE_2 * self.scale;
        let width = at.right - at.left - edge * 2.0;
        let pen = Pen::of(design::CAPTION * self.scale);
        // Measured here because here is the only place that can measure
        // wrapped text, and stored so that the card opens on it, as its
        // width already is.
        let height = self.canvas.height_of(&said, pen, width);
        store(&REFUSAL_HEIGHT, height + edge * 2.0);
        self.canvas.draw_text(
            &said,
            pen,
            self.colours.warning,
            Rect::at(at.left + edge, at.top + edge, width, height),
        );
    }

    /// The bar of the four readings: a word over a number, four times,
    /// and the stream's sentence below.
    pub(super) fn measures(&self, at: Rect) {
        let (canvas, scale, colours) = (self.canvas, self.scale, self.colours);
        let edge = design::SPACE_2 * scale;
        let top = at.top + edge;
        let bar = READINGS_BAR.lock().expect("menu's readings");
        for (rank, reading) in READINGS.iter().enumerate() {
            let left =
                at.left + edge + rank as f32 * (layout::READING + layout::BETWEEN_READINGS) * scale;
            let column = layout::READING * scale;
            canvas.draw_text(
                &zyr_i18n::text(reading.label),
                Pen::of(design::CAPTION * scale),
                colours.text_faint,
                Rect::at(left, top, column, load(&CAPTION_HEIGHT)),
            );
            canvas.draw_text(
                &bar.figures[rank],
                Pen::of(design::BODY * scale),
                colours.text,
                Rect::at(
                    left,
                    top + load(&CAPTION_HEIGHT) + layout::UNDER_THE_LABEL * scale,
                    column,
                    load(&BODY_HEIGHT),
                ),
            );
        }
        if !bar.stream.is_empty() {
            canvas.draw_text(
                &bar.stream,
                Pen::of(design::CAPTION * scale),
                colours.text_faint,
                Rect::at(
                    at.left + edge,
                    top + load(&CAPTION_HEIGHT)
                        + layout::UNDER_THE_LABEL * scale
                        + load(&BODY_HEIGHT)
                        + design::SPACE_1 * scale,
                    at.right - at.left - edge * 2.0,
                    load(&CAPTION_HEIGHT),
                ),
            );
        }
    }

    /// A setting's panel, on the side of the card it did not set out
    /// from: its values, one of which carries the mark.
    ///
    /// No title. One knows where one is: the line that opened it is
    /// facing it, its chevron has turned round towards it, and clicking
    /// it again closes it. A title that repeats the word next to it takes
    /// a line to teach nothing.
    fn panel(&self, setting: Setting, hover: Option<Target>) {
        let (canvas, scale, colours) = (self.canvas, self.scale, self.colours);
        let Some(place) = panel(canvas, setting, scale) else {
            return;
        };
        let radius = design::RADIUS * scale;
        canvas.shadow(place, radius, colours.shadow_2, scale);
        canvas.fill(place, radius, colours.surface_1);
        canvas.stroke_inside(
            place,
            radius,
            layout::HAIRLINE * scale,
            colours.border_strong,
        );

        let values = panel_walk(canvas, setting, scale);
        let side = layout::BRAND * scale;
        let session_menu = SESSION_MENU.lock().expect("menu's settings");
        let Some(menu) = session_menu.as_ref() else {
            return;
        };
        let values_here = setting.values(menu);
        let at = setting.current(menu);
        for (rank, place) in values.iter().enumerate() {
            let Some(value) = values_here.get(rank) else {
                break;
            };
            self.hover(
                *place,
                (hover == Some(Target::Value(rank))).then_some(colours.surface_3),
            );
            if *value == at {
                canvas.icon(
                    &icons::TICK,
                    Rect::at(
                        place.left + design::SPACE_2 * scale,
                        place.top + (place.bottom - place.top - side) / 2.0,
                        side,
                        side,
                    ),
                    colours.accent_bright,
                );
            }
            canvas.draw_text(
                &setting.label(menu, value),
                Pen::of(design::BODY * scale),
                colours.text,
                Rect {
                    left: place.left + (design::SPACE_2 + layout::ICON + design::SPACE_3) * scale,
                    ..*place
                },
            );
            self.on_the_right(
                *place,
                &setting.aside(menu, value),
                design::CAPTION * scale,
                colours.text_faint,
            );
        }
    }
}
