//! What draws the ZyrDesk interface, without a browser.
//!
//! A canvas is a rectangle of pixels that each carry their own transparency.
//! Handed to a layered window, it **is** the window: there is no shape to
//! cut, no ground to erase, no frame, and clicks go through by themselves
//! wherever the picture is clear. That is what settled the logo's edging
//! after twelve trials.
//!
//! An ordinary window, for its part, is framed by the system and opaque: the
//! canvas is poured into it when the system asks for a repaint. Both draw
//! the same way and only differ at the very end, `shifted` on one side and
//! `copy_to` on the other.
//!
//! **Direct2D and DirectWrite**, provided by Windows: nothing is bundled,
//! and the text is rendered by the renderer that renders the system's own,
//! so it looks like the system's own.
//!
//! **Drawn by the processor, and deliberately so.** The graphics card is
//! already decoding 4K video at sixty frames a second; asking it to draw a
//! card on top of that would add one more customer to the longest queue in
//! the product. A menu card costs two or three milliseconds of processor
//! time, and only when something changes: when it opens, when the mouse
//! passes from one line to another, at the second that makes the figures
//! move. Zero the rest of the time.
//!
//! Lengths here are counted in **real pixels**, as everywhere on the Rust
//! side. What the design system writes is in page pixels: `scale` does the
//! conversion, once, on the way in.
//!
//! This is a complete layer and not what the first screen needs: the logo
//! uses only its fill and its outline, the menu adds the text, the icons and
//! the shadows, the home window the rest. A layer cut to fit its first
//! client is reopened for every one that follows, and a layer that gets
//! reopened is a layer whose rules nobody knows any more.
#![allow(dead_code)]

use windows::Win32::Foundation::{HWND, POINT, RECT, SIZE};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D_SIZE_F, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_BEZIER_SEGMENT, D2D1_COLOR_F,
    D2D1_FIGURE_BEGIN_HOLLOW, D2D1_FIGURE_END_CLOSED, D2D1_FIGURE_END_OPEN, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_ARC_SEGMENT, D2D1_ARC_SIZE_LARGE, D2D1_ARC_SIZE_SMALL,
    D2D1_CAP_STYLE_ROUND, D2D1_DASH_STYLE_DASH, D2D1_DASH_STYLE_SOLID, D2D1_DRAW_TEXT_OPTIONS_NONE,
    D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_FEATURE_LEVEL_DEFAULT, D2D1_LINE_JOIN_ROUND,
    D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_SOFTWARE, D2D1_RENDER_TARGET_USAGE_NONE,
    D2D1_ROUNDED_RECT, D2D1_STROKE_STYLE_PROPERTIES, D2D1_SWEEP_DIRECTION_CLOCKWISE,
    D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE, D2D1CreateFactory, ID2D1DCRenderTarget, ID2D1Factory,
    ID2D1PathGeometry, ID2D1SolidColorBrush, ID2D1StrokeStyle,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
    DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_ALIGNMENT_TRAILING,
    DWRITE_TEXT_METRICS, DWRITE_TEXT_RANGE, DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_CHARACTER,
    DWRITE_WORD_WRAPPING_NO_WRAP, DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat,
    IDWriteTextLayout, IDWriteTextLayout1,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS,
    DeleteDC, DeleteObject, GetDC, HBITMAP, HDC, HGDIOBJ, ReleaseDC, SelectObject,
};
use windows::Win32::UI::WindowsAndMessaging::{ULW_ALPHA, UpdateLayeredWindow};
use windows::core::{HSTRING, Interface};
use windows_numerics::{Matrix3x2, Vector2};

use crate::design::{Colour, Shadow};

/// What this module's lines are filed under.
const TAG: &str = "paint";

/// Writes a line under this module's tag.
fn note(what: &str) {
    crate::journal::note_about(TAG, what);
}

/// The font family, the system's own, in the order the renderer looks
/// for it.
///
/// The same as the stylesheet's, to the letter: two families for one
/// product make two products. Windows 11 has the first, Windows 10 the
/// second, and DirectWrite works down the list on its own.
const FAMILY: &str = "Segoe UI Variable Text";
const FAMILY_BEFORE: &str = "Segoe UI";

/// The fixed-width family, and the one that came before it, in the same
/// order and for the same reason: what the stylesheet asks for wherever
/// characters must line up under one another.
const MONO: &str = "Cascadia Mono";
const MONO_BEFORE: &str = "Consolas";

