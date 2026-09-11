//! Windows' clipboard, read and written from the desktop it belongs to.
//!
//! A clipboard is opened, read or written, and closed at once. It is one
//! thing shared by every program on the window station, and a program
//! holding it open is a program no one else can copy or paste around:
//! everything slow here happens before it is opened and after it is
//! closed.
//!
//! Which format is looked at, and in what order, is the one decision of
//! this file that is not Windows'. Text comes first, because a program
//! that offers both almost always means the text: a range of cells, a
//! line of a table, a link. A picture comes next as the PNG its own
//! program put there, and last as the bitmap every Windows program has
//! always been able to put there.

mod picture;

use std::path::PathBuf;
use std::time::Duration;

use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData,
    GetClipboardFormatNameW, GetClipboardSequenceNumber, IsClipboardFormatAvailable, OpenClipboard,
    RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
};
use windows::Win32::System::Ole::{CF_BITMAP, CF_DIB, CF_DIBV5, CF_HDROP, CF_TEXT, CF_UNICODETEXT};
use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};
use windows::core::w;
use zyr_proto::clipboard::{Clip, Kind};

use crate::files::{self, Walked};
use crate::{Found, Trouble};

/// How many times opening the clipboard is tried before giving up.
///
/// Not a fault to be refused once: the clipboard belongs to whoever holds
/// it, every program on the desktop reaches for it, and the one that has
/// it lets go within a moment. Refusing on the first no would mean losing
/// what somebody copied because a text editor happened to be reading it.
const TRIES: u32 = 8;

/// How long is left between two of those.
const BETWEEN_TRIES: Duration = Duration::from_millis(20);

/// The clipboard, open, and closed whatever happens next.
///
/// A clipboard left open is a desktop where nothing can be copied or
/// pasted any more, so closing it is not tidiness: every path out of the
/// two functions below goes through this.
struct Open;

impl Open {
    fn now() -> Result<Self, Trouble> {
        let mut refused = None;
        for turn in 0..TRIES {
            // SAFETY: naming no window makes the open belong to this
            // task rather than to a window, which is what a program with
            // no window of its own wants.
            match unsafe { OpenClipboard(None) } {
                Ok(()) => return Ok(Self),
                Err(e) => refused = Some(e),
            }
            if turn + 1 < TRIES {
                std::thread::sleep(BETWEEN_TRIES);
            }
        }
        Err(Trouble::of(format!(
            "le presse-papiers est resté pris par un autre programme : {}",
            refused.map(|e| e.to_string()).unwrap_or_default()
        )))
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        // SAFETY: balances the one open above, and only that one.
        let _ = unsafe { CloseClipboard() };
    }
}

pub fn what_it_holds() -> Result<Option<Found>, Trouble> {
    let png_format = the_png_format();
    let dropped = {
        let _open = Open::now()?;

        if let Some(said) = text_on_it()
            && !said.is_empty()
        {
            return Ok(Some(Found::of(Clip::text(&said))));
        }
        // The picture as its own program left it, which is the only
        // shape that keeps what a bitmap cannot hold: what is
        // see-through in it.
        if let Some(png) = bytes_on_it(png_format) {
            return Ok(Some(Found::of(Clip::picture(png))));
        }
        // And otherwise the bitmap, which is what a screenshot is.
        // Nothing here reads what a bitmap says about see-through parts:
        // a program that had any to say would have offered the shape
        // above, and a screenshot's spare byte per pixel is famously
        // whatever the screen happened to leave there. Read as anything
        // but opaque, it turns a screenshot black.
        if let Some(dib) = bytes_on_it(u32::from(CF_DIB.0)) {
            return picture::a_png_of(&dib).map(|png| Some(Found::of(Clip::picture(png))));
        }
        // Last, files, and the clipboard is let go of before they are
        // looked at: walking a folder of ten thousand files is a walk
        // across a disk, and nothing on this desktop should wait on it
        // to be able to copy or paste.
        the_drop_on_it()
    };
    let Some(dropped) = dropped else {
        return Ok(None);
    };
    let Walked {
        listed,
        really,
        cut_short,
    } = files::walked(&dropped);
    if listed.is_empty() {
        return Ok(None);
    }
    Ok(Some(Found {
        clip: Clip::files(&listed),
        really,
        cut_short,
    }))
}

