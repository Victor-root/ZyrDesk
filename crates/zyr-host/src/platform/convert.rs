//! The screen's image into the encoder's picture, on the graphics card.
//!
//! The shaders are compiled when the engine starts, from the text of
//! `convert.hlsl`, by the compiler every Windows 10 carries
//! (d3dcompiler_47): nothing to build or ship beside the program. Two
//! passes draw a picture: luma into the Y plane of the encoder's own NV12
//! texture, chroma into its UV plane, both through views of the one
//! texture, so the picture is made where the encoder reads it. The image
//! is scaled on the way, the pointer drawn over it, and what lies outside
//! the image is black.
//!
//! An encoder that reads memory is given the same picture, drawn into a
//! texture of ours and copied back. A card that cannot render into NV12
//! at all is given the image in red, green and blue, converted to NV12
//! on the processor.

use std::ffi::c_void;

use windows::Win32::Graphics::Direct3D::Fxc::{
    D3DCOMPILE_ENABLE_STRICTNESS, D3DCOMPILE_OPTIMIZATION_LEVEL3, D3DCompile,
};
use windows::Win32::Graphics::Direct3D::{
    D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST, ID3DBlob, ID3DInclude,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_CONSTANT_BUFFER, D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE,
    D3D11_BUFFER_DESC, D3D11_COMPARISON_NEVER, D3D11_CPU_ACCESS_READ, D3D11_CULL_NONE,
    D3D11_FILL_SOLID, D3D11_FILTER, D3D11_FILTER_MIN_MAG_MIP_LINEAR,
    D3D11_FILTER_MIN_MAG_MIP_POINT, D3D11_FLOAT32_MAX, D3D11_FORMAT_SUPPORT_CPU_LOCKABLE,
    D3D11_FORMAT_SUPPORT_RENDER_TARGET, D3D11_MAP_READ, D3D11_MAPPED_SUBRESOURCE,
    D3D11_RASTERIZER_DESC, D3D11_RENDER_TARGET_VIEW_DESC, D3D11_RENDER_TARGET_VIEW_DESC_0,
    D3D11_RTV_DIMENSION_TEXTURE2D, D3D11_SAMPLER_DESC, D3D11_SUBRESOURCE_DATA, D3D11_TEX2D_RTV,
    D3D11_TEXTURE_ADDRESS_CLAMP, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, D3D11_USAGE_IMMUTABLE,
    D3D11_USAGE_STAGING, D3D11_VIEWPORT, ID3D11BlendState, ID3D11Buffer, ID3D11ClassLinkage,
    ID3D11DepthStencilView, ID3D11Device, ID3D11DeviceContext, ID3D11InputLayout,
    ID3D11PixelShader, ID3D11RasterizerState, ID3D11RenderTargetView, ID3D11SamplerState,
    ID3D11ShaderResourceView, ID3D11Texture2D, ID3D11VertexShader,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_NV12, DXGI_FORMAT_R8_UNORM,
    DXGI_FORMAT_R8G8_UNORM, DXGI_SAMPLE_DESC,
};
use windows::core::{PCSTR, s};
use zyr_codec::Nv12Planes;

use super::failed;
use crate::color::{BLACK, bgra_to_nv12, bt709_limited};
use crate::picture::{Rect, Size};
use crate::pointer::Shape;

/// The shaders, as text.
const SHADERS: &str = include_str!("convert.hlsl");

/// What the shaders are told for a picture: the layout of their constant
/// buffer, 80 bytes.
#[repr(C)]
#[derive(Clone, Copy)]
struct Constants {
    y_weights: [f32; 4],
    u_weights: [f32; 4],
    v_weights: [f32; 4],
    pointer_rect: [f32; 4],
    luma_step: [f32; 2],
    rotation: u32,
    pointer_shown: u32,
}

const _: () = assert!(size_of::<Constants>() == 80);

/// The luma of black, and the chroma of grey, as render targets clear.
const LUMA_BLACK: [f32; 4] = [BLACK.0 as f32 / 255.0, 0.0, 0.0, 0.0];
const CHROMA_GREY: [f32; 4] = [BLACK.1 as f32 / 255.0, BLACK.1 as f32 / 255.0, 0.0, 0.0];
const COLOUR_BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