/// A piece of an icon, written in the same words as the drawing it
/// comes from.
pub enum Stroke {
    /// The "d" of an SVG path, taken as it is.
    ///
    /// Taken over and not translated: an icon transcribed by hand is an
    /// icon that ends up no longer being the same, and these are already
    /// written once. What is understood here is what they use: move to,
    /// line to, horizontally, vertically, a curve, an arc, and close.
    SvgPath(&'static str),
    /// A rounded rectangle: x, y, width, height and radius.
    RoundRect(f32, f32, f32, f32, f32),
}

/// An icon: its strokes, the grid they are written in, and the
/// thickness of its stroke in that grid.
///
/// It carries its grid with it, as a vector drawing does: that is what
/// lets it be placed in any rect without anyone having to know what
/// units it was drawn in.
pub struct Icon {
    pub grid: f32,
    pub thickness: f32,
    pub strokes: &'static [Stroke],
}

/// Where a word is aligned in the rect it is given.
#[derive(Clone, Copy, PartialEq)]
pub enum Align {
    Left,
    Centre,
    Right,
}

/// What a word does when it does not fit in its rect.
#[derive(Clone, Copy, PartialEq)]
pub enum Overflow {
    /// It wraps onto the next line, like a paragraph.
    Wrap,
    /// It stops on an ellipsis, like a computer name longer than its card.
    Ellipsis,
    /// It carries on, and it is up to the rect to hold it in: a journal
    /// line does not wrap, it scrolls.
    Visible,
}

/// How a word is written.
///
/// All together because it is all decided together: a text layout is set
/// once and for all when it is made, and setting it afterwards on a
/// shared font also changes what the **measurements** use. So a pen is
/// both what is asked for and the key to what has already been made.
#[derive(Clone, Copy, PartialEq)]
pub struct Pen {
    pub size: f32,
    pub bold: bool,
    pub align: Align,
    /// Fixed width: what the stylesheet asks for in a fingerprint, a
    /// journal, a code and a key combination, where each character must
    /// take up the same room as its neighbour.
    pub mono: bool,
    pub overflow: Overflow,
    /// What is added between two characters, in real pixels.
    ///
    /// What the stylesheet calls `letter-spacing`: a section label in
    /// capitals and a pairing code read badly when packed tight, and it is
    /// the only place where the space between letters is a choice of the
    /// design.
    pub spacing: f32,
}

impl Pen {
    /// An ordinary word at this size, aligned left, which wraps onto
    /// the next line when it does not fit.
    pub const fn of(size: f32) -> Self {
        Pen {
            size,
            bold: false,
            align: Align::Left,
            mono: false,
            overflow: Overflow::Wrap,
            spacing: 0.0,
        }
    }

    /// The same, with the characters spread apart by that many times
    /// their size: the stylesheet writes it in `em`.
    pub fn spaced(self, part: f32) -> Self {
        Pen {
            spacing: self.size * part,
            ..self
        }
    }

    pub const fn in_bold(self) -> Self {
        Pen { bold: true, ..self }
    }

    pub const fn aligned(self, align: Align) -> Self {
        Pen { align, ..self }
    }

    pub const fn monospaced(self) -> Self {
        Pen { mono: true, ..self }
    }

    pub const fn ellipsized(self) -> Self {
        Pen {
            overflow: Overflow::Ellipsis,
            ..self
        }
    }

    pub const fn overflowing(self) -> Self {
        Pen {
            overflow: Overflow::Visible,
            ..self
        }
    }
}

/// A pen in the form its font is found by: its size counted in
/// thousandths of a pixel, since a floating-point number cannot be
/// compared any other way without risking making the same font again for
/// every line.
///
/// The spacing between characters is not part of it, and that is not an
/// oversight: it is set on the layout of a word and not on the font, so
/// two pens that differ only by it share the same one.
#[derive(Clone, Copy, PartialEq)]
struct Key {
    size: u32,
    bold: bool,
    align: Align,
    mono: bool,
    overflow: Overflow,
}

impl Key {
    fn of(pen: Pen) -> Self {
        Key {
            size: (pen.size * 1000.0).round() as u32,
            bold: pen.bold,
            align: pen.align,
            mono: pen.mono,
            overflow: pen.overflow,
        }
    }
}

/// A rectangle in real pixels, the way this whole file counts it.
#[derive(Clone, Copy)]
pub struct Rect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Rect {
    /// The rectangle with the given top left corner, of this width
    /// and this height.
    pub fn at(left: f32, top: f32, width: f32, height: f32) -> Self {
        Rect {
            left,
            top,
            right: left + width,
            bottom: top + height,
        }
    }

    /// The same one, pushed out on every side. A negative amount pulls
    /// it in.
    pub fn grown(&self, by: f32) -> Self {
        Rect {
            left: self.left - by,
            top: self.top - by,
            right: self.right + by,
            bottom: self.bottom + by,
        }
    }

    /// The same one, shifted.
    pub fn shifted(&self, dx: f32, dy: f32) -> Self {
        Rect {
            left: self.left + dx,
            top: self.top + dy,
            right: self.right + dx,
            bottom: self.bottom + dy,
        }
    }

