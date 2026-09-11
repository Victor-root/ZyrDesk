//! Standing in, on this computer's clipboard, for files that are on the
//! other one.
//!
//! This is what makes the bytes cross at the paste and not at the copy.
//! Windows has one way of holding files that are not there yet, and every
//! program that ever offered an attachment without writing it to a disk
//! first uses it: the clipboard holds a list of names and weights, and an
//! object that hands over the contents of one of them when somebody
//! finally asks. Nobody asks until somebody pastes.
//!
//! So this product puts such an object on the clipboard. It answers two
//! things: the list, out of what the far computer named, and the contents
//! of one file, out of the folder its bytes are arriving in. The first
//! answer costs nothing. The second is what starts the transfer, and it
//! waits while the bytes come.
//!
//! # What it costs to hold it
//!
//! An object on a clipboard lives in the program that put it there, and
//! dies with it. So the helper that sets this one stays alive for as long
//! as the clipboard holds it, instead of ending after a few seconds like
//! the one that only reads. That is the whole of the difference, and it
//! is the price of the bytes not crossing until they are wanted.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{
    DV_E_FORMATETC, DV_E_TYMED, E_NOTIMPL, E_UNEXPECTED, GlobalFree, S_FALSE, S_OK,
};
use windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_NORMAL;
use windows::Win32::System::Com::{
    DATADIR_GET, FORMATETC, IAgileObject, IAgileObject_Impl, IDataObject, IDataObject_Impl,
    IEnumFORMATETC, IEnumFORMATETC_Impl, IEnumSTATDATA, ISequentialStream_Impl, IStream,
    IStream_Impl, LOCKTYPE, STATFLAG, STATSTG, STGC, STGMEDIUM, STGMEDIUM_0, STGTY_STREAM,
    STREAM_SEEK, STREAM_SEEK_CUR, STREAM_SEEK_END, STREAM_SEEK_SET, TYMED_HGLOBAL, TYMED_ISTREAM,
};
use windows::Win32::System::DataExchange::RegisterClipboardFormatW;
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::System::Ole::{OleInitialize, OleSetClipboard, OleUninitialize};
use windows::Win32::UI::Shell::{
    CFSTR_FILECONTENTS, CFSTR_FILEDESCRIPTORW, FD_ATTRIBUTES, FD_FILESIZE, FD_PROGRESSUI,
    FILEDESCRIPTORW, IDataObjectAsyncCapability, IDataObjectAsyncCapability_Impl,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, MSG, MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx, PM_REMOVE,
    PeekMessageW, QS_ALLINPUT, TranslateMessage,
};
use windows::core::{BOOL, Ref, implement};
use zyr_proto::clipboard::{Listed, Listing};

use crate::Trouble;

/// How long a file's next bytes are waited for before the paste is given
/// up as not going to finish.
///
/// Generous, because what is being waited on is a network and a person:
/// the first bytes cannot start moving until the service has noticed the
/// paste, and a link that stalls for half a minute is a link that may
/// well come back. Past this, Windows is told the copy failed, which is
/// far better than a copy dialog that never ends.
const PATIENCE: Duration = Duration::from_secs(60);

/// How often the file is looked at again while its bytes are arriving.
const LOOK_AGAIN: Duration = Duration::from_millis(40);

/// Whether anybody has asked for the contents of a file yet.
///
/// Which is to say: whether somebody pasted. Nothing else in Windows
/// asks for those, and this is the only moment the product has to learn
/// that the bytes are wanted.
static ASKED_FOR: AtomicBool = AtomicBool::new(false);

thread_local! {
    /// What this program put on the clipboard, and how many times the
    /// clipboard had changed by the time it was there.
    ///
    /// The object is held because one on a clipboard belongs to the
    /// program that put it there: letting go of it is the clipboard
    /// losing the files. The number beside it is how this side tells
    /// later whether it is still the one being held, and both are kept
    /// per thread because a clipboard belongs to one.
    static STANDING: RefCell<Option<(IDataObject, u32)>> = const { RefCell::new(None) };
}

