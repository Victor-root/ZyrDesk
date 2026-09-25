//! The screen, through Desktop Duplication.
//!
//! Windows hands each new image of a screen as a texture of the card the
//! screen is plugged into. It is copied at once into a texture of ours,
//! which the conversion draws from as often as needed (a key frame asked
//! for, a still screen sent again), and Windows gets its own back before
//! the next wait. The pointer comes separately: where it is, and its
//! shape when it changes.
//!
//! Duplication stops whenever the desktop switches (Ctrl+Alt+Del, the
//! lock screen, an administrator prompt) or a screen changes its mode.
//! It is taken up again on the new desktop, trying every 5 ms for the
//! first 400 ms and every 25 ms after, and the log says how long the
//! picture was frozen. A screen that has vanished is waited for 3 s, for
//! a mode change takes it out of the lists for a moment; after that, the
//! main screen is filmed instead.

use std::thread;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::E_ACCESSDENIED;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_SHADER_RESOURCE, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT,
    ID3D11ShaderResourceView, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_ADAPTER_DESC1, DXGI_ERROR_ACCESS_DENIED, DXGI_ERROR_ACCESS_LOST,
    DXGI_ERROR_DEVICE_REMOVED, DXGI_ERROR_DEVICE_RESET, DXGI_ERROR_NOT_CURRENTLY_AVAILABLE,
    DXGI_ERROR_SESSION_DISCONNECTED, DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO,
    DXGI_OUTDUPL_POINTER_SHAPE_INFO, DXGI_OUTDUPL_POINTER_SHAPE_TYPE_COLOR,
    DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MASKED_COLOR, DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MONOCHROME,
    IDXGIAdapter1, IDXGIFactory1, IDXGIOutput1, IDXGIOutput5, IDXGIOutputDuplication,
    IDXGIResource,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};
use windows::core::{Interface, w};
use zyr_codec::{Frame, GpuVendor, Input, VideoEncoder};
use zyr_media::service::Display;
use zyr_proto::log::Log;

use super::convert::{Converter, PointerImages, Scene};
use super::desktop::InputDesktop;
use super::device::{Device, Output};
use super::displays::{Filmable, chosen, filmable};
use super::tuning::ThreadTask;
use super::{Counter, failed};
use crate::parts::{Aimed, Captured, Drawing, Feed, Screen, ScreenError};
use crate::picture::{Rect, Size};
use crate::pointer::{Kind, shape};

/// How often duplication is tried again at first after it stopped, and
/// for how long.
const QUICKLY: Duration = Duration::from_millis(5);
const QUICK_FOR: Duration = Duration::from_millis(400);

/// How often after that.
const SLOWLY: Duration = Duration::from_millis(25);

/// How long a screen that vanished is waited for before the main one is
/// filmed instead.
const VANISHED_FOR: Duration = Duration::from_secs(3);

/// How often a capture that does not come back is said to the log.
const SAY_EVERY: Duration = Duration::from_secs(10);

/// While duplication is stopped.
struct Lost {
    since: Instant,
    next_try: Instant,
    /// Since when the screen has been missing from the lists.
    missing_since: Option<Instant>,
    /// Why the last try failed, and when that was last said.
    last_refusal: String,
    said: Instant,
}

impl Lost {
    fn now(why: String) -> Self {
        let now = Instant::now();
        Self {
            since: now,
            next_try: now,
            missing_since: None,
            last_refusal: why,
            said: now,
        }
    }
}

/// The screen being filmed.
struct Filming {
    display: Display,
    /// Where DXGI has it, to duplicate it again without listing the
    /// screens while they have not changed.
    output: Output,
    area: Rect,
    /// Quarter turns of the screen, clockwise.
    rotation: u32,
    duplication: Option<IDXGIOutputDuplication>,
}

/// The latest image, ours to draw from as often as needed.
struct Latest {
    texture: ID3D11Texture2D,
    view: ID3D11ShaderResourceView,
    width: u32,
    height: u32,
    format: DXGI_FORMAT,
}

/// The pointer, as Windows last said.
#[derive(Default)]
struct Pointer {
    x: i32,
    y: i32,
    visible: bool,
    images: Option<(u32, u32, PointerImages)>,
    /// Where its shape is read into, kept from one change to the next.
    bytes: Vec<u8>,
}

