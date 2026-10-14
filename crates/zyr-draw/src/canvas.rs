//! What draws the ZyrDesk interface, without a browser.
//!
//! A canvas is a rectangle of pixels that each carry their own transparency.
//! Handed to a layered window, it **is** the window: there is no shape to
//! cut, no ground to erase, no frame, and clicks go through by themselves
//! wherever the picture is clear. That is what settled the logo's edging
//! after twelve trials.
//!
//! An ordinary window, for its part, is framed by the system and opaque,
//! and its canvas is the window itself (`for_window`): the graphics card
//! draws into it and shows it at the screen's refresh. Both draw the same
//! way and only differ at the very end, `lay_on` on one side and the
//! frame's own end on the other.
//!
//! **Direct2D and DirectWrite**, provided by Windows: nothing is bundled,
//! and the text is rendered by the renderer that renders the system's own,
//! so it looks like the system's own.
//!
//! **What a session shows is drawn by the processor, and deliberately
//! so.** The graphics card is then already decoding 4K video at sixty
//! frames a second; asking it to draw a card on top of that would add one
//! more customer to the longest queue in the product. A menu card costs
//! two or three milliseconds of processor time, and only when something
//! changes: when it opens, when the mouse passes from one line to
//! another, at the second that makes the figures move. Zero the rest of
//! the time.
//!
//! **The home window is drawn by the graphics card.** As large as the
//! screen, it cost the processor 12 to 17 milliseconds a frame at 175 %,
//! more than a refresh lasts, and everything in it that moved did so at
//! half pace. It does not move during a session, which is when the card
//! is busy.
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

use windows::Win32::Foundation::{HWND, POINT, RECT, SIZE};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D_SIZE_F, D2D_SIZE_U, D2D1_ALPHA_MODE_IGNORE, D2D1_ALPHA_MODE_PREMULTIPLIED,
    D2D1_BEZIER_SEGMENT, D2D1_COLOR_F, D2D1_FIGURE_BEGIN_HOLLOW, D2D1_FIGURE_END_CLOSED,
    D2D1_FIGURE_END_OPEN, D2D1_GRADIENT_STOP, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_ARC_SEGMENT, D2D1_ARC_SIZE_LARGE, D2D1_ARC_SIZE_SMALL,
    D2D1_BITMAP_BRUSH_PROPERTIES, D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
    D2D1_BITMAP_INTERPOLATION_MODE_NEAREST_NEIGHBOR, D2D1_BITMAP_PROPERTIES, D2D1_BRUSH_PROPERTIES,
    D2D1_CAP_STYLE_ROUND, D2D1_COMPATIBLE_RENDER_TARGET_OPTIONS_NONE, D2D1_DASH_STYLE_DASH,
    D2D1_DASH_STYLE_SOLID, D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_EXTEND_MODE_CLAMP,
    D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_FEATURE_LEVEL_DEFAULT, D2D1_GAMMA_2_2,
    D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_LAYER_OPTIONS_NONE, D2D1_LAYER_PARAMETERS,
    D2D1_LINE_JOIN_ROUND, D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES, D2D1_PRESENT_OPTIONS_NONE,
    D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE, D2D1_RENDER_TARGET_TYPE_DEFAULT,
    D2D1_RENDER_TARGET_TYPE_SOFTWARE, D2D1_RENDER_TARGET_USAGE_NONE, D2D1_ROUNDED_RECT,
    D2D1_STROKE_STYLE_PROPERTIES, D2D1_SWEEP_DIRECTION_CLOCKWISE,
    D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE, D2D1_TEXT_ANTIALIAS_MODE_CLEARTYPE,
    D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE, D2D1CreateFactory, ID2D1Bitmap, ID2D1DCRenderTarget,
    ID2D1Factory, ID2D1HwndRenderTarget, ID2D1PathGeometry, ID2D1RenderTarget,
    ID2D1SolidColorBrush, ID2D1StrokeStyle,
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

use crate::art::Scene;
use crate::design::{Colour, Shadow};
use crate::icons::{Icon, Stroke};
use crate::path::Step;
use crate::picture::Picture;

/// The font family, the system's own, in the order the renderer looks
/// for it.
///
/// One family for the whole product: two families for one product make
/// two products. Windows 11 has the first, Windows 10 the second, and
/// DirectWrite works down the list on its own.
const FAMILY: &str = "Segoe UI Variable Text";
const FAMILY_BEFORE: &str = "Segoe UI";

/// The fixed-width family, and the one that came before it, in the same
/// order and for the same reason: for wherever characters must line up
/// under one another.
const MONO: &str = "Cascadia Mono";
const MONO_BEFORE: &str = "Consolas";