/// This thread's place in the system's own arrangement, held for as long
/// as this is.
///
/// Nothing here works without it. An object put on a clipboard is put
/// there through OLE, OLE only speaks to a thread that has opened an
/// apartment, and what it opens for that thread is a window of its own
/// through which every other program's questions arrive. Which is the
/// other half of the price: this thread has to read its messages, or the
/// paste in the other program waits for an answer nobody is listening
/// for. See [`answer_for`].
pub struct Attending(bool);

impl Attending {
    /// Opens it, or says why not.
    pub fn opened() -> Result<Self, Trouble> {
        // SAFETY: nothing is touched but this thread's own apartment.
        match unsafe { OleInitialize(None) } {
            Ok(()) => Ok(Self(true)),
            // A thread already in an apartment of another kind keeps the
            // one it had, which is an answer and not a fault; what will
            // not work is putting something on the clipboard from it, and
            // that is what the refusal says.
            Err(e) => Err(Trouble::of(format!(
                "ce programme n'a pas sa place auprès du presse-papiers : {e}"
            ))),
        }
    }
}

impl Drop for Attending {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: balances the one call above, and only that one.
            unsafe { OleUninitialize() };
        }
    }
}

/// Waits that long, answering meanwhile what the system asks of this
/// thread.
///
/// Not a plain sleep, and the difference is the whole of whether a paste
/// works: the clipboard's own window lives on this thread, every other
/// program's questions about what is on the clipboard arrive there as
/// messages, and a thread that never reads its messages is a paste that
/// hangs until Windows gives up on it.
pub fn answer_for(how_long: Duration) {
    let until = Instant::now() + how_long;
    loop {
        let Some(left) = until.checked_duration_since(Instant::now()) else {
            return;
        };
        // SAFETY: no handle is waited on, only this thread's own queue.
        unsafe {
            MsgWaitForMultipleObjectsEx(
                None,
                u32::try_from(left.as_millis()).unwrap_or(u32::MAX),
                QS_ALLINPUT,
                MWMO_INPUTAVAILABLE,
            )
        };
        let mut said = MSG::default();
        // SAFETY: a message of ours, filled and then handed straight back.
        while unsafe { PeekMessageW(&mut said, None, 0, 0, PM_REMOVE) }.as_bool() {
            unsafe {
                let _ = TranslateMessage(&said);
                DispatchMessageW(&said);
            }
        }
    }
}

/// Puts an object on the clipboard that stands in for those files.
///
/// The folder is where their bytes will arrive. Nothing is read from it
/// here: what is put on the clipboard is the list and a promise, and the
/// promise is only called in when somebody pastes.
pub fn stand_in_for(listed: &Listing, folder: &Path) -> Result<(), Trouble> {
    let object: IDataObject = FarFiles {
        listed: listed.clone(),
        folder: folder.to_path_buf(),
        descriptor: a_format(CFSTR_FILEDESCRIPTORW),
        contents: a_format(CFSTR_FILECONTENTS),
        in_operation: AtomicBool::new(false),
    }
    .into();

    ASKED_FOR.store(false, Ordering::SeqCst);
    // SAFETY: an object this thread made, handed to OLE, which holds it
    // for as long as the clipboard does.
    unsafe { OleSetClipboard(&object) }
        .map_err(|e| Trouble::of(format!("les fichiers n'ont pas pu être posés : {e}")))?;
    let since = super::times_it_changed();
    STANDING.with(|held| *held.borrow_mut() = Some((object, since)));
    Ok(())
}

