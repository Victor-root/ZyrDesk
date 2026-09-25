//! Drawing decoded pictures into the window, on the graphics card.
//!
//! A flip-model swap chain on the window, as Windows 10 composes best:
//! pictures are presented with no wait for the screen's refresh, the
//! newest one wins at composition, and nothing tears. Five buffers
//! rather than three, whose starvation Moonlight measured on AMD cards;
//! with no wait they add no delay. The maximum frame latency is left
//! alone: at 1, Moonlight found presenting waits for the compositor as
//! if it synchronised with the screen.
//!
//! The decoder's NV12 pictures become RGB in a pixel shader (BT.709,
//! limited range), drawn as large as fits with the picture's shape and
//! black around. On Intel cards the shader reads the decoder's own
//! textures, which a copy would cost dearly there; elsewhere each
//! picture is first copied into a texture of the player's, the way
//! Moonlight found fastest on NVIDIA and AMD cards.
//!
//! Decoding and drawing happen on the same thread, one after the
//! other, so the device's immediate context is never used by two
//! threads at once: FFmpeg's own lock on it is all it needs.

use std::ffi::c_void;
use std::sync::Arc;

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Direct3D::Fxc::{
    D3DCOMPILE_ENABLE_STRICTNESS, D3DCOMPILE_OPTIMIZATION_LEVEL3, D3DCompile,
};
use windows::Win32::Graphics::Direct3D::{
    D3D_FEATURE_LEVEL_11_1, D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
    D3D11_SRV_DIMENSION_TEXTURE2DARRAY, ID3DBlob,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_CONSTANT_BUFFER, D3D11_BIND_DECODER, D3D11_BIND_SHADER_RESOURCE, D3D11_BOX,
    D3D11_BUFFER_DESC, D3D11_COMPARISON_NEVER, D3D11_FILTER_MIN_MAG_MIP_LINEAR, D3D11_FLOAT32_MAX,
    D3D11_SAMPLER_DESC, D3D11_SHADER_RESOURCE_VIEW_DESC, D3D11_SHADER_RESOURCE_VIEW_DESC_0,
    D3D11_TEX2D_ARRAY_SRV, D3D11_TEXTURE_ADDRESS_CLAMP, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT,
    D3D11_VIEWPORT, ID3D11Buffer, ID3D11DepthStencilView, ID3D11PixelShader,
    ID3D11RenderTargetView, ID3D11SamplerState, ID3D11ShaderResourceView, ID3D11Texture2D,
    ID3D11VertexShader,
};
use windows::Win32::Graphics::Dwm::DwmEnableMMCSS;
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_UNSPECIFIED, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_NV12,
    DXGI_FORMAT_R8_UNORM, DXGI_FORMAT_R8G8_UNORM, DXGI_FORMAT_UNKNOWN, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    DXGI_ERROR_DEVICE_HUNG, DXGI_ERROR_DEVICE_REMOVED, DXGI_ERROR_DEVICE_RESET,
    DXGI_ERROR_DRIVER_INTERNAL_ERROR, DXGI_FEATURE_PRESENT_ALLOW_TEARING,
    DXGI_MWA_NO_WINDOW_CHANGES, DXGI_PRESENT, DXGI_SCALING_STRETCH, DXGI_SWAP_CHAIN_DESC1,
    DXGI_SWAP_CHAIN_FLAG, DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING, DXGI_SWAP_EFFECT_FLIP_DISCARD,
    DXGI_USAGE_RENDER_TARGET_OUTPUT, IDXGIFactory5, IDXGISwapChain1,
};
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;
use windows::core::{BOOL, Error, HRESULT, Interface, PCSTR, s, w};
use zyr_codec::{D3d11Picture, DecodeOutput, DecodedFrame, Ffmpeg, VideoDecoder};
use zyr_media::codec::CodecSet;
use zyr_proto::log::Log;

use super::device::{self, Device, INTEL};
use super::{Multimedia, code_and_words, failure};
use crate::present::{Fault, Presenter, Rect, Shown, letterbox};

/// The shaders, compiled when the player starts: d3dcompiler_47.dll
/// comes with every Windows 10, and nothing is built ahead of time.
const SHADERS: &str = r#"
cbuffer Reach : register(b0)
{
    // Where the picture ends in its texture, which decoders make larger
    // than the picture: its right and bottom edges for luma, and the
    // centre of the last chroma sample inside it, so that the padding
    // never bleeds into the colours.
    float2 luma_reach;
    float2 chroma_reach;
};

