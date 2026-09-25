//! Direct3D 11: FFmpeg working on the engine's own device.
//!
//! Neither the encoder nor the decoder gets a device of its own. FFmpeg
//! is handed the one the engine already works with, so a picture goes
//! from capture to encoder, and from decoder to screen, without leaving
//! the graphics card or crossing from one device to another.

use std::ffi::{c_int, c_void};
use std::ptr::{self, NonNull};
use std::sync::Arc;

use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_DECODER, D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_BOX,
    D3D11_DECODER_PROFILE_AV1_VLD_PROFILE0, D3D11_DECODER_PROFILE_H264_VLD_NOFGT,
    D3D11_DECODER_PROFILE_HEVC_VLD_MAIN, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, ID3D11Device,
    ID3D11DeviceContext, ID3D11Multithread, ID3D11Texture2D, ID3D11VideoDevice,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_NV12, DXGI_SAMPLE_DESC};
use windows::core::Interface;
use zyr_media::codec::{CodecSet, VideoCodec};

use crate::error::CodecError;
use crate::library::Ffmpeg;
use crate::owned::{CodecContext, OwnedFrame};
use crate::sys;

/// A texture from an encoder's pool, for the caller to render the next
/// picture into before handing it back to [`crate::VideoEncoder::encode`].
pub struct GpuFrame {
    frame: OwnedFrame,
    texture: ID3D11Texture2D,
}

impl GpuFrame {
    /// NV12, the encoder's size, bound as a render target: a texture of
    /// its own, not an array (its index is 0).
    pub fn texture(&self) -> &ID3D11Texture2D {
        &self.texture
    }
}

/// The textures an encoder is fed from.
pub(crate) struct Surfaces {
    /// Our own frames context: NV12 render targets, one texture each,
    /// made as the pool runs short and reused after.
    frames: BufferRef,
    /// For Quick Sync, the same textures seen as its surfaces.
    qsv: Option<BufferRef>,
}

impl Surfaces {
    pub(crate) fn new(
        ff: &Arc<Ffmpeg>,
        device: &ID3D11Device,
        width: u32,
        height: u32,
        for_qsv: bool,
    ) -> Result<Self, CodecError> {
        let device_context = device_context(ff, device, 0)?;
        let qsv_device = if for_qsv {
            // Intel's runtime works on the device from threads of its own.
            let multithread: ID3D11Multithread = device
                .cast()
                .map_err(|e| graphics("la protection du périphérique Direct3D 11", &e))?;
            // SAFETY: a plain setter on a live interface. It returns the
            // state before, which does not matter here.
            let _ = unsafe { multithread.SetMultithreadProtected(true) };
            let mut derived = ptr::null_mut();
            // SAFETY: an initialised D3D11VA device context; FFmpeg puts
            // a new reference in `derived` or fails.
            let made = unsafe {
                ff.avutil.av_hwdevice_ctx_create_derived(
                    &mut derived,
                    sys::AVHWDeviceType::AV_HWDEVICE_TYPE_QSV,
                    device_context.as_ptr(),
                    0,
                )
            };
            ff.check(made, "préparation de Quick Sync sur le périphérique")?;
            // SAFETY: the reference FFmpeg just made, ours alone.
            Some(unsafe { BufferRef::from_raw(ff, derived, "un contexte Quick Sync") }?)
        } else {
            None
        };

        // SAFETY: an initialised device context; returns a new frames
        // context or null.
        let raw = unsafe { ff.avutil.av_hwframe_ctx_alloc(device_context.as_ptr()) };
        // SAFETY: the reference FFmpeg just made, ours alone.
        let frames = unsafe { BufferRef::from_raw(ff, raw, "un ensemble de textures") }?;
        // SAFETY: a frames context of a D3D11VA device, not yet
        // initialised: `data` is its AVHWFramesContext and `hwctx` its
        // AVD3D11VAFramesContext.
        unsafe {
            let context = frames.data().cast::<sys::AVHWFramesContext>();
            (*context).format = sys::AVPixelFormat::AV_PIX_FMT_D3D11;
            (*context).sw_format = sys::AVPixelFormat::AV_PIX_FMT_NV12;
            (*context).width = width as c_int;
            (*context).height = height as c_int;
            // No array made up front: each texture is made on its own
            // when needed, which is what lets the caller render into it.
            (*context).initial_pool_size = 0;
            let d3d11 = (*context).hwctx.cast::<sys::AVD3D11VAFramesContext>();
            (*d3d11).BindFlags = D3D11_BIND_RENDER_TARGET.0 as u32;
        }
        // SAFETY: the frames context configured above.
        let initialised = unsafe { ff.avutil.av_hwframe_ctx_init(frames.as_ptr()) };
        ff.check(initialised, "préparation des textures de l'encodeur")?;

        let qsv = match qsv_device {
            Some(qsv_device) => {
                let mut derived = ptr::null_mut();
                // SAFETY: an initialised D3D11 frames context and the
                // Quick Sync device derived from its device.
                let made = unsafe {
                    ff.avutil.av_hwframe_ctx_create_derived(
                        &mut derived,
                        sys::AVPixelFormat::AV_PIX_FMT_QSV,
                        qsv_device.as_ptr(),
                        frames.as_ptr(),
                        0,
                    )
                };
                ff.check(made, "préparation des textures de Quick Sync")?;
                // SAFETY: the reference FFmpeg just made, ours alone.
                Some(unsafe { BufferRef::from_raw(ff, derived, "des textures Quick Sync") }?)
            }
            None => None,
        };
        Ok(Self { frames, qsv })
    }