/// A pointer ready to be drawn.
pub(super) struct PointerImages {
    paint: ID3D11ShaderResourceView,
    flip: ID3D11ShaderResourceView,
}

/// What a picture is drawn from.
pub(super) struct Scene<'a> {
    /// The latest image, as captured; black while there is none.
    pub(super) image: Option<&'a ID3D11ShaderResourceView>,
    /// Quarter turns of the screen.
    pub(super) rotation: u32,
    /// The pointer, and where it lies in the image, from 0 to 1: left,
    /// top, right, bottom.
    pub(super) pointer: Option<(&'a PointerImages, [f32; 4])>,
    /// Where the image goes in the picture.
    pub(super) placement: Rect,
}

/// Where pictures for encoders that read memory are drawn, and read back.
struct Readback {
    size: Size,
    /// Drawn in NV12, or in red, green and blue.
    nv12: bool,
    target: ID3D11Texture2D,
    staging: ID3D11Texture2D,
}

pub(super) struct Converter {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    vertex: ID3D11VertexShader,
    luma: ID3D11PixelShader,
    chroma: ID3D11PixelShader,
    colour: ID3D11PixelShader,
    samplers: [Option<ID3D11SamplerState>; 2],
    rasterizer: ID3D11RasterizerState,
    constants: ID3D11Buffer,
    black: ID3D11ShaderResourceView,
    clear: ID3D11ShaderResourceView,
    /// Whether this card renders into NV12 textures.
    nv12: bool,
    /// Whether NV12 textures can be read back.
    nv12_readable: bool,
    readback: Option<Readback>,
}

impl Converter {
    pub(super) fn new(
        device: &ID3D11Device,
        context: &ID3D11DeviceContext,
    ) -> Result<Self, String> {
        let vertex_code = compile(s!("main_vs"), s!("vs_5_0"))?;
        let mut vertex = None;
        // SAFETY: bytecode the compiler just made for this stage.
        unsafe {
            device.CreateVertexShader(&vertex_code, None::<&ID3D11ClassLinkage>, Some(&mut vertex))
        }
        .map_err(|e| failed("creating the vertex shader", &e))?;
        let pixel = |entry: PCSTR, what: &str| -> Result<ID3D11PixelShader, String> {
            let code = compile(entry, s!("ps_5_0"))?;
            let mut shader = None;
            // SAFETY: bytecode the compiler just made for this stage.
            unsafe {
                device.CreatePixelShader(&code, None::<&ID3D11ClassLinkage>, Some(&mut shader))
            }
            .map_err(|e| failed(&format!("creating the {what} shader"), &e))?;
            shader.ok_or_else(|| format!("the {what} shader came back empty"))
        };
        let luma = pixel(s!("main_luma"), "luma")?;
        let chroma = pixel(s!("main_chroma"), "chroma")?;
        let colour = pixel(s!("main_colour"), "colour")?;
        let samplers = [
            Some(sampler(device, D3D11_FILTER_MIN_MAG_MIP_LINEAR)?),
            Some(sampler(device, D3D11_FILTER_MIN_MAG_MIP_POINT)?),
        ];
        let rasterizer = rasterizer(device)?;
        let constants = constant_buffer(device)?;
        let black = image(device, 1, 1, &[0, 0, 0, 255], "the black image")?;
        let clear = image(device, 1, 1, &[0, 0, 0, 0], "the clear image")?;
        // SAFETY: questions about a format, plain values.
        let support = unsafe { device.CheckFormatSupport(DXGI_FORMAT_NV12) }.unwrap_or(0);
        Ok(Self {
            device: device.clone(),
            context: context.clone(),
            vertex: vertex.ok_or("the vertex shader came back empty")?,
            luma,
            chroma,
            colour,
            samplers,
            rasterizer,
            constants,
            black,
            clear,
            nv12: support & D3D11_FORMAT_SUPPORT_RENDER_TARGET.0 as u32 != 0,
            nv12_readable: support & D3D11_FORMAT_SUPPORT_CPU_LOCKABLE.0 as u32 != 0,
            readback: None,
        })
    }

    /// Whether the card renders into the encoders' NV12 textures.
    pub(super) fn renders_nv12(&self) -> bool {
        self.nv12
    }