Texture2DArray<float> luma : register(t0);
Texture2DArray<float2> chroma : register(t1);
SamplerState smooth : register(s0);

struct Between
{
    float4 position : SV_Position;
    float2 at : TEXCOORD0;
};

// One triangle covering the viewport, which the picture fills.
Between cover(uint corner : SV_VertexID)
{
    Between between;
    float2 at = float2((corner << 1) & 2, corner & 2);
    between.position = float4(at * float2(2.0, -2.0) + float2(-1.0, 1.0), 0.0, 1.0);
    between.at = at;
    return between;
}

// BT.709, limited range, to RGB.
float4 paint(Between between) : SV_Target
{
    float2 at = between.at * luma_reach;
    float y = luma.Sample(smooth, float3(at, 0.0));
    float2 uv = chroma.Sample(smooth, float3(min(at, chroma_reach), 0.0));
    float3 yuv = float3(y, uv) - float3(16.0 / 255.0, 128.0 / 255.0, 128.0 / 255.0);
    float3 rgb = float3(
        1.1644 * yuv.x + 1.7927 * yuv.z,
        1.1644 * yuv.x - 0.2132 * yuv.y - 0.5329 * yuv.z,
        1.1644 * yuv.x + 2.1124 * yuv.y);
    return float4(saturate(rgb), 1.0);
}
"#;

/// The views of the two planes of an NV12 picture: luma, then chroma.
type Planes = [Option<ID3D11ShaderResourceView>; 2];

/// Buffers of the swap chain: the three the default frame latency
/// queues, the one drawn into, and one the compositor may hold on to.
const BUFFERS: u32 = 5;

/// What the graphics card reports when it went away.
const GONE: [HRESULT; 4] = [
    DXGI_ERROR_DEVICE_REMOVED,
    DXGI_ERROR_DEVICE_RESET,
    DXGI_ERROR_DEVICE_HUNG,
    DXGI_ERROR_DRIVER_INTERNAL_ERROR,
];

/// The window, drawn into by the graphics card.
pub struct Screen {
    hwnd: HWND,
    /// Everything made on the card; nothing while it is being made
    /// again.
    gpu: Option<Gpu>,
    /// Whether the compositor was asked to schedule this process's
    /// presentation as multimedia work, to be undone at the end.
    mmcss: bool,
    /// The video thread itself, scheduled as playback.
    _playback: Multimedia,
    log: Log,
}

impl Screen {
    /// Makes the device, the swap chain on `hwnd` and the shaders. To be
    /// called on the video thread, which it keeps.
    pub fn open(hwnd: isize, log: &Log) -> Result<Screen, String> {
        let mut screen = Screen {
            hwnd: HWND(hwnd as *mut c_void),
            gpu: None,
            mmcss: false,
            _playback: Multimedia::join(w!("Playback"), log),
            log: log.clone(),
        };
        // SAFETY: a switch for this process, taking a plain value.
        match unsafe { DwmEnableMMCSS(true) } {
            Ok(()) => screen.mmcss = true,
            Err(e) => log.write(&failure("DwmEnableMMCSS(TRUE)", &e)),
        }
        screen.gpu = Some(Gpu::create(screen.hwnd, log)?);
        Ok(screen)
    }

    fn gpu(&mut self) -> Result<&mut Gpu, Fault> {
        self.gpu
            .as_mut()
            .ok_or_else(|| Fault::Lost("the graphics card is still being made again".to_string()))
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        self.gpu = None;
        if self.mmcss {
            // SAFETY: as when it was turned on.
            if let Err(e) = unsafe { DwmEnableMMCSS(false) } {
                self.log.write(&failure("DwmEnableMMCSS(FALSE)", &e));
            }
        }
    }
}

impl Presenter for Screen {
    fn output(&self) -> Option<DecodeOutput> {
        self.gpu.as_ref().map(|gpu| DecodeOutput::D3d11 {
            device: gpu.device.device.clone(),
        })
    }

