//! A picture of the floating button's own window, written beside the
//! journal.
//!
//! Three answers about this button's white artefact have been reasoned
//! out and three have been wrong: the far computer's screen seen
//! through it, the window's frame, the window's unerased ground. Each
//! was argued from a screenshot, and a screenshot of a cut window shows
//! only what the cut lets through. What nobody has ever looked at is the
//! window itself, the part the cut hides included, which is where the
//! artefact has to come from.
//!
//! So the button photographs itself. `PrintWindow` with the flag that
//! renders the full content is the one call that reaches a window drawn
//! by a composition surface, which is what a web view is; the cut plays
//! no part in it, so what comes back is everything the window holds.
//! Written as a bitmap because a bitmap is a header and the pixels, and
//! anything more would be a dependency for a picture nobody keeps.

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject,
};
use windows_sys::Win32::Storage::Xps::PrintWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowRect, PW_RENDERFULLCONTENT};

use crate::journal::note;

/// How much of the window is kept, counted from its top right corner.
///
/// The corner the button hangs from, so the logo is always in it, and
/// wide enough to carry a good part of what the cut throws away. The
/// whole window would be a session's worth of megabytes for a picture
/// that is looked at once.
const SIDE: i32 = 320;

/// How many are kept in a session.
///
/// One a session would miss the interesting ones: the artefact shows
/// when the window is resized, and this window is resized four or five
/// times before anybody has clicked anything.
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
    if rank >= AT_MOST {
        return;
    }
    match take_it(button) {
        Some((pixels, wide, high)) => {
            let where_to = zyr_proto::paths::logs_dir().join(format!("bouton-{}.bmp", rank + 1));
            match write_it(&where_to, &pixels, wide, high) {
                Ok(()) => note(&format!(
                    "bouton flottant photographié dans {} : {wide}x{high} du coin haut droit de sa fenêtre, {why}",
                    where_to.display()
                )),
                Err(fault) => note(&format!(
                    "bouton flottant : la photo n'a pas pu être écrite : {fault}"
                )),
            }
        }
        None => note("bouton flottant : la photo a été refusée par le système"),
    }
}

/// The top right corner of the window, in whatever it really holds.
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
    let wide = place.right - place.left;
    let high = place.bottom - place.top;
    if wide <= 0 || high <= 0 {
        return None;
    }

    // SAFETY: every object made here is ours and every one of them is
    // freed on every road out; the pixels are read only while the
    // surface holding them is still alive.
    unsafe {
        let screen = GetDC(std::ptr::null_mut());
        let taking = CreateCompatibleDC(screen);
        ReleaseDC(std::ptr::null_mut(), screen);
        if taking.is_null() {
            return None;
        }
        let (sheet, pixels) = crate::picture::plain_surface(taking, (wide, high));
        let mut kept = None;
        if !sheet.is_null() && !pixels.is_null() {
            let before = SelectObject(taking, sheet.cast());
            if PrintWindow(button, taking, PW_RENDERFULLCONTENT) != 0 {
                let side = SIDE.min(wide).min(high);
                let from = ((wide - side) as usize) * 4;
                let line = (wide as usize) * 4;
                let all = std::slice::from_raw_parts(pixels as *const u8, line * high as usize);
                let mut some = Vec::with_capacity((side * side * 4) as usize);
                for y in 0..side as usize {
                    let row = y * line + from;
                    some.extend_from_slice(&all[row..row + (side as usize) * 4]);
                }
                kept = Some((some, side, side));
            }
            SelectObject(taking, before);
            DeleteObject(sheet.cast());
        }
        DeleteDC(taking);
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
