//! The fields text is typed into.
//!
//! Writing text is the one place where Windows does better than any
//! drawing: the caret, the selection, the clipboard, the keyboards that
//! compose their letters. So these are Windows' own fields, laid inside a
//! frame the window draws, and they live only as long as the dialog that
//! carries them.

use std::sync::Mutex;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use zyr_draw::Rect;
use zyr_draw::design::{self, Colour};
use zyr_win32::wide;

use super::Painter;

/// A field, each with its own place.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Field {
    Fingerprint,
    Address,
    Name,
    Server,
    User,
    Password,
    DeviceName,
    Email,
    Invitation,
    NewName,
    Sift,
}

impl Field {
    /// What a field shows while empty: what is expected there, by
    /// example.
    fn example(self) -> String {
        match self {
            Field::Fingerprint => zyr_i18n::say!("field.fingerprint_example"),
            Field::Address => zyr_i18n::say!("field.address_example"),
            Field::Name => zyr_i18n::say!("field.name_example"),
            Field::Server => zyr_i18n::say!("field.server_example"),
            Field::User => zyr_i18n::say!("field.user_example"),
            Field::Password => String::new(),
            Field::DeviceName => zyr_i18n::say!("field.name_example"),
            Field::Email => zyr_i18n::say!("field.email_example"),
            Field::Invitation => zyr_i18n::say!("field.invitation_example"),
            Field::NewName => zyr_i18n::say!("field.name_example"),
            Field::Sift => zyr_i18n::say!("field.sift_example"),
        }
    }
}

/// The fields open, with the place the last frame gave each.
struct Open {
    parent: isize,
    fields: Vec<(Field, isize)>,
    places: Vec<(Field, Rect)>,
    /// The font they write with, made for the screen's scale.
    font: isize,
    /// The brush their background is painted with, kept to be given back
    /// with the next one: the system reads it and does not keep it.
    brush: isize,
}

static OPEN: Mutex<Open> = Mutex::new(Open {
    parent: 0,
    fields: Vec::new(),
    places: Vec::new(),
    font: 0,
    brush: 0,
});

/// The message nothing in the system sends, by which a field asks the
/// window to do what Enter or Escape do: both close the dialog, and so
/// destroy the field that is answering.
pub const ACT: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 1;

/// The name under which the field's guard is set.
const GUARD: usize = 3;

/// Opens these fields, empty, in that window, in place of any open.
///
/// Nothing here holds the list of fields while Windows is called: a
/// field being made, moved or destroyed may ask its window for its
/// colours on the spot, and that question reads the list.
pub fn open(parent: isize, which: &[Field], scale: f32) {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::Controls::EM_SETCUEBANNER;
    use windows_sys::Win32::UI::Shell::SetWindowSubclass;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, ES_AUTOHSCROLL, ES_PASSWORD, SendMessageW, WS_CHILD, WS_VISIBLE,
    };

    close();
    let class = wide("EDIT");
    let mut made = Vec::with_capacity(which.len());
    for field in which.iter().copied() {
        let secret = if field == Field::Password {
            ES_PASSWORD as u32
        } else {
            0
        };
        // SAFETY: a field of the system, a child of our window, made on
        // the thread that owns both.
        let edit = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                std::ptr::null(),
                WS_CHILD | WS_VISIBLE | ES_AUTOHSCROLL as u32 | secret,
                0,
                0,
                0,
                0,
                parent as HWND,
                std::ptr::null_mut(),
                GetModuleHandleW(std::ptr::null()),
                std::ptr::null(),
            )
        };
        if edit.is_null() {
            continue;
        }
        let example = wide(field.example());
        // SAFETY: a field of ours, given a word that outlives the call,
        // then a guard that is a plain function of this program.
        unsafe {
            SendMessageW(edit, EM_SETCUEBANNER, 1, example.as_ptr() as isize);
            SetWindowSubclass(edit, Some(guard), GUARD, 0);
        }
        made.push((field, edit as isize));
    }
    {
        let mut open = OPEN.lock().expect("the fields");
        open.parent = parent;
        open.fields = made;
    }
    dress(scale);
}

/// Closes the open fields.
pub fn close() {
    use windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow;

    let closing = {
        let mut open = OPEN.lock().expect("the fields");
        open.places.clear();
        std::mem::take(&mut open.fields)
    };
    for (_, edit) in closing {
        // SAFETY: a field of ours, destroyed once.
        unsafe { DestroyWindow(edit as HWND) };
    }
}

/// What is written in a field.
pub fn text(field: Field) -> String {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowTextLengthW, GetWindowTextW};

    let Some(edit) = handle(field) else {
        return String::new();
    };
    // SAFETY: a field of ours, whose text is read into a buffer of the
    // length it has just said.
    unsafe {
        let length = GetWindowTextLengthW(edit);
        if length <= 0 {
            return String::new();
        }
        let mut read = vec![0u16; length as usize + 1];
        let got = GetWindowTextW(edit, read.as_mut_ptr(), length + 1);
        String::from_utf16_lossy(&read[..got.max(0) as usize])
    }
}

