//! FFmpeg objects owned on the Rust side.
//!
//! Each one holds the `Ffmpeg` that made it, so the libraries outlive
//! it, and is freed by the function its header names, exactly once, when
//! its owner goes. Nothing FFmpeg allocated is ever handed out of this
//! crate: what leaves it is copied, or wrapped in one of these.

use std::ffi::{CStr, CString, c_int};
use std::ptr::{self, NonNull};
use std::sync::Arc;

use crate::error::{AGAIN, CodecError, END};
use crate::library::Ffmpeg;
use crate::sys;

/// An `AVCodecContext`: one encoder or decoder.
pub(crate) struct CodecContext {
    ff: Arc<Ffmpeg>,
    raw: NonNull<sys::AVCodecContext>,
    codec: NonNull<sys::AVCodec>,
    /// FFmpeg's name for the codec, for the messages.
    name: &'static str,
}

impl CodecContext {
    /// A context for the encoder FFmpeg knows by that name.
    pub(crate) fn encoder(ff: &Arc<Ffmpeg>, name: &'static str) -> Result<Self, CodecError> {
        let wanted = c_name(name)?;
        // SAFETY: a terminated name; FFmpeg returns a static codec or null.
        let codec = unsafe { ff.avcodec.avcodec_find_encoder_by_name(wanted.as_ptr()) };
        Self::with(ff, name, codec)
    }

    /// A context for the decoder FFmpeg knows by that name.
    pub(crate) fn decoder(ff: &Arc<Ffmpeg>, name: &'static str) -> Result<Self, CodecError> {
        let wanted = c_name(name)?;
        // SAFETY: as above.
        let codec = unsafe { ff.avcodec.avcodec_find_decoder_by_name(wanted.as_ptr()) };
        Self::with(ff, name, codec)
    }

    fn with(
        ff: &Arc<Ffmpeg>,
        name: &'static str,
        codec: *const sys::AVCodec,
    ) -> Result<Self, CodecError> {
        let codec = NonNull::new(codec.cast_mut()).ok_or(CodecError::Missing { codec: name })?;
        // SAFETY: the codec is one FFmpeg returned.
        let raw = unsafe { ff.avcodec.avcodec_alloc_context3(codec.as_ptr()) };
        let raw = NonNull::new(raw).ok_or(CodecError::OutOfMemory {
            what: "un contexte de codec",
        })?;
        Ok(Self {
            ff: Arc::clone(ff),
            raw,
            codec,
            name,
        })
    }

    pub(crate) fn get(&self) -> &sys::AVCodecContext {
        // SAFETY: allocated by avcodec_alloc_context3 and only freed by
        // Drop; FFmpeg only touches it inside calls made through `self`.
        unsafe { self.raw.as_ref() }
    }

    pub(crate) fn fields(&mut self) -> &mut sys::AVCodecContext {
        // SAFETY: as above, and `&mut self` makes the borrow exclusive.
        unsafe { self.raw.as_mut() }
    }

    /// Opens the codec with these private options.
    ///
    /// An option the codec does not know is refused, where FFmpeg itself
    /// would go on without it: a misspelt name would otherwise quietly
    /// give back the latency it was there to remove.
    pub(crate) fn open(&mut self, options: &[(&str, &str)]) -> Result<(), CodecError> {
        let mut dictionary = Options::new(&self.ff);
        for (key, value) in options {
            dictionary.set(key, value)?;
        }
        // SAFETY: context and codec belong together, and FFmpeg leaves
        // in the dictionary only the entries it did not use.
        let opened = unsafe {
            self.ff.avcodec.avcodec_open2(
                self.raw.as_ptr(),
                self.codec.as_ptr(),
                dictionary.as_mut_ptr(),
            )
        };
        self.ff
            .check(opened, &format!("ouverture de {}", self.name))?;
        let unknown = dictionary.left();
        if !unknown.is_empty() {
            return Err(CodecError::UnknownOptions {
                codec: self.name,
                options: unknown,
            });
        }
        Ok(())
    }

    /// Hands a frame to an encoder.
    pub(crate) fn send_frame(&mut self, frame: &OwnedFrame) -> Result<(), CodecError> {
        // SAFETY: an open context and a frame of ours; FFmpeg takes its
        // own reference to the frame's buffers.
        let sent = unsafe {
            self.ff
                .avcodec
                .avcodec_send_frame(self.raw.as_ptr(), frame.as_ptr())
        };
        self.ff
            .check(sent, &format!("envoi d'une image à {}", self.name))?;
        Ok(())
    }

