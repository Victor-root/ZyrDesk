//! A picture of the floating button as the screen shows it, written
//! beside the journal.
//!
//! It photographed itself with `PrintWindow` first, and that is why two
//! archives of pictures came back with nothing in them. `PrintWindow`
//! does not copy anything: it asks the window to draw itself again, and
//! a window asked to draw itself again draws what the page says, which
//! is right by definition. The fault is not in what the page says. It is
//! in what ends up on the screen: a pale hairline two pixels wide, along
//! the outside of the button's left edge, exactly in the one pixel the
//! cut takes beyond what the page paints. Measured on Victor's frame at
//! 203,209,216 where the same margin on the other three edges is 2 to 8.
//!
//! So it is copied from the screen instead, which is the only surface
//! where the compositor, the cut and the page are all already added up.
//! Written as a bitmap because a bitmap is a header and the pixels, and
//! anything more would be a dependency for a picture nobody keeps.

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, ReleaseDC, SRCCOPY, SelectObject,
};
use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect;

use crate::journal::note;

/// How much of the screen is kept, counted from the window's top right
/// corner.
///
/// The corner the button hangs from, so the logo is always in it, and
/// wide enough to carry the menu's first lines and a band of the picture
/// beside them. The whole window would be a session's worth of megabytes
/// for a picture that is looked at once.
const SIDE: i32 = 320;

/// How many are kept at a time.
///
/// A ring rather than the first eight: this window is resized half a
/// dozen times while a session opens, and the first eight were all of
/// them, none of which anybody had clicked. The last eight are the ones
/// somebody has just caused.
const AT_MOST: usize = 8;

static TAKEN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Forgets how many have been taken, so a new button starts over.
pub fn start_over() {
    TAKEN.store(0, std::sync::atomic::Ordering::Relaxed);
}

/// Photographs the button's window and writes it beside the journal.
///
/// Says nothing and does nothing if anything refuses: this is a picture
/// for a fault that is being chased, and no fault of a session's is
/// worth a session.
pub fn portrait_of_the_button(button: HWND, why: &str) {
    let rank = TAKEN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    match take_it(button) {
        Some((pixels, wide, high)) => {
            let where_to =
                zyr_proto::paths::logs_dir().join(format!("bouton-{}.bmp", rank % AT_MOST + 1));
            match write_it(&where_to, &pixels, wide, high) {
                Ok(()) => note(&format!(
                    "bouton flottant photographié dans {} : photo {}, {wide}x{high} de l'écran au coin haut droit de sa fenêtre, {why}",
                    where_to.display(),
                    rank + 1
                )),
                Err(fault) => note(&format!(
                    "bouton flottant : la photo n'a pas pu être écrite : {fault}"
                )),
            }
        }
        None => note("bouton flottant : la photo a été refusée par le système"),
    }
}

/// The screen at the window's top right corner, as it stands.
fn take_it(button: HWND) -> Option<(Vec<u8>, i32, i32)> {
    let mut place = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: our own window, and the slot is ours. A refusal leaves it
    // as it was and is one of the answers.
    if unsafe { GetWindowRect(button, &mut place) } == 0 {
        return None;
    }
    let side = SIDE
        .min(place.right - place.left)
        .min(place.bottom - place.top);
    if side <= 0 {
        return None;
    }

    // SAFETY: every object made here is ours and every one of them is
    // freed on every road out; the pixels are read only while the
    // surface holding them is still alive.
    unsafe {
        let screen = GetDC(std::ptr::null_mut());
        if screen.is_null() {
            return None;
        }
        let taking = CreateCompatibleDC(screen);
        if taking.is_null() {
            ReleaseDC(std::ptr::null_mut(), screen);
            return None;
        }
        let (sheet, pixels) = crate::picture::plain_surface(taking, (side, side));
        let mut kept = None;
        if !sheet.is_null() && !pixels.is_null() {
            let before = SelectObject(taking, sheet.cast());
            if BitBlt(
                taking,
                0,
                0,
                side,
                side,
                screen,
                place.right - side,
                place.top,
                SRCCOPY,
            ) != 0
            {
                let all =
                    std::slice::from_raw_parts(pixels as *const u8, (side * side * 4) as usize);
                kept = Some((all.to_vec(), side, side));
            }
            SelectObject(taking, before);
            DeleteObject(sheet.cast());
        }
        DeleteDC(taking);
        ReleaseDC(std::ptr::null_mut(), screen);
        kept
    }
}

/// Writes those pixels as a bitmap: fourteen bytes of file, forty of
/// picture, then the rows from the bottom, which is the way a bitmap
/// reads.
fn write_it(where_to: &PathBuf, pixels: &[u8], wide: i32, high: i32) -> std::io::Result<()> {
    let weight = (wide * high * 4) as u32;
    let start = 14u32 + 40;
    let mut file = fs::File::create(where_to)?;
    file.write_all(b"BM")?;
    file.write_all(&(start + weight).to_le_bytes())?;
    file.write_all(&0u16.to_le_bytes())?;
    file.write_all(&0u16.to_le_bytes())?;
    file.write_all(&start.to_le_bytes())?;
    file.write_all(&40u32.to_le_bytes())?;
    file.write_all(&wide.to_le_bytes())?;
    file.write_all(&high.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?;
    file.write_all(&32u16.to_le_bytes())?;
    file.write_all(&0u32.to_le_bytes())?;
    file.write_all(&weight.to_le_bytes())?;
    for _ in 0..4 {
        file.write_all(&0u32.to_le_bytes())?;
    }
    let line = (wide as usize) * 4;
    for y in (0..high as usize).rev() {
        file.write_all(&pixels[y * line..(y + 1) * line])?;
    }
    Ok(())
}