    /// The pixel format the encoder is opened with.
    pub(crate) fn pixel_format(&self) -> sys::AVPixelFormat {
        match self.qsv {
            Some(_) => sys::AVPixelFormat::AV_PIX_FMT_QSV,
            None => sys::AVPixelFormat::AV_PIX_FMT_D3D11,
        }
    }

    /// A reference to the frames context the encoder reads, for its
    /// `hw_frames_ctx`.
    pub(crate) fn frames_for_encoder(&self) -> Result<*mut sys::AVBufferRef, CodecError> {
        self.qsv.as_ref().unwrap_or(&self.frames).new_ref()
    }

    /// A texture from the pool.
    pub(crate) fn frame(&self, ff: &Arc<Ffmpeg>) -> Result<GpuFrame, CodecError> {
        let frame = OwnedFrame::new(ff)?;
        // SAFETY: an initialised frames context and an empty frame of
        // ours, which FFmpeg fills with a texture of the pool.
        let got = unsafe {
            ff.avutil
                .av_hwframe_get_buffer(self.frames.as_ptr(), frame.as_ptr(), 0)
        };
        ff.check(got, "réservation d'une texture de l'encodeur")?;
        let raw = frame.get().data[0].cast::<c_void>();
        // SAFETY: a D3D11 frame holds its texture in `data[0]`; the new
        // reference taken here keeps it whatever becomes of the frame.
        let texture = unsafe { ID3D11Texture2D::from_raw_borrowed(&raw) }
            .cloned()
            .ok_or(CodecError::OutOfMemory {
                what: "une texture de l'encodeur",
            })?;
        Ok(GpuFrame { frame, texture })
    }

    /// The frame the encoder takes: the texture itself, or for Quick Sync
    /// the surface it maps to.
    ///
    /// Only a texture of this pool: the encoder reads one at its own
    /// size, whatever the size of the texture it is given.
    pub(crate) fn for_encoder(
        &self,
        ff: &Arc<Ffmpeg>,
        frame: GpuFrame,
    ) -> Result<OwnedFrame, CodecError> {
        // SAFETY: a frame from av_hwframe_get_buffer holds a reference to
        // the frames context it came from, alive as long as the frame.
        let pool = unsafe { frame.frame.get().hw_frames_ctx.as_ref() };
        if !pool.is_some_and(|pool| pool.data == self.frames.data()) {
            return Err(CodecError::Invalid(
                "cette texture vient d'un autre encodeur".to_string(),
            ));
        }
        let Some(qsv) = &self.qsv else {
            return Ok(frame.frame);
        };
        let mut mapped = OwnedFrame::new(ff)?;
        let fields = mapped.fields();
        fields.format = sys::AVPixelFormat::AV_PIX_FMT_QSV.0;
        fields.hw_frames_ctx = qsv.new_ref()?;
        // SAFETY: a D3D11 frame of the frames context the Quick Sync one
        // was derived from, and an empty Quick Sync frame; the mapping
        // keeps its own reference to the texture. No access flag: both
        // of hwcontext_qsv's ways of mapping a D3D11 texture in take
        // none, and the encoder only reads it.
        let done = unsafe {
            ff.avutil
                .av_hwframe_map(mapped.as_ptr(), frame.frame.as_ptr(), 0)
        };
        ff.check(done, "passage d'une texture à Quick Sync")?;
        Ok(mapped)
    }
}