    /// Tells an encoder no frame will follow, so it hands over what it
    /// still holds.
    pub(crate) fn send_end(&mut self) -> Result<(), CodecError> {
        // SAFETY: an open context; a null frame is how FFmpeg is told.
        let sent = unsafe {
            self.ff
                .avcodec
                .avcodec_send_frame(self.raw.as_ptr(), ptr::null())
        };
        self.ff
            .check(sent, &format!("fin du flux de {}", self.name))?;
        Ok(())
    }

    /// The next packet out of an encoder, into `packet`; false when there
    /// is none yet.
    pub(crate) fn receive_packet(&mut self, packet: &mut OwnedPacket) -> Result<bool, CodecError> {
        // SAFETY: an open context and a packet of ours, which FFmpeg
        // empties before filling.
        let received = unsafe {
            self.ff
                .avcodec
                .avcodec_receive_packet(self.raw.as_ptr(), packet.as_ptr())
        };
        self.received(received, "d'un paquet")
    }

    /// Hands bytes to a decoder.
    ///
    /// They are lent, not given: the packet points at them without
    /// owning them, which makes FFmpeg copy them, with the padding its
    /// readers need past the end, before it looks at them.
    pub(crate) fn send_bytes(
        &mut self,
        packet: &mut LentPacket,
        data: &[u8],
    ) -> Result<(), CodecError> {
        let size = c_int::try_from(data.len()).map_err(|_| {
            CodecError::Invalid(format!("paquet de {} octets, trop grand", data.len()))
        })?;
        let lent = packet.0.fields();
        lent.data = data.as_ptr().cast_mut();
        lent.size = size;
        // SAFETY: a lent packet never has a buffer of its own, so FFmpeg
        // copies the bytes and never writes to them; they outlive the
        // call.
        let sent = unsafe {
            self.ff
                .avcodec
                .avcodec_send_packet(self.raw.as_ptr(), packet.0.as_ptr())
        };
        let lent = packet.0.fields();
        lent.data = ptr::null_mut();
        lent.size = 0;
        self.ff
            .check(sent, &format!("envoi d'un paquet à {}", self.name))?;
        Ok(())
    }

    /// The next frame out of a decoder, into `frame`; false when there is
    /// none yet.
    pub(crate) fn receive_frame(&mut self, frame: &mut OwnedFrame) -> Result<bool, CodecError> {
        // SAFETY: an open context and a frame of ours, which FFmpeg
        // empties before filling.
        let received = unsafe {
            self.ff
                .avcodec
                .avcodec_receive_frame(self.raw.as_ptr(), frame.as_ptr())
        };
        self.received(received, "d'une image")
    }

    fn received(&self, code: c_int, what: &str) -> Result<bool, CodecError> {
        if code == AGAIN || code == END {
            return Ok(false);
        }
        self.ff
            .check(code, &format!("lecture {what} de {}", self.name))?;
        Ok(true)
    }
}

impl Drop for CodecContext {
    fn drop(&mut self) {
        let mut raw = self.raw.as_ptr();
        // SAFETY: allocated by avcodec_alloc_context3, freed once here.
        unsafe { self.ff.avcodec.avcodec_free_context(&mut raw) };
    }
}

/// An `AVFrame`: one picture or one slice of sound.
pub(crate) struct OwnedFrame {
    ff: Arc<Ffmpeg>,
    raw: NonNull<sys::AVFrame>,
}

impl OwnedFrame {
    pub(crate) fn new(ff: &Arc<Ffmpeg>) -> Result<Self, CodecError> {
        // SAFETY: allocates an empty frame or returns null.
        let raw = unsafe { ff.avutil.av_frame_alloc() };
        let raw = NonNull::new(raw).ok_or(CodecError::OutOfMemory { what: "une image" })?;
        Ok(Self {
            ff: Arc::clone(ff),
            raw,
        })
    }

    pub(crate) fn as_ptr(&self) -> *mut sys::AVFrame {
        self.raw.as_ptr()
    }

    pub(crate) fn get(&self) -> &sys::AVFrame {
        // SAFETY: allocated by av_frame_alloc and only freed by Drop.
        unsafe { self.raw.as_ref() }
    }

    pub(crate) fn fields(&mut self) -> &mut sys::AVFrame {
        // SAFETY: as above, and `&mut self` makes the borrow exclusive.
        unsafe { self.raw.as_mut() }
    }

    /// Gives the frame buffers for the format and size already set.
    pub(crate) fn allocate(&mut self) -> Result<(), CodecError> {
        // SAFETY: a frame of ours with its format and size set; 0 lets
        // FFmpeg pick the alignment its code wants.
        let allocated = unsafe { self.ff.avutil.av_frame_get_buffer(self.raw.as_ptr(), 0) };
        self.ff.check(allocated, "réservation d'une image")?;
        Ok(())
    }
}

impl Drop for OwnedFrame {
    fn drop(&mut self) {
        let mut raw = self.raw.as_ptr();
        // SAFETY: allocated by av_frame_alloc, freed once here, with the
        // references it holds.
        unsafe { self.ff.avutil.av_frame_free(&mut raw) };
    }
}