pub(super) struct DuplicatedScreen {
    log: Log,
    counter: Counter,
    desktop: InputDesktop,
    factory: IDXGIFactory1,
    device: Device,
    converter: Converter,
    /// Bumped each time the device is made again.
    generation: u64,
    /// Whether duplicating the newer way was refused and said so.
    newer_refused: bool,
    filming: Option<Filming>,
    lost: Option<Lost>,
    latest: Option<Latest>,
    pointer: Pointer,
    /// Last, so that the thread leaves its class once all the rest is
    /// let go of.
    _task: ThreadTask,
}

impl DuplicatedScreen {
    /// Made on the thread that captures, which it attaches to the input
    /// desktop and puts in the multimedia class for capture.
    pub(super) fn new(log: Log) -> Result<Self, ScreenError> {
        let task = ThreadTask::join(w!("Capture"), &log);
        let mut desktop = InputDesktop::new();
        desktop.follow_saying(&log);
        let factory = new_factory()?;
        // The card of the main screen, or the first card when no screen
        // is on: the encoders are tried on it while nobody watches yet.
        let screens = filmable(&factory, &log);
        let device = match chosen(screens, "") {
            Some(screen) => Device::on(&screen.output.adapter, &screen.output.card, &log),
            None => {
                first_card(&factory).and_then(|(adapter, card)| Device::on(&adapter, &card, &log))
            }
        }
        .map_err(|e| trouble(&log, e))?;
        let converter =
            Converter::new(&device.device, &device.context).map_err(|e| trouble(&log, e))?;
        if !converter.renders_nv12() {
            log.write(&format!(
                "{} cannot draw into NV12 textures: pictures go to the encoders through memory",
                device.name
            ));
        }
        Ok(Self {
            log,
            counter: Counter::new(),
            desktop,
            factory,
            device,
            converter,
            generation: 1,
            newer_refused: false,
            filming: None,
            lost: None,
            latest: None,
            pointer: Pointer::default(),
            _task: task,
        })
    }

    /// The screens now, from a list of DXGI's that is up to date.
    fn screens(&mut self) -> Vec<Filmable> {
        // SAFETY: a question to a live factory.
        if !unsafe { self.factory.IsCurrent() }.as_bool() {
            match new_factory() {
                Ok(factory) => self.factory = factory,
                Err(e) => self.log.write(&e.0),
            }
        }
        filmable(&self.factory, &self.log)
    }

    /// Films `screen`: on its own card, which the device moves to if need
    /// be. Duplication is tried for `patience`, and else left stopped for
    /// `wait` to take up; why it failed is given back.
    fn film(
        &mut self,
        screen: Filmable,
        patience: Duration,
    ) -> Result<Option<String>, ScreenError> {
        let output = &screen.output;
        if output.card.AdapterLuid != self.device.card {
            self.remake_device(output)
                .map_err(|e| trouble(&self.log, e))?;
        }
        // Read again: a screen that changed its mode is somewhere else now.
        // SAFETY: a getter on a live output.
        let area = unsafe { output.output.GetDesc() }
            .map_or(output.desc.DesktopCoordinates, |desc| {
                desc.DesktopCoordinates
            });
        let gdi = output.gdi_name();
        let started = Instant::now();
        let duplicated = loop {
            match self.duplicate(output) {
                Ok(duplication) => break Ok(duplication),
                Err(_) if started.elapsed() < patience => thread::sleep(QUICKLY),
                Err(e) => break Err(failed(&format!("duplicating {gdi}"), &e)),
            }
        };
        let (duplication, refused) = match duplicated {
            Ok(duplication) => {
                self.lost = None;
                (Some(duplication), None)
            }
            Err(why) => {
                match &mut self.lost {
                    Some(lost) => lost.last_refusal.clone_from(&why),
                    None => self.lost = Some(Lost::now(why.clone())),
                }
                (None, Some(why))
            }
        };
        let rotation = duplication.as_ref().map_or(0, |duplication| {
            // SAFETY: a getter on a live duplication.
            let desc = unsafe { duplication.GetDesc() };
            quarter_turns(desc.Rotation.0)
        });
        let area = Rect::new(
            area.left,
            area.top,
            area.right.saturating_sub(area.left).unsigned_abs(),
            area.bottom.saturating_sub(area.top).unsigned_abs(),
        );
        let display = Display {
            width: area.width,
            height: area.height,
            ..screen.display
        };
        self.filming = Some(Filming {
            display,
            output: screen.output,
            area,
            rotation,
            duplication,
        });
        Ok(refused)
    }

