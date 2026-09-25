//! The graphics cards, their screens, and the Direct3D 11 device the
//! engine works on.
//!
//! One device, on the card the filmed screen is plugged into: Desktop
//! Duplication only works there, and the conversion and the encoder
//! work on the same device so that a picture never leaves the card nor
//! crosses from one device to another. The capture, the conversion and
//! the encoding all run on the engine's pictures thread, one after the
//! other, so the device is never waited on by another thread of ours;
//! it is protected all the same, since some encoders work on it from
//! threads of their own.
//!
//! A card whose driver offers no video (the basic display driver of a
//! machine without its graphics driver, many virtual machines) refuses
//! a device made for video. It gets one without: the screen is filmed
//! all the same, and only encoders that read memory can work.

use windows::Win32::Foundation::{E_INVALIDARG, E_POINTER, HMODULE, LUID};
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_CREATE_DEVICE_FLAG, D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
    D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Multithread,
};
use windows::Win32::Graphics::Dxgi::{
    DXGI_ADAPTER_DESC1, DXGI_ERROR_NOT_FOUND, DXGI_ERROR_UNSUPPORTED, DXGI_OUTPUT_DESC,
    IDXGIAdapter1, IDXGIDevice, IDXGIDevice1, IDXGIFactory1, IDXGIOutput,
};
use windows::core::Interface;
use zyr_codec::GpuVendor;
use zyr_proto::log::Log;

use super::failed;

/// Highest priority DXGI gives a device's work on the graphics card.
const GPU_PRIORITY: i32 = 7;

/// A screen of a graphics card, as DXGI lists them.
#[derive(Clone)]
pub(super) struct Output {
    pub(super) adapter: IDXGIAdapter1,
    pub(super) card: DXGI_ADAPTER_DESC1,
    pub(super) output: IDXGIOutput,
    pub(super) desc: DXGI_OUTPUT_DESC,
}

impl Output {
    /// The name Windows takes orders about the screen under,
    /// `\\.\DISPLAY1` and its like.
    pub(super) fn gdi_name(&self) -> String {
        wide(&self.desc.DeviceName)
    }
}

/// Every screen on the desktop, card by card.
pub(super) fn outputs(factory: &IDXGIFactory1, log: &Log) -> Vec<Output> {
    let mut found = Vec::new();
    for index in 0.. {
        // SAFETY: a plain index; the adapter comes back owned.
        let adapter = match unsafe { factory.EnumAdapters1(index) } {
            Ok(adapter) => adapter,
            Err(e) if e.code() == DXGI_ERROR_NOT_FOUND => break,
            Err(e) => {
                log.write(&failed(&format!("listing graphics card {index}"), &e));
                break;
            }
        };
        // SAFETY: a getter on a live adapter.
        let card = match unsafe { adapter.GetDesc1() } {
            Ok(card) => card,
            Err(e) => {
                log.write(&failed(&format!("describing graphics card {index}"), &e));
                continue;
            }
        };
        for number in 0.. {
            // SAFETY: a plain index; the output comes back owned.
            let output = match unsafe { adapter.EnumOutputs(number) } {
                Ok(output) => output,
                Err(e) if e.code() == DXGI_ERROR_NOT_FOUND => break,
                Err(e) => {
                    log.write(&failed(
                        &format!("listing screen {number} of {}", wide(&card.Description)),
                        &e,
                    ));
                    break;
                }
            };
            // SAFETY: a getter on a live output.
            match unsafe { output.GetDesc() } {
                Ok(desc) if desc.AttachedToDesktop.as_bool() => found.push(Output {
                    adapter: adapter.clone(),
                    card,
                    output,
                    desc,
                }),
                Ok(_) => {}
                Err(e) => log.write(&failed(&format!("describing screen {number}"), &e)),
            }
        }
    }
    found
}

/// The device the engine works on, on one graphics card.
pub(super) struct Device {
    pub(super) device: ID3D11Device,
    pub(super) context: ID3D11DeviceContext,
    /// The device's own lock, which the encoders' threads take for each
    /// call they make on it; none if the device would not give it.
    pub(super) lock: Option<ID3D11Multithread>,
    pub(super) card: LUID,
    pub(super) vendor: GpuVendor,
    pub(super) name: String,
}