    fn d2d(&self) -> D2D_RECT_F {
        D2D_RECT_F {
            left: self.left,
            top: self.top,
            right: self.right,
            bottom: self.bottom,
        }
    }
}

/// A canvas: pixels, the means to draw them, and the means to hand them
/// to a window.
///
/// Built once per window and kept: what costs here is building it, not
/// drawing in it.
pub struct Canvas {
    width: i32,
    height: i32,
    surface: HDC,
    bitmap: HBITMAP,
    before: HGDIOBJ,
    target: ID2D1DCRenderTarget,
    brush: ID2D1SolidColorBrush,
    writer: IDWriteFactory,
    /// The text layouts already asked for, one per pen: making them
    /// costs, using them does not, and a menu uses two sizes for fifteen
    /// lines.
    fonts: std::cell::RefCell<Vec<(Key, IDWriteTextFormat)>>,
    /// The paths already read, once each: an icon is a text, and reading
    /// it again for every frame would mean reading it fifteen times per
    /// drawing for the same stroke. The ones that cannot be read are
    /// kept too, otherwise their refusal would be reported again on
    /// every frame.
    paths: std::cell::RefCell<Vec<(&'static str, Option<ID2D1PathGeometry>)>>,
    /// The ends of the strokes and their corners, rounded: that is
    /// what the icons ask for, and asking for it once is better than
    /// asking again for every stroke.
    style: ID2D1StrokeStyle,
    /// And the same one dashed, for what is waiting to be filled.
    dashed: ID2D1StrokeStyle,
    factory: ID2D1Factory,
}

impl Canvas {
    /// A canvas of this size, in real pixels.
    ///
    /// Rendered by the processor and not by the graphics card: see the
    /// top of this file. It is also what avoids having to survive the
    /// loss of a graphics device, which happens precisely when a driver
    /// restarts, that is, at the worst moment of a session.
    pub fn new(width: i32, height: i32) -> Option<Canvas> {
        if width <= 0 || height <= 0 {
            return None;
        }
        // SAFETY: every object asked of the system is ours until `Drop`
        // gives it back, and none of it leaves here.
        unsafe {
            let screen = GetDC(None);
            let surface = CreateCompatibleDC(Some(screen));
            let mut info: BITMAPINFO = std::mem::zeroed();
            info.bmiHeader = BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                // The right way up, which for an image is said with a
                // negative height.
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            };
            let mut pixels: *mut std::ffi::c_void = std::ptr::null_mut();
            let bitmap =
                CreateDIBSection(Some(surface), &info, DIB_RGB_COLORS, &mut pixels, None, 0)
                    .ok()?;
            let before = SelectObject(surface, bitmap.into());
            ReleaseDC(None, screen);

            let factory: ID2D1Factory =
                D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None).ok()?;
            let target = factory
                .CreateDCRenderTarget(&D2D1_RENDER_TARGET_PROPERTIES {
                    r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
                    pixelFormat: D2D1_PIXEL_FORMAT {
                        format: DXGI_FORMAT_B8G8R8A8_UNORM,
                        alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                    },
                    // Everything is already counted in real pixels on
                    // this side, so the renderer is asked not to rescale
                    // anything.
                    dpiX: 96.0,
                    dpiY: 96.0,
                    usage: D2D1_RENDER_TARGET_USAGE_NONE,
                    minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
                })
                .ok()?;
            let brush = target
                .CreateSolidColorBrush(&D2D1_COLOR_F::default(), None)
                .ok()?;
            let writer: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).ok()?;
            let stroke = |dashes| D2D1_STROKE_STYLE_PROPERTIES {
                startCap: D2D1_CAP_STYLE_ROUND,
                endCap: D2D1_CAP_STYLE_ROUND,
                dashCap: D2D1_CAP_STYLE_ROUND,
                lineJoin: D2D1_LINE_JOIN_ROUND,
                miterLimit: 10.0,
                dashStyle: dashes,
                dashOffset: 0.0,
            };
            let style = factory
                .CreateStrokeStyle(&stroke(D2D1_DASH_STYLE_SOLID), None)
                .ok()?;
            let dashed = factory
                .CreateStrokeStyle(&stroke(D2D1_DASH_STYLE_DASH), None)
                .ok()?;