/// The paths the Explorer put on the clipboard, when it put any.
///
/// Their own paths on this computer, which is all a clipboard ever holds
/// of a file: what is copied is the name and never the thing.
fn the_drop_on_it() -> Option<Vec<PathBuf>> {
    // SAFETY: reads the clipboard this task holds open, and a drop is a
    // moveable memory handle like everything else it carries.
    let dropped = unsafe {
        IsClipboardFormatAvailable(u32::from(CF_HDROP.0)).ok()?;
        HDROP(GetClipboardData(u32::from(CF_HDROP.0)).ok()?.0)
    };
    // SAFETY: a handle the clipboard just gave us. Naming this file asks
    // how many there are rather than for one of them.
    let how_many = unsafe { DragQueryFileW(dropped, u32::MAX, None) };
    let mut paths = Vec::with_capacity(how_many as usize);
    for which in 0..how_many {
        // SAFETY: the same handle, asked first how long the name is and
        // then for the name in a buffer of that length and one more for
        // the nought it writes.
        let taken = unsafe { DragQueryFileW(dropped, which, None) };
        if taken == 0 {
            continue;
        }
        let mut spelled = vec![0u16; taken as usize + 1];
        // SAFETY: the same again, into a buffer made to the size it
        // just asked for.
        let written = unsafe { DragQueryFileW(dropped, which, Some(&mut spelled)) };
        if written == 0 {
            continue;
        }
        paths.push(PathBuf::from(String::from_utf16_lossy(
            &spelled[..written as usize],
        )));
    }
    (!paths.is_empty()).then_some(paths)
}

pub fn hold_this(clip: &Clip) -> Result<Vec<String>, Trouble> {
    let mut refused = Vec::new();
    // Everything is made ready before the clipboard is opened. Turning a
    // PNG back into a bitmap is the slowest thing this crate does, and
    // doing it with the clipboard in hand would stop every program on the
    // desktop from copying or pasting for as long as it took.
    let ready: Vec<(u32, Vec<u8>)> = match clip.kind() {
        Kind::Text => {
            let said = clip
                .said()
                .ok_or_else(|| Trouble::of("ce texte n'en est pas un"))?;
            vec![(u32::from(CF_UNICODETEXT.0), the_way_windows_spells(said))]
        }
        // Twice over: as the PNG it came as, for the programs that ask
        // for one and want what is see-through in it, and as the bitmap
        // for every other program. Windows works the older shapes out
        // from that second one by itself, so nothing else has to be put
        // there.
        //
        // The bitmap is the one of the two that has to be made here, and
        // it is made without stopping the rest: losing it costs the
        // programs that read nothing else, and failing the whole because
        // of it would leave a clipboard emptied with nothing put back,
        // which is worse than either.
        Kind::Picture => {
            let mut both = vec![(the_png_format(), clip.bytes().to_vec())];
            match picture::a_bitmap_of(clip.bytes()) {
                Ok(bitmap) => both.push((u32::from(CF_DIBV5.0), bitmap)),
                Err(e) => refused.push(format!(
                    "l'image n'est posée qu'en PNG, le bitmap que lisent les autres programmes \
                     n'a pas pu être fait : {e}"
                )),
            }
            both
        }
        // Putting files on a clipboard is not putting anything on it: it
        // is standing in for files that live on the other computer, and
        // answering Windows when somebody pastes them. That is a thing
        // this crate holds rather than a thing it hands over, and it
        // lives next door.
        Kind::Files => {
            return Err(Trouble::of(
                "des fichiers ne se posent pas ainsi : ils se tiennent",
            ));
        }
    };

    let _open = Open::now()?;
    // SAFETY: the clipboard is ours for as long as the guard above lives.
    unsafe { EmptyClipboard() }
        .map_err(|e| Trouble::of(format!("le presse-papiers n'a pas pu être vidé : {e}")))?;
    let mut put = 0;
    for (format, bytes) in ready {
        match Block::holding(&bytes).and_then(|block| block.given_to(format)) {
            Ok(()) => put += 1,
            Err(e) => refused.push(e.to_string()),
        }
    }
    if put == 0 {
        return Err(Trouble::of(format!(
            "rien n'a pu être posé au presse-papiers : {}",
            refused.join(" ; ")
        )));
    }
    Ok(refused)
}

