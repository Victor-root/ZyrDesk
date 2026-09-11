//! Turning what a clipboard carries into a PNG, and back.
//!
//! Windows has an imaging component of its own, the one every program on
//! the machine already uses to open a picture, and it is what does the
//! work here. Nothing of an encoder is carried by this product: a PNG
//! made by the system is a PNG every program reads, and a decoder written
//! here would be a second one to keep right.
//!
//! Both ways go through the same two shapes. A clipboard hands over a
//! packed bitmap, which GDI turns into a picture the imaging understands;
//! the imaging turns that into a PNG. Coming back, the imaging turns the
//! PNG into four bytes a pixel, and those become a packed bitmap again.
//! The arithmetic of that packing is next door, in `packed`, where it can
//! be tested on a machine that is not Windows.

use std::ffi::c_void;

use windows::Win32::Foundation::{E_FAIL, HGLOBAL};
use windows::Win32::Graphics::Gdi::{
    BITMAPINFO, BITMAPINFOHEADER, CBM_INIT, CreateDIBitmap, DIB_RGB_COLORS, DeleteObject, GetDC,
    HBITMAP, HDC, HGDIOBJ, ReleaseDC,
};
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_ContainerFormatPng, GUID_WICPixelFormat32bppBGRA, IWICBitmap,
    IWICBitmapFrameEncode, IWICImagingFactory, WICBitmapDitherTypeNone, WICBitmapEncoderNoCache,
    WICBitmapIgnoreAlpha, WICBitmapPaletteTypeCustom, WICDecodeMetadataCacheOnDemand,
};
use windows::Win32::System::Com::StructuredStorage::{
    CreateStreamOnHGlobal, GetHGlobalFromStream, IPropertyBag2,
};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
    IStream, STATFLAG_NONAME, STATSTG, STREAM_SEEK_SET,
};
use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};

use crate::Trouble;
use crate::packed;

/// The most pixels a picture may have to cross.
///
/// Sixteen screens of four million pixels. Past that, what is on the
/// clipboard is not something a session should be asked to carry, and the
/// arithmetic below would be working on numbers no longer worth trusting.
const MOST_PIXELS: usize = 64_000_000;

/// A PNG made of the packed bitmap a clipboard handed over.
pub fn a_png_of(dib: &[u8]) -> Result<Vec<u8>, Trouble> {
    let start = packed::where_the_pixels_start(dib)
        .ok_or_else(|| Trouble::of("ce que le presse-papiers porte ne décrit pas une image"))?;
    let _com = Com::up();
    let factory = imaging()?;
    let bitmap = Bitmap::of(dib, start)?;

    // SAFETY: a bitmap this thread just made and still holds. Its fourth
    // byte a pixel is left out of it on purpose: see the caller.
    let picture = unsafe {
        factory.CreateBitmapFromHBITMAP(bitmap.0, Default::default(), WICBitmapIgnoreAlpha)
    }
    .map_err(|e| Trouble::of(format!("l'image du presse-papiers n'a pas été lue : {e}")))?;

    let stream = a_stream()?;
    written_as_a_png(&factory, &picture, &stream)
        .map_err(|e| Trouble::of(format!("l'image n'a pas pu être encodée : {e}")))?;
    whole_of(&stream)
}

/// Writes that picture into that stream, as a PNG.
///
/// Its own function and not a run of lines above, so that the seven calls
/// it takes can each hand back the same refusal: an encoder is opened, a
/// page is made on it, the picture is written into the page, and the page
/// and the encoder are each closed. None of the seven says anything
/// another would not.
fn written_as_a_png(
    factory: &IWICImagingFactory,
    picture: &IWICBitmap,
    stream: &IStream,
) -> windows::core::Result<()> {
    // SAFETY: interfaces this thread obtained and still holds.
    unsafe {
        let encoder = factory.CreateEncoder(&GUID_ContainerFormatPng, std::ptr::null())?;
        encoder.Initialize(stream, WICBitmapEncoderNoCache)?;
        let mut page: Option<IWICBitmapFrameEncode> = None;
        let mut how: Option<IPropertyBag2> = None;
        encoder.CreateNewFrame(&mut page, &mut how)?;
        let page = page.ok_or_else(|| windows::core::Error::from_hresult(E_FAIL))?;
        page.Initialize(how.as_ref())?;
        page.WriteSource(picture, std::ptr::null())?;
        page.Commit()?;
        encoder.Commit()
    }
}