impl Device {
    /// A device on `adapter`, with what the capture, the conversion and
    /// the encoders need of it.
    pub(super) fn on(
        adapter: &IDXGIAdapter1,
        card: &DXGI_ADAPTER_DESC1,
        log: &Log,
    ) -> Result<Self, String> {
        let name = wide(&card.Description);
        let made = match create(
            adapter,
            D3D11_CREATE_DEVICE_VIDEO_SUPPORT | D3D11_CREATE_DEVICE_BGRA_SUPPORT,
        ) {
            // What was asked is refused, not the card: a card being reset
            // answers otherwise, and is not settled with less for good.
            Err(refused) if [E_INVALIDARG, DXGI_ERROR_UNSUPPORTED].contains(&refused.code()) => {
                log.write(&failed(
                    &format!(
                        "creating a Direct3D 11 device for video on {name} (trying one without: \
                         only encoders that read memory can work on it)"
                    ),
                    &refused,
                ));
                create(adapter, D3D11_CREATE_DEVICE_BGRA_SUPPORT)
            }
            made => made,
        };
        let (device, context) =
            made.map_err(|e| failed(&format!("creating a Direct3D 11 device on {name}"), &e))?;
        let lock = tune(&device, log);
        log.write(&format!(
            "Direct3D 11 device made on {name} (vendor 0x{:04x})",
            card.VendorId
        ));
        Ok(Self {
            device,
            context,
            lock,
            card: card.AdapterLuid,
            vendor: GpuVendor::from_pci(card.VendorId),
            name,
        })
    }
}

/// A device of `flags` on `adapter`, at feature level 11.1 or 11.0.
fn create(
    adapter: &IDXGIAdapter1,
    flags: D3D11_CREATE_DEVICE_FLAG,
) -> windows::core::Result<(ID3D11Device, ID3D11DeviceContext)> {
    match create_at(
        adapter,
        flags,
        &[D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0],
    ) {
        // A runtime that does not know 11.1 refuses the list whole.
        Err(e) if e.code() == E_INVALIDARG => create_at(adapter, flags, &[D3D_FEATURE_LEVEL_11_0]),
        made => made,
    }
}

fn create_at(
    adapter: &IDXGIAdapter1,
    flags: D3D11_CREATE_DEVICE_FLAG,
    levels: &[D3D_FEATURE_LEVEL],
) -> windows::core::Result<(ID3D11Device, ID3D11DeviceContext)> {
    let mut device = None;
    let mut context = None;
    // SAFETY: an adapter of ours, so the driver type is UNKNOWN as the
    // call requires; the device and context come back owned.
    unsafe {
        D3D11CreateDevice(
            adapter,
            D3D_DRIVER_TYPE_UNKNOWN,
            HMODULE::default(),
            flags,
            Some(levels),
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )
    }?;
    match (device, context) {
        (Some(device), Some(context)) => Ok((device, context)),
        _ => Err(E_POINTER.into()),
    }
}

/// Puts the device's work first on the card, keeps one frame queued at
/// most, and lets encoders use it from their own threads, giving back
/// the lock that makes this safe. A refusal costs smoothness, not the
/// session: it is said, and the device used.
fn tune(device: &ID3D11Device, log: &Log) -> Option<ID3D11Multithread> {
    let priority = device.cast::<IDXGIDevice>().and_then(|dxgi| {
        // SAFETY: a setter on a live device, with a value in its range.
        unsafe { dxgi.SetGPUThreadPriority(GPU_PRIORITY) }
    });
    if let Err(e) = priority {
        log.write(&failed("raising the device's priority on the card", &e));
    }
    let latency = device.cast::<IDXGIDevice1>().and_then(|dxgi| {
        // SAFETY: a setter on a live device.
        unsafe { dxgi.SetMaximumFrameLatency(1) }
    });
    if let Err(e) = latency {
        log.write(&failed("keeping one frame queued at most", &e));
    }
    match device.cast::<ID3D11Multithread>() {
        Ok(multithread) => {
            // SAFETY: a setter on a live device; what it returns is the
            // state before, of no use here.
            let _ = unsafe { multithread.SetMultithreadProtected(true) };
            Some(multithread)
        }
        Err(e) => {
            log.write(&failed("protecting the device between threads", &e));
            None
        }
    }
}

/// The device's lock, held for as long as this lives.
///
/// Each call on a protected device is whole, but a picture takes many:
/// the lock keeps an encoder's thread from slipping its own between two
/// of them and changing what is bound. Never held across a wait on
/// anything but the card.
pub(super) struct Locked<'a> {
    lock: Option<&'a ID3D11Multithread>,
}

impl<'a> Locked<'a> {
    pub(super) fn take(lock: Option<&'a ID3D11Multithread>) -> Self {
        if let Some(lock) = lock {
            // SAFETY: the lock of a live device, left in drop on this same
            // thread; it may be taken again by the thread that holds it.
            unsafe { lock.Enter() };
        }
        Self { lock }
    }
}

impl Drop for Locked<'_> {
    fn drop(&mut self) {
        if let Some(lock) = self.lock {
            // SAFETY: entered in `take` by this thread.
            unsafe { lock.Leave() };
        }
    }
}

/// A name Windows wrote into a fixed array, up to its nought.
pub(super) fn wide(letters: &[u16]) -> String {
    let end = letters
        .iter()
        .position(|letter| *letter == 0)
        .unwrap_or(letters.len());
    String::from_utf16_lossy(&letters[..end])
}