/// The names of everything on this computer's clipboard right now.
///
/// Read for the journal and for nothing else. It is the one line that
/// says, after the fact, why something somebody copied never crossed: a
/// clipboard carrying a shape this product does not take says nothing of
/// itself, and every other trace of that moment looks exactly like a
/// clipboard nobody touched.
pub fn what_is_offered() -> String {
    let Ok(_open) = Open::now() else {
        return "le presse-papiers n'a pas pu être ouvert".to_string();
    };
    let mut named = Vec::new();
    let mut format = 0;
    loop {
        // SAFETY: reads the clipboard this task holds open, nought
        // starting the walk and nought ending it.
        format = unsafe { EnumClipboardFormats(format) };
        if format == 0 {
            break;
        }
        named.push(the_name_of(format));
    }
    if named.is_empty() {
        return "rien".to_string();
    }
    named.join(", ")
}

/// What a format is called, in the words of whoever registered it or in
/// this product's own for the ones Windows has always had.
fn the_name_of(format: u32) -> String {
    let mut spelled = [0u16; 80];
    // SAFETY: the slice is ours, and the call is told how long it is.
    let taken = unsafe { GetClipboardFormatNameW(format, &mut spelled) };
    if taken > 0 {
        return String::from_utf16_lossy(&spelled[..taken as usize]);
    }
    // The ones Windows has always had are numbered and not named, and
    // their numbers say nothing to whoever reads a journal.
    match format {
        known if known == u32::from(CF_UNICODETEXT.0) => "texte".to_string(),
        known if known == u32::from(CF_TEXT.0) => "texte ancien".to_string(),
        known if known == u32::from(CF_DIB.0) => "bitmap".to_string(),
        known if known == u32::from(CF_DIBV5.0) => "bitmap récent".to_string(),
        known if known == u32::from(CF_BITMAP.0) => "image".to_string(),
        other => format!("format {other}"),
    }
}

pub fn times_it_changed() -> u32 {
    // SAFETY: the call takes nothing and answers a number.
    unsafe { GetClipboardSequenceNumber() }
}

/// The number Windows files the picture format under on this machine.
///
/// It is looked up by name and not written down: the system hands out
/// these numbers as programs ask for them, so the same name is a
/// different number on another computer and after a restart. The name is
/// the one every program that carries a picture this way already uses.
fn the_png_format() -> u32 {
    // SAFETY: a name that lives for the whole of the program.
    unsafe { RegisterClipboardFormatW(w!("PNG")) }
}

/// Whether the clipboard has something under that number, and what.
///
/// Nothing is never a fault here: a format that is not offered, one whose
/// block cannot be locked, and an empty block all mean the same thing to
/// whoever asked, which is to look at the next format down.
fn bytes_on_it(format: u32) -> Option<Vec<u8>> {
    if format == 0 {
        return None;
    }
    // SAFETY: both calls read the clipboard this task holds open.
    unsafe {
        IsClipboardFormatAvailable(format).ok()?;
        let handle = GetClipboardData(format).ok()?;
        whole_of(handle)
    }
}