    /// Duplicates a screen, with the formats the conversion reads, or the
    /// older way where the newer one is not there.
    fn duplicate(&mut self, output: &Output) -> windows::core::Result<IDXGIOutputDuplication> {
        self.desktop.follow_saying(&self.log);
        if let Ok(output5) = output.output.cast::<IDXGIOutput5>() {
            // SAFETY: a device of the card the screen is plugged into, and
            // a list of one format.
            match unsafe {
                output5.DuplicateOutput1(&self.device.device, 0, &[DXGI_FORMAT_B8G8R8A8_UNORM])
            } {
                Ok(duplication) => return Ok(duplication),
                // Refusals of a moment, which the older call would meet
                // as well.
                Err(e) if losing(e.code()) => return Err(e),
                Err(e) => {
                    if !self.newer_refused {
                        self.newer_refused = true;
                        self.log.write(&failed(
                            "duplicating a screen the newer way (the older is used)",
                            &e,
                        ));
                    }
                }
            }
        }
        let output1: IDXGIOutput1 = output.output.cast()?;
        // SAFETY: a device of the card the screen is plugged into.
        unsafe { output1.DuplicateOutput(&self.device.device) }
    }

    /// What is filmed now, as the engine hears it.
    fn aimed(&self) -> Option<Aimed> {
        let filming = self.filming.as_ref()?;
        Some(Aimed {
            display: filming.display.clone(),
            area: filming.area,
            desktop: desktop(),
            device: self.generation,
        })
    }

    /// Duplication stopped: why, said once.
    fn lose(&mut self, e: &windows::core::Error) {
        let Some(filming) = &mut self.filming else {
            return;
        };
        filming.duplication = None;
        let why = failed(
            &format!(
                "capturing {} (desktop {})",
                filming.output.gdi_name(),
                self.desktop.name()
            ),
            e,
        );
        self.log.write(&why);
        self.lost = Some(Lost::now(why));
    }

    /// Takes duplication up again, until `until` at the latest.
    fn recover(&mut self, until: Instant) -> Result<Captured, ScreenError> {
        loop {
            let now = Instant::now();
            let Some(lost) = &mut self.lost else {
                return Ok(Captured::Nothing);
            };
            if now.duration_since(lost.said) >= SAY_EVERY {
                lost.said = now;
                self.log.write(&format!(
                    "capture still stopped after {} s: {}",
                    now.duration_since(lost.since).as_secs(),
                    lost.last_refusal
                ));
            }
            let (since, next_try) = (lost.since, lost.next_try);
            if now >= next_try {
                if let Some(captured) = self.try_again(since)? {
                    return Ok(captured);
                }
                if let Some(lost) = &mut self.lost {
                    let pace = if since.elapsed() < QUICK_FOR {
                        QUICKLY
                    } else {
                        SLOWLY
                    };
                    lost.next_try = Instant::now() + pace;
                }
                continue;
            }
            if now >= until {
                return Ok(Captured::Nothing);
            }
            thread::sleep(next_try.min(until).saturating_duration_since(now));
        }
    }

    /// One try at taking duplication up again: what to tell the engine
    /// if it worked.
    fn try_again(&mut self, since: Instant) -> Result<Option<Captured>, ScreenError> {
        let before = self.aimed();
        // SAFETY: a question to a live device.
        if let Err(e) = unsafe { self.device.device.GetDeviceRemovedReason() } {
            self.log.write(&failed("the Direct3D device", &e));
            self.device_lost();
        }
        let Some(filming) = &self.filming else {
            return Ok(None);
        };
        // The same screen on the same card, while DXGI's lists hold: after
        // a desktop switch, what was filmed is filmed again straight away.
        // SAFETY: a question to a live factory.
        let screen = if unsafe { self.factory.IsCurrent() }.as_bool()
            && filming.output.card.AdapterLuid == self.device.card
        {
            Filmable {
                display: filming.display.clone(),
                output: filming.output.clone(),
            }
        } else {
            match self.screen_again(&filming.display.id.clone()) {
                Some(screen) => screen,
                None => return Ok(None),
            }
        };
        if self.film(screen, Duration::ZERO)?.is_some() {
            return Ok(None);
        }
        self.log.write(&format!(
            "capture back after {} ms, on the desktop {}",
            since.elapsed().as_millis(),
            self.desktop.name()
        ));
        Ok(Some(match self.aimed() {
            Some(aimed) if Some(&aimed) != before.as_ref() => Captured::Moved(aimed),
            _ => Captured::Nothing,
        }))
    }