/// An `AVPacket`.
pub(crate) struct OwnedPacket {
    ff: Arc<Ffmpeg>,
    raw: NonNull<sys::AVPacket>,
}

impl OwnedPacket {
    pub(crate) fn new(ff: &Arc<Ffmpeg>) -> Result<Self, CodecError> {
        // SAFETY: allocates an empty packet or returns null.
        let raw = unsafe { ff.avcodec.av_packet_alloc() };
        let raw = NonNull::new(raw).ok_or(CodecError::OutOfMemory { what: "un paquet" })?;
        Ok(Self {
            ff: Arc::clone(ff),
            raw,
        })
    }

    pub(crate) fn as_ptr(&self) -> *mut sys::AVPacket {
        self.raw.as_ptr()
    }

    pub(crate) fn get(&self) -> &sys::AVPacket {
        // SAFETY: allocated by av_packet_alloc and only freed by Drop.
        unsafe { self.raw.as_ref() }
    }

    pub(crate) fn fields(&mut self) -> &mut sys::AVPacket {
        // SAFETY: as above, and `&mut self` makes the borrow exclusive.
        unsafe { self.raw.as_mut() }
    }

    /// A copy of the bytes the packet holds.
    pub(crate) fn to_vec(&self) -> Vec<u8> {
        let packet = self.get();
        let size = usize::try_from(packet.size).unwrap_or(0);
        if packet.data.is_null() || size == 0 {
            return Vec::new();
        }
        // SAFETY: FFmpeg says `size` bytes live at `data` until the
        // packet is next emptied, which needs `&mut self`.
        unsafe { std::slice::from_raw_parts(packet.data, size) }.to_vec()
    }
}

impl Drop for OwnedPacket {
    fn drop(&mut self) {
        let mut raw = self.raw.as_ptr();
        // SAFETY: allocated by av_packet_alloc, freed once here, with its
        // buffer. A lent packet is emptied right after the call that
        // borrowed its bytes, so it never frees what is not FFmpeg's.
        unsafe { self.ff.avcodec.av_packet_free(&mut raw) };
    }
}

/// A packet that only ever points at bytes it does not own, to hand
/// them to a decoder: never received into, so it never holds a buffer.
pub(crate) struct LentPacket(OwnedPacket);

impl LentPacket {
    pub(crate) fn new(ff: &Arc<Ffmpeg>) -> Result<Self, CodecError> {
        OwnedPacket::new(ff).map(Self)
    }
}

/// An `AVDictionary` of options for opening a codec.
struct Options {
    ff: Arc<Ffmpeg>,
    raw: *mut sys::AVDictionary,
}

impl Options {
    fn new(ff: &Arc<Ffmpeg>) -> Self {
        Self {
            ff: Arc::clone(ff),
            raw: ptr::null_mut(),
        }
    }

    fn set(&mut self, key: &str, value: &str) -> Result<(), CodecError> {
        let key = c_name(key)?;
        let value = c_name(value)?;
        // SAFETY: terminated strings, which FFmpeg copies; it allocates
        // the dictionary on the first entry.
        let set = unsafe {
            self.ff
                .avutil
                .av_dict_set(&mut self.raw, key.as_ptr(), value.as_ptr(), 0)
        };
        self.ff.check(set, "préparation des réglages")?;
        Ok(())
    }

    fn as_mut_ptr(&mut self) -> *mut *mut sys::AVDictionary {
        &mut self.raw
    }

    /// Every entry still there, as `key=value`.
    fn left(&self) -> Vec<String> {
        let mut entries = Vec::new();
        let mut entry: *const sys::AVDictionaryEntry = ptr::null();
        loop {
            // SAFETY: walks the dictionary from its start; an entry stays
            // valid as long as the dictionary is not changed, and holds
            // two terminated strings.
            let (key, value) = unsafe {
                entry = self.ff.avutil.av_dict_iterate(self.raw, entry);
                let Some(found) = entry.as_ref() else {
                    return entries;
                };
                (CStr::from_ptr(found.key), CStr::from_ptr(found.value))
            };
            entries.push(format!(
                "{}={}",
                key.to_string_lossy(),
                value.to_string_lossy()
            ));
        }
    }
}

impl Drop for Options {
    fn drop(&mut self) {
        // SAFETY: null or allocated by av_dict_set, freed once here.
        unsafe { self.ff.avutil.av_dict_free(&mut self.raw) };
    }
}

/// A name or value as C wants it.
fn c_name(text: &str) -> Result<CString, CodecError> {
    CString::new(text).map_err(|_| CodecError::Invalid(format!("« {text} » contient un zéro")))
}