    /// The pointer's images on the card.
    pub(super) fn pointer(&self, shape: &Shape) -> Result<PointerImages, String> {
        Ok(PointerImages {
            paint: image(
                &self.device,
                shape.width,
                shape.height,
                &shape.paint,
                "the pointer",
            )?,
            flip: image(
                &self.device,
                shape.width,
                shape.height,
                &shape.flip,
                "the pointer's mask",
            )?,
        })
    }

    /// Draws the picture into an NV12 texture of an encoder.
    pub(super) fn draw_into_texture(
        &self,
        scene: &Scene<'_>,
        texture: &ID3D11Texture2D,
    ) -> Result<(), String> {
        let picture = size_of_texture(texture);
        self.nv12_passes(scene, texture, picture)
    }

    /// Draws the picture and copies it into memory, for an encoder that
    /// reads it there.
    pub(super) fn draw_into_memory(
        &mut self,
        scene: &Scene<'_>,
        picture: Size,
        planes: &mut Nv12Planes<'_>,
    ) -> Result<(), String> {
        let readback = self.readback(picture)?;
        let (target, staging, nv12) = (
            readback.target.clone(),
            readback.staging.clone(),
            readback.nv12,
        );
        if nv12 {
            self.nv12_passes(scene, &target, picture)?;
        } else {
            self.colour_pass(scene, &target, picture)?;
        }
        // SAFETY: two textures of this device of the same size and format.
        unsafe { self.context.CopyResource(&staging, &target) };
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        // SAFETY: a staging texture of ours, readable by the processor;
        // mapping waits for the copy above.
        unsafe {
            self.context
                .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
        }
        .map_err(|e| failed("reading the picture back", &e))?;
        let pitch = mapped.RowPitch as usize;
        let (width, height) = (picture.width as usize, picture.height as usize);
        if nv12 {
            // SAFETY: a mapped NV12 texture holds its luma rows, then its
            // chroma rows, each `pitch` bytes apart and `width` bytes long,
            // as long as it is mapped.
            let bytes = unsafe {
                std::slice::from_raw_parts(
                    mapped.pData.cast::<u8>(),
                    pitch * (height + height / 2 - 1) + width,
                )
            };
            for row in 0..height {
                planes.luma[row * planes.luma_stride..][..width]
                    .copy_from_slice(&bytes[row * pitch..][..width]);
            }
            let chroma = &bytes[pitch * height..];
            for row in 0..height / 2 {
                planes.chroma[row * planes.chroma_stride..][..width]
                    .copy_from_slice(&chroma[row * pitch..][..width]);
            }
        } else {
            // SAFETY: a mapped texture of `height` rows `pitch` bytes apart,
            // each four bytes a pixel.
            let bytes = unsafe {
                std::slice::from_raw_parts(
                    mapped.pData.cast::<u8>(),
                    pitch * (height - 1) + width * 4,
                )
            };
            bgra_to_nv12(bytes, pitch, picture, planes);
        }
        // SAFETY: the subresource mapped above, and nothing of it is used
        // after.
        unsafe { self.context.Unmap(&staging, 0) };
        Ok(())
    }

    /// The textures to draw and read back pictures of `size` with.
    fn readback(&mut self, size: Size) -> Result<&Readback, String> {
        if self
            .readback
            .as_ref()
            .is_none_or(|readback| readback.size != size)
        {
            let nv12 = self.nv12 && self.nv12_readable;
            let format = if nv12 {
                DXGI_FORMAT_NV12
            } else {
                DXGI_FORMAT_B8G8R8A8_UNORM
            };
            let target = texture(
                &self.device,
                size,
                format,
                D3D11_BIND_RENDER_TARGET.0 as u32,
                false,
                "the picture for an encoder that reads memory",
            )?;
            let staging = texture(
                &self.device,
                size,
                format,
                0,
                true,
                "the copy of the picture the processor reads",
            )?;
            self.readback = Some(Readback {
                size,
                nv12,
                target,
                staging,
            });
        }
        self.readback
            .as_ref()
            .ok_or_else(|| "no texture to read the picture back from".to_string())
    }