            Some(Canvas {
                width,
                height,
                surface,
                bitmap,
                before,
                target,
                brush,
                writer,
                fonts: std::cell::RefCell::new(Vec::new()),
                paths: std::cell::RefCell::new(Vec::new()),
                style,
                dashed,
                factory,
            })
        }
    }

    /// Its length on each side, for whoever needs to know whether it is
    /// still the right size.
    pub fn size(&self) -> (i32, i32) {
        (self.width, self.height)
    }

    /// Opens the drawing, with the whole canvas in this colour.
    ///
    /// `Colour::TRANSPARENT` for a layered window, where the transparency
    /// lets what is behind show through; a background from the design
    /// system for an ordinary window, which is opaque and has nothing
    /// behind it.
    pub fn begin(&self, background: Colour) {
        let whole = RECT {
            left: 0,
            top: 0,
            right: self.width,
            bottom: self.height,
        };
        // SAFETY: a target and a surface of ours, bound for the
        // length of the drawing as the documentation asks.
        unsafe {
            let _ = self.target.BindDC(self.surface, &whole);
            self.target.BeginDraw();
            self.target.Clear(Some(&tint(background)));
        }
    }

    /// Closes the drawing, and says whether the renderer
    /// accepted it.
    pub fn finish(&self) -> bool {
        // SAFETY: the target opened just above.
        unsafe { self.target.EndDraw(None, None).is_ok() }
    }

    /// Hands the canvas to the window, picture and transparency
    /// included, and places it at this spot on the screen.
    ///
    /// One single call for the place and for the picture: so the window
    /// can never be seen in its new place with its old picture.
    pub fn lay_on(&self, window: isize, x: i32, y: i32) -> bool {
        let at = POINT { x, y };
        let size = SIZE {
            cx: self.width,
            cy: self.height,
        };
        let source = POINT { x: 0, y: 0 };
        let blend = windows::Win32::Graphics::Gdi::BLENDFUNCTION {
            BlendOp: windows::Win32::Graphics::Gdi::AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: windows::Win32::Graphics::Gdi::AC_SRC_ALPHA as u8,
        };
        // SAFETY: a window of ours and a surface of ours.
        unsafe {
            UpdateLayeredWindow(
                HWND(window as *mut std::ffi::c_void),
                None,
                Some(&at),
                Some(&size),
                Some(self.surface),
                Some(&source),
                windows::Win32::Foundation::COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            )
            .is_ok()
        }
    }

    /// Pours the canvas into a surface, at this spot.
    ///
    /// What an ordinary window needs, framed and opaque, which repaints
    /// itself when the system asks it to: `shifted` hands over the picture
    /// and the place in a single gesture, which a layered window allows
    /// and an ordinary window knows nothing of. The transparency does not
    /// travel here, and has no business here: what is poured was drawn on
    /// a background.
    pub fn copy_to(&self, dc: HDC, x: i32, y: i32) -> bool {
        use windows::Win32::Graphics::Gdi::{BitBlt, SRCCOPY};

        // SAFETY: a surface of ours, copied as it is into the one the
        // system has just lent.
        unsafe {
            BitBlt(
                dc,
                x,
                y,
                self.width,
                self.height,
                Some(self.surface),
                0,
                0,
                SRCCOPY,
            )
            .is_ok()
        }
    }

    /// Turns the canvas into a system icon.
    ///
    /// What the notification area needs, since it takes not a picture but
    /// an icon. The system keeps a copy of it, so the canvas stays ours;
    /// what comes back belongs to whoever asks for it, until they give it
    /// back.
    ///
    /// The mask is that of an icon with four bytes per pixel: all zeros,
    /// the transparency being carried by the pixels themselves.
    pub fn to_icon(&self) -> Option<windows::Win32::UI::WindowsAndMessaging::HICON> {
        use windows::Win32::Graphics::Gdi::CreateBitmap;
        use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, ICONINFO};

        // A row of a drawing at one bit per pixel is counted in
        // sixteen-bit words, which the size below rounds up to.
        let per_row = ((self.width as usize).div_ceil(16)) * 2;
        let blank = vec![0u8; per_row * self.height.max(0) as usize];
        // SAFETY: a drawing made here and given back here, and an icon
        // that the system copies before handing back control.
        unsafe {
            let mask = CreateBitmap(
                self.width,
                self.height,
                1,
                1,
                Some(blank.as_ptr().cast::<std::ffi::c_void>()),
            );
            if mask.is_invalid() {
                return None;
            }
            let icon = CreateIconIndirect(&ICONINFO {
                fIcon: true.into(),
                xHotspot: 0,
                yHotspot: 0,
                hbmMask: mask,
                hbmColor: self.bitmap,
            });
            let _ = DeleteObject(mask.into());
            icon.ok()
        }
    }

    /// A rectangle with rounded corners, filled.
    pub fn fill(&self, rect: Rect, radius: f32, colour: Colour) {
        // SAFETY: a brush and a target of ours, between the start and
        // the end of a drawing.
        unsafe {
            self.brush.SetColor(&tint(colour));
            self.target.FillRoundedRectangle(
                &D2D1_ROUNDED_RECT {
                    rect: rect.d2d(),
                    radiusX: radius,
                    radiusY: radius,
                },
                &self.brush,
            );
        }
    }

    /// The outline of a rectangle with rounded corners, drawn
    /// **straddling** its edge: half inside, half outside.
    ///
    /// That is what a stroke does in a vector drawing, so it is what is
    /// needed to redraw a drawing.
    pub fn stroke_on(&self, rect: Rect, radius: f32, thickness: f32, colour: Colour) {
        // SAFETY: as above.
        unsafe {
            self.brush.SetColor(&tint(colour));
            self.target.DrawRoundedRectangle(
                &D2D1_ROUNDED_RECT {
                    rect: rect.d2d(),
                    radiusX: radius,
                    radiusY: radius,
                },
                &self.brush,
                thickness,
                None,
            );
        }
    }

    /// The same, but held **entirely inside** the rect.
    ///
    /// That is what a border does in a page, so it is what is needed to
    /// redraw an interface that the design system describes. Both exist
    /// because both are used, and mixing them up shifts an edge by half
    /// a stroke.
    pub fn stroke_inside(&self, rect: Rect, radius: f32, thickness: f32, colour: Colour) {
        self.stroke_on(
            rect.grown(-thickness / 2.0),
            (radius - thickness / 2.0).max(0.0),
            thickness,
            colour,
        );
    }

    /// The outline of a rounded rectangle, dashed.
    ///
    /// What the stylesheet writes as `border-style: dashed`, and which
    /// says one thing only in the whole product: this is waiting to be
    /// filled. A full card is edged with a solid line.
    pub fn stroke_dashed(&self, rect: Rect, radius: f32, thickness: f32, colour: Colour) {
        // SAFETY: as above, with the dashed style made at the same
        // time as the other one.
        unsafe {
            self.brush.SetColor(&tint(colour));
            self.target.DrawRoundedRectangle(
                &D2D1_ROUNDED_RECT {
                    rect: rect.d2d(),
                    radiusX: radius,
                    radiusY: radius,
                },
                &self.brush,
                thickness,
                &self.dashed,
            );
        }
    }

    /// The drop shadow of a rounded rectangle, in real pixels.
    ///
    /// Made of the outline drawn again further and further out, each
    /// time very faint, which builds up a soft border from the edge
    /// outwards. A Gaussian blur would need a graphics device and all
    /// its torments, for a difference nobody sees on a sixteen-pixel
    /// shadow lying under a card.
    pub fn shadow(&self, rect: Rect, radius: f32, shadow: Shadow, scale: f32) {
        let blur = shadow.soft * scale;
        if blur <= 0.0 {
            return;
        }
        let shifted = rect.shifted(shadow.across * scale, shadow.down * scale);
        let steps = blur.ceil().max(1.0) as i32;
        let mut tint = shadow.tint;
        tint.alpha = shadow.tint.alpha / steps as f32;
        for step in 0..steps {
            let gap = blur * (1.0 - step as f32 / steps as f32);
            self.fill(shifted.grown(gap), radius + gap, tint);
        }
    }

    /// Draws without letting anything out of this rect.
    ///
    /// What is needed to show part of a shape without making a second
    /// one: the two sides of a switch are a single rounded rectangle,
    /// and each lets only its own half show.
    pub fn clipped(&self, rect: Rect, inside: impl FnOnce()) {
        // SAFETY: a target of ours, between the start and the end of a
        // drawing, whose clip is closed again before handing back
        // control.
        unsafe {
            self.target
                .PushAxisAlignedClip(&rect.d2d(), D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
        }
        inside();
        // SAFETY: the clip set just above.
        unsafe { self.target.PopAxisAlignedClip() };
    }

    /// Writes a word in this rect, aligned as the pen says and centred
    /// vertically.
    ///
    /// Centred vertically, so a wrapped block wants a rect of its own
    /// height: `height` gives it.
    pub fn draw_text(&self, text: &str, pen: Pen, colour: Colour, rect: Rect) {
        let Some(layout) =
            self.text_layout(text, pen, rect.right - rect.left, rect.bottom - rect.top)
        else {
            return;
        };
        // SAFETY: a layout of ours, used for the length of one drawing.
        unsafe {
            self.brush.SetColor(&tint(colour));
            self.target.DrawTextLayout(
                point((rect.left, rect.top)),
                &layout,
                &self.brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
            );
        }
    }

    /// How wide a word would be, for the places whose width is that of
    /// their longest line.
    pub fn width_of(&self, text: &str, pen: Pen) -> f32 {
        self.measure(text, pen, UNBOUNDED)
            .map_or(0.0, |measure| measure.widthIncludingTrailingWhitespace)
    }

    /// The height a word takes, wrapped at this width.
    ///
    /// What is needed to stack paragraphs: what each one takes up depends
    /// on the room it is left, and nobody can guess it without laying it
    /// out.
    pub fn height_of(&self, text: &str, pen: Pen, width: f32) -> f32 {
        self.measure(text, pen, width)
            .map_or(pen.size, |measure| measure.height)
    }

    /// The height of a line of text written with this pen.
    ///
    /// It is not the size of the characters: a twelve-pixel line takes up
    /// about sixteen, the space above and below being what the font
    /// itself asks for. That is the height a page's layout uses, and
    /// stacking text by its size rather than by its height squeezes
    /// everything that is stacked.
    pub fn line_height(&self, pen: Pen) -> f32 {
        // Two letters that reach to the top and to the bottom: the height
        // of a line does not depend on what is written in it, but an empty
        // line has none.
        self.height_of("Hg", pen, UNBOUNDED)
    }

    /// What a word measures, laid out away from any drawing.
    ///
    /// In a box that is wide but **finite**: measuring in a boundless
    /// box makes the calculation lose all its precision, and the width
    /// then comes back as nothing at all. That is what squeezed the
    /// menu's switches down to the width of their margin alone.
    fn measure(&self, text: &str, pen: Pen, width: f32) -> Option<DWRITE_TEXT_METRICS> {
        let layout = self.text_layout(text, pen, width, UNBOUNDED)?;
        // SAFETY: a layout of ours, measured and given back at once.
        unsafe {
            let mut measure = DWRITE_TEXT_METRICS::default();
            layout.GetMetrics(&mut measure).ok()?;
            Some(measure)
        }
    }

    /// A word laid out in this box, ready to be measured or drawn.
    ///
    /// The same path for both, and that is the whole point: what is
    /// measured is exactly what will be drawn, spacing between characters
    /// included.
    fn text_layout(
        &self,
        text: &str,
        pen: Pen,
        width: f32,
        height: f32,
    ) -> Option<IDWriteTextLayout> {
        let font = self.font(pen)?;
        // SAFETY: a factory and a layout of ours.
        unsafe {
            let layout: IDWriteTextLayout = self
                .writer
                .CreateTextLayout(&utf16(text), &font, width, height)
                .ok()?;
            if pen.spacing != 0.0 {
                // Behind the word and not in front of it: that is what
                // `letter-spacing` does, spreading the characters apart
                // without moving the first one away from its edge.
                if let Ok(spaced) = layout.cast::<IDWriteTextLayout1>() {
                    let _ = spaced.SetCharacterSpacing(
                        0.0,
                        pen.spacing,
                        0.0,
                        DWRITE_TEXT_RANGE {
                            startPosition: 0,
                            length: u32::MAX,
                        },
                    );
                }
            }
            Some(layout)
        }
    }

    /// The font of this pen, made once.
    ///
    /// The whole pen makes the key, and that is not a detail: a layout is
    /// set once and for all when it is made. Set afterwards on a shared
    /// font, it also changes the one the **measurements** use, and a
    /// measurement taken in a right-aligned box is then worth nothing.
    fn font(&self, pen: Pen) -> Option<IDWriteTextFormat> {
        let key = Key::of(pen);
        if let Some((_, found)) = self.fonts.borrow().iter().find(|(other, _)| *other == key) {
            return Some(found.clone());
        }
        let made = self.make_font(pen)?;
        self.fonts.borrow_mut().push((key, made.clone()));
        Some(made)
    }

    /// Asks for the family wanted, and for the one before it if the
    /// machine does not have the first.
    fn make_font(&self, pen: Pen) -> Option<IDWriteTextFormat> {
        let weight = if pen.bold {
            DWRITE_FONT_WEIGHT_SEMI_BOLD
        } else {
            DWRITE_FONT_WEIGHT_NORMAL
        };
        let families = if pen.mono {
            [MONO, MONO_BEFORE]
        } else {
            [FAMILY, FAMILY_BEFORE]
        };
        // SAFETY: a factory of ours; a refusal is an answer and not a
        // fault, hence the second attempt.
        unsafe {
            for family in families {
                let Ok(font) = self.writer.CreateTextFormat(
                    &HSTRING::from(family),
                    None,
                    weight,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    pen.size,
                    &HSTRING::from("fr-FR"),
                ) else {
                    continue;
                };
                let _ = font.SetTextAlignment(match pen.align {
                    Align::Left => DWRITE_TEXT_ALIGNMENT_LEADING,
                    Align::Centre => DWRITE_TEXT_ALIGNMENT_CENTER,
                    Align::Right => DWRITE_TEXT_ALIGNMENT_TRAILING,
                });
                let _ = font.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
                self.set_the_overflow(&font, pen.overflow);
                return Some(font);
            }
        }
        None
    }

    /// Sets what this font does with a word that is too long.
    ///
    /// The ellipsis is a drawing, and a drawing is asked of the font that
    /// will carry it: that is why this comes after the font and not
    /// before.
    fn set_the_overflow(&self, font: &IDWriteTextFormat, overflow: Overflow) {
        if overflow == Overflow::Wrap {
            return;
        }
        // SAFETY: a font of ours, and a trimming sign asked of the
        // factory for that very font.
        unsafe {
            let _ = font.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
            if overflow != Overflow::Ellipsis {
                return;
            }
            let Ok(points) = self.writer.CreateEllipsisTrimmingSign(font) else {
                return;
            };
            let _ = font.SetTrimming(
                &DWRITE_TRIMMING {
                    granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                    delimiter: 0,
                    delimiterCount: 0,
                },
                &points,
            );
        }
    }
}