    /// The screen `id` in DXGI's lists made again, or the main screen
    /// once it has been gone for long enough.
    fn screen_again(&mut self, id: &str) -> Option<Filmable> {
        let mut screens = self.screens();
        match screens.iter().position(|screen| screen.display.id == id) {
            Some(at) => {
                if let Some(lost) = &mut self.lost {
                    lost.missing_since = None;
                }
                Some(screens.swap_remove(at))
            }
            None => {
                let now = Instant::now();
                let missing_since = self
                    .lost
                    .as_mut()
                    .map_or(now, |lost| *lost.missing_since.get_or_insert(now));
                if now.duration_since(missing_since) < VANISHED_FOR {
                    return None;
                }
                let main = chosen(screens, "")?;
                self.log.write(&format!(
                    "the screen {id} has been gone for {} s: filming {} ({}) instead",
                    VANISHED_FOR.as_secs(),
                    main.display.name,
                    main.display.id
                ));
                Some(main)
            }
        }
    }

    /// The device is gone (a driver update, a card reset): a new one on
    /// the same card.
    fn device_lost(&mut self) {
        let Some(filming) = &self.filming else {
            return;
        };
        let output = filming.output.clone();
        if let Err(e) = self.remake_device(&output) {
            self.log.write(&e);
        }
    }

    /// A new device, on the card of `output`: everything made on the one
    /// before is let go of.
    fn remake_device(&mut self, output: &Output) -> Result<(), String> {
        let device = Device::on(&output.adapter, &output.card, &self.log)?;
        let converter = Converter::new(&device.device, &device.context)?;
        self.device = device;
        self.converter = converter;
        self.generation += 1;
        self.latest = None;
        self.pointer.images = None;
        Ok(())
    }

    /// Takes in a frame Windows handed over: the image, the pointer.
    fn take(
        &mut self,
        info: &DXGI_OUTDUPL_FRAME_INFO,
        resource: Option<IDXGIResource>,
    ) -> Result<Captured, ScreenError> {
        let presented = info.LastPresentTime != 0;
        if presented && let Some(resource) = resource {
            let texture: ID3D11Texture2D = resource
                .cast()
                .map_err(|e| trouble(&self.log, failed("reading the screen's image", &e)))?;
            self.keep(&texture)?;
        }
        if info.LastMouseUpdateTime != 0 {
            let position = info.PointerPosition;
            self.pointer.x = position.Position.x;
            self.pointer.y = position.Position.y;
            self.pointer.visible = position.Visible.as_bool();
        }
        if info.PointerShapeBufferSize > 0 {
            self.read_pointer(info.PointerShapeBufferSize);
        }
        Ok(if presented {
            Captured::Image {
                at: self.counter.instant(info.LastPresentTime),
            }
        } else if info.LastMouseUpdateTime != 0 || info.PointerShapeBufferSize > 0 {
            Captured::Pointer
        } else {
            Captured::Nothing
        })
    }

    /// Copies the image into ours.
    fn keep(&mut self, texture: &ID3D11Texture2D) -> Result<(), ScreenError> {
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        // SAFETY: a getter on a live texture.
        unsafe { texture.GetDesc(&mut desc) };
        let fits = self.latest.as_ref().is_some_and(|latest| {
            (latest.width, latest.height, latest.format) == (desc.Width, desc.Height, desc.Format)
        });
        if !fits {
            self.latest = Some(latest(&self.device, &desc).map_err(|e| trouble(&self.log, e))?);
        }
        if let Some(latest) = &self.latest {
            // SAFETY: two textures of this device, of the same size and
            // format; Windows' own is held until the frame is released.
            unsafe { self.device.context.CopyResource(&latest.texture, texture) };
        }
        Ok(())
    }