    /// Luma then chroma, into the two planes of an NV12 texture.
    fn nv12_passes(
        &self,
        scene: &Scene<'_>,
        texture: &ID3D11Texture2D,
        picture: Size,
    ) -> Result<(), String> {
        let luma = self.view(texture, DXGI_FORMAT_R8_UNORM, "the luma plane")?;
        let chroma = self.view(texture, DXGI_FORMAT_R8G8_UNORM, "the chroma plane")?;
        let bars = bars(scene.placement, picture);
        self.prepare(scene);
        let place = scene.placement;
        self.pass(
            &luma,
            &self.luma,
            viewport(place, 1),
            bars.then_some(LUMA_BLACK),
        );
        self.pass(
            &chroma,
            &self.chroma,
            viewport(place, 2),
            bars.then_some(CHROMA_GREY),
        );
        self.finish();
        Ok(())
    }

    /// Red, green and blue, into a texture of ours.
    fn colour_pass(
        &self,
        scene: &Scene<'_>,
        texture: &ID3D11Texture2D,
        picture: Size,
    ) -> Result<(), String> {
        let target = self.view(texture, DXGI_FORMAT_B8G8R8A8_UNORM, "the colour picture")?;
        let bars = bars(scene.placement, picture);
        self.prepare(scene);
        self.pass(
            &target,
            &self.colour,
            viewport(scene.placement, 1),
            bars.then_some(COLOUR_BLACK),
        );
        self.finish();
        Ok(())
    }

    fn view(
        &self,
        texture: &ID3D11Texture2D,
        format: DXGI_FORMAT,
        what: &str,
    ) -> Result<ID3D11RenderTargetView, String> {
        let desc = D3D11_RENDER_TARGET_VIEW_DESC {
            Format: format,
            ViewDimension: D3D11_RTV_DIMENSION_TEXTURE2D,
            Anonymous: D3D11_RENDER_TARGET_VIEW_DESC_0 {
                Texture2D: D3D11_TEX2D_RTV { MipSlice: 0 },
            },
        };
        let mut view = None;
        // SAFETY: a texture of this device bound as a render target, and a
        // format that names one of its planes.
        unsafe {
            self.device
                .CreateRenderTargetView(texture, Some(&desc), Some(&mut view))
        }
        .map_err(|e| failed(&format!("drawing into {what}"), &e))?;
        view.ok_or_else(|| format!("no view of {what}"))
    }

    /// Sets up the pipeline for the scene.
    fn prepare(&self, scene: &Scene<'_>) {
        let [y, u, v] = bt709_limited();
        let place = scene.placement;
        let (pointer, pointer_rect) = match scene.pointer {
            Some((images, rect)) => (Some(images), rect),
            None => (None, [0.0; 4]),
        };
        let constants = Constants {
            y_weights: y,
            u_weights: u,
            v_weights: v,
            pointer_rect,
            luma_step: [
                1.0 / place.width.max(1) as f32,
                1.0 / place.height.max(1) as f32,
            ],
            rotation: scene.rotation,
            pointer_shown: u32::from(pointer.is_some()),
        };
        let views = [
            Some(scene.image.unwrap_or(&self.black).clone()),
            Some(pointer.map_or(&self.clear, |images| &images.paint).clone()),
            Some(pointer.map_or(&self.clear, |images| &images.flip).clone()),
        ];
        // SAFETY: every object bound is of this device, the constant
        // buffer is exactly the size of what is copied into it, and this
        // thread is the only one drawing on the device.
        unsafe {
            self.context.UpdateSubresource(
                &self.constants,
                0,
                None,
                (&raw const constants).cast::<c_void>(),
                0,
                0,
            );
            self.context
                .IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            self.context.IASetInputLayout(None::<&ID3D11InputLayout>);
            self.context.VSSetShader(&self.vertex, None);
            self.context.RSSetState(&self.rasterizer);
            self.context
                .OMSetBlendState(None::<&ID3D11BlendState>, None, u32::MAX);
            self.context.PSSetShaderResources(0, Some(&views));
            self.context.PSSetSamplers(0, Some(&self.samplers));
            self.context
                .PSSetConstantBuffers(0, Some(&[Some(self.constants.clone())]));
        }
    }

