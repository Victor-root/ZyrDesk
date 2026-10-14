//! A picture read from a file, at the size it will be shown, or from the
//! program itself.
//!
//! The wallpaper of a computer, shown on its card, and the scenery the
//! home window is dressed in. Read by the Windows Imaging Component, which ships
//! with Windows and reads every format a wallpaper comes in, so nothing
//! is bundled to read them.
//!
//! Brought down once, when it is read, to the size it will be shown at,
//! and never up: a four thousand pixel photograph drawn into a card two
//! hundred pixels wide at every frame would cost the processor for
//! nothing, and shrinking it that far in one step at drawing time would
//! scatter its pixels. Read away from the thread that draws, since a
//! large photograph takes a moment to decode.

/// What only Windows can do: read the file.
#[cfg(windows)]
mod reading {
    use super::covering;

    use std::path::Path;
    use std::sync::atomic::{AtomicU64, Ordering};

    use windows::Win32::Foundation::GENERIC_READ;
    use windows::Win32::Graphics::Imaging::{
        CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICBitmapDecoder,
        IWICImagingFactory, WICBitmapDitherTypeNone, WICBitmapInterpolationModeHighQualityCubic,
        WICBitmapPaletteTypeCustom, WICDecodeMetadataCacheOnDemand,
    };
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
        CoUninitialize,
    };
    use windows::core::HSTRING;

    /// What tells one picture from another, for the canvas that keeps it
    /// ready to draw.
    static NEXT: AtomicU64 = AtomicU64::new(1);

    /// A picture, ready to be handed to a canvas: its pixels, four bytes
    /// each, in the order and with the transparency already applied that
    /// the canvas draws in.
    pub struct Picture {
        pub(crate) id: u64,
        pub(crate) width: u32,
        pub(crate) height: u32,
        pub(crate) pixels: Vec<u8>,
    }

    impl Picture {
        /// The picture in this file, brought down until it just covers
        /// `room`, in real pixels, keeping its shape.
        ///
        /// Nothing when the file cannot be read as a picture: a wallpaper
        /// can be gone, or be a format the system has no reader for, and
        /// the card then shows something else rather than nothing.
        pub fn read(path: &Path, room: (u32, u32)) -> Option<Picture> {
            with_the_system(|| {
                decoded(
                    |factory| {
                        // SAFETY: an object of the system, and a name that
                        // outlives the call.
                        unsafe {
                            factory.CreateDecoderFromFilename(
                                &HSTRING::from(path.as_os_str()),
                                None,
                                GENERIC_READ,
                                WICDecodeMetadataCacheOnDemand,
                            )
                        }
                        .ok()
                    },
                    room,
                )
            })
        }

        /// The picture written in these bytes, at its own size: one the
        /// program carries, made for what it is drawn on.
        pub fn decode(bytes: &'static [u8]) -> Option<Picture> {
            with_the_system(|| {
                decoded(
                    |factory| {
                        // SAFETY: objects of the system, and bytes that
                        // live as long as the program.
                        unsafe {
                            let stream = factory.CreateStream().ok()?;
                            stream.InitializeFromMemory(bytes).ok()?;
                            factory
                                .CreateDecoderFromStream(
                                    &stream,
                                    std::ptr::null(),
                                    WICDecodeMetadataCacheOnDemand,
                                )
                                .ok()
                        }
                    },
                    (u32::MAX, u32::MAX),
                )
            })
        }

        /// Its width and height, in real pixels.
        pub fn size(&self) -> (u32, u32) {
            (self.width, self.height)
        }
    }

    /// Runs `read` with the system's objects at hand on this thread.
    fn with_the_system(read: impl FnOnce() -> Option<Picture>) -> Option<Picture> {
        // SAFETY: the threading of this thread, said once for what follows
        // and taken back before leaving. Already said another way, it
        // stands as it is, and the reading works all the same.
        let started = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
        let read = read();
        if started {
            // SAFETY: the `CoInitializeEx` above, which succeeded, and
            // after every object it served has been let go.
            unsafe { CoUninitialize() };
        }
        read
    }

    /// The reading itself, from what `open` opens, every object of which
    /// is let go when it returns.
    fn decoded(
        open: impl FnOnce(&IWICImagingFactory) -> Option<IWICBitmapDecoder>,
        room: (u32, u32),
    ) -> Option<Picture> {
        // SAFETY: objects of the system, each held for the length of this
        // call, and pixels copied into a buffer of exactly the size asked.
        unsafe {
            let factory: IWICImagingFactory =
                CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok()?;
            let decoder = open(&factory)?;
            let frame = decoder.GetFrame(0).ok()?;
            let (mut wide, mut high) = (0u32, 0u32);
            frame.GetSize(&mut wide, &mut high).ok()?;
            let (width, height) = covering((wide, high), room)?;

            let scaler = factory.CreateBitmapScaler().ok()?;
            scaler
                .Initialize(
                    &frame,
                    width,
                    height,
                    WICBitmapInterpolationModeHighQualityCubic,
                )
                .ok()?;
            let converter = factory.CreateFormatConverter().ok()?;
            converter
                .Initialize(
                    &scaler,
                    &GUID_WICPixelFormat32bppPBGRA,
                    WICBitmapDitherTypeNone,
                    None,
                    0.0,
                    WICBitmapPaletteTypeCustom,
                )
                .ok()?;
            let stride = width.checked_mul(4)?;
            let mut pixels = vec![0u8; stride as usize * height as usize];
            converter
                .CopyPixels(std::ptr::null(), stride, &mut pixels)
                .ok()?;
            Some(Picture {
                id: NEXT.fetch_add(1, Ordering::Relaxed),
                width,
                height,
                pixels,
            })
        }
    }
}

#[cfg(windows)]
pub use reading::Picture;

/// The size a picture of this size is brought down to so that it just
/// covers `room`, keeping its shape and never growing.
///
/// Covers and does not fit: a card shows its picture to the edges and
/// crops what is beyond, as a wallpaper does on a screen of another
/// shape.
fn covering((wide, high): (u32, u32), (room_wide, room_high): (u32, u32)) -> Option<(u32, u32)> {
    if wide == 0 || high == 0 {
        return None;
    }
    let part = (room_wide.max(1) as f64 / wide as f64)
        .max(room_high.max(1) as f64 / high as f64)
        .min(1.0);
    let brought = |side: u32| ((side as f64 * part).round() as u32).max(1);
    Some((brought(wide), brought(high)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_is_brought_down_to_just_cover_its_room() {
        // A landscape photograph into a squarer card: the height decides,
        // and the sides are cropped when it is drawn.
        assert_eq!(covering((3840, 2160), (400, 300)), Some((533, 300)));
        // A tall one into a wide card: the width decides.
        assert_eq!(covering((1000, 2000), (400, 100)), Some((400, 800)));
    }

    #[test]
    fn a_small_picture_is_never_made_larger() {
        assert_eq!(covering((320, 200), (640, 400)), Some((320, 200)));
    }

    #[test]
    fn a_picture_of_no_size_is_no_picture() {
        assert_eq!(covering((0, 200), (640, 400)), None);
    }
}