/// The text on the clipboard, as Rust spells text.
fn text_on_it() -> Option<String> {
    let raw = bytes_on_it(u32::from(CF_UNICODETEXT.0))?;
    let (pairs, _) = raw.as_chunks::<2>();
    let mut spelled: Vec<u16> = pairs.iter().map(|pair| u16::from_le_bytes(*pair)).collect();
    // The block ends on a nought that is not part of the text, and a
    // block is allowed to be larger than what it holds.
    if let Some(end) = spelled.iter().position(|&unit| unit == 0) {
        spelled.truncate(end);
    }
    Some(String::from_utf16_lossy(&spelled))
}

/// Text the way Windows keeps it: its own spelling, ending on a nought.
fn the_way_windows_spells(said: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(said.len() * 2 + 2);
    for unit in said.encode_utf16().chain(std::iter::once(0)) {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out
}

/// Copies out what a clipboard handle points at.
///
/// # Safety
///
/// The handle comes from the clipboard this task holds open, and is only
/// read for as long as that lasts.
unsafe fn whole_of(handle: HANDLE) -> Option<Vec<u8>> {
    let block = HGLOBAL(handle.0);
    // SAFETY: a handle the clipboard just gave us, which is moveable
    // memory like everything it carries.
    let size = unsafe { GlobalSize(block) };
    if size == 0 {
        return None;
    }
    // SAFETY: the same handle, unlocked below on every way out.
    let at = unsafe { GlobalLock(block) };
    if at.is_null() {
        return None;
    }
    let mut out = vec![0u8; size];
    // SAFETY: the block is that many bytes, by the call above, and the
    // slice was made that size a line ago.
    unsafe { std::ptr::copy_nonoverlapping(at.cast::<u8>(), out.as_mut_ptr(), size) };
    // SAFETY: balances the lock above. It answers a refusal when the last
    // lock goes, which is the ordinary case and not a fault.
    let _ = unsafe { GlobalUnlock(block) };
    Some(out)
}

/// Moveable memory holding what is about to be handed to the clipboard.
///
/// Given away it belongs to the system, which frees it in its own time;
/// refused it is ours to free, and this is what makes sure exactly one of
/// the two happens.
struct Block(HGLOBAL);

impl Block {
    fn holding(bytes: &[u8]) -> Result<Self, Trouble> {
        // SAFETY: moveable is what the clipboard takes, and nothing else.
        let block = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len()) }
            .map_err(|e| Trouble::of(format!("la mémoire a manqué : {e}")))?;
        let block = Self(block);
        // SAFETY: our own block, unlocked before this function ends.
        let at = unsafe { GlobalLock(block.0) };
        if at.is_null() {
            return Err(Trouble::of("la mémoire à copier n'a pas pu être tenue"));
        }
        // SAFETY: the block was asked for at exactly that size.
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), at.cast::<u8>(), bytes.len()) };
        // SAFETY: balances the lock above, refusing when the last lock
        // goes, which is the ordinary case.
        let _ = unsafe { GlobalUnlock(block.0) };
        Ok(block)
    }

    /// Hands it over under that format, after which it is the system's.
    fn given_to(self, format: u32) -> Result<(), Trouble> {
        // SAFETY: the clipboard is open and emptied by the caller, and
        // this is moveable memory, which is what it takes.
        match unsafe { SetClipboardData(format, Some(HANDLE(self.0.0))) } {
            Ok(_) => {
                // The system owns it now and will free it itself: giving
                // it back here would free memory the clipboard is holding.
                std::mem::forget(self);
                Ok(())
            }
            Err(e) => Err(Trouble::of(format!(
                "le presse-papiers a refusé ce qu'on lui donnait : {e}"
            ))),
        }
    }
}

impl Drop for Block {
    fn drop(&mut self) {
        // SAFETY: reached only when the clipboard did not take it, so it
        // is still ours.
        let _ = unsafe { GlobalFree(Some(self.0)) };
    }
}