    /// One draw of the triangle, into `target`, where `viewport` says,
    /// the whole target cleared first if `clear` says to what.
    fn pass(
        &self,
        target: &ID3D11RenderTargetView,
        shader: &ID3D11PixelShader,
        viewport: D3D11_VIEWPORT,
        clear: Option<[f32; 4]>,
    ) {
        // SAFETY: objects of this device, set up by `prepare`.
        unsafe {
            if let Some(colour) = clear {
                self.context.ClearRenderTargetView(target, &colour);
            }
            self.context.OMSetRenderTargets(
                Some(&[Some(target.clone())]),
                None::<&ID3D11DepthStencilView>,
            );
            self.context.RSSetViewports(Some(&[viewport]));
            self.context.PSSetShader(shader, None);
            self.context.Draw(3, 0);
        }
    }

    /// Lets go of what the passes bound, so that the image can be copied
    /// into again and the encoder can read the picture.
    fn finish(&self) {
        // SAFETY: unbinding, with nothing in place of each slot.
        unsafe {
            self.context
                .OMSetRenderTargets(None, None::<&ID3D11DepthStencilView>);
            self.context
                .PSSetShaderResources(0, Some(&[None, None, None]));
        }
    }
}

/// Whether the image leaves black bars in the picture.
fn bars(placement: Rect, picture: Size) -> bool {
    placement != Rect::new(0, 0, picture.width, picture.height)
}

/// The viewport of `placement`, in pixels of a plane `scale` times
/// smaller than the picture.
fn viewport(placement: Rect, scale: u32) -> D3D11_VIEWPORT {
    D3D11_VIEWPORT {
        TopLeftX: (placement.x / scale as i32) as f32,
        TopLeftY: (placement.y / scale as i32) as f32,
        Width: (placement.width / scale) as f32,
        Height: (placement.height / scale) as f32,
        MinDepth: 0.0,
        MaxDepth: 1.0,
    }
}

fn size_of_texture(texture: &ID3D11Texture2D) -> Size {
    let mut desc = D3D11_TEXTURE2D_DESC::default();
    // SAFETY: a getter on a live texture.
    unsafe { texture.GetDesc(&mut desc) };
    Size::new(desc.Width, desc.Height)
}

/// Compiles one entry point of the shaders.
fn compile(entry: PCSTR, target: PCSTR) -> Result<Vec<u8>, String> {
    let mut code: Option<ID3DBlob> = None;
    let mut errors: Option<ID3DBlob> = None;
    // SAFETY: the text and its length describe the same bytes; the names
    // are terminated constants; the blobs come back owned.
    let compiled = unsafe {
        D3DCompile(
            SHADERS.as_ptr().cast(),
            SHADERS.len(),
            s!("convert.hlsl"),
            None,
            None::<&ID3DInclude>,
            entry,
            target,
            D3DCOMPILE_ENABLE_STRICTNESS | D3DCOMPILE_OPTIMIZATION_LEVEL3,
            0,
            &mut code,
            Some(&mut errors),
        )
    };
    // SAFETY: the name is a terminated constant.
    let name = unsafe { entry.to_string() }.unwrap_or_default();
    if let Err(e) = compiled {
        let said = errors.as_ref().map(blob_text).unwrap_or_default();
        return Err(format!(
            "{}; the compiler says: {said}",
            failed(&format!("compiling the shader {name}"), &e)
        ));
    }
    code.as_ref()
        .map(|blob| blob_bytes(blob).to_vec())
        .ok_or_else(|| format!("the shader {name} came back empty"))
}

fn blob_bytes(blob: &ID3DBlob) -> &[u8] {
    // SAFETY: a blob holds as many bytes as it says, for as long as it
    // lives, which the borrow ties the slice to.
    unsafe {
        std::slice::from_raw_parts(blob.GetBufferPointer().cast::<u8>(), blob.GetBufferSize())
    }
}

fn blob_text(blob: &ID3DBlob) -> String {
    String::from_utf8_lossy(blob_bytes(blob))
        .trim_end_matches('\0')
        .trim()
        .to_string()
}

