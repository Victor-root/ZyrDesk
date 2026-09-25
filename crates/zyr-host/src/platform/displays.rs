//! The screens that can be filmed, by the names the rest of the product
//! knows them by.
//!
//! A screen's id is the monitor's own device path, the one
//! `zyr_screen::arrangement` notes screens under: it survives a restart
//! and a replug, where the `\\.\DISPLAYn` name moves with whatever else
//! is plugged in. Windows gives none for some screens; the GDI name
//! stands in then. The name shown is what the monitor calls itself,
//! which is also how the virtual screen is recognised.

use windows::Win32::Graphics::Dxgi::IDXGIFactory1;
use zyr_media::service::Display;
use zyr_proto::log::Log;

use super::device::{Output, outputs};

/// A screen that can be filmed, and where DXGI has it.
pub(super) struct Filmable {
    pub(super) display: Display,
    pub(super) output: Output,
}

/// Every screen on the desktop now.
pub(super) fn filmable(factory: &IDXGIFactory1, log: &Log) -> Vec<Filmable> {
    let seats = zyr_screen::arrangement::as_it_stands();
    let names = zyr_screen::desktop::how_the_screens_introduce_themselves();
    outputs(factory, log)
        .into_iter()
        .map(|output| {
            let gdi = output.gdi_name();
            let seat = seats.iter().find(|seat| seat.adapter == gdi);
            let id = seat
                .map(|seat| seat.screen.clone())
                .filter(|screen| !screen.is_empty())
                .unwrap_or_else(|| gdi.clone());
            let name = names
                .iter()
                .find(|(named, _)| *named == gdi)
                .map(|(_, name)| name.clone())
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| gdi.clone());
            let area = output.desc.DesktopCoordinates;
            // Where Windows says nothing, the main screen is the one at
            // the desktop's origin.
            let main = seat.map_or(area.left == 0 && area.top == 0, |seat| seat.main);
            Filmable {
                display: Display {
                    id,
                    main,
                    width: area.right.saturating_sub(area.left).unsigned_abs(),
                    height: area.bottom.saturating_sub(area.top).unsigned_abs(),
                    name,
                },
                output,
            }
        })
        .collect()
}

/// The screen whose id is `wanted`, else the main one, else the first.
pub(super) fn chosen(screens: Vec<Filmable>, wanted: &str) -> Option<Filmable> {
    let at = screens
        .iter()
        .position(|screen| !wanted.is_empty() && screen.display.id == wanted)
        .or_else(|| screens.iter().position(|screen| screen.display.main))
        .or((!screens.is_empty()).then_some(0))?;
    screens.into_iter().nth(at)
}