/// How the player gets at a decoded picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sampling {
    /// It samples the decoder's own texture.
    Direct,
    /// The driver would not let decoder textures be sampled: each picture
    /// is copied into a texture of the decoder's first.
    Copied,
}

/// A picture decoded by the graphics card.
pub struct D3d11Picture {
    texture: ID3D11Texture2D,
    index: u32,
    width: u32,
    height: u32,
    /// The decoder's surface, kept from being decoded into again for as
    /// long as the picture is out. Never read, and named so.
    _surface: Option<OwnedFrame>,
}

impl D3d11Picture {
    /// NV12, sampleable: a texture array, of which the picture is
    /// [`D3d11Picture::index`].
    ///
    /// A copied picture lives in a texture the decoder reuses: it is to
    /// be drawn before the next packet is decoded, which the player's
    /// single video thread does anyway.
    pub fn texture(&self) -> &ID3D11Texture2D {
        &self.texture
    }

    pub fn index(&self) -> u32 {
        self.index
    }

    /// The picture's size, which the texture may exceed (decoders align
    /// their surfaces).
    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }
}

/// Pictures a player may still hold while the next ones decode: the one
/// on screen, the one being drawn, and two waiting for their refresh of
/// the screen. The decoder's pool is made that much larger than decoding
/// alone needs.
const HELD_BY_PLAYER: c_int = 4;

/// The decoding side, attached to a decoder's context.
pub(crate) struct Pictures {
    sampling: Sampling,
    device: ID3D11Device,
    immediate: ID3D11DeviceContext,
    /// The texture pictures are copied into, and its size, when they are.
    copy: Option<(ID3D11Texture2D, u32, u32)>,
}

impl Pictures {
    /// Makes the decoder of `context` decode on `device`.
    pub(crate) fn attach(
        ff: &Arc<Ffmpeg>,
        context: &mut CodecContext,
        device: &ID3D11Device,
    ) -> Result<Self, CodecError> {
        let sampling = if sampleable(device) {
            Sampling::Direct
        } else {
            Sampling::Copied
        };
        let bind = match sampling {
            Sampling::Direct => D3D11_BIND_SHADER_RESOURCE.0 as u32,
            Sampling::Copied => 0,
        };
        let device_context = device_context(ff, device, bind)?;
        let fields = context.fields();
        fields.hw_device_ctx = device_context.new_ref()?;
        fields.get_format = Some(choose_d3d11);
        fields.extra_hw_frames = HELD_BY_PLAYER;
        // SAFETY: a getter on a live device.
        let immediate = unsafe { device.GetImmediateContext() }
            .map_err(|e| graphics("le contexte du périphérique Direct3D 11", &e))?;
        Ok(Self {
            sampling,
            device: device.clone(),
            immediate,
            copy: None,
        })
    }

    pub(crate) fn sampling(&self) -> Sampling {
        self.sampling
    }

