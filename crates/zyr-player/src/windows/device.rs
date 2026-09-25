//! The Direct3D 11 device the player decodes and draws with.
//!
//! Made on the graphics card that drives the screen the window is on,
//! so that pictures go from the decoder to the screen without crossing
//! from one card to another; any other card only if that one refuses.
//! Video support for the decoder, BGRA support for drawing.

use windows::Win32::Foundation::{HMODULE, HWND};
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_CREATE_DEVICE_VIDEO_SUPPORT, D3D11_SDK_VERSION,
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory2, DXGI_ADAPTER_DESC1, DXGI_ADAPTER_FLAG_SOFTWARE, DXGI_CREATE_FACTORY_FLAGS,
    DXGI_ERROR_NOT_FOUND, IDXGIAdapter1, IDXGIFactory2,
};
use windows::Win32::Graphics::Gdi::{HMONITOR, MONITOR_DEFAULTTONEAREST, MonitorFromWindow};
use zyr_proto::log::Log;

use super::failure;

/// Intel's PCI vendor number.
pub const INTEL: u32 = 0x8086;

/// A device, with what made it and whose card it is on.
pub struct Device {
    /// The factory of the device's card: the swap chain has to come from
    /// it.
    pub factory: IDXGIFactory2,
    pub device: ID3D11Device,
    pub context: ID3D11DeviceContext,
    /// The card maker's PCI number.
    pub vendor: u32,
    pub level: D3D_FEATURE_LEVEL,
}

/// Makes the device for drawing into `hwnd`.
pub fn create(hwnd: HWND, log: &Log) -> Result<Device, String> {
    // SAFETY: a factory asked for with no flag.
    let factory: IDXGIFactory2 = unsafe { CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0)) }
        .map_err(|e| failure("CreateDXGIFactory2", &e))?;
    // SAFETY: any window handle gets an answer, the nearest screen.
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };

    let mut cards = Vec::new();
    for index in 0.. {
        // SAFETY: enumerating the factory's cards, which ends with
        // DXGI_ERROR_NOT_FOUND.
        let adapter = match unsafe { factory.EnumAdapters1(index) } {
            Ok(adapter) => adapter,
            Err(e) if e.code() == DXGI_ERROR_NOT_FOUND => break,
            Err(e) => {
                log.write(&failure(&format!("EnumAdapters1({index})"), &e));
                break;
            }
        };
        // SAFETY: a getter on a card just obtained.
        let description = match unsafe { adapter.GetDesc1() } {
            Ok(description) => description,
            Err(e) => {
                log.write(&failure(&format!("GetDesc1 of card {index}"), &e));
                continue;
            }
        };
        // The processor pretending to be a card decodes nothing.
        if description.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
            continue;
        }
        let drives = drives(&adapter, monitor);
        cards.push((adapter, description, drives));
    }
    // The card driving the window's screen first, the others after in
    // Windows' order.
    cards.sort_by_key(|(_, _, drives)| !drives);

    let mut refused = Vec::new();
    for (adapter, description, drives) in cards {
        let name = name(&description);
        let mut device = None;
        let mut context = None;
        let mut level = D3D_FEATURE_LEVEL::default();
        // SAFETY: a card of this factory, a list of levels that outlives
        // the call, and out-parameters of ours.
        let made = unsafe {
            D3D11CreateDevice(
                &adapter,
                D3D_DRIVER_TYPE_UNKNOWN,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_VIDEO_SUPPORT | D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&[D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0]),
                D3D11_SDK_VERSION,
                Some(&mut device),
                Some(&mut level),
                Some(&mut context),
            )
        };
        match (made, device, context) {
            (Ok(()), Some(device), Some(context)) => {
                log.write(&format!(
                    "drawing with {name} (vendor {:#06x}, device {:#06x}, level {:#x}){}",
                    description.VendorId,
                    description.DeviceId,
                    level.0,
                    if drives {
                        ", the card of the window's screen"
                    } else {
                        ", not the card of the window's screen"
                    }
                ));
                return Ok(Device {
                    factory,
                    device,
                    context,
                    vendor: description.VendorId,
                    level,
                });
            }
            (Err(e), _, _) => {
                let text = failure(&format!("D3D11CreateDevice on {name}"), &e);
                log.write(&text);
                refused.push(text);
            }
            (Ok(()), _, _) => {
                let text = format!("D3D11CreateDevice on {name} gave no device");
                log.write(&text);
                refused.push(text);
            }
        }
    }
    Err(if refused.is_empty() {
        "Aucune carte graphique de cet ordinateur ne peut afficher l'image.".to_string()
    } else {
        format!(
            "Aucune carte graphique de cet ordinateur ne peut afficher l'image ({}).",
            refused.join(" ; ")
        )
    })
}

/// Whether one of the card's outputs is that screen.
fn drives(adapter: &IDXGIAdapter1, monitor: HMONITOR) -> bool {
    // SAFETY: enumerating the card's outputs, which ends with an error.
    (0..)
        .map_while(|index| unsafe { adapter.EnumOutputs(index) }.ok())
        // SAFETY: a getter on an output just obtained.
        .any(|output| unsafe { output.GetDesc() }.is_ok_and(|desc| desc.Monitor == monitor))
}

fn name(description: &DXGI_ADAPTER_DESC1) -> String {
    let length = description
        .Description
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(description.Description.len());
    String::from_utf16_lossy(&description.Description[..length])
}