/// Wide enough for no word to wrap, and no wider.
const UNBOUNDED: f32 = 100_000.0;

impl Drop for Canvas {
    fn drop(&mut self) {
        // SAFETY: everything given back here was asked for in `new`,
        // and in the reverse order.
        unsafe {
            let _ = SelectObject(self.surface, self.before);
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.surface);
        }
    }
}

/// A colour of the design system, in the numbers the renderer expects.
fn tint(colour: Colour) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: colour.red,
        g: colour.green,
        b: colour.blue,
        a: colour.alpha,
    }
}

impl Canvas {
    /// Places an icon in this rect.
    ///
    /// The icon is drawn in its own grid and the rect decides its size:
    /// the stroke follows, since the renderer scales everything, its
    /// thickness included. That is what keeps an icon itself at a hundred
    /// and twenty-five per cent as much as at a hundred and seventy-five,
    /// where an enlarged picture thickens and blurs.
    pub fn icon(&self, icon: &Icon, rect: Rect, colour: Colour) {
        let part = (rect.right - rect.left) / icon.grid;
        // SAFETY: a target and a brush of ours, between the start and
        // the end of a drawing. The grid is set straight again before
        // handing back control, otherwise everything that followed would
        // be drawn in the icon's grid.
        unsafe {
            self.target.SetTransform(&Matrix3x2 {
                M11: part,
                M12: 0.0,
                M21: 0.0,
                M22: part,
                M31: rect.left,
                M32: rect.top,
            });
            self.brush.SetColor(&tint(colour));
            for stroke in icon.strokes {
                match stroke {
                    Stroke::RoundRect(x, y, width, height, radius) => {
                        self.target.DrawRoundedRectangle(
                            &D2D1_ROUNDED_RECT {
                                rect: Rect::at(*x, *y, *width, *height).d2d(),
                                radiusX: *radius,
                                radiusY: *radius,
                            },
                            &self.brush,
                            icon.thickness,
                            &self.style,
                        )
                    }
                    Stroke::SvgPath(said) => {
                        if let Some(path) = self.path_of(said) {
                            self.target.DrawGeometry(
                                &path,
                                &self.brush,
                                icon.thickness,
                                &self.style,
                            );
                        }
                    }
                }
            }
            self.target.SetTransform(&Matrix3x2 {
                M11: 1.0,
                M12: 0.0,
                M21: 0.0,
                M22: 1.0,
                M31: 0.0,
                M32: 0.0,
            });
        }
    }