/// Whether this program is still the one holding the clipboard.
///
/// The moment somebody copies anything else, anywhere on this computer,
/// this stops being true: what was standing in is gone, and the program
/// holding it has nothing left to hold.
///
/// Told by the count of changes and not by asking OLE which object it
/// carries, because OLE answers that question with a yes and a no that
/// are both successes, and the two are the same value to anything on this
/// side. The count is the same question read the other way round: nobody
/// can have taken the clipboard without it moving.
pub fn still_standing() -> bool {
    STANDING.with(|held| {
        let held = held.borrow();
        let Some(&(_, since)) = held.as_ref() else {
            return false;
        };
        // Nought is Windows refusing to say, which it does to a program
        // that may not touch the clipboard at all. Letting go on that
        // would throw away a clipboard nobody has taken.
        let now = super::times_it_changed();
        now == 0 || now == since
    })
}

/// Whether somebody has pasted what is being stood in for.
///
/// Read rather than pushed: this is asked at every turn of a loop that
/// was already turning, and it answers a single bit.
pub fn somebody_pasted() -> bool {
    ASKED_FOR.load(Ordering::SeqCst)
}

/// Lets go of the promise that was being held.
///
/// Cleared when this program is still the one holding the clipboard,
/// since what it holds is a promise about to be broken and a clipboard
/// offering files nobody can send is worse than an empty one. Only
/// dropped otherwise: by then the clipboard is somebody else's, and
/// clearing it would throw away whatever they just copied.
pub fn let_go() {
    if still_standing() {
        // SAFETY: nothing is named, which is what empties a clipboard.
        // A refusal leaves it as it was, which the drop below covers.
        let _ = unsafe { OleSetClipboard(None) };
    }
    STANDING.with(|held| *held.borrow_mut() = None);
    ASKED_FOR.store(false, Ordering::SeqCst);
}

/// The number Windows files a format under on this machine.
fn a_format(named: windows::core::PCWSTR) -> u16 {
    // SAFETY: a name that lives for the whole of the program.
    unsafe { RegisterClipboardFormatW(named) as u16 }
}

/// The object itself: a list of files that are somewhere else.
///
/// It answers on whichever thread asks, which is what `IAgileObject`
/// declares and what everything it holds allows: a list nobody changes
/// after it is made, a folder that is only read, and one flag that is
/// atomic. The declaration is not a nicety. Without it every question
/// Windows asks comes back to the one thread that put the object on the
/// clipboard, and the answer that waits a minute for bytes off a network
/// would be that thread waiting, with the clipboard and the helper's own
/// work waiting behind it.
#[implement(IDataObject, IDataObjectAsyncCapability, IAgileObject)]
struct FarFiles {
    listed: Listing,
    folder: PathBuf,
    descriptor: u16,
    contents: u16,
    in_operation: AtomicBool,
}

impl FarFiles_Impl {
    /// The list, as Windows wants it: how many, then one block each.
    fn the_list(&self) -> windows::core::Result<STGMEDIUM> {
        let files = self.listed.files();
        let each = size_of::<FILEDESCRIPTORW>();
        let weight = size_of::<u32>() + each * files.len();

        // SAFETY: moveable memory, which is what a clipboard takes, and
        // handed over below without ever being freed here.
        let block = unsafe { GlobalAlloc(GMEM_MOVEABLE, weight) }?;
        // SAFETY: our own block, unlocked before this function ends.
        let at = unsafe { GlobalLock(block) };
        if at.is_null() {
            // Nobody has been handed it yet, so nobody else will free it.
            // SAFETY: our own block, never locked and never given away.
            let _ = unsafe { GlobalFree(Some(block)) };
            return Err(E_UNEXPECTED.into());
        }
        // SAFETY: the block was asked for at exactly that size, and
        // everything written below stays inside it.
        unsafe {
            std::ptr::write_unaligned(at.cast::<u32>(), files.len() as u32);
            let first = at
                .cast::<u8>()
                .add(size_of::<u32>())
                .cast::<FILEDESCRIPTORW>();
            for (rank, file) in files.iter().enumerate() {
                std::ptr::write_unaligned(first.add(rank), described(file));
            }
            let _ = GlobalUnlock(block);
        }
        Ok(STGMEDIUM {
            tymed: TYMED_HGLOBAL.0 as u32,
            u: STGMEDIUM_0 { hGlobal: block },
            pUnkForRelease: std::mem::ManuallyDrop::new(None),
        })
    }