    /// Reads the pointer's new shape.
    fn read_pointer(&mut self, size: u32) {
        let Some(duplication) = self
            .filming
            .as_ref()
            .and_then(|filming| filming.duplication.clone())
        else {
            return;
        };
        self.pointer.bytes.resize(size as usize, 0);
        let mut required = 0;
        let mut info = DXGI_OUTDUPL_POINTER_SHAPE_INFO::default();
        // SAFETY: the buffer holds `size` bytes, as said to the call.
        let read = unsafe {
            duplication.GetFramePointerShape(
                size,
                self.pointer.bytes.as_mut_ptr().cast(),
                &mut required,
                &mut info,
            )
        };
        if let Err(e) = read {
            self.log.write(&failed("reading the pointer's shape", &e));
            return;
        }
        let kind = match info.Type {
            t if t == DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MONOCHROME.0 as u32 => Kind::Monochrome,
            t if t == DXGI_OUTDUPL_POINTER_SHAPE_TYPE_COLOR.0 as u32 => Kind::Color,
            t if t == DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MASKED_COLOR.0 as u32 => Kind::MaskedColor,
            other => {
                self.log.write(&format!(
                    "a pointer of an unknown kind ({other}) is not drawn"
                ));
                self.pointer.images = None;
                return;
            }
        };
        let bytes = &self.pointer.bytes[..(required as usize).min(self.pointer.bytes.len())];
        let Some(shape) = shape(kind, info.Width, info.Height, info.Pitch, bytes) else {
            self.log.write(&format!(
                "a pointer of {}x{} that its {} bytes cannot hold is not drawn",
                info.Width,
                info.Height,
                bytes.len()
            ));
            self.pointer.images = None;
            return;
        };
        match self.converter.pointer(&shape) {
            Ok(images) => self.pointer.images = Some((shape.width, shape.height, images)),
            Err(e) => {
                self.log.write(&e);
                self.pointer.images = None;
            }
        }
    }
}

impl Screen for DuplicatedScreen {
    fn displays(&mut self) -> Vec<Display> {
        self.screens()
            .into_iter()
            .map(|screen| screen.display)
            .collect()
    }

    fn aim(&mut self, display: &str) -> Result<Aimed, ScreenError> {
        let screens = self.screens();
        let screen = chosen(screens, display).ok_or_else(|| {
            ScreenError("Aucun écran n'est allumé sur l'ordinateur d'en face.".to_string())
        })?;
        if let Some(why) = self.film(screen, QUICK_FOR)? {
            self.log
                .write(&format!("{why}; trying again while the session goes on"));
        }
        self.aimed().ok_or_else(|| {
            ScreenError("L'écran de l'ordinateur d'en face ne peut pas être filmé.".to_string())
        })
    }

    fn encoder_input(&self) -> Input {
        if self.converter.renders_nv12() {
            Input::D3d11 {
                device: self.device.device.clone(),
            }
        } else {
            Input::Cpu
        }
    }

    fn vendor(&self) -> GpuVendor {
        self.device.vendor
    }

    fn wait(&mut self, until: Instant) -> Result<Captured, ScreenError> {
        let Some(duplication) = self
            .filming
            .as_ref()
            .map(|filming| filming.duplication.clone())
        else {
            thread::sleep(until.saturating_duration_since(Instant::now()));
            return Ok(Captured::Nothing);
        };
        let Some(duplication) = duplication else {
            return self.recover(until);
        };
        let timeout = until
            .saturating_duration_since(Instant::now())
            .as_millis()
            .min(u128::from(u32::MAX)) as u32;
        let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut resource = None;
        // SAFETY: plain out values; the frame acquired is released below
        // whatever happens to it.
        let acquired = unsafe { duplication.AcquireNextFrame(timeout, &mut info, &mut resource) };
        match acquired {
            Ok(()) => {
                let taken = self.take(&info, resource);
                // SAFETY: the frame acquired above.
                if let Err(e) = unsafe { duplication.ReleaseFrame() } {
                    self.lose(&e);
                }
                taken
            }
            Err(e) if e.code() == DXGI_ERROR_WAIT_TIMEOUT => Ok(Captured::Nothing),
            Err(e) if losing(e.code()) => {
                self.lose(&e);
                self.recover(until)
            }
            Err(e) => {
                self.lose(&e);
                Err(trouble(&self.log, failed("waiting for the screen", &e)))
            }
        }
    }