fn sampler(device: &ID3D11Device, filter: D3D11_FILTER) -> Result<ID3D11SamplerState, String> {
    let desc = D3D11_SAMPLER_DESC {
        Filter: filter,
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
    let mut state = None;
    // SAFETY: a complete description; the state comes back owned.
    unsafe { device.CreateSamplerState(&desc, Some(&mut state)) }
        .map_err(|e| failed("creating a sampler", &e))?;
    state.ok_or_else(|| "a sampler came back empty".to_string())
}

fn rasterizer(device: &ID3D11Device) -> Result<ID3D11RasterizerState, String> {
    let desc = D3D11_RASTERIZER_DESC {
        FillMode: D3D11_FILL_SOLID,
        CullMode: D3D11_CULL_NONE,
        FrontCounterClockwise: false.into(),
        DepthBias: 0,
        DepthBiasClamp: 0.0,
        SlopeScaledDepthBias: 0.0,
        DepthClipEnable: true.into(),
        ScissorEnable: false.into(),
        MultisampleEnable: false.into(),
        AntialiasedLineEnable: false.into(),
    };
    let mut state = None;
    // SAFETY: a complete description; the state comes back owned.
    unsafe { device.CreateRasterizerState(&desc, Some(&mut state)) }
        .map_err(|e| failed("creating the rasterizer state", &e))?;
    state.ok_or_else(|| "the rasterizer state came back empty".to_string())
}

fn constant_buffer(device: &ID3D11Device) -> Result<ID3D11Buffer, String> {
    let desc = D3D11_BUFFER_DESC {
        ByteWidth: size_of::<Constants>() as u32,
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
        CPUAccessFlags: 0,
        MiscFlags: 0,
        StructureByteStride: 0,
    };
    let mut buffer = None;
    // SAFETY: a complete description, a size that is a multiple of 16 as
    // constant buffers require; the buffer comes back owned.
    unsafe { device.CreateBuffer(&desc, None, Some(&mut buffer)) }
        .map_err(|e| failed("creating the shaders' constants", &e))?;
    buffer.ok_or_else(|| "the shaders' constants came back empty".to_string())
}

/// A texture of blue, green, red and alpha bytes, set once, to sample.
fn image(
    device: &ID3D11Device,
    width: u32,
    height: u32,
    bgra: &[u8],
    what: &str,
) -> Result<ID3D11ShaderResourceView, String> {
    if bgra.len() != width as usize * height as usize * 4 {
        return Err(format!("{what}: {} bytes for {width}x{height}", bgra.len()));
    }
    let desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_IMMUTABLE,
        BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
        CPUAccessFlags: 0,
        MiscFlags: 0,
    };
    let data = D3D11_SUBRESOURCE_DATA {
        pSysMem: bgra.as_ptr().cast(),
        SysMemPitch: width * 4,
        SysMemSlicePitch: 0,
    };
    let mut texture = None;
    // SAFETY: the data holds `height` rows of `width * 4` bytes, checked
    // above, and is copied by the call.
    unsafe { device.CreateTexture2D(&desc, Some(&data), Some(&mut texture)) }
        .map_err(|e| failed(&format!("creating {what}"), &e))?;
    let texture = texture.ok_or_else(|| format!("{what} came back empty"))?;
    let mut view = None;
    // SAFETY: a texture of this device made to be sampled.
    unsafe { device.CreateShaderResourceView(&texture, None, Some(&mut view)) }
        .map_err(|e| failed(&format!("sampling {what}"), &e))?;
    view.ok_or_else(|| format!("no view of {what}"))
}

/// A texture of `size` in `format`, to draw into (`bind`) or, if
/// `staging`, for the processor to read.
fn texture(
    device: &ID3D11Device,
    size: Size,
    format: DXGI_FORMAT,
    bind: u32,
    staging: bool,
    what: &str,
) -> Result<ID3D11Texture2D, String> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: size.width,
        Height: size.height,
        MipLevels: 1,
        ArraySize: 1,
        Format: format,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: if staging {
            D3D11_USAGE_STAGING
        } else {
            D3D11_USAGE_DEFAULT
        },
        BindFlags: bind,
        CPUAccessFlags: if staging {
            D3D11_CPU_ACCESS_READ.0 as u32
        } else {
            0
        },
        MiscFlags: 0,
    };
    let mut texture = None;
    // SAFETY: a complete description; the texture comes back owned.
    unsafe { device.CreateTexture2D(&desc, None, Some(&mut texture)) }
        .map_err(|e| failed(&format!("creating {what}"), &e))?;
    texture.ok_or_else(|| format!("{what} came back empty"))
}