    /// The picture a decoded frame holds.
    pub(crate) fn picture(&mut self, frame: OwnedFrame) -> Result<D3d11Picture, CodecError> {
        let fields = frame.get();
        if sys::AVPixelFormat(fields.format) != sys::AVPixelFormat::AV_PIX_FMT_D3D11 {
            return Err(CodecError::Invalid(
                "le décodeur a rendu une image hors de la carte graphique".to_string(),
            ));
        }
        let (width, height) = (fields.width.unsigned_abs(), fields.height.unsigned_abs());
        // A D3D11 frame holds the index of its slice in `data[1]`.
        let index = u32::try_from(fields.data[1].addr()).map_err(|_| {
            CodecError::Invalid("le décodeur a rendu une texture d'indice impossible".to_string())
        })?;
        let raw = fields.data[0].cast::<c_void>();
        // SAFETY: a D3D11 frame holds its texture in `data[0]`, alive as
        // long as the frame is.
        let source = unsafe { ID3D11Texture2D::from_raw_borrowed(&raw) }.ok_or_else(|| {
            CodecError::Invalid("le décodeur a rendu une image sans texture".to_string())
        })?;
        match self.sampling {
            Sampling::Direct => Ok(D3d11Picture {
                texture: source.clone(),
                index,
                width,
                height,
                _surface: Some(frame),
            }),
            Sampling::Copied => {
                let target = self.copy_target(width, height)?;
                let area = D3D11_BOX {
                    left: 0,
                    top: 0,
                    front: 0,
                    right: width,
                    bottom: height,
                    back: 1,
                };
                // SAFETY: both textures are NV12 on this device, the area
                // lies inside each, and the slice exists; the decoder
                // runs on this same thread, never beside this call.
                unsafe {
                    self.immediate.CopySubresourceRegion(
                        &target,
                        0,
                        0,
                        0,
                        0,
                        source,
                        index,
                        Some(&area),
                    )
                };
                Ok(D3d11Picture {
                    texture: target,
                    index: 0,
                    width,
                    height,
                    _surface: None,
                })
            }
        }
    }

    /// The texture to copy a picture of that size into.
    fn copy_target(&mut self, width: u32, height: u32) -> Result<ID3D11Texture2D, CodecError> {
        if let Some((texture, made_width, made_height)) = &self.copy
            && (*made_width, *made_height) == (width, height)
        {
            return Ok(texture.clone());
        }
        let description = nv12(width, height, 1, D3D11_BIND_SHADER_RESOURCE.0);
        let mut texture = None;
        // SAFETY: a valid description; the texture comes back in `texture`.
        unsafe {
            self.device
                .CreateTexture2D(&description, None, Some(&mut texture))
        }
        .map_err(|e| graphics("la texture où copier les images décodées", &e))?;
        let texture = texture.ok_or(CodecError::OutOfMemory {
            what: "la texture où copier les images décodées",
        })?;
        self.copy = Some((texture.clone(), width, height));
        Ok(texture)
    }
}

/// The codecs `device` decodes in hardware, among those FFmpeg can
/// decode with it.
pub(crate) fn supported(
    device: &ID3D11Device,
    has_decoder: impl Fn(VideoCodec) -> bool,
) -> CodecSet {
    let Ok(video) = device.cast::<ID3D11VideoDevice>() else {
        return CodecSet::empty();
    };
    VideoCodec::ALL
        .into_iter()
        .filter(|codec| has_decoder(*codec))
        .filter(|codec| {
            let profile = match codec {
                VideoCodec::H264 => D3D11_DECODER_PROFILE_H264_VLD_NOFGT,
                VideoCodec::Hevc => D3D11_DECODER_PROFILE_HEVC_VLD_MAIN,
                VideoCodec::Av1 => D3D11_DECODER_PROFILE_AV1_VLD_PROFILE0,
            };
            // SAFETY: a question about a profile and a format, both plain
            // values.
            unsafe { video.CheckVideoDecoderFormat(&profile, DXGI_FORMAT_NV12) }
                .is_ok_and(|yes| yes.as_bool())
        })
        .collect()
}

/// Whether the driver lets decoder textures be sampled too, tried on a
/// small texture array like the one FFmpeg decodes into.
fn sampleable(device: &ID3D11Device) -> bool {
    let description = nv12(
        64,
        64,
        2,
        D3D11_BIND_DECODER.0 | D3D11_BIND_SHADER_RESOURCE.0,
    );
    let mut texture = None;
    // SAFETY: a valid description; the texture, if made, is released
    // when `texture` goes.
    let made = unsafe { device.CreateTexture2D(&description, None, Some(&mut texture)) };
    made.is_ok() && texture.is_some()
}

fn nv12(width: u32, height: u32, array: u32, bind: i32) -> D3D11_TEXTURE2D_DESC {
    D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: array,
        Format: DXGI_FORMAT_NV12,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: bind as u32,
        CPUAccessFlags: 0,
        MiscFlags: 0,
    }
}