/// The packed bitmap that goes back on a clipboard, made of a PNG.
pub fn a_bitmap_of(png: &[u8]) -> Result<Vec<u8>, Trouble> {
    let _com = Com::up();
    let factory = imaging()?;
    let stream = a_stream()?;
    filled_with(&stream, png)?;

    // SAFETY: interfaces this thread obtained and still holds. The
    // picture is asked for as four bytes a pixel whatever the PNG was,
    // which is the one shape a clipboard carries.
    let (wide, high, converter) = unsafe {
        let decoder = factory
            .CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand)
            .map_err(|e| Trouble::of(format!("cette image ne se lit pas : {e}")))?;
        let frame = decoder
            .GetFrame(0)
            .map_err(|e| Trouble::of(format!("cette image n'a pas de première page : {e}")))?;
        let converter = factory
            .CreateFormatConverter()
            .map_err(|e| Trouble::of(format!("l'imagerie de Windows n'a pas répondu : {e}")))?;
        converter
            .Initialize(
                &frame,
                &GUID_WICPixelFormat32bppBGRA,
                WICBitmapDitherTypeNone,
                None,
                0.0,
                WICBitmapPaletteTypeCustom,
            )
            .map_err(|e| Trouble::of(format!("cette image n'a pas pu être convertie : {e}")))?;
        let (mut wide, mut high) = (0u32, 0u32);
        converter
            .GetSize(&mut wide, &mut high)
            .map_err(|e| Trouble::of(format!("cette image ne dit pas sa taille : {e}")))?;
        (wide, high, converter)
    };

    let stride = (wide as usize)
        .checked_mul(4)
        .ok_or_else(|| Trouble::of("cette image est plus large que tout"))?;
    let room = stride
        .checked_mul(high as usize)
        .filter(|_| wide > 0 && high > 0 && (wide as usize) * (high as usize) <= MOST_PIXELS)
        .ok_or_else(|| Trouble::of(format!("une image de {wide} sur {high} ne se colle pas")))?;

    let mut pixels = vec![0u8; room];
    // SAFETY: no part asked for means the whole of it, and the slice was
    // made to the size the two numbers above give.
    unsafe { converter.CopyPixels(std::ptr::null(), stride as u32, &mut pixels) }
        .map_err(|e| Trouble::of(format!("les pixels de cette image sont illisibles : {e}")))?;
    Ok(packed::a_packed_bitmap(wide, high, &pixels))
}

/// Windows' imaging, asked for once per question.
fn imaging() -> Result<IWICImagingFactory, Trouble> {
    // SAFETY: a standard class asked of COM, with no aggregation.
    unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER) }
        .map_err(|e| Trouble::of(format!("l'imagerie de Windows ne s'ouvre pas : {e}")))
}

/// A stream in memory, growing as it is written to.
fn a_stream() -> Result<IStream, Trouble> {
    // SAFETY: naming no memory has it allocate its own, and freeing it
    // with the stream is what the second argument asks for.
    unsafe { CreateStreamOnHGlobal(HGLOBAL(std::ptr::null_mut()), true) }
        .map_err(|e| Trouble::of(format!("la mémoire a manqué : {e}")))
}

/// Puts those bytes in a stream and rewinds it.
fn filled_with(stream: &IStream, bytes: &[u8]) -> Result<(), Trouble> {
    // SAFETY: a stream this thread made, written from a slice it holds.
    unsafe {
        stream
            .Write(bytes.as_ptr().cast::<c_void>(), bytes.len() as u32, None)
            .ok()
            .map_err(|e| Trouble::of(format!("l'image n'a pas pu être posée : {e}")))?;
        stream
            .Seek(0, STREAM_SEEK_SET, None)
            .map_err(|e| Trouble::of(format!("l'image n'a pas pu être rembobinée : {e}")))
    }
}