/// Which family a word is written in.
#[derive(Clone, Copy, PartialEq)]
pub enum Family {
    /// The system's own, for everything a person reads.
    Text,
    /// Fixed width: for a fingerprint, a journal, a code and a key
    /// combination, where each character must take up the same room as
    /// its neighbour.
    Mono,
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
    pub family: Family,
    pub overflow: Overflow,
    /// What is added between two characters, in real pixels.
    ///
    /// What CSS calls `letter-spacing`: a section label in capitals and a
    /// fingerprint read badly when packed tight, and they are the only
    /// places where the space between letters is a choice of the design.
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
            family: Family::Text,
            overflow: Overflow::Wrap,
            spacing: 0.0,
        }
    }

    /// The same, with the characters spread apart by that many times
    /// their size, which CSS writes in `em`.
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
        Pen {
            family: Family::Mono,
            ..self
        }
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
    family: Family,
    overflow: Overflow,
}

impl Key {
    fn of(pen: Pen) -> Self {
        Key {
            size: (pen.size * 1000.0).round() as u32,
            bold: pen.bold,
            align: pen.align,
            family: pen.family,
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
    output: Output,
    target: ID2D1RenderTarget,
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
    /// The pictures already handed to the renderer, the most recently
    /// drawn last: a wallpaper is drawn at every frame, and handing its
    /// pixels over again each time would copy them for nothing.
    bitmaps: std::cell::RefCell<Vec<(u64, ID2D1Bitmap)>>,
    /// The abstract pictures already drawn, at the size, scale and in the
    /// colours they were drawn for, the most recently drawn last. Drawn
    /// once and then laid down like a photograph: its gradients cost far
    /// more than laying it down.
    scenes: std::cell::RefCell<Vec<Drawn>>,
    shades: std::cell::RefCell<Vec<Shade>>,
    /// What was drawn so far in a frame, kept to be laid down again as
    /// it is: see `keep`.
    kept: std::cell::RefCell<Option<ID2D1Bitmap>>,
    /// How what is drawn now is grown and moved: see `growing`.
    growth: std::cell::Cell<Matrix3x2>,
}

/// Where a canvas's pixels go.
enum Output {
    /// Pixels of our own, drawn by the processor: laid on a layered
    /// window, or turned into an icon.
    Pixels {
        surface: HDC,
        bitmap: HBITMAP,
        before: HGDIOBJ,
        binder: ID2D1DCRenderTarget,
    },
    /// A framed window the graphics card draws straight into, shown at
    /// the beat of the screen.
    Window(ID2D1HwndRenderTarget),
}

impl Drop for Output {
    fn drop(&mut self) {
        if let Output::Pixels {
            surface,
            bitmap,
            before,
            ..
        } = self
        {
            // SAFETY: everything given back here was asked for when the
            // pixels were made, and in the reverse order.
            unsafe {
                let _ = SelectObject(*surface, *before);
                let _ = DeleteObject((*bitmap).into());
                let _ = DeleteDC(*surface);
            }
        }
    }
}

/// A shadow already drawn: the size of what casts it, its corners, its
/// softness and its tint, in the bits they are written with, and its
/// pixels, with the room its softness spreads into all round.
struct Shade {
    key: ShadeKey,
    bitmap: ID2D1Bitmap,
}

type ShadeKey = (u32, u32, u32, u32, [u32; 3]);

/// An abstract picture already drawn: what it shows, how many pixels
/// wide and high and at what scale, in hundredths, and its pixels.
struct Drawn {
    scene: Scene,
    size: (u32, u32, u32),
    bitmap: ID2D1Bitmap,
}

/// How many pictures, and how many abstract ones, the canvas keeps
/// ready. More than a home screen shows at once, so none is made again
/// while the hand moves, and few enough that the ones nobody shows any
/// more are let go.
const PICTURES_KEPT: usize = 24;
const SCENES_KEPT: usize = 16;
/// How many shadows: one per size of card, dialog and block on screen.
const SHADES_KEPT: usize = 16;

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
        // SAFETY: every object asked of the system is ours until the
        // output gives it back, and none of it leaves here.
        unsafe {
            let screen = GetDC(None);
            let surface = CreateCompatibleDC(Some(screen));
            ReleaseDC(None, screen);
            let Some(bitmap) = pixels(surface, width, height) else {
                let _ = DeleteDC(surface);
                return None;
            };
            let before = SelectObject(surface, bitmap.into());
            let factory: ID2D1Factory =
                D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None).ok()?;
            let binder = factory
                .CreateDCRenderTarget(&properties(D2D1_RENDER_TARGET_TYPE_SOFTWARE, format()))
                .ok()?;
            let target: ID2D1RenderTarget = binder.cast().ok()?;
            Canvas::around(
                factory,
                target,
                Output::Pixels {
                    surface,
                    bitmap,
                    before,
                    binder,
                },
                (width, height),
            )
        }
    }

    /// A canvas that is a framed window, `width` by `height` real
    /// pixels, drawn by the graphics card and shown at the beat of the
    /// screen: what the home window, as large as the screen, needs to
    /// move at the screen's pace, where the processor could not keep up.
    ///
    /// It is never drawn during a session: what is drawn then stays on
    /// the processor (`new`). The graphics card can lose what it holds
    /// when its driver restarts: `finish` then says no, and the canvas is
    /// made again.
    pub fn for_window(window: isize, width: i32, height: i32) -> Option<Canvas> {
        if width <= 0 || height <= 0 {
            return None;
        }
        // SAFETY: a window of the caller's, drawn into by a target of
        // ours, and nothing that leaves here.
        unsafe {
            let factory: ID2D1Factory =
                D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None).ok()?;
            let drawn = factory
                .CreateHwndRenderTarget(
                    &properties(
                        D2D1_RENDER_TARGET_TYPE_DEFAULT,
                        D2D1_PIXEL_FORMAT {
                            format: DXGI_FORMAT_B8G8R8A8_UNORM,
                            // An opaque window: what is shown has nothing
                            // behind it, and its text can then be smoothed
                            // the way Windows smooths its own.
                            alphaMode: D2D1_ALPHA_MODE_IGNORE,
                        },
                    ),
                    &D2D1_HWND_RENDER_TARGET_PROPERTIES {
                        hwnd: HWND(window as *mut std::ffi::c_void),
                        pixelSize: D2D_SIZE_U {
                            width: width as u32,
                            height: height as u32,
                        },
                        // Shown at the screen's refresh, never torn.
                        presentOptions: D2D1_PRESENT_OPTIONS_NONE,
                    },
                )
                .ok()?;
            let target: ID2D1RenderTarget = drawn.cast().ok()?;
            Canvas::around(factory, target, Output::Window(drawn), (width, height))
        }
    }

    /// What every canvas holds around its target: its brush, its words,
    /// its strokes, and the room for what it draws once.
    ///
    /// # Safety
    ///
    /// A factory and a target of ours, the target made by the factory.
    unsafe fn around(
        factory: ID2D1Factory,
        target: ID2D1RenderTarget,
        output: Output,
        (width, height): (i32, i32),
    ) -> Option<Canvas> {
        // SAFETY: as the caller vouches.
        unsafe {
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
                output,
                target,
                brush,
                writer,
                fonts: std::cell::RefCell::new(Vec::new()),
                paths: std::cell::RefCell::new(Vec::new()),
                style,
                dashed,
                factory,
                bitmaps: std::cell::RefCell::new(Vec::new()),
                scenes: std::cell::RefCell::new(Vec::new()),
                shades: std::cell::RefCell::new(Vec::new()),
                kept: std::cell::RefCell::new(None),
                growth: std::cell::Cell::new(Matrix3x2::identity()),
            })
        }
    }

    /// Gives the canvas a new size, keeping everything it has made.
    ///
    /// What a window being resized by hand asks for at every step: only
    /// the pixels change, and making the fonts, the shapes and the
    /// pictures again at each of them would be work thrown away a
    /// sixtieth of a second later. Says whether it could.
    pub fn resize(&mut self, width: i32, height: i32) -> bool {
        if width <= 0 || height <= 0 {
            return false;
        }
        if (width, height) == (self.width, self.height) {
            return true;
        }
        match &mut self.output {
            Output::Pixels {
                surface, bitmap, ..
            } => {
                // SAFETY: a surface of ours, given new pixels, the old
                // ones given back once they are no longer selected into
                // it.
                unsafe {
                    let Some(made) = pixels(*surface, width, height) else {
                        return false;
                    };
                    let _ = SelectObject(*surface, made.into());
                    let _ = DeleteObject((*bitmap).into());
                    *bitmap = made;
                }
            }
            Output::Window(drawn) => {
                // SAFETY: a target of ours, outside a drawing.
                let resized = unsafe {
                    drawn.Resize(&D2D_SIZE_U {
                        width: width as u32,
                        height: height as u32,
                    })
                };
                if resized.is_err() {
                    return false;
                }
            }
        }
        self.width = width;
        self.height = height;
        // What was kept is of the old size.
        self.kept.replace(None);
        true
    }

    /// Its length on each side, for whoever needs to know whether it is
    /// still the right size.
    pub fn size(&self) -> (i32, i32) {
        (self.width, self.height)
    }

    /// Keeps what has been drawn so far in this frame, to be laid down
    /// again as it is by `lay_kept`, and says whether it could.
    ///
    /// What lies under a dialog does not change while the dialog arrives
    /// over it: kept once, it costs one picture per frame instead of
    /// a whole page drawn again.
    pub fn keep(&self) -> bool {
        let mut kept = self.kept.borrow_mut();
        // SAFETY: a target of ours between the start and the end of a
        // drawing, and a bitmap made from it, of its size.
        unsafe {
            if kept.is_none() {
                *kept = self
                    .target
                    .CreateBitmap(
                        D2D_SIZE_U {
                            width: self.width as u32,
                            height: self.height as u32,
                        },
                        None,
                        0,
                        &D2D1_BITMAP_PROPERTIES {
                            pixelFormat: self.target.GetPixelFormat(),
                            dpiX: 96.0,
                            dpiY: 96.0,
                        },
                    )
                    .ok();
            }
            kept.as_ref().is_some_and(|bitmap| {
                bitmap
                    .CopyFromRenderTarget(None, &self.target, None)
                    .is_ok()
            })
        }
    }

    /// Lays down what `keep` kept, at the place it was drawn, and says
    /// whether there was anything.
    pub fn lay_kept(&self) -> bool {
        let kept = self.kept.borrow();
        let Some(bitmap) = kept.as_ref() else {
            return false;
        };
        // SAFETY: a target of ours between the start and the end of a
        // drawing, and a bitmap made from it.
        unsafe {
            self.target.DrawBitmap(
                bitmap,
                None,
                1.0,
                D2D1_BITMAP_INTERPOLATION_MODE_NEAREST_NEIGHBOR,
                None,
            );
        }
        true
    }

    /// Opens the drawing, with the whole canvas in this colour.
    ///
    /// `Colour::TRANSPARENT` for a layered window, where the transparency
    /// lets what is behind show through; a background from the design
    /// system for an ordinary window, which is opaque and has nothing
    /// behind it.
    ///
    /// On an opaque background the text is smoothed the way Windows
    /// smooths its own (ClearType), which reads sharper than shades of
    /// grey. ClearType needs to know what lies under each letter, so a
    /// canvas that lets the screen show through keeps the grey.
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
            if let Output::Pixels {
                surface, binder, ..
            } = &self.output
            {
                let _ = binder.BindDC(*surface, &whole);
            }
            self.target
                .SetTextAntialiasMode(if background.alpha >= 1.0 {
                    D2D1_TEXT_ANTIALIAS_MODE_CLEARTYPE
                } else {
                    D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE
                });
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
        let Output::Pixels { surface, .. } = &self.output else {
            return false;
        };
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
                Some(*surface),
                Some(&source),
                windows::Win32::Foundation::COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
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
    pub fn to_icon(&self) -> Option<isize> {
        let Output::Pixels { bitmap, .. } = &self.output else {
            return None;
        };
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
                hbmColor: *bitmap,
            });
            let _ = DeleteObject(mask.into());
            icon.ok().map(|icon| icon.0 as isize)
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
    /// What CSS writes as `border-style: dashed`, and which says one thing
    /// only in the whole product: this is waiting to be filled. A full card
    /// is edged with a solid line.
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
    /// Made of the outline drawn again from further out to further in,
    /// each time faint, which builds up a soft border that is half dark
    /// on the edge itself, as a blurred shadow is. The layers are drawn
    /// at full strength and the whole laid down at the shadow's own
    /// opacity: a shadow of a few per cent split in dozens of layers
    /// would round each one to a whole step of colour, and come out
    /// darker than asked. A Gaussian blur would need a graphics device and all
    /// its torments, for a difference nobody sees on a sixteen-pixel
    /// shadow lying under a card.
    ///
    /// Its softness is one layer per pixel, a few dozen for a dialog: so
    /// each shadow is drawn once, at its size, and laid down at every
    /// frame like a photograph. A dialog that arrives over the page
    /// then costs one picture per frame, not thirty fills of its size.
    pub fn shadow(&self, rect: Rect, radius: f32, shadow: Shadow, scale: f32) {
        let blur = shadow.soft * scale;
        if blur <= 0.0 {
            return;
        }
        let shifted = rect.shifted(shadow.across * scale, shadow.down * scale);
        let margin = blur.ceil();
        let (wide, high) = (
            (shifted.right - shifted.left).round().max(1.0),
            (shifted.bottom - shifted.top).round().max(1.0),
        );
        let colour = shadow.tint;
        let key = (
            wide as u32,
            high as u32,
            radius.to_bits(),
            blur.to_bits(),
            [
                colour.red.to_bits(),
                colour.green.to_bits(),
                colour.blue.to_bits(),
            ],
        );
        let (whole_wide, whole_high) = (wide + margin * 2.0, high + margin * 2.0);
        let Some(bitmap) =
            self.shade_of(key, (whole_wide, whole_high), margin, radius, blur, colour)
        else {
            return;
        };
        self.lay(
            &bitmap,
            (whole_wide, whole_high),
            Rect::at(
                shifted.left - margin,
                shifted.top - margin,
                whole_wide,
                whole_high,
            ),
            0.0,
            colour.alpha,
        );
    }

    /// The shadow for that key, at full strength, drawn the first time it
    /// is asked for: one rounded layer per pixel of softness on each side
    /// of the edge.
    fn shade_of(
        &self,
        key: ShadeKey,
        (whole_wide, whole_high): (f32, f32),
        margin: f32,
        radius: f32,
        blur: f32,
        colour: Colour,
    ) -> Option<ID2D1Bitmap> {
        let mut kept = self.shades.borrow_mut();
        if let Some(at) = kept.iter().position(|shade| shade.key == key) {
            let found = kept.remove(at);
            let bitmap = found.bitmap.clone();
            kept.push(found);
            return Some(bitmap);
        }
        let steps = (blur.ceil().max(1.0) as i32) * 2;
        let cast = Rect::at(
            margin,
            margin,
            whole_wide - margin * 2.0,
            whole_high - margin * 2.0,
        );
        // SAFETY: a drawing of its own, made beside the canvas's and
        // closed before its picture is taken, with a brush made from it.
        let made = unsafe {
            let layer = self
                .target
                .CreateCompatibleRenderTarget(
                    Some(&D2D_SIZE_F {
                        width: whole_wide,
                        height: whole_high,
                    }),
                    None,
                    Some(&format()),
                    D2D1_COMPATIBLE_RENDER_TARGET_OPTIONS_NONE,
                )
                .ok()?;
            let solid = Colour {
                alpha: 1.0,
                ..colour
            };
            let brush = layer.CreateSolidColorBrush(&tint(solid), None).ok()?;
            layer.BeginDraw();
            layer.Clear(Some(&tint(Colour::TRANSPARENT)));
            let narrowest = (cast.right - cast.left).min(cast.bottom - cast.top) / 2.0;
            for step in 0..steps {
                let gap = (blur * (1.0 - 2.0 * step as f32 / steps as f32)).max(-narrowest);
                // Each layer covers the ones before it: this share of
                // what is left makes the strength grow evenly, layer
                // after layer, up to all of it under the last.
                brush.SetColor(&tint(solid.faded(1.0 / (steps - step) as f32)));
                layer.FillRoundedRectangle(
                    &D2D1_ROUNDED_RECT {
                        rect: cast.grown(gap).d2d(),
                        radiusX: (radius + gap).max(0.0),
                        radiusY: (radius + gap).max(0.0),
                    },
                    &brush,
                );
            }
            layer.EndDraw(None, None).ok()?;
            layer.GetBitmap().ok()?
        };
        if kept.len() >= SHADES_KEPT {
            kept.remove(0);
        }
        kept.push(Shade {
            key,
            bitmap: made.clone(),
        });
        Some(made)
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

    /// Draws `inside` grown to `factor` around the middle of `rect`, and
    /// seen through at `opacity`: how something arrives, from a little
    /// smaller and faint to its place.
    ///
    /// The faint part goes through a layer, which costs: a frame at its
    /// full opacity draws straight on the canvas.
    pub fn growing(&self, rect: Rect, factor: f32, opacity: f32, inside: impl FnOnce()) {
        let before = self.growth.get();
        let centre = Vector2 {
            X: (rect.left + rect.right) / 2.0,
            Y: (rect.top + rect.bottom) / 2.0,
        };
        let growth = Matrix3x2::scale_around(factor, factor, centre) * before;
        let faint = opacity < 1.0;
        self.growth.set(growth);
        // SAFETY: a target of ours, between the start and the end of a
        // drawing, whose grid and layer are set back before handing back
        // control.
        unsafe {
            self.target.SetTransform(&growth);
            if faint {
                self.target.PushLayer(
                    &D2D1_LAYER_PARAMETERS {
                        contentBounds: Rect::at(0.0, 0.0, self.width as f32, self.height as f32)
                            .d2d(),
                        geometricMask: std::mem::ManuallyDrop::new(None),
                        maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                        maskTransform: Matrix3x2::identity(),
                        opacity,
                        opacityBrush: std::mem::ManuallyDrop::new(None),
                        layerOptions: D2D1_LAYER_OPTIONS_NONE,
                    },
                    None,
                );
            }
        }
        inside();
        // SAFETY: the layer and the grid set just above.
        unsafe {
            if faint {
                self.target.PopLayer();
            }
            self.target.SetTransform(&before);
        }
        self.growth.set(before);
    }

    /// Where this rect is seen on the canvas, once grown by `growing`.
    pub fn as_seen(&self, rect: Rect) -> Rect {
        let growth = self.growth.get();
        let x = |x: f32| x * growth.M11 + growth.M31;
        let y = |y: f32| y * growth.M22 + growth.M32;
        Rect {
            left: x(rect.left),
            top: y(rect.top),
            right: x(rect.right),
            bottom: y(rect.bottom),
        }
    }

    /// Lays a picture over this rounded rectangle, covering it to its
    /// edges and cropping what is beyond, as a wallpaper covers a screen
    /// of another shape.
    pub fn picture(&self, picture: &Picture, rect: Rect, radius: f32, opacity: f32) {
        self.picture_within(picture, rect, rect, radius, opacity);
    }

    /// Lays a picture so that it covers `cover`, and lets only what falls
    /// in the rounded `rect` show: a piece of a picture laid over a larger
    /// place, in its place.
    pub fn picture_within(
        &self,
        picture: &Picture,
        cover: Rect,
        rect: Rect,
        radius: f32,
        opacity: f32,
    ) {
        let Some(bitmap) = self.bitmap_of(picture) else {
            return;
        };
        self.lay_within(
            &bitmap,
            (picture.width as f32, picture.height as f32),
            cover,
            rect,
            radius,
            opacity,
        );
    }

    /// Lays ZyrDesk's abstract picture over this rounded rectangle.
    ///
    /// Drawn at the exact size of the rectangle once, then laid down at
    /// every frame like a photograph. `scale` is how much a page pixel
    /// counts for, which the light on the ridges is drawn at.
    pub fn art(&self, scene: &Scene, rect: Rect, radius: f32, scale: f32) {
        let size = (
            (rect.right - rect.left).round().max(1.0) as u32,
            (rect.bottom - rect.top).round().max(1.0) as u32,
        );
        let key = (size.0, size.1, (scale * 100.0).round() as u32);
        let Some(bitmap) = self.scene_of(scene, key, scale) else {
            return;
        };
        self.lay(&bitmap, (size.0 as f32, size.1 as f32), rect, radius, 1.0);
    }

    /// Fills the rounded `rect` with colours that change along the line
    /// from `from` to `to`: each stop a share of the way and its colour.
    pub fn gradient(
        &self,
        rect: Rect,
        radius: f32,
        (from, to): ((f32, f32), (f32, f32)),
        stops: &[(f32, Colour)],
    ) {
        let stops: Vec<D2D1_GRADIENT_STOP> = stops
            .iter()
            .map(|(position, colour)| D2D1_GRADIENT_STOP {
                position: *position,
                color: tint(*colour),
            })
            .collect();
        // SAFETY: a target of ours between the start and the end of a
        // drawing, and a brush made from it for this one fill.
        unsafe {
            let Ok(collection) = self.target.CreateGradientStopCollection(
                &stops,
                D2D1_GAMMA_2_2,
                D2D1_EXTEND_MODE_CLAMP,
            ) else {
                return;
            };
            let Ok(brush) = self.target.CreateLinearGradientBrush(
                &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                    startPoint: Vector2 {
                        X: from.0,
                        Y: from.1,
                    },
                    endPoint: Vector2 { X: to.0, Y: to.1 },
                },
                None,
                &collection,
            ) else {
                return;
            };
            self.target.FillRoundedRectangle(
                &D2D1_ROUNDED_RECT {
                    rect: rect.d2d(),
                    radiusX: radius,
                    radiusY: radius,
                },
                &brush,
            );
        }
    }

    /// The renderer's copy of this picture, made the first time it is
    /// drawn.
    fn bitmap_of(&self, picture: &Picture) -> Option<ID2D1Bitmap> {
        let mut kept = self.bitmaps.borrow_mut();
        if let Some(at) = kept.iter().position(|(id, _)| *id == picture.id) {
            let found = kept.remove(at);
            let bitmap = found.1.clone();
            kept.push(found);
            return Some(bitmap);
        }
        // SAFETY: a target of ours, given pixels that outlive the call
        // and are four bytes each in rows of exactly the width said.
        let made = unsafe {
            self.target.CreateBitmap(
                D2D_SIZE_U {
                    width: picture.width,
                    height: picture.height,
                },
                Some(picture.pixels.as_ptr().cast()),
                picture.width * 4,
                &D2D1_BITMAP_PROPERTIES {
                    pixelFormat: format(),
                    dpiX: 96.0,
                    dpiY: 96.0,
                },
            )
        }
        .ok()?;
        if kept.len() >= PICTURES_KEPT {
            kept.remove(0);
        }
        kept.push((picture.id, made.clone()));
        Some(made)
    }

    /// The abstract picture for this scene at this size, drawn the first
    /// time it is asked for.
    fn scene_of(&self, scene: &Scene, key: (u32, u32, u32), scale: f32) -> Option<ID2D1Bitmap> {
        let mut kept = self.scenes.borrow_mut();
        if let Some(at) = kept
            .iter()
            .position(|drawn| drawn.scene == *scene && drawn.size == key)
        {
            let found = kept.remove(at);
            let bitmap = found.bitmap.clone();
            kept.push(found);
            return Some(bitmap);
        }
        // SAFETY: a drawing of its own, made beside the canvas's and
        // closed before its picture is taken; drawing into it while the
        // canvas is being drawn is what such a drawing is for.
        let made = unsafe {
            let layer = self
                .target
                .CreateCompatibleRenderTarget(
                    Some(&D2D_SIZE_F {
                        width: key.0 as f32,
                        height: key.1 as f32,
                    }),
                    None,
                    Some(&format()),
                    D2D1_COMPATIBLE_RENDER_TARGET_OPTIONS_NONE,
                )
                .ok()?;
            layer.BeginDraw();
            layer.Clear(Some(&tint(Colour::TRANSPARENT)));
            crate::art::draw(
                &layer,
                &self.factory,
                &self.style,
                scene,
                (key.0 as f32, key.1 as f32),
                scale,
            );
            layer.EndDraw(None, None).ok()?;
            layer.GetBitmap().ok()?
        };
        if kept.len() >= SCENES_KEPT {
            kept.remove(0);
        }
        kept.push(Drawn {
            scene: *scene,
            size: key,
            bitmap: made.clone(),
        });
        Some(made)
    }

    /// Lays a bitmap of that size over a rounded rectangle, covering it
    /// and centred.
    ///
    /// Through a brush that fills the rounded shape rather than a bitmap
    /// drawn and clipped: that is what rounds its corners as smoothly as
    /// any other shape, without a layer.
    fn lay(&self, bitmap: &ID2D1Bitmap, size: (f32, f32), rect: Rect, radius: f32, opacity: f32) {
        self.lay_within(bitmap, size, rect, rect, radius, opacity);
    }

    /// Lays a picture so that it covers `cover`, and lets only what falls
    /// in the rounded `rect` show.
    fn lay_within(
        &self,
        bitmap: &ID2D1Bitmap,
        (wide, high): (f32, f32),
        cover: Rect,
        rect: Rect,
        radius: f32,
        opacity: f32,
    ) {
        let (room_wide, room_high) = (cover.right - cover.left, cover.bottom - cover.top);
        if wide <= 0.0 || high <= 0.0 || room_wide <= 0.0 || room_high <= 0.0 {
            return;
        }
        let part = (room_wide / wide).max(room_high / high);
        // SAFETY: a target of ours between the start and the end of a
        // drawing, and a brush made from it for this one fill.
        unsafe {
            let Ok(brush) = self.target.CreateBitmapBrush(
                bitmap,
                Some(&D2D1_BITMAP_BRUSH_PROPERTIES {
                    extendModeX: D2D1_EXTEND_MODE_CLAMP,
                    extendModeY: D2D1_EXTEND_MODE_CLAMP,
                    interpolationMode: D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                }),
                Some(&D2D1_BRUSH_PROPERTIES {
                    opacity,
                    transform: Matrix3x2 {
                        M11: part,
                        M12: 0.0,
                        M21: 0.0,
                        M22: part,
                        M31: cover.left + (room_wide - wide * part) / 2.0,
                        M32: cover.top + (room_high - high * part) / 2.0,
                    },
                }),
            ) else {
                return;
            };
            self.target.FillRoundedRectangle(
                &D2D1_ROUNDED_RECT {
                    rect: rect.d2d(),
                    radiusX: radius,
                    radiusY: radius,
                },
                &brush,
            );
        }
    }

    /// Writes a word in this rect, aligned as the pen says and centred
    /// vertically.
    ///
    /// Centred vertically, so a wrapped block wants a rect of its own
    /// height: `height_of` gives it.
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
        let families = match pen.family {
            Family::Text => [FAMILY, FAMILY_BEFORE],
            Family::Mono => [MONO, MONO_BEFORE],
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

/// Pixels of this size for this surface, four bytes each and the right
/// way up.
///
/// # Safety
///
/// `surface` is a surface of ours; what comes back is the caller's to
/// give back.
unsafe fn pixels(surface: HDC, width: i32, height: i32) -> Option<HBITMAP> {
    let mut info: BITMAPINFO = unsafe { std::mem::zeroed() };
    info.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: width,
        // The right way up, which for an image is said with a negative
        // height.
        biHeight: -height,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        ..Default::default()
    };
    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    // SAFETY: as the caller vouches, and a description of ours.
    unsafe { CreateDIBSection(Some(surface), &info, DIB_RGB_COLORS, &mut bits, None, 0) }.ok()
}