    /// The path of this drawing, read once.
    ///
    /// An unreadable path is remembered as such and reported only once.
    /// Remembering it is not thrift: without it, it would be read again,
    /// and so reported again, for every frame drawn.
    fn path_of(&self, said: &'static str) -> Option<ID2D1PathGeometry> {
        if let Some((_, found)) = self
            .paths
            .borrow()
            .iter()
            .find(|(other, _)| std::ptr::eq(*other, said))
        {
            return found.clone();
        }
        let made = self.read_path(said);
        if made.is_none() {
            // Reported, not kept quiet. An icon is made of several
            // strokes: the one that cannot be read disappears, the
            // others stay, and what is shown is an unrecognisable icon
            // with nothing to say that it is incomplete. It happened
            // once, to the menu's crossed-out eye, whose outline is the
            // only Bézier curve in the product.
            note(&format!("dessin : chemin non lu, « {said} »"));
        }
        self.paths.borrow_mut().push((said, made.clone()));
        made
    }

    /// Reads the "d" of an SVG path and turns it into a shape.
    ///
    /// What is understood is what the icons of this product use, and
    /// nothing more: move to, line to, horizontal line, vertical line,
    /// a curve, an arc, and close. An unknown letter stops the reading
    /// rather than being skipped: a half-drawn icon looks like a
    /// defect, a missing icon like an oversight, and the second one
    /// gets looked into. It is `path` that says so out loud.
    fn read_path(&self, said: &str) -> Option<ID2D1PathGeometry> {
        // SAFETY: a shape and its sink, both ours, closed again before
        // leaving.
        unsafe {
            let shape = self.factory.CreatePathGeometry().ok()?;
            let sink = shape.Open().ok()?;
            let mut words = Tokens::over(said);
            let (mut at, mut start) = ((0.0f32, 0.0f32), (0.0f32, 0.0f32));
            let mut figure_open = false;
            let mut letter = ' ';
            while let Some(next) = words.letter_or_number() {
                if let Some(this_one) = next {
                    letter = this_one;
                }
                let relative = letter.is_lowercase();
                let mut number = || words.number();
                match letter.to_ascii_uppercase() {
                    'M' => {
                        let (x, y) = (number()?, number()?);
                        at = if relative {
                            (at.0 + x, at.1 + y)
                        } else {
                            (x, y)
                        };
                        if figure_open {
                            sink.EndFigure(D2D1_FIGURE_END_OPEN);
                        }
                        sink.BeginFigure(point(at), D2D1_FIGURE_BEGIN_HOLLOW);
                        start = at;
                        figure_open = true;
                        letter = if relative { 'l' } else { 'L' };
                    }
                    'L' => {
                        let (x, y) = (number()?, number()?);
                        at = if relative {
                            (at.0 + x, at.1 + y)
                        } else {
                            (x, y)
                        };
                        sink.AddLine(point(at));
                    }
                    'H' => {
                        let x = number()?;
                        at.0 = if relative { at.0 + x } else { x };
                        sink.AddLine(point(at));
                    }
                    'V' => {
                        let y = number()?;
                        at.1 = if relative { at.1 + y } else { y };
                        sink.AddLine(point(at));
                    }
                    'C' => {
                        // Both handles are counted from the point the
                        // curve starts from, so before having left it.
                        let (x1, y1) = (number()?, number()?);
                        let (x2, y2) = (number()?, number()?);
                        let (x, y) = (number()?, number()?);
                        let (first_control, second_control) = if relative {
                            ((at.0 + x1, at.1 + y1), (at.0 + x2, at.1 + y2))
                        } else {
                            ((x1, y1), (x2, y2))
                        };
                        at = if relative {
                            (at.0 + x, at.1 + y)
                        } else {
                            (x, y)
                        };
                        sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                            point1: point(first_control),
                            point2: point(second_control),
                            point3: point(at),
                        });
                    }
                    'A' => {
                        let (rx, ry) = (number()?, number()?);
                        let rotation = number()?;
                        let (large_arc, sweep) = (number()?, number()?);
                        let (x, y) = (number()?, number()?);
                        at = if relative {
                            (at.0 + x, at.1 + y)
                        } else {
                            (x, y)
                        };
                        sink.AddArc(&D2D1_ARC_SEGMENT {
                            point: point(at),
                            size: D2D_SIZE_F {
                                width: rx,
                                height: ry,
                            },
                            rotationAngle: rotation,
                            sweepDirection: if sweep != 0.0 {
                                D2D1_SWEEP_DIRECTION_CLOCKWISE
                            } else {
                                D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE
                            },
                            arcSize: if large_arc != 0.0 {
                                D2D1_ARC_SIZE_LARGE
                            } else {
                                D2D1_ARC_SIZE_SMALL
                            },
                        });
                    }
                    'Z' => {
                        if figure_open {
                            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                            figure_open = false;
                        }
                        at = start;
                    }
                    _ => return None,
                }
            }
            if figure_open {
                sink.EndFigure(D2D1_FIGURE_END_OPEN);
            }
            sink.Close().ok()?;
            Some(shape)
        }
    }
}