    /// The contents of one file, as a stream that waits for them.
    ///
    /// The first ask is the paste: nothing else in Windows wants these,
    /// and it is the only word the product gets that the bytes are
    /// wanted at all.
    fn a_file(&self, rank: i32) -> windows::core::Result<STGMEDIUM> {
        let file = usize::try_from(rank)
            .ok()
            .and_then(|rank| self.listed.at(rank))
            .ok_or(DV_E_FORMATETC)?;
        ASKED_FOR.store(true, Ordering::SeqCst);

        let stream: IStream = Arriving {
            path: self.folder.join(file.path()),
            size: file.bytes(),
            at: Mutex::new(0),
        }
        .into();
        Ok(STGMEDIUM {
            tymed: TYMED_ISTREAM.0 as u32,
            u: STGMEDIUM_0 {
                pstm: std::mem::ManuallyDrop::new(Some(stream)),
            },
            pUnkForRelease: std::mem::ManuallyDrop::new(None),
        })
    }

    /// Which of the two this is asking for, if either.
    fn asked(&self, wanted: *const FORMATETC) -> Option<(u16, i32, u32)> {
        if wanted.is_null() {
            return None;
        }
        // SAFETY: a block the caller owns for the length of the call.
        let wanted = unsafe { &*wanted };
        Some((wanted.cfFormat, wanted.lindex, wanted.tymed))
    }
}

/// One file, in the block Windows reads a list from.
///
/// The name is spelled out whole before anything is put in the block,
/// rather than written into it afterwards: the block's fields sit wherever
/// the one before them ended, so there is no borrowing one of them.
fn described(file: &Listed) -> FILEDESCRIPTORW {
    // The path within what was copied, in the separator Windows writes:
    // the shell makes the folders above it as it goes, which is how a
    // folder copied whole arrives as that folder.
    let mut spelled = [0u16; 260];
    let name: Vec<u16> = file
        .path()
        .replace('/', "\\")
        .encode_utf16()
        .take(spelled.len() - 1)
        .collect();
    spelled[..name.len()].copy_from_slice(&name);

    FILEDESCRIPTORW {
        dwFlags: (FD_FILESIZE.0 | FD_ATTRIBUTES.0 | FD_PROGRESSUI.0) as u32,
        nFileSizeHigh: (file.bytes() >> 32) as u32,
        nFileSizeLow: file.bytes() as u32,
        // Ordinary, which is what every file this carries is: what is
        // read-only or hidden over there has no business being so here,
        // the copy being a new file on a new machine.
        dwFileAttributes: FILE_ATTRIBUTE_NORMAL.0,
        cFileName: spelled,
        ..Default::default()
    }
}

impl IDataObject_Impl for FarFiles_Impl {
    fn GetData(&self, wanted: *const FORMATETC) -> windows::core::Result<STGMEDIUM> {
        let Some((format, rank, tymed)) = self.asked(wanted) else {
            return Err(DV_E_FORMATETC.into());
        };
        if format == self.descriptor {
            if tymed & TYMED_HGLOBAL.0 as u32 == 0 {
                return Err(DV_E_TYMED.into());
            }
            return self.the_list();
        }
        if format == self.contents {
            if tymed & TYMED_ISTREAM.0 as u32 == 0 {
                return Err(DV_E_TYMED.into());
            }
            return self.a_file(rank);
        }
        Err(DV_E_FORMATETC.into())
    }