    fn decodable(&self, ff: &Arc<Ffmpeg>) -> CodecSet {
        match &self.gpu {
            Some(gpu) => VideoDecoder::supported(ff, &gpu.device.device),
            None => CodecSet::empty(),
        }
    }

    fn present(&mut self, picture: &DecodedFrame) -> Result<Shown, Fault> {
        match picture {
            DecodedFrame::D3d11(picture) => {
                self.gpu()?.draw(picture)?;
                Ok(Shown { checksum: None })
            }
            DecodedFrame::Cpu(_) => Err(Fault::Failed(
                "a picture decoded on the processor cannot be drawn by the graphics card"
                    .to_string(),
            )),
        }
    }

    fn resize(&mut self, width: u32, height: u32) -> Result<(), Fault> {
        self.gpu()?.resize(width, height)
    }

    fn picture_rect(&self) -> Option<Rect> {
        self.gpu.as_ref().and_then(|gpu| gpu.rect)
    }

    fn renew(&mut self) -> Result<(), String> {
        // The old swap chain goes first: a window takes one at a time.
        self.gpu = None;
        self.gpu = Some(Gpu::create(self.hwnd, &self.log)?);
        Ok(())
    }
}

/// Views of a texture the shader reads directly, slice by slice.
struct Views {
    texture: ID3D11Texture2D,
    size: (u32, u32),
    slices: Vec<(u32, Planes)>,
}

/// The player's own texture, which pictures are copied into.
struct Private {
    texture: ID3D11Texture2D,
    size: (u32, u32),
    views: Planes,
}

/// The shaders and what they read besides the picture.
struct Drawing {
    vertex: ID3D11VertexShader,
    pixel: ID3D11PixelShader,
    sampler: ID3D11SamplerState,
    /// Where the picture ends in its texture: see the shaders.
    constants: ID3D11Buffer,
}

impl Drawing {
    fn create(device: &Device) -> Result<Drawing, String> {
        let d3d = &device.device;
        let vertex_code = compile(s!("cover"), s!("vs_4_0"))?;
        let pixel_code = compile(s!("paint"), s!("ps_4_0"))?;
        let mut vertex = None;
        // SAFETY: bytecode the compiler just produced, and an out-parameter
        // of ours.
        unsafe { d3d.CreateVertexShader(bytes(&vertex_code), None, Some(&mut vertex)) }
            .map_err(|e| failure("CreateVertexShader", &e))?;
        let mut pixel = None;
        // SAFETY: the same.
        unsafe { d3d.CreatePixelShader(bytes(&pixel_code), None, Some(&mut pixel)) }
            .map_err(|e| failure("CreatePixelShader", &e))?;

        let sampling = D3D11_SAMPLER_DESC {
            Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
            AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
            AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
            AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
            MipLODBias: 0.0,
            MaxAnisotropy: 1,
            ComparisonFunc: D3D11_COMPARISON_NEVER,
            BorderColor: [0.0; 4],
            MinLOD: 0.0,
            MaxLOD: D3D11_FLOAT32_MAX,
        };
        let mut sampler = None;
        // SAFETY: a description that outlives the call.
        unsafe { d3d.CreateSamplerState(&sampling, Some(&mut sampler)) }
            .map_err(|e| failure("CreateSamplerState", &e))?;

        let buffer = D3D11_BUFFER_DESC {
            ByteWidth: size_of::<[f32; 4]>() as u32,
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
            CPUAccessFlags: 0,
            MiscFlags: 0,
            StructureByteStride: 0,
        };
        let mut constants = None;
        // SAFETY: a description and no initial data; the buffer is
        // written before it is first read.
        unsafe { d3d.CreateBuffer(&buffer, None, Some(&mut constants)) }
            .map_err(|e| failure("CreateBuffer for the constants", &e))?;

        let made_or = |what: &str| format!("{what} made nothing");
        Ok(Drawing {
            vertex: vertex.ok_or_else(|| made_or("CreateVertexShader"))?,
            pixel: pixel.ok_or_else(|| made_or("CreatePixelShader"))?,
            sampler: sampler.ok_or_else(|| made_or("CreateSamplerState"))?,
            constants: constants.ok_or_else(|| made_or("CreateBuffer"))?,
        })
    }
}