/// Everything a stream holds, copied out.
///
/// Its own length and not the memory behind it: a stream that grows asks
/// for more room than it has filled, and the difference would be read as
/// a picture running past its end.
fn whole_of(stream: &IStream) -> Result<Vec<u8>, Trouble> {
    let mut about = STATSTG::default();
    // SAFETY: a stream this thread made. Asking for no name is what keeps
    // the answer free of anything to give back.
    unsafe { stream.Stat(&mut about, STATFLAG_NONAME) }
        .map_err(|e| Trouble::of(format!("l'image encodée ne dit pas sa taille : {e}")))?;
    let size = about.cbSize as usize;
    if size == 0 {
        return Err(Trouble::of("l'image encodée est vide"));
    }
    // SAFETY: the memory behind a stream this thread made, unlocked below.
    let block = unsafe { GetHGlobalFromStream(stream) }
        .map_err(|e| Trouble::of(format!("l'image encodée est hors d'atteinte : {e}")))?;
    // SAFETY: the same block.
    let at = unsafe { GlobalLock(block) };
    if at.is_null() {
        return Err(Trouble::of("l'image encodée n'a pas pu être tenue"));
    }
    let mut out = vec![0u8; size];
    // SAFETY: the stream said it holds that many bytes, and the slice was
    // made that size a line ago.
    unsafe { std::ptr::copy_nonoverlapping(at.cast::<u8>(), out.as_mut_ptr(), size) };
    // SAFETY: balances the lock above, refusing when the last lock goes,
    // which is the ordinary case.
    let _ = unsafe { GlobalUnlock(block) };
    Ok(out)
}

/// A picture GDI made of a packed bitmap, given back whatever happens.
struct Bitmap(HBITMAP);

impl Bitmap {
    fn of(dib: &[u8], pixels_at: usize) -> Result<Self, Trouble> {
        // Copied into memory lined up on four bytes, which is what the
        // header in front of a bitmap is read as. What a clipboard hands
        // over is a run of bytes, lined up on nothing in particular.
        let lined_up = four_by_four(dib);
        let head = lined_up.as_ptr().cast::<u8>();

        let screen = Screen::of_the_desktop()?;
        // SAFETY: the header and the pixels both point inside the copy
        // above, which outlives the call; `where_the_pixels_start` put
        // that offset inside it.
        let bitmap = unsafe {
            CreateDIBitmap(
                screen.0,
                Some(head.cast::<BITMAPINFOHEADER>()),
                CBM_INIT as u32,
                Some(head.add(pixels_at).cast::<c_void>()),
                Some(head.cast::<BITMAPINFO>()),
                DIB_RGB_COLORS,
            )
        };
        if bitmap.is_invalid() {
            return Err(Trouble::of(
                "Windows n'a pas su faire une image de ce que portait le presse-papiers",
            ));
        }
        Ok(Self(bitmap))
    }
}

impl Drop for Bitmap {
    fn drop(&mut self) {
        // SAFETY: a picture this thread made and nothing else holds.
        let _ = unsafe { DeleteObject(HGDIOBJ(self.0.0)) };
    }
}

/// Those bytes again, in memory lined up on four.
fn four_by_four(bytes: &[u8]) -> Vec<u32> {
    let mut out = vec![0u32; bytes.len().div_ceil(4)];
    // SAFETY: the destination was made to hold at least that many bytes,
    // and a number of four bytes takes any run of four bytes.
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), out.as_mut_ptr().cast::<u8>(), bytes.len())
    };
    out
}

/// The desktop's own drawing surface, given back whatever happens.
///
/// Asked for and not made, because what is wanted from it is the colours
/// the screen shows: a bitmap of few enough shades carries its own table,
/// and turning that table into colours is a question about a screen.
struct Screen(HDC);

impl Screen {
    fn of_the_desktop() -> Result<Self, Trouble> {
        // SAFETY: naming no window means the whole of the screen.
        let dc = unsafe { GetDC(None) };
        if dc.is_invalid() {
            return Err(Trouble::of("cet écran n'a pas de surface à dessiner"));
        }
        Ok(Self(dc))
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        // SAFETY: balances the one ask above, named the same way.
        unsafe { ReleaseDC(None, self.0) };
    }
}

/// COM, brought up for as long as one picture takes.
///
/// The same guard as the one the sound of this product raises, and for
/// the same reason: everything above is a pointer that only means
/// anything while COM stands. A thread that already had COM up in another
/// apartment keeps the one it had, which is an answer and not a fault,
/// and taking COM down then would take down somebody else's.
struct Com(bool);

impl Com {
    fn up() -> Self {
        // SAFETY: nothing is touched but this thread's own apartment.
        let outcome = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        Self(outcome.is_ok())
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: balances the one call above, and only that one.
            unsafe { CoUninitialize() };
        }
    }
}