/// What an SVG path says, letter by letter and number by number.
///
/// A minus sign opens a number, it does not separate: that is the rule of
/// this language, and it is what allows writing "a9 9 0 1 1-12.8 0" with
/// no space before the twelve.
struct Tokens<'a> {
    rest: &'a str,
}

impl<'a> Tokens<'a> {
    fn over(said: &'a str) -> Self {
        Tokens { rest: said }
    }

    fn skip(&mut self) {
        self.rest = self.rest.trim_start_matches([' ', ',', '\t', '\n']);
    }

    /// The next thing: a letter, or nothing when a number is coming, or
    /// the end.
    fn letter_or_number(&mut self) -> Option<Option<char>> {
        self.skip();
        let first = self.rest.chars().next()?;
        if first.is_ascii_alphabetic() {
            self.rest = &self.rest[first.len_utf8()..];
            return Some(Some(first));
        }
        Some(None)
    }

    fn number(&mut self) -> Option<f32> {
        self.skip();
        let mut end = 0;
        for (at, character) in self.rest.char_indices() {
            let open = at == 0 && (character == '-' || character == '+');
            if character.is_ascii_digit() || character == '.' || open {
                end = at + character.len_utf8();
            } else {
                break;
            }
        }
        if end == 0 {
            return None;
        }
        let (read, rest) = self.rest.split_at(end);
        self.rest = rest;
        read.parse().ok()
    }
}

/// A point, in the numbers the renderer expects.
fn point(at: (f32, f32)) -> Vector2 {
    Vector2 { X: at.0, Y: at.1 }
}

/// A word in the characters Windows counts, which are not the ones
/// Rust counts.
fn utf16(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}