/// Picks Direct3D 11 among the formats the decoder offers, and nothing
/// else: FFmpeg's default would quietly fall back to the processor.
unsafe extern "C" fn choose_d3d11(
    _context: *mut sys::AVCodecContext,
    offered: *const sys::AVPixelFormat,
) -> sys::AVPixelFormat {
    let mut next = offered;
    // SAFETY: FFmpeg hands a list ended by AV_PIX_FMT_NONE.
    unsafe {
        while *next != sys::AVPixelFormat::AV_PIX_FMT_NONE {
            if *next == sys::AVPixelFormat::AV_PIX_FMT_D3D11 {
                return *next;
            }
            next = next.add(1);
        }
    }
    sys::AVPixelFormat::AV_PIX_FMT_NONE
}

/// `device`, wrapped into an FFmpeg device context, with `bind` added to
/// the textures FFmpeg makes on it.
fn device_context(
    ff: &Arc<Ffmpeg>,
    device: &ID3D11Device,
    bind: u32,
) -> Result<BufferRef, CodecError> {
    // SAFETY: allocates a D3D11VA device context or returns null.
    let raw = unsafe {
        ff.avutil
            .av_hwdevice_ctx_alloc(sys::AVHWDeviceType::AV_HWDEVICE_TYPE_D3D11VA)
    };
    // SAFETY: the reference FFmpeg just made, ours alone.
    let context = unsafe { BufferRef::from_raw(ff, raw, "un contexte Direct3D 11") }?;
    // SAFETY: a D3D11VA device context, not yet initialised: `data` is
    // its AVHWDeviceContext and `hwctx` its AVD3D11VADeviceContext. FFmpeg
    // releases the device with the context, so it is given a reference
    // of its own.
    unsafe {
        let hardware = context.data().cast::<sys::AVHWDeviceContext>();
        let d3d11 = (*hardware).hwctx.cast::<sys::AVD3D11VADeviceContext>();
        (*d3d11).device = device.clone().into_raw();
        (*d3d11).BindFlags = bind;
    }
    // SAFETY: the context configured above.
    let initialised = unsafe { ff.avutil.av_hwdevice_ctx_init(context.as_ptr()) };
    ff.check(initialised, "préparation de Direct3D 11 pour FFmpeg")?;
    Ok(context)
}

fn graphics(what: &'static str, error: &windows::core::Error) -> CodecError {
    CodecError::Graphics {
        what,
        reason: error.message(),
    }
}

/// An `AVBufferRef`: one reference to what FFmpeg counts references to,
/// here its hardware device and frames contexts.
pub(crate) struct BufferRef {
    ff: Arc<Ffmpeg>,
    raw: NonNull<sys::AVBufferRef>,
}

impl BufferRef {
    /// Takes over a reference FFmpeg handed out, or says what could not
    /// be had when it handed out none.
    ///
    /// # Safety
    ///
    /// `raw` must be null or a reference nobody else will unref.
    unsafe fn from_raw(
        ff: &Arc<Ffmpeg>,
        raw: *mut sys::AVBufferRef,
        what: &'static str,
    ) -> Result<Self, CodecError> {
        let raw = NonNull::new(raw).ok_or(CodecError::OutOfMemory { what })?;
        Ok(Self {
            ff: Arc::clone(ff),
            raw,
        })
    }

    fn as_ptr(&self) -> *mut sys::AVBufferRef {
        self.raw.as_ptr()
    }

    /// What the buffer holds.
    fn data(&self) -> *mut u8 {
        // SAFETY: a live reference, whose `data` stays set as long as it
        // lives.
        unsafe { self.raw.as_ref().data }
    }

    /// A new reference, for a field FFmpeg unrefs itself.
    fn new_ref(&self) -> Result<*mut sys::AVBufferRef, CodecError> {
        // SAFETY: a live reference; returns a new one or null.
        let raw = unsafe { self.ff.avutil.av_buffer_ref(self.raw.as_ptr()) };
        if raw.is_null() {
            return Err(CodecError::OutOfMemory {
                what: "une référence de contexte matériel",
            });
        }
        Ok(raw)
    }
}

impl Drop for BufferRef {
    fn drop(&mut self) {
        let mut raw = self.raw.as_ptr();
        // SAFETY: a reference of ours, let go of once here.
        unsafe { self.ff.avutil.av_buffer_unref(&mut raw) };
    }
}