/// Everything made on one graphics card.
struct Gpu {
    device: Device,
    swap: IDXGISwapChain1,
    flags: DXGI_SWAP_CHAIN_FLAG,
    target: Option<ID3D11RenderTargetView>,
    /// The size of the back buffers.
    size: (u32, u32),
    /// The window has no area (minimised): nothing is drawn.
    hidden: bool,
    drawing: Drawing,
    /// What the constants hold now.
    reach: [f32; 4],
    views: Option<Views>,
    private: Option<Private>,
    rect: Option<Rect>,
    log: Log,
}

impl Gpu {
    fn create(hwnd: HWND, log: &Log) -> Result<Gpu, String> {
        let device = device::create(hwnd, log)?;
        // Everything the drawing needs, before the window is taken by a
        // swap chain.
        let drawing = Drawing::create(&device)?;

        // Tearing is allowed on the swap chain when the system offers it,
        // for a setting to ask for one day; pictures are presented
        // without it.
        let tearing = device
            .factory
            .cast::<IDXGIFactory5>()
            .ok()
            .is_some_and(|factory| {
                let mut allowed = BOOL(0);
                // SAFETY: a BOOL-sized answer into a BOOL of ours.
                unsafe {
                    factory.CheckFeatureSupport(
                        DXGI_FEATURE_PRESENT_ALLOW_TEARING,
                        (&raw mut allowed).cast::<c_void>(),
                        size_of::<BOOL>() as u32,
                    )
                }
                .is_ok()
                    && allowed.as_bool()
            });
        let flags = if tearing {
            DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING
        } else {
            DXGI_SWAP_CHAIN_FLAG(0)
        };

        let mut client = RECT::default();
        // SAFETY: a window handle the window vouches for, and a rectangle
        // of ours.
        unsafe { GetClientRect(hwnd, &mut client) }.map_err(|e| failure("GetClientRect", &e))?;
        let width = u32::try_from(client.right - client.left).unwrap_or(0);
        let height = u32::try_from(client.bottom - client.top).unwrap_or(0);
        let hidden = width == 0 || height == 0;
        let description = DXGI_SWAP_CHAIN_DESC1 {
            // A window with no area yet gets the smallest buffers, made
            // to its size once it has one.
            Width: width.max(1),
            Height: height.max(1),
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            Stereo: BOOL(0),
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: BUFFERS,
            Scaling: DXGI_SCALING_STRETCH,
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
            AlphaMode: DXGI_ALPHA_MODE_UNSPECIFIED,
            Flags: flags.0 as u32,
        };
        // SAFETY: the device of the factory's card, a window of this
        // process and a description that outlives the call.
        let swap = unsafe {
            device
                .factory
                .CreateSwapChainForHwnd(&device.device, hwnd, &description, None, None)
        }
        .map_err(|e| failure("CreateSwapChainForHwnd", &e))?;
        // The window is ZyrDesk's: Alt+Enter and Print Screen are its
        // business, never DXGI's.
        // SAFETY: the same window, and a plain flag.
        if let Err(e) = unsafe {
            device
                .factory
                .MakeWindowAssociation(hwnd, DXGI_MWA_NO_WINDOW_CHANGES)
        } {
            log.write(&failure("MakeWindowAssociation", &e));
        }
        // SAFETY: a getter on the swap chain just made.
        let made =
            unsafe { swap.GetDesc1() }.map_err(|e| failure("GetDesc1 of the swap chain", &e))?;

        let reach = [1.0; 4];
        let gpu = Gpu {
            device,
            swap,
            flags,
            target: None,
            size: (made.Width, made.Height),
            hidden,
            drawing,
            reach,
            views: None,
            private: None,
            rect: None,
            log: log.clone(),
        };
        gpu.upload(reach);
        log.write(&format!(
            "swap chain of {}x{}, {BUFFERS} buffers, flip discard, tearing {}",
            made.Width,
            made.Height,
            if tearing { "allowed" } else { "not offered" }
        ));
        Ok(gpu)
    }