    fn draw(
        &mut self,
        encoder: &VideoEncoder,
        feed: Feed,
        drawing: &Drawing,
    ) -> Result<Frame, ScreenError> {
        let rotation = self.filming.as_ref().map_or(0, |filming| filming.rotation);
        let area = self
            .filming
            .as_ref()
            .map_or(Size::new(1, 1), |filming| filming.area.size());
        let pointer = self
            .pointer
            .images
            .as_ref()
            .and_then(|(width, height, images)| {
                let shown = drawing.pointer && self.pointer.visible;
                let (w, h) = (area.width.max(1) as f32, area.height.max(1) as f32);
                let (x, y) = (self.pointer.x as f32, self.pointer.y as f32);
                shown.then_some((
                    images,
                    [
                        x / w,
                        y / h,
                        (x + *width as f32) / w,
                        (y + *height as f32) / h,
                    ],
                ))
            });
        let scene = Scene {
            image: self.latest.as_ref().map(|latest| &latest.view),
            rotation,
            pointer,
            placement: drawing.placement,
        };
        match feed {
            Feed::Texture => {
                let frame = encoder
                    .frame_for_gpu()
                    .map_err(|e| trouble(&self.log, e.to_string()))?;
                self.converter
                    .draw_into_texture(&scene, frame.texture())
                    .map_err(|e| trouble(&self.log, e))?;
                Ok(Frame::Gpu(frame))
            }
            Feed::Memory => {
                let mut frame = encoder
                    .frame_for_cpu()
                    .map_err(|e| trouble(&self.log, e.to_string()))?;
                let picture = Size::new(frame.width(), frame.height());
                self.converter
                    .draw_into_memory(&scene, picture, &mut frame.planes())
                    .map_err(|e| trouble(&self.log, e))?;
                Ok(Frame::Cpu(frame))
            }
        }
    }
}

/// Refusals that end duplication: it is taken up again.
fn losing(code: windows::core::HRESULT) -> bool {
    [
        DXGI_ERROR_ACCESS_LOST,
        DXGI_ERROR_ACCESS_DENIED,
        DXGI_ERROR_DEVICE_REMOVED,
        DXGI_ERROR_DEVICE_RESET,
        DXGI_ERROR_SESSION_DISCONNECTED,
        DXGI_ERROR_NOT_CURRENTLY_AVAILABLE,
        E_ACCESSDENIED,
    ]
    .contains(&code)
}

/// Windows' rotation of a screen, in quarter turns clockwise.
fn quarter_turns(rotation: i32) -> u32 {
    // IDENTITY is 1, ROTATE90 2, ROTATE180 3, ROTATE270 4; UNSPECIFIED 0.
    u32::try_from(rotation - 1).unwrap_or(0).min(3)
}

/// The desktop spanning every screen, in pixels.
fn desktop() -> Rect {
    // SAFETY: plain questions.
    unsafe {
        Rect::new(
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN).unsigned_abs(),
            GetSystemMetrics(SM_CYVIRTUALSCREEN).unsigned_abs(),
        )
    }
}

fn new_factory() -> Result<IDXGIFactory1, ScreenError> {
    // SAFETY: a plain constructor.
    unsafe { CreateDXGIFactory1() }.map_err(|e| {
        ScreenError(format!(
            "Les écrans de l'ordinateur d'en face ne peuvent pas être listés ({}).",
            failed("creating a DXGI factory", &e)
        ))
    })
}

/// The first graphics card, for when no screen is on.
fn first_card(factory: &IDXGIFactory1) -> Result<(IDXGIAdapter1, DXGI_ADAPTER_DESC1), String> {
    // SAFETY: a plain index and a getter on what it gives.
    unsafe {
        let adapter = factory
            .EnumAdapters1(0)
            .map_err(|e| failed("finding a graphics card", &e))?;
        let card = adapter
            .GetDesc1()
            .map_err(|e| failed("describing the graphics card", &e))?;
        Ok((adapter, card))
    }
}

/// Our copy of the latest image, of the size and format Windows hands.
fn latest(device: &Device, handed: &D3D11_TEXTURE2D_DESC) -> Result<Latest, String> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: handed.Width,
        Height: handed.Height,
        MipLevels: 1,
        ArraySize: 1,
        Format: handed.Format,
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
    // SAFETY: a complete description; the texture comes back owned.
    unsafe {
        device
            .device
            .CreateTexture2D(&desc, None, Some(&mut texture))
    }
    .map_err(|e| failed("creating the copy of the screen's image", &e))?;
    let texture = texture.ok_or("the copy of the screen's image came back empty")?;
    let mut view = None;
    // SAFETY: a texture of this device made to be sampled.
    unsafe {
        device
            .device
            .CreateShaderResourceView(&texture, None, Some(&mut view))
    }
    .map_err(|e| failed("sampling the copy of the screen's image", &e))?;
    Ok(Latest {
        texture,
        view: view.ok_or("no view of the copy of the screen's image")?,
        width: handed.Width,
        height: handed.Height,
        format: handed.Format,
    })
}

/// Said to the log, and in a sentence for the viewer.
fn trouble(log: &Log, e: String) -> ScreenError {
    log.write(&e);
    ScreenError(format!(
        "La capture de l'écran de l'ordinateur d'en face a échoué : {e}"
    ))
}