/// What a target is asked for: who renders it and in which pixels, at the
/// scale where a pixel is a pixel, everything being already counted in
/// real pixels on this side.
fn properties(
    kind: D2D1_RENDER_TARGET_TYPE,
    pixels: D2D1_PIXEL_FORMAT,
) -> D2D1_RENDER_TARGET_PROPERTIES {
    D2D1_RENDER_TARGET_PROPERTIES {
        r#type: kind,
        pixelFormat: pixels,
        dpiX: 96.0,
        dpiY: 96.0,
        usage: D2D1_RENDER_TARGET_USAGE_NONE,
        minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
    }
}

/// The pixels every drawing here is made of: four bytes, blue first,
/// with their transparency already applied.
fn format() -> D2D1_PIXEL_FORMAT {
    D2D1_PIXEL_FORMAT {
        format: DXGI_FORMAT_B8G8R8A8_UNORM,
        alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
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
            let grid = Matrix3x2 {
                M11: part,
                M12: 0.0,
                M21: 0.0,
                M22: part,
                M31: rect.left,
                M32: rect.top,
            };
            self.target.SetTransform(&(grid * self.growth.get()));
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
            self.target.SetTransform(&self.growth.get());
        }
    }

    /// The path of this drawing, read once.
    ///
    /// An unreadable path is remembered as such, so it is not read again
    /// for every frame drawn. None ships: every path of the product is
    /// read by this brick's tests, since an icon is made of several
    /// strokes and the one that cannot be read disappears while the
    /// others stay, which shows an unrecognisable icon with nothing to say
    /// that it is incomplete. It happened once, to the menu's crossed-out
    /// eye, whose outline is the only Bézier curve in the product.
    fn path_of(&self, said: &'static str) -> Option<ID2D1PathGeometry> {
        if let Some((_, found)) = self
            .paths
            .borrow()
            .iter()
            .find(|(other, _)| std::ptr::eq(*other, said))
        {
            return found.clone();
        }
        let made = self.shape_of(said);
        self.paths.borrow_mut().push((said, made.clone()));
        made
    }

    /// Turns the "d" of an SVG path into a shape.
    fn shape_of(&self, said: &str) -> Option<ID2D1PathGeometry> {
        let steps = crate::path::read(said)?;
        // SAFETY: a shape and its sink, both ours, closed again before
        // leaving.
        unsafe {
            let shape = self.factory.CreatePathGeometry().ok()?;
            let sink = shape.Open().ok()?;
            let mut figure_open = false;
            for step in steps {
                match step {
                    Step::Start(at) => {
                        if figure_open {
                            sink.EndFigure(D2D1_FIGURE_END_OPEN);
                        }
                        sink.BeginFigure(point(at), D2D1_FIGURE_BEGIN_HOLLOW);
                        figure_open = true;
                    }
                    Step::Line(to) => sink.AddLine(point(to)),
                    Step::Curve { first, second, to } => sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                        point1: point(first),
                        point2: point(second),
                        point3: point(to),
                    }),
                    Step::Arc {
                        to,
                        radii,
                        rotation,
                        large,
                        clockwise,
                    } => sink.AddArc(&D2D1_ARC_SEGMENT {
                        point: point(to),
                        size: D2D_SIZE_F {
                            width: radii.0,
                            height: radii.1,
                        },
                        rotationAngle: rotation,
                        sweepDirection: if clockwise {
                            D2D1_SWEEP_DIRECTION_CLOCKWISE
                        } else {
                            D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE
                        },
                        arcSize: if large {
                            D2D1_ARC_SIZE_LARGE
                        } else {
                            D2D1_ARC_SIZE_SMALL
                        },
                    }),
                    Step::Close => {
                        if figure_open {
                            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                            figure_open = false;
                        }
                    }
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

/// A point, in the numbers the renderer expects.
fn point(at: (f32, f32)) -> Vector2 {
    Vector2 { X: at.0, Y: at.1 }
}

/// A word in the characters Windows counts, which are not the ones
/// Rust counts.
fn utf16(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}