    fn resize(&mut self, width: u32, height: u32) -> Result<(), Fault> {
        if width == 0 || height == 0 {
            self.hidden = true;
            self.rect = None;
            return Ok(());
        }
        self.hidden = false;
        // Nothing may hold on to the old buffers: the view of the one
        // drawn into, and the context's binding of it.
        self.target = None;
        // SAFETY: unbinding every render target.
        unsafe {
            self.device
                .context
                .OMSetRenderTargets(None, None::<&ID3D11DepthStencilView>)
        };
        // Zero for everything: the same number of buffers and format, at
        // the window's size as it is now.
        // SAFETY: no reference to a buffer is left, as above.
        unsafe {
            self.swap
                .ResizeBuffers(0, 0, 0, DXGI_FORMAT_UNKNOWN, self.flags)
        }
        .map_err(|e| self.fault("ResizeBuffers", e.code()))?;
        // SAFETY: a getter on the swap chain.
        let made = unsafe { self.swap.GetDesc1() }
            .map_err(|e| self.fault("GetDesc1 of the swap chain", e.code()))?;
        self.size = (made.Width, made.Height);
        Ok(())
    }

    fn draw(&mut self, picture: &D3d11Picture) -> Result<(), Fault> {
        if self.hidden {
            return Ok(());
        }
        let (width, height) = (picture.width(), picture.height());
        let source = picture.texture();
        let mut source_description = D3D11_TEXTURE2D_DESC::default();
        // SAFETY: a getter on a live texture, into a description of ours.
        unsafe { source.GetDesc(&mut source_description) };
        let bind = source_description.BindFlags;
        let sampleable = bind & D3D11_BIND_SHADER_RESOURCE.0 as u32 != 0;
        let decoder_surface = bind & D3D11_BIND_DECODER.0 as u32 != 0;
        // Intel cards read the decoder's textures directly, except those
        // before level 11.1, whose video and 3D engines Moonlight had to
        // fence apart for that: they copy, like everyone else.
        let intel = self.device.vendor == INTEL && self.device.level.0 >= D3D_FEATURE_LEVEL_11_1.0;
        let direct = sampleable && (!decoder_surface || intel);
        let (views, texture_size) = if direct {
            self.views_of(source, &source_description, picture.index())?
        } else {
            let slice = picture.index() * source_description.MipLevels.max(1);
            self.copied(source, slice, (width, height))?
        };

        let (texture_width, texture_height) = (texture_size.0 as f32, texture_size.1 as f32);
        let reach = [
            width as f32 / texture_width,
            height as f32 / texture_height,
            width.saturating_sub(1) as f32 / texture_width,
            height.saturating_sub(1) as f32 / texture_height,
        ];
        if reach != self.reach {
            self.reach = reach;
            self.upload(reach);
        }

        let target = self.target()?;
        let context = &self.device.context;
        self.rect = letterbox((width, height), self.size);
        // SAFETY: every object bound was made on this device and outlives
        // the draw; the slices outlive their calls.
        unsafe {
            context.OMSetRenderTargets(
                Some(&[Some(target.clone())]),
                None::<&ID3D11DepthStencilView>,
            );
            context.ClearRenderTargetView(&target, &[0.0, 0.0, 0.0, 1.0]);
            if let Some((left, top, width, height)) = self.rect {
                context.RSSetViewports(Some(&[D3D11_VIEWPORT {
                    TopLeftX: left as f32,
                    TopLeftY: top as f32,
                    Width: width as f32,
                    Height: height as f32,
                    MinDepth: 0.0,
                    MaxDepth: 1.0,
                }]));
                context.IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
                context.IASetInputLayout(None);
                context.VSSetShader(&self.drawing.vertex, None);
                context.PSSetShader(&self.drawing.pixel, None);
                context.PSSetShaderResources(0, Some(&views));
                context.PSSetSamplers(0, Some(&[Some(self.drawing.sampler.clone())]));
                context.PSSetConstantBuffers(0, Some(&[Some(self.drawing.constants.clone())]));
                context.Draw(3, 0);
                // The decoder writes into these textures next: they are
                // let go of as soon as they are read.
                context.PSSetShaderResources(0, Some(&[None, None]));
            }
        }
        // No wait for the screen's refresh and no flag: the newest
        // picture wins at composition, without tearing.
        // SAFETY: a swap chain of this device, drawn into above.
        let presented = unsafe { self.swap.Present(0, DXGI_PRESENT(0)) };
        if presented.is_err() {
            return Err(self.fault("Present", presented));
        }
        Ok(())
    }