    /// Never: what this hands over is a block it made and a stream it
    /// made, and neither fits in one the caller allocated.
    fn GetDataHere(
        &self,
        _wanted: *const FORMATETC,
        _into: *mut STGMEDIUM,
    ) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn QueryGetData(&self, wanted: *const FORMATETC) -> windows::core::HRESULT {
        match self.asked(wanted) {
            Some((format, _, tymed))
                if format == self.descriptor && tymed & TYMED_HGLOBAL.0 as u32 != 0 =>
            {
                S_OK
            }
            Some((format, _, tymed))
                if format == self.contents && tymed & TYMED_ISTREAM.0 as u32 != 0 =>
            {
                S_OK
            }
            _ => DV_E_FORMATETC,
        }
    }

    /// Nothing of ours has a plainer spelling than the one it was asked
    /// under, which is what this answers.
    fn GetCanonicalFormatEtc(
        &self,
        _wanted: *const FORMATETC,
        plainest: *mut FORMATETC,
    ) -> windows::core::HRESULT {
        if !plainest.is_null() {
            // SAFETY: a block the caller owns for the length of the call.
            unsafe { (*plainest).ptd = std::ptr::null_mut() };
        }
        S_FALSE
    }

    /// Nothing is put into this object: it stands in for files that are
    /// somewhere else, and nothing here is anybody's to change.
    fn SetData(
        &self,
        _what: *const FORMATETC,
        _medium: *const STGMEDIUM,
        _release: BOOL,
    ) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn EnumFormatEtc(&self, direction: u32) -> windows::core::Result<IEnumFORMATETC> {
        if direction != DATADIR_GET.0 as u32 {
            return Err(E_NOTIMPL.into());
        }
        Ok(TheseFormats {
            offered: vec![
                a_formatetc(self.descriptor, -1, TYMED_HGLOBAL.0 as u32),
                // Which file it is belongs to the asking and not to the
                // offer: what is advertised here is that contents can be
                // had at all, and every one of them the same way.
                a_formatetc(self.contents, -1, TYMED_ISTREAM.0 as u32),
            ],
            at: Mutex::new(0),
        }
        .into())
    }

    /// Nothing here ever changes, so there is nothing to be told about.
    fn DAdvise(
        &self,
        _what: *const FORMATETC,
        _how: u32,
        _sink: Ref<'_, windows::Win32::System::Com::IAdviseSink>,
    ) -> windows::core::Result<u32> {
        Err(windows::Win32::Foundation::OLE_E_ADVISENOTSUPPORTED.into())
    }

    fn DUnadvise(&self, _which: u32) -> windows::core::Result<()> {
        Err(windows::Win32::Foundation::OLE_E_ADVISENOTSUPPORTED.into())
    }

    fn EnumDAdvise(&self) -> windows::core::Result<IEnumSTATDATA> {
        Err(windows::Win32::Foundation::OLE_E_ADVISENOTSUPPORTED.into())
    }
}

impl IAgileObject_Impl for FarFiles_Impl {}

impl IDataObjectAsyncCapability_Impl for FarFiles_Impl {
    /// Taken note of and nothing more: this object answers the same on
    /// whichever thread asks, so what the copying program settles on
    /// changes nothing here.
    fn SetAsyncMode(&self, _asynchronous: BOOL) -> windows::core::Result<()> {
        Ok(())
    }

    /// Yes. The copying program asks this before it decides which thread
    /// to copy on, and reading a network on the thread that draws its
    /// windows is that window frozen for the length of the copy.
    fn GetAsyncMode(&self) -> windows::core::Result<BOOL> {
        Ok(true.into())
    }