/// Writes this in a field, in place of what it held: what a dialog
/// already knows is not typed again.
pub fn write(field: Field, text: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::SetWindowTextW;

    if let Some(edit) = handle(field) {
        let words = wide(text);
        // SAFETY: a field of ours, and a text that outlives the call.
        unsafe { SetWindowTextW(edit, words.as_ptr()) };
    }
}

/// Puts the keyboard in a field.
pub fn focus(field: Field) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;

    if let Some(edit) = handle(field) {
        // SAFETY: a field of ours, on its thread.
        unsafe { SetFocus(edit) };
    }
}

/// The field the keyboard is in, if any.
pub fn focused() -> Option<Field> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetFocus;

    // SAFETY: a question about this thread's keyboard.
    let now = unsafe { GetFocus() } as isize;
    OPEN.lock()
        .expect("the fields")
        .fields
        .iter()
        .find(|(_, edit)| *edit == now)
        .map(|(field, _)| *field)
}

/// Which field this window of the system is, if it is one.
pub fn which(window: isize) -> Option<Field> {
    OPEN.lock()
        .expect("the fields")
        .fields
        .iter()
        .find(|(_, edit)| *edit == window)
        .map(|(field, _)| *field)
}

fn handle(field: Field) -> Option<HWND> {
    OPEN.lock()
        .expect("the fields")
        .fields
        .iter()
        .find(|(open, _)| *open == field)
        .map(|(_, edit)| *edit as HWND)
}

/// Forgets where the fields go, at the start of a frame: a field the
/// frame does not lay down is put away.
pub fn unplace() {
    OPEN.lock().expect("the fields").places.clear();
}

/// Sets each field where the last frame laid its frame, inside it:
/// after the frame and never while drawing it, since moving a window
/// while painting one's own mixes two pictures.
pub fn lay_out(scale: f32) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos};

    let side = design::SPACE_3 * scale;
    let line = (design::BODY * 1.45 * scale).round();
    let placed: Vec<(isize, (i32, i32, i32, i32))> = {
        let open = OPEN.lock().expect("the fields");
        open.fields
            .iter()
            .map(|(field, edit)| {
                let place = match open.places.iter().find(|(placed, _)| placed == field) {
                    Some((_, at)) => (
                        (at.left + side).round() as i32,
                        (at.top + ((at.bottom - at.top) - line) / 2.0).round() as i32,
                        (at.right - at.left - side * 2.0).round() as i32,
                        line as i32,
                    ),
                    None => (0, 0, 0, 0),
                };
                (*edit, place)
            })
            .collect()
    };
    for (edit, (x, y, width, height)) in placed {
        // SAFETY: a field of ours, moved on the thread that owns it.
        unsafe {
            SetWindowPos(
                edit as HWND,
                std::ptr::null_mut(),
                x,
                y,
                width,
                height,
                SWP_NOACTIVATE | SWP_NOZORDER,
            )
        };
    }
}

/// Makes the fields' font again for this scale, and sets it on them: a
/// field of the system carries its own font, which does not follow what
/// is drawn around it.
pub fn dress(scale: f32) {
    use windows_sys::Win32::Graphics::Gdi::{
        CLEARTYPE_QUALITY, CreateFontW, DEFAULT_CHARSET, DEFAULT_PITCH, DeleteObject, FF_DONTCARE,
        FW_NORMAL, OUT_DEFAULT_PRECIS,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_SETFONT};

    let family = wide(FAMILY);
    // SAFETY: a name that outlives the call, and a font that is ours
    // until it is given back.
    let font = unsafe {
        CreateFontW(
            -((design::BODY * scale).round() as i32),
            0,
            0,
            0,
            FW_NORMAL as i32,
            0,
            0,
            0,
            u32::from(DEFAULT_CHARSET),
            u32::from(OUT_DEFAULT_PRECIS),
            0,
            u32::from(CLEARTYPE_QUALITY),
            (DEFAULT_PITCH | FF_DONTCARE) as u32,
            family.as_ptr(),
        )
    };
    if font.is_null() {
        return;
    }
    let (fields, before) = {
        let mut open = OPEN.lock().expect("the fields");
        let before = std::mem::replace(&mut open.font, font as isize);
        let fields: Vec<isize> = open.fields.iter().map(|(_, edit)| *edit).collect();
        (fields, before)
    };
    for edit in fields {
        // SAFETY: a field of ours, given a font that outlives it.
        unsafe { SendMessageW(edit as HWND, WM_SETFONT, font as usize, 1) };
    }
    if before != 0 {
        // SAFETY: the font before, given back once nothing uses it.
        unsafe { DeleteObject(before as _) };
    }
}

/// The family the fields write with: the drawing's own.
const FAMILY: &str = "Segoe UI Variable Text";