    /// The view of the back buffer, made again after a resize.
    fn target(&mut self) -> Result<ID3D11RenderTargetView, Fault> {
        if let Some(target) = &self.target {
            return Ok(target.clone());
        }
        // Flip-model buffers turn under a single view of the first one.
        // SAFETY: buffer 0 of a swap chain that has it.
        let buffer: ID3D11Texture2D =
            unsafe { self.swap.GetBuffer(0) }.map_err(|e| self.fault("GetBuffer(0)", e.code()))?;
        let mut target = None;
        // SAFETY: a texture of this device, the default view of it.
        unsafe {
            self.device
                .device
                .CreateRenderTargetView(&buffer, None, Some(&mut target))
        }
        .map_err(|e| self.fault("CreateRenderTargetView", e.code()))?;
        let target = target
            .ok_or_else(|| Fault::Failed("CreateRenderTargetView made no view".to_string()))?;
        self.target = Some(target.clone());
        Ok(target)
    }

    /// The views of one slice of a texture the shader reads directly,
    /// and the texture's size.
    fn views_of(
        &mut self,
        texture: &ID3D11Texture2D,
        description: &D3D11_TEXTURE2D_DESC,
        slice: u32,
    ) -> Result<(Planes, (u32, u32)), Fault> {
        // Another texture (a new decoder): the views of the old one go,
        // and with them the last hold on it.
        if self
            .views
            .as_ref()
            .is_some_and(|views| views.texture != *texture)
        {
            self.views = None;
        }
        let views = match &mut self.views {
            Some(views) => views,
            None => self.views.insert(Views {
                texture: texture.clone(),
                size: (description.Width, description.Height),
                slices: Vec::new(),
            }),
        };
        if let Some((_, made)) = views.slices.iter().find(|(at, _)| *at == slice) {
            return Ok((made.clone(), views.size));
        }
        let size = views.size;
        let made =
            planes(&self.device, texture, slice).map_err(|(what, code)| self.fault(&what, code))?;
        if let Some(views) = &mut self.views {
            views.slices.push((slice, made.clone()));
        }
        Ok((made, size))
    }

    /// Copies the picture into the player's own texture, and gives its
    /// views and size.
    fn copied(
        &mut self,
        source: &ID3D11Texture2D,
        slice: u32,
        (width, height): (u32, u32),
    ) -> Result<(Planes, (u32, u32)), Fault> {
        // NV12 goes by pairs of rows and columns.
        let size = (width.next_multiple_of(2), height.next_multiple_of(2));
        if self
            .private
            .as_ref()
            .is_none_or(|private| private.size != size)
        {
            self.private = None;
            let description = D3D11_TEXTURE2D_DESC {
                Width: size.0,
                Height: size.1,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_NV12,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
                CPUAccessFlags: 0,
                MiscFlags: 0,
            };
            let mut texture = None;
            // SAFETY: a description that outlives the call, no initial
            // data.
            unsafe {
                self.device
                    .device
                    .CreateTexture2D(&description, None, Some(&mut texture))
            }
            .map_err(|e| self.fault("CreateTexture2D for the copies", e.code()))?;
            let texture = texture.ok_or_else(|| {
                Fault::Failed("CreateTexture2D for the copies made nothing".to_string())
            })?;
            let views = planes(&self.device, &texture, 0)
                .map_err(|(what, code)| self.fault(&what, code))?;
            self.log.write(&format!(
                "pictures are copied into a texture of {}x{} before being drawn",
                size.0, size.1
            ));
            self.private = Some(Private {
                texture,
                size,
                views,
            });
        }
        let Some(private) = &self.private else {
            return Err(Fault::Failed("no texture to copy into".to_string()));
        };
        let area = D3D11_BOX {
            left: 0,
            top: 0,
            front: 0,
            right: size.0,
            bottom: size.1,
            back: 1,
        };
        // SAFETY: two NV12 textures of this device; the area lies inside
        // both, the decoder's surfaces being at least the picture's size
        // rounded up.
        unsafe {
            self.device.context.CopySubresourceRegion(
                &private.texture,
                0,
                0,
                0,
                0,
                source,
                slice,
                Some(&area),
            )
        };
        Ok((private.views.clone(), private.size))
    }