    fn StartOperation(
        &self,
        _reserved: Ref<'_, windows::Win32::System::Com::IBindCtx>,
    ) -> windows::core::Result<()> {
        self.in_operation.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn InOperation(&self) -> windows::core::Result<BOOL> {
        Ok(self.in_operation.load(Ordering::SeqCst).into())
    }

    fn EndOperation(
        &self,
        _how: windows::core::HRESULT,
        _reserved: Ref<'_, windows::Win32::System::Com::IBindCtx>,
        _effects: u32,
    ) -> windows::core::Result<()> {
        self.in_operation.store(false, Ordering::SeqCst);
        Ok(())
    }
}

/// One of the shapes this object offers.
fn a_formatetc(format: u16, index: i32, tymed: u32) -> FORMATETC {
    FORMATETC {
        cfFormat: format,
        ptd: std::ptr::null_mut(),
        dwAspect: windows::Win32::System::Com::DVASPECT_CONTENT.0,
        lindex: index,
        tymed,
    }
}

/// The two shapes, walked through for whoever asks what is on offer.
#[implement(IEnumFORMATETC)]
struct TheseFormats {
    offered: Vec<FORMATETC>,
    at: Mutex<usize>,
}

impl IEnumFORMATETC_Impl for TheseFormats_Impl {
    fn Next(&self, how_many: u32, into: *mut FORMATETC, taken: *mut u32) -> windows::core::HRESULT {
        let mut at = self.at.lock().expect("format offert");
        let mut given = 0;
        while given < how_many as usize && *at < self.offered.len() {
            // SAFETY: a run of blocks the caller owns, as many as it
            // said it had room for.
            unsafe { std::ptr::write(into.add(given), self.offered[*at]) };
            *at += 1;
            given += 1;
        }
        if !taken.is_null() {
            // SAFETY: a block the caller owns for the length of the call.
            unsafe { *taken = given as u32 };
        }
        if given == how_many as usize {
            S_OK
        } else {
            S_FALSE
        }
    }

    fn Skip(&self, how_many: u32) -> windows::core::Result<()> {
        let mut at = self.at.lock().expect("format offert");
        *at = at.saturating_add(how_many as usize);
        if *at > self.offered.len() {
            *at = self.offered.len();
            return Err(S_FALSE.into());
        }
        Ok(())
    }

    fn Reset(&self) -> windows::core::Result<()> {
        *self.at.lock().expect("format offert") = 0;
        Ok(())
    }

    fn Clone(&self) -> windows::core::Result<IEnumFORMATETC> {
        Ok(TheseFormats {
            offered: self.offered.clone(),
            at: Mutex::new(*self.at.lock().expect("format offert")),
        }
        .into())
    }
}

/// One file whose bytes are still arriving, read as though they were all
/// there.
///
/// This is where a paste waits. Everything else about the transfer runs
/// somewhere else and at its own pace; what happens here is that a read
/// of bytes that have not come yet comes back a moment later instead of
/// coming back empty, which is what makes a file arriving over a network
/// look like a file on a disk to whoever is copying it.
#[implement(IStream, IAgileObject)]
struct Arriving {
    path: PathBuf,
    size: u64,
    at: Mutex<u64>,
}

impl IAgileObject_Impl for Arriving_Impl {}

impl Arriving_Impl {
    /// Reads what is there, waiting for what is not yet.
    ///
    /// Fills what it was given rather than handing back the little that
    /// happens to have landed: short of what was asked for is how every
    /// reader of a stream is told it has reached the end, and a file
    /// still coming down a network would say that at every turn. So the
    /// only short answer here is the true end of the file.
    fn read_what_has_come(&self, into: &mut [u8]) -> std::io::Result<usize> {
        use std::io::{Read, Seek, SeekFrom};

        let mut at = self.at.lock().expect("où en est la lecture");
        let room = into.len().min(self.size.saturating_sub(*at) as usize);
        if room == 0 {
            return Ok(0);
        }
        let mut taken = 0;
        let mut waited_until = Instant::now() + PATIENCE;
        while taken < room {
            // Opened at each turn rather than held: the file is being
            // written by another program as this reads it, and one
            // opened before it existed stays a file that does not exist.
            if let Ok(mut open) = std::fs::File::open(&self.path) {
                open.seek(SeekFrom::Start(*at + taken as u64))?;
                let read = open.read(&mut into[taken..room])?;
                if read > 0 {
                    taken += read;
                    // Waited for afresh: what is being borne with is a
                    // link that has gone quiet, not one that is slow.
                    waited_until = Instant::now() + PATIENCE;
                    continue;
                }
            }
            if Instant::now() > waited_until {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "les octets ne sont jamais arrivés",
                ));
            }
            std::thread::sleep(LOOK_AGAIN);
        }
        *at += taken as u64;
        Ok(taken)
    }
}