/// The colours of a field's inside, asked by the system as it paints
/// one: without this, a white field would punch a hole in a dark window.
pub fn tint(surface: WPARAM, background: Colour, ink: Colour) -> LRESULT {
    use windows_sys::Win32::Graphics::Gdi::{
        CreateSolidBrush, DeleteObject, SetBkColor, SetTextColor,
    };

    let mut open = OPEN.lock().expect("the fields");
    // SAFETY: the surface the system has just lent, and a brush it reads
    // now and that is given back with the next one.
    unsafe {
        SetTextColor(surface as _, rgb(ink));
        SetBkColor(surface as _, rgb(background));
        if open.brush != 0 {
            DeleteObject(open.brush as _);
        }
        open.brush = CreateSolidBrush(rgb(background)) as isize;
        open.brush
    }
}

/// A colour of the design system, in the number GDI reads.
fn rgb(colour: Colour) -> u32 {
    let part = |of: f32| (of.clamp(0.0, 1.0) * 255.0).round() as u32;
    part(colour.red) | (part(colour.green) << 8) | (part(colour.blue) << 16)
}

/// What Tab, Enter and Escape do in a field.
///
/// A field is a window of its own: those keys never get through it to
/// the window around, and a dialog in which one cannot go from one field
/// to the next is not a dialog. So they are caught here.
///
/// SAFETY: called by the system on the thread that owns this field, with
/// the arguments it documents.
unsafe extern "system" fn guard(
    window: HWND,
    message: u32,
    holding: WPARAM,
    with: LPARAM,
    _guard: usize,
    _data: usize,
) -> LRESULT {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyState, SetFocus, VK_ESCAPE, VK_RETURN, VK_SHIFT, VK_TAB,
    };
    use windows_sys::Win32::UI::Shell::DefSubclassProc;
    use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CHAR, WM_KEYDOWN};

    // The character that follows those keys is swallowed with them:
    // without this the field beeps, none of them being one it writes.
    if message == WM_CHAR && matches!(holding, 9 | 13 | 27) {
        return 0;
    }
    if message == WM_KEYDOWN {
        let key = holding as u16;
        if key == VK_TAB {
            // SAFETY: a question about this thread's keyboard.
            let backwards = unsafe { GetKeyState(i32::from(VK_SHIFT)) } < 0;
            if let Some(next) = next_field(window as isize, backwards) {
                // SAFETY: a field of ours, on its thread.
                unsafe { SetFocus(next as HWND) };
            }
            return 0;
        }
        if key == VK_RETURN || key == VK_ESCAPE {
            let parent = OPEN.lock().expect("the fields").parent;
            // SAFETY: our window, posted a message that is ours alone.
            unsafe { PostMessageW(parent as HWND, ACT, usize::from(key == VK_RETURN), 0) };
            return 0;
        }
    }
    // SAFETY: the arguments the system gave, handed on as they are.
    unsafe { DefSubclassProc(window, message, holding, with) }
}

/// The field Tab leads to from this one: the next laid down by the last
/// frame, in their order, going round. A field the dialog does not show
/// at the moment, like those of an account being created while one is
/// signing in, is passed over.
fn next_field(from: isize, backwards: bool) -> Option<isize> {
    let open = OPEN.lock().expect("the fields");
    let shown: Vec<isize> = open
        .fields
        .iter()
        .filter(|(field, edit)| {
            *edit == from || open.places.iter().any(|(placed, _)| placed == field)
        })
        .map(|(_, edit)| *edit)
        .collect();
    let at = shown.iter().position(|edit| *edit == from)?;
    let count = shown.len();
    let next = if backwards {
        (at + count - 1) % count
    } else {
        (at + 1) % count
    };
    Some(shown[next])
}

impl Painter<'_> {
    /// A field: its label above, its frame, the real field inside, and
    /// what it has to say below. Gives back the height it takes.
    pub fn field(
        &mut self,
        (left, top, width): (f32, f32, f32),
        field: Field,
        label: &str,
        hint: Option<(&str, Colour)>,
    ) -> f32 {
        let colours = self.colours;
        let label_pen = self.caption().in_bold();
        let label_height = self.line(label_pen);
        self.text(
            label,
            label_pen,
            colours.text_soft,
            Rect::at(left, top, width, label_height),
        );
        let gap = self.px(design::SPACE_2);
        let at = Rect::at(left, top + label_height + gap, width, self.px(FIELD));
        let radius = self.px(design::RADIUS_SMALL);
        let inside = focused() == Some(field);
        self.fill(at, radius, colours.surface_2);
        if inside {
            self.fill(
                at.grown(self.px(3.0)),
                radius + self.px(3.0),
                colours.accent.faded(0.14),
            );
            self.fill(at, radius, colours.surface_2);
            self.stroke_of(at, radius, self.px(1.5), colours.accent);
        } else {
            self.stroke(at, radius, colours.border_strong);
        }
        if !self.is_silent() {
            let seen = self.canvas.as_seen(at);
            OPEN.lock().expect("the fields").places.push((field, seen));
        }
        let mut taken = label_height + gap + self.px(FIELD);
        if let Some((hint, ink)) = hint {
            taken += self.px(design::SPACE_1);
            taken += self.paragraph(hint, self.caption(), ink, (left, top + taken, width));
        }
        taken
    }
}

/// A field's height.
const FIELD: f32 = 40.0;