    fn upload(&self, reach: [f32; 4]) {
        // SAFETY: the whole of a constant buffer of this device, from
        // four floats that outlive the call.
        unsafe {
            self.device.context.UpdateSubresource(
                &self.drawing.constants,
                0,
                None,
                reach.as_ptr().cast::<c_void>(),
                0,
                0,
            )
        };
    }

    /// What a failure of the card means: gone, with why the device was
    /// removed, or this picture alone.
    fn fault(&self, what: &str, code: HRESULT) -> Fault {
        let text = format!(
            "{what}: {}",
            code_and_words(code, &Error::from(code).message())
        );
        if !GONE.contains(&code) {
            return Fault::Failed(text);
        }
        // SAFETY: a question to the device, answered with a code.
        let reason = match unsafe { self.device.device.GetDeviceRemovedReason() } {
            Ok(()) => "the device says it is still there".to_string(),
            Err(e) => format!("removed: {}", code_and_words(e.code(), &e.message())),
        };
        Fault::Lost(format!("{text}; {reason}"))
    }
}

impl Drop for Gpu {
    fn drop(&mut self) {
        // A swap chain is destroyed once the context lets go of it and
        // flushes: otherwise the window would still be taken when the
        // next one is made on it.
        self.target = None;
        // SAFETY: plain calls on a live context.
        unsafe {
            self.device.context.ClearState();
            self.device.context.Flush();
        }
    }
}

/// Views of the two planes of one slice of an NV12 texture: luma, then
/// chroma with U and V side by side. Both as arrays of one slice, which
/// is what the shader reads, whether the texture is an array or not.
fn planes(
    device: &Device,
    texture: &ID3D11Texture2D,
    slice: u32,
) -> Result<Planes, (String, HRESULT)> {
    let mut made = [None, None];
    for (view, format) in made
        .iter_mut()
        .zip([DXGI_FORMAT_R8_UNORM, DXGI_FORMAT_R8G8_UNORM])
    {
        let description = D3D11_SHADER_RESOURCE_VIEW_DESC {
            Format: format,
            ViewDimension: D3D11_SRV_DIMENSION_TEXTURE2DARRAY,
            Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
                Texture2DArray: D3D11_TEX2D_ARRAY_SRV {
                    MostDetailedMip: 0,
                    MipLevels: 1,
                    FirstArraySlice: slice,
                    ArraySize: 1,
                },
            },
        };
        // SAFETY: an NV12 texture of this device with a slice there, and
        // a description that outlives the call.
        unsafe {
            device
                .device
                .CreateShaderResourceView(texture, Some(&description), Some(view))
        }
        .map_err(|e| {
            (
                format!("CreateShaderResourceView of slice {slice} ({format:?})"),
                e.code(),
            )
        })?;
    }
    Ok(made)
}

/// Compiles one entry point of the shaders.
fn compile(entry: PCSTR, target: PCSTR) -> Result<ID3DBlob, String> {
    let mut code = None;
    let mut errors = None;
    // SAFETY: the text and its length, names that are C strings, and
    // out-parameters of ours.
    let compiled = unsafe {
        D3DCompile(
            SHADERS.as_ptr().cast::<c_void>(),
            SHADERS.len(),
            s!("zyr-player"),
            None,
            None,
            entry,
            target,
            D3DCOMPILE_OPTIMIZATION_LEVEL3 | D3DCOMPILE_ENABLE_STRICTNESS,
            0,
            &mut code,
            Some(&mut errors),
        )
    };
    // SAFETY: a C string the macro made.
    let name = unsafe { entry.to_string() }.unwrap_or_default();
    match (compiled, code) {
        (Ok(()), Some(code)) => Ok(code),
        (Ok(()), None) => Err(format!("D3DCompile of {name} gave no code")),
        (Err(e), _) => {
            let said = errors
                .as_ref()
                .map(|errors| String::from_utf8_lossy(bytes(errors)).trim().to_string())
                .unwrap_or_default();
            Err(format!(
                "{}: {said}",
                failure(&format!("D3DCompile of {name}"), &e)
            ))
        }
    }
}

/// The bytes a blob holds.
fn bytes(blob: &ID3DBlob) -> &[u8] {
    // SAFETY: a blob holds that many bytes at that address for as long as
    // it lives, which the borrow keeps.
    unsafe {
        std::slice::from_raw_parts(blob.GetBufferPointer().cast::<u8>(), blob.GetBufferSize())
    }
}