impl ISequentialStream_Impl for Arriving_Impl {
    fn Read(
        &self,
        into: *mut core::ffi::c_void,
        room: u32,
        read: *mut u32,
    ) -> windows::core::HRESULT {
        if into.is_null() {
            return E_UNEXPECTED;
        }
        // SAFETY: a block the caller owns, of the length it just said.
        let slice = unsafe { std::slice::from_raw_parts_mut(into.cast::<u8>(), room as usize) };
        let taken = self.read_what_has_come(slice);
        if !read.is_null() {
            // SAFETY: a block the caller owns for the length of the call.
            unsafe { *read = *taken.as_ref().unwrap_or(&0) as u32 };
        }
        match taken {
            Ok(taken) if taken as u32 == room => S_OK,
            // Fewer than were asked for is the end of the file, which is
            // what every reader of a stream expects to see once.
            Ok(_) => S_FALSE,
            Err(_) => windows::Win32::Foundation::E_FAIL,
        }
    }

    /// Nothing writes into a file that lives on another computer.
    fn Write(
        &self,
        _from: *const core::ffi::c_void,
        _how_many: u32,
        _written: *mut u32,
    ) -> windows::core::HRESULT {
        E_NOTIMPL
    }
}

impl IStream_Impl for Arriving_Impl {
    fn Seek(&self, by: i64, from: STREAM_SEEK, landed: *mut u64) -> windows::core::Result<()> {
        let mut at = self.at.lock().expect("où en est la lecture");
        let base = match from {
            STREAM_SEEK_SET => 0,
            STREAM_SEEK_CUR => *at as i64,
            STREAM_SEEK_END => self.size as i64,
            _ => return Err(E_UNEXPECTED.into()),
        };
        let now = base.checked_add(by).ok_or(E_UNEXPECTED)?;
        if now < 0 {
            return Err(E_UNEXPECTED.into());
        }
        *at = now as u64;
        if !landed.is_null() {
            // SAFETY: a block the caller owns for the length of the call.
            unsafe { *landed = *at };
        }
        Ok(())
    }

    fn Stat(&self, about: *mut STATSTG, _how: &STATFLAG) -> windows::core::Result<()> {
        if about.is_null() {
            return Err(E_UNEXPECTED.into());
        }
        // SAFETY: a block the caller owns for the length of the call, and
        // no name is put in it, so there is nothing for anybody to free.
        unsafe {
            *about = STATSTG {
                cbSize: self.size,
                r#type: STGTY_STREAM.0 as u32,
                ..Default::default()
            };
        }
        Ok(())
    }

    fn Clone(&self) -> windows::core::Result<IStream> {
        Ok(Arriving {
            path: self.path.clone(),
            size: self.size,
            at: Mutex::new(*self.at.lock().expect("où en est la lecture")),
        }
        .into())
    }

    /// The rest belongs to a stream somebody writes to, and this is a
    /// stream that stands in for a file on another computer.
    fn SetSize(&self, _size: u64) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn CopyTo(
        &self,
        _into: Ref<'_, IStream>,
        _how_many: u64,
        _read: *mut u64,
        _written: *mut u64,
    ) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn Commit(&self, _how: &STGC) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn Revert(&self) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn LockRegion(&self, _from: u64, _how_many: u64, _how: &LOCKTYPE) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn UnlockRegion(&self, _from: u64, _how_many: u64, _how: u32) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }
}
