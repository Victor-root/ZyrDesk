//! Where this computer's screens are, and how to put them back there.
//!
//! A session changes the screens of the computer being watched. Whatever
//! it changes, the person sitting in front of that computer gets their
//! desk back exactly as they left it when the session ends: which screens
//! were on, which were off, where each one sat relative to the others,
//! what size and rate it drew at, which way round it was turned, and
//! which of them Windows called the main one.
//!
//! That is a promise this product makes and keeps itself. The host engine
//! offers to do it and cannot be trusted with it: it puts back an
//! arrangement it noted at its own start, it gives up when something else
//! has moved a screen in the meantime, and what it does when it gives up
//! is switch every screen it can find back on. A screen its owner had
//! deliberately turned off came back at every start, and one that was on
//! stayed off. So the engine is told to leave the screens alone entirely,
//! and this file is what notes them and what puts them back.
//!
//! # Where this runs
//!
//! In the session that owns the screen, like the magnification beside it.
//! Everything Windows says about the arrangement of screens is answered
//! for the window station of whoever asks, and a service sits on one with
//! no screens at all: asked from there, there is nothing to note and
//! nothing to put back.
//!
//! # Why the old call and not the new one
//!
//! Windows has two ways to move screens around. The newer one describes a
//! whole desktop at once and is what the magnification's private message
//! rides on; the older one takes one screen at a time, writes each into
//! the registry without applying it, and then applies the lot in a single
//! step. The second is what this wants: an arrangement has to arrive all
//! at once, because half an arrangement is a desktop with two screens
//! sitting on top of each other, and Windows would be within its rights
//! to refuse the half that comes second.

use std::fmt;
use std::str::FromStr;

/// One screen of this computer, as it stood at some moment.
///
/// Carries both names it answers to, and they are not the same kind of
/// name. `adapter` is what Windows takes orders about, and it is only a
/// position in a list: unplug a monitor and the one after it moves up.
/// `screen` is the screen itself, and it survives everything short of
/// being plugged into another socket. Noting both is what lets an
/// arrangement be put back onto the screens it was taken from rather than
/// onto whatever now answers to the same list position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seat {
    /// What Windows takes orders about, `\\.\DISPLAY1` and its like.
    pub adapter: String,
    /// What this screen is, whoever it is plugged into. Empty when
    /// Windows would not say, which happens for a screen that is off.
    pub screen: String,
    /// Whether it was drawing at all. A screen that is off has no size
    /// and no place, and that is exactly the state this exists to
    /// remember: it is the one somebody chose on purpose and the one the
    /// engine kept undoing.
    pub on: bool,
    pub wide: u32,
    pub high: u32,
    /// Times a second, as Windows counts it.
    pub refresh: u32,
    /// Where its top left corner sits on the desktop, in pixels, counted
    /// from the main screen's own corner. This is the whole of « my
    /// screen 1 is on the right and screen 2 on the left ».
    pub at: (i32, i32),
    /// Which way round it is turned, in quarter turns clockwise.
    pub turned: u32,
    /// Whether Windows called this one the main screen.
    pub main: bool,
    /// How much larger than life it draws, in per cent. Nought when it
    /// could not be read, which is not worth failing anything over: a
    /// screen put back at the right size and the wrong magnification is
    /// most of the way home.
    pub scale: u32,
}

/// One spelling of a seat, written here and read here.
///
/// A line of `key=value` and not a shape from a library: this is written
/// to a file that outlives a session, and sometimes outlives the run of
/// the program that wrote it. It is read back by a later run, and by
/// whoever opens the file to find out why their screens came back wrong.
impl fmt::Display for Seat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "adapter={} screen={} on={} size={}x{} rate={} at={},{} turned={} main={} scale={}",
            self.adapter,
            if self.screen.is_empty() {
                "none"
            } else {
                &self.screen
            },
            if self.on { "yes" } else { "no" },
            self.wide,
            self.high,
            self.refresh,
            self.at.0,
            self.at.1,
            self.turned,
            if self.main { "yes" } else { "no" },
            self.scale,
        )
    }
}

impl FromStr for Seat {
    type Err = String;

    fn from_str(line: &str) -> Result<Self, Self::Err> {
        let mut adapter = None;
        let mut screen = String::new();
        let mut on = false;
        let mut size = None;
        let mut refresh = 0;
        let mut at = (0, 0);
        let mut turned = 0;
        let mut main = false;
        let mut scale = 0;
        for field in line.split_whitespace() {
            let Some((key, value)) = field.split_once('=') else {
                return Err(format!("« {field} » n'est pas clé=valeur"));
            };
            let number = |what: &str| {
                value
                    .parse::<u32>()
                    .map_err(|_| format!("{what} attendu en nombre : {value}"))
            };
            match key {
                "adapter" => adapter = Some(value.to_string()),
                "screen" => {
                    screen = if value == "none" {
                        String::new()
                    } else {
                        value.to_string()
                    }
                }
                "on" => on = value == "yes",
                "size" => {
                    let (wide, high) = value
                        .split_once('x')
                        .ok_or_else(|| format!("taille attendue LARGEURxHAUTEUR : {value}"))?;
                    size = Some((
                        wide.parse::<u32>().map_err(|_| "largeur".to_string())?,
                        high.parse::<u32>().map_err(|_| "hauteur".to_string())?,
                    ));
                }
                "rate" => refresh = number("cadence")?,
                "at" => {
                    let (x, y) = value
                        .split_once(',')
                        .ok_or_else(|| format!("place attendue X,Y : {value}"))?;
                    at = (
                        x.parse::<i32>().map_err(|_| "abscisse".to_string())?,
                        y.parse::<i32>().map_err(|_| "ordonnée".to_string())?,
                    );
                }
                "turned" => turned = number("quarts de tour")?,
                "main" => main = value == "yes",
                "scale" => scale = number("agrandissement")?,
                other => return Err(format!("« {other} » n'est pas un champ d'écran")),
            }
        }
        let adapter = adapter.ok_or_else(|| "aucun écran nommé sur la ligne".to_string())?;
        let (wide, high) = size.unwrap_or((0, 0));
        Ok(Seat {
            adapter,
            screen,
            on,
            wide,
            high,
            refresh,
            at,
            turned,
            main,
            scale,
        })
    }
}

/// Every screen of this computer, in the order Windows lists them.
///
/// One line each, so a file holding one is read by eye as easily as by
/// program. Blank lines and anything that will not read are skipped
/// rather than failing the whole: an arrangement that puts back three
/// screens out of four is worth more than one that puts back none.
pub fn written(seats: &[Seat]) -> String {
    seats
        .iter()
        .map(|seat| seat.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Reads back what [`written`] wrote.
pub fn read(text: &str) -> Vec<Seat> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| line.parse().ok())
        .collect()
}

#[cfg(windows)]
mod windows_only {
    use super::Seat;

    use windows_sys::Win32::Graphics::Gdi::{
        CDS_NORESET, CDS_SET_PRIMARY, CDS_UPDATEREGISTRY, ChangeDisplaySettingsExW, DEVMODEW,
        DISP_CHANGE_SUCCESSFUL, DISPLAY_DEVICE_ATTACHED_TO_DESKTOP, DISPLAY_DEVICE_PRIMARY_DEVICE,
        DISPLAY_DEVICEW, DM_BITSPERPEL, DM_DISPLAYFREQUENCY, DM_DISPLAYORIENTATION, DM_PELSHEIGHT,
        DM_PELSWIDTH, DM_POSITION, EDS_RAWMODE, ENUM_CURRENT_SETTINGS, EnumDisplayDevicesW,
        EnumDisplaySettingsExW,
    };

    /// What every one of this computer's screens is doing now.
    ///
    /// Screens that are off are in here too, and they are the point: a
    /// screen somebody turned off on purpose is the one thing an
    /// arrangement most needs to carry, and the one the engine kept
    /// switching back on.
    pub fn as_it_stands() -> Vec<Seat> {
        let mut seats = Vec::new();
        for index in 0.. {
            let mut adapter: DISPLAY_DEVICEW = unsafe { std::mem::zeroed() };
            adapter.cb = size_of::<DISPLAY_DEVICEW>() as u32;
            // SAFETY: the slot is ours with its size written in it as the
            // call requires, and no name is given so the list is the
            // machine's own adapters.
            if unsafe { EnumDisplayDevicesW(std::ptr::null(), index, &mut adapter, 0) } == 0 {
                break;
            }
            let name = super::read_wide(&adapter.DeviceName);
            if name.is_empty() {
                continue;
            }
            let on = adapter.StateFlags & DISPLAY_DEVICE_ATTACHED_TO_DESKTOP != 0;
            let main = adapter.StateFlags & DISPLAY_DEVICE_PRIMARY_DEVICE != 0;
            let mut mode: DEVMODEW = unsafe { std::mem::zeroed() };
            mode.dmSize = size_of::<DEVMODEW>() as u16;
            // SAFETY: a name the system just wrote and ended itself, and
            // a slot of ours carrying its own size.
            let read = unsafe {
                EnumDisplaySettingsExW(
                    adapter.DeviceName.as_ptr(),
                    ENUM_CURRENT_SETTINGS,
                    &mut mode,
                    0,
                )
            };
            let (wide, high, refresh, at, turned) = if read == 0 {
                (0, 0, 0, (0, 0), 0)
            } else {
                // SAFETY: the two halves of this union are told apart by
                // what the call was asked for, and a display mode is
                // always the second: the first describes paper.
                let placed = unsafe { mode.Anonymous1.Anonymous2 };
                (
                    mode.dmPelsWidth,
                    mode.dmPelsHeight,
                    mode.dmDisplayFrequency,
                    (placed.dmPosition.x, placed.dmPosition.y),
                    placed.dmDisplayOrientation,
                )
            };
            seats.push(Seat {
                screen: the_screen_itself(&adapter.DeviceName),
                scale: crate::magnify::of(&name).unwrap_or(0),
                adapter: name,
                on,
                wide,
                high,
                refresh,
                at,
                turned,
                main,
            });
        }
        seats
    }

    /// What the screen plugged into that adapter is, by a name that
    /// outlives a restart.
    ///
    /// Empty when Windows will not say, which is the ordinary answer for
    /// an adapter with nothing drawing on it. That costs nothing: an
    /// adapter with no screen on it is put back off, which is what it
    /// already is.
    fn the_screen_itself(adapter: &[u16]) -> String {
        let mut screen: DISPLAY_DEVICEW = unsafe { std::mem::zeroed() };
        screen.cb = size_of::<DISPLAY_DEVICEW>() as u32;
        // SAFETY: the name is one the system wrote and ended itself, and
        // the slot is ours with its size in it. Nought is the first
        // screen on that adapter, which is the only one there ever is.
        if unsafe { EnumDisplayDevicesW(adapter.as_ptr(), 0, &mut screen, 0) } == 0 {
            return String::new();
        }
        super::read_wide(&screen.DeviceID)
    }

    /// Puts an arrangement back, saying whether the screens are where
    /// they were and what happened in one sentence per screen.
    ///
    /// The answer is a word of its own and not a sentence to be searched
    /// for. Whoever asks has to know whether to try again, and reading
    /// that out of the last line cost an evening: a line added after it
    /// turned every successful return into a failure, and the watch that
    /// tries again did so every two seconds for as long as the service
    /// ran.
    ///
    /// What it answers about is the arrangement: which screens are on,
    /// where, and at what size. How large each draws is put back too, and
    /// is deliberately not part of the answer. A desk back in every
    /// respect but one number is a desk somebody can use, and it is not
    /// worth taking a screen away from them every two seconds over.
    ///
    /// Nothing here fails a session. This runs when a session is already
    /// over, and the worst it can do is leave a desktop the way the
    /// session left it, which is what happened before this existed. What
    /// it must never do is stop half way and leave two screens sitting on
    /// top of each other, which is why every screen is written down
    /// without being applied and the whole lot is applied at the end.
    pub fn put_back(seats: &[Seat]) -> (bool, Vec<String>) {
        let mut said = Vec::new();
        let mut written = 0;
        // Read once and not once per screen. A desk of twenty adapters,
        // which is what Windows lists on an ordinary machine, would
        // otherwise cost twenty full enumerations and four hundred asks
        // about magnifications, for one answer that does not change while
        // this loop runs: nothing is applied until the end.
        let now = as_it_stands();
        for seat in seats {
            let Some(here) = now.iter().find(|other| other.adapter == seat.adapter) else {
                said.push(format!(
                    "{} is no longer one of this computer's screens, so it was left alone",
                    seat.adapter
                ));
                continue;
            };
            // The list position is not the screen. A monitor unplugged
            // while the session ran moves every screen after it up one,
            // and putting a 4K arrangement onto whatever now answers to
            // that position is worse than doing nothing at all.
            if !seat.screen.is_empty() && !here.screen.is_empty() && here.screen != seat.screen {
                said.push(format!(
                    "{} is not the screen it was, so it was left alone",
                    seat.adapter
                ));
                continue;
            }
            match ask_for(seat) {
                true => written += 1,
                false => said.push(format!("Windows would not take {} back", seat.adapter)),
            }
        }
        if written == 0 {
            said.push("no screen could be put back the way it was".to_string());
            return (false, said);
        }
        // SAFETY: no name and no mode is how this call is told to apply
        // everything written down above, which is the whole point of
        // having written it down without applying it.
        let applied = unsafe {
            ChangeDisplaySettingsExW(
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null_mut(),
                0,
                std::ptr::null(),
            )
        };
        if applied != DISP_CHANGE_SUCCESSFUL {
            said.push(format!(
                "Windows refused the arrangement of {written} screens ({})",
                why(applied)
            ));
            return (false, said);
        }
        said.push(format!(
            "this computer's screens are back the way they were ({written} of them)"
        ));
        // The one thing the call above cannot undo, because it cannot do
        // it either: a desktop that was made larger than the panel it is
        // drawn on. Asked of every screen that is on and not only of the
        // ones whose size came back wrong, because the size is only half
        // of it: a desktop put back to the size of its panel while still
        // laid out to be shrunk into it is a desktop Windows still calls
        // stretched, and everything that reads a screen afterwards has to
        // be told so. Asking costs nothing on a screen that was never
        // stretched, which answers that it has nothing to do.
        for seat in seats.iter().filter(|seat| seat.on) {
            said.extend(crate::stretched::put_at(
                &seat.adapter,
                seat.wide,
                seat.high,
            ));
        }
        // And how large each of them draws, which the call above knows
        // nothing about: it is carried on the newer half of Windows, so
        // it is put back separately and afterwards. Afterwards because a
        // magnification belongs to a screen at a size, and the sizes have
        // only just come back.
        //
        // This is the half that was missing, and it is the one somebody
        // notices: a session that could not take the size it asked for
        // changed nothing but the magnification, so nothing here was ever
        // out of place, and a laptop was handed back drawing everything a
        // third too large until its owner put it right by hand.
        //
        // Asked even of a screen whose magnification could not be read
        // when the desk was noted, and that is not a detail: a screen
        // left holding a step that means nothing cannot be read, so the
        // next session notes nothing for it, so nothing is put back, so
        // it stays that way. Once in that state a laptop never comes out
        // of it on its own, and the only thing that ever did was somebody
        // opening the display settings by hand.
        for seat in seats.iter().filter(|seat| seat.on) {
            said.extend(drawn_as_before(seat));
        }
        (true, said)
    }

    /// Puts one screen's magnification back, insisting a little, and says
    /// nothing at all when it was already right.
    ///
    /// Only one of the ways a screen can decline to say how large it
    /// draws is worth waiting for: the arrangement has only just been
    /// applied, and the two halves of Windows do not arrive together, so
    /// a screen asked about too early is simply not described yet.
    ///
    /// The other ways are not reasons to give up, and treating them as
    /// such is what left a laptop at the wrong magnification. A screen
    /// sitting at a step that is no magnification is a screen holding the
    /// remains of what a session set: 175 % asked for while its desktop
    /// was 4K, counted from a recommendation that changed when the
    /// desktop came home. There is nothing to read there and nothing to
    /// wait for. It is written.
    fn drawn_as_before(seat: &Seat) -> Option<String> {
        use crate::magnify::NotRead;

        const TRIES: u32 = 10;
        const BETWEEN: std::time::Duration = std::time::Duration::from_millis(100);

        let mut seen = crate::magnify::reading(&seat.adapter);
        for _ in 1..TRIES {
            if !matches!(seen, Err(NotRead::NoSuchScreen)) {
                break;
            }
            std::thread::sleep(BETWEEN);
            seen = crate::magnify::reading(&seat.adapter);
        }
        match seen {
            // Already where it was noted.
            Ok(now) if now == seat.scale => None,
            // Nothing was noted for it, and it answers: whatever it is
            // drawing at, it is drawing at something, and this has no
            // business choosing for it.
            Ok(_) if seat.scale == 0 => None,
            Err(why @ NotRead::NoSuchScreen) => Some(format!(
                "{} was not put back to {} %: {why}",
                seat.adapter, seat.scale
            )),
            // Wrong, or holding a step that is no magnification at all.
            // Both are written rather than read, and nothing noted then
            // means asking Windows for the one it recommends: that is the
            // way out of a screen stuck holding what a session left, and
            // there is no better answer to be had, the one it had being
            // exactly what cannot be read.
            _ => Some(crate::magnify::magnify(&seat.adapter, seat.scale)),
        }
    }

    /// What Windows means by the number it answers with.
    ///
    /// Named rather than printed, because the one that matters is easy to
    /// mistake for a failure of ours: a screen that will not take a size
    /// is a screen saying so, not a bug at this end.
    fn why(answer: i32) -> String {
        match answer {
            -1 => "it failed".to_string(),
            -2 => "that screen does not have that size".to_string(),
            -4 => "the flags were wrong".to_string(),
            -5 => "one of the values was wrong".to_string(),
            other => format!("answer {other}"),
        }
    }

    /// Puts one screen at that size, and says what happened.
    ///
    /// The whole of what a session does to the computer it watches, and
    /// deliberately the smallest thing that could work: one screen, its
    /// size, nothing moved, nothing switched off. What it costs the
    /// person sitting in front of that computer is their main screen
    /// changing size for the length of a session, which is what every
    /// remote desktop that does not grow a screen of its own costs them,
    /// and it is put back afterwards from the arrangement noted first.
    ///
    /// Never fails a session: a computer that will not take the size
    /// serves the one it has, and the picture is stretched at the other
    /// end exactly as it was before any of this existed.
    pub fn put_at(screen: &str, wide: u32, high: u32) -> String {
        let Some(mode) = the_mode_for(screen, wide, high) else {
            // Not the end of it, and this is where a laptop is won or
            // lost. A panel that offers nothing larger than itself can
            // still be given a desktop larger than itself, drawn whole
            // and shrunk into it by the graphics card, and that is what
            // the newer half of Windows is for.
            // Read before anything is changed, and that is the whole of
            // this line. Read after, it lists the size that was just
            // granted and reads as a contradiction: « offers no 3840x2160
            // (3840x2160, ...) », which is true twice over and useless.
            let had = offered(screen);
            let stretched = crate::stretched::put_at(screen, wide, high)
                .unwrap_or_else(|| format!("{screen} already draws a {wide}x{high} desktop"));
            return format!(
                "{screen} offered no {wide}x{high} of its own ({had}), so a desktop that size is \
                 asked for instead: {stretched}"
            );
        };
        let name = super::wide(screen);
        // SAFETY: the name and the mode are ours and outlive the call.
        // Applied on its own and not written down for later, because
        // this one is wanted now: the picture opens on it.
        let answer = unsafe {
            ChangeDisplaySettingsExW(
                name.as_ptr(),
                &mode,
                std::ptr::null_mut(),
                CDS_UPDATEREGISTRY,
                std::ptr::null(),
            )
        };
        if answer == DISP_CHANGE_SUCCESSFUL {
            format!(
                "{screen} is showing {wide}x{high} at {} Hz",
                mode.dmDisplayFrequency
            )
        } else {
            format!(
                "Windows would not put {screen} at {wide}x{high}: {}",
                why(answer)
            )
        }
    }

    /// The mode this screen offers at that size, described the way
    /// Windows describes it.
    ///
    /// Asked for rather than made up, and that is the whole of this
    /// function. A mode built here from a width and a height alone
    /// carries no colour depth and no rate, and Windows matches it
    /// against what the driver offers: a size the screen can perfectly
    /// well draw is then refused with « that screen does not have that
    /// size », which is what happened to a laptop asked for 3840x2160.
    ///
    /// Asked twice when the first answer is no. The plain list is what
    /// the monitor says of itself, and it stops at the size of the
    /// panel; the raw one is what the graphics card will really produce,
    /// which on nearly every laptop includes sizes larger than the panel,
    /// drawn whole and then shrunk into it. That second list is what lets
    /// a 1920x1200 laptop serve a 4K desktop, and it is where the ability
    /// this product grew a whole virtual screen for was sitting all
    /// along.
    ///
    /// Among the modes at that size, the rate the screen is running at
    /// wins, then the fastest: a session must not quietly drop a 144 Hz
    /// screen to 60.
    fn the_mode_for(screen: &str, wide: u32, high: u32) -> Option<DEVMODEW> {
        let name = super::wide(screen);
        let now = at_present(&name);
        let mut best: Option<DEVMODEW> = None;
        for raw in [0, EDS_RAWMODE] {
            for index in 0.. {
                let mut mode: DEVMODEW = unsafe { std::mem::zeroed() };
                mode.dmSize = size_of::<DEVMODEW>() as u16;
                // SAFETY: a name of ours, ended, and a slot of ours
                // carrying its own size as the call requires.
                if unsafe { EnumDisplaySettingsExW(name.as_ptr(), index, &mut mode, raw) } == 0 {
                    break;
                }
                if mode.dmPelsWidth != wide || mode.dmPelsHeight != high {
                    continue;
                }
                if mode.dmDisplayFrequency == now {
                    return Some(with_everything(mode));
                }
                if best.is_none_or(|kept| kept.dmDisplayFrequency < mode.dmDisplayFrequency) {
                    best = Some(mode);
                }
            }
            if best.is_some() {
                break;
            }
        }
        best.map(with_everything)
    }

    /// Says which of the mode's fields are meant, which is every one that
    /// describes a picture.
    ///
    /// The mode came from Windows whole, so all of them are filled in and
    /// all of them are wanted. Only the place is left out: this changes
    /// one screen's size and moves nothing.
    fn with_everything(mut mode: DEVMODEW) -> DEVMODEW {
        mode.dmFields = DM_PELSWIDTH | DM_PELSHEIGHT | DM_BITSPERPEL | DM_DISPLAYFREQUENCY;
        mode
    }

    /// The rate that screen is running at now, or nought.
    fn at_present(name: &[u16]) -> u32 {
        let mut mode: DEVMODEW = unsafe { std::mem::zeroed() };
        mode.dmSize = size_of::<DEVMODEW>() as u16;
        // SAFETY: as above, asking for what is in use rather than a list.
        let read =
            unsafe { EnumDisplaySettingsExW(name.as_ptr(), ENUM_CURRENT_SETTINGS, &mut mode, 0) };
        if read == 0 {
            0
        } else {
            mode.dmDisplayFrequency
        }
    }

    /// The sizes that screen will draw, for the journal when one of them
    /// is not the one wanted.
    ///
    /// The whole point is to be readable by somebody deciding whether a
    /// size they asked for was unreasonable, so it is the sizes and not
    /// the modes: a screen offers the same size at several rates and
    /// depths, and naming it once is enough.
    fn offered(screen: &str) -> String {
        let name = super::wide(screen);
        let mut sizes: Vec<(u32, u32)> = Vec::new();
        for raw in [0, EDS_RAWMODE] {
            for index in 0.. {
                let mut mode: DEVMODEW = unsafe { std::mem::zeroed() };
                mode.dmSize = size_of::<DEVMODEW>() as u16;
                // SAFETY: as above.
                if unsafe { EnumDisplaySettingsExW(name.as_ptr(), index, &mut mode, raw) } == 0 {
                    break;
                }
                let size = (mode.dmPelsWidth, mode.dmPelsHeight);
                if !sizes.contains(&size) {
                    sizes.push(size);
                }
            }
        }
        // Biggest first: what somebody reading this wants to know is how
        // large this screen goes, and the list is long.
        sizes.sort_unstable_by_key(|(wide, high)| std::cmp::Reverse(wide * high));
        sizes.truncate(8);
        sizes
            .iter()
            .map(|(wide, high)| format!("{wide}x{high}"))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Moves this computer's desktop onto that screen, at that size, and
    /// makes everything else sit around it.
    ///
    /// For the one computer that cannot draw the size a session asks for
    /// on any screen it owns: the screen it grows for itself takes that
    /// size, and the desktop moves there for the length of the session.
    ///
    /// Nothing is switched off, and that is the whole of the safety. The
    /// screens somebody is looking at stay lit and keep their own sizes;
    /// what they show is the desktop's far side rather than its middle.
    /// If this computer were to fall over on the spot, Windows would find
    /// the grown screen gone at the next start and put the desktop back
    /// on a real one by itself, because there is always a real one on.
    pub fn put_the_desktop_on(screen: &str, wide: u32, high: u32) -> (bool, Vec<String>) {
        let now = as_it_stands();
        let Some(here) = now.iter().find(|seat| seat.adapter == screen).cloned() else {
            return (
                false,
                vec![format!(
                    "{screen} is not one of this computer's screens, so the desktop stays where it \
                     is"
                )],
            );
        };
        if !here.on {
            return (
                false,
                vec![format!(
                    "{screen} is not switched on, so the desktop stays where it is"
                )],
            );
        }
        // Everything is placed from the new main screen's corner, because
        // that is what being the main screen means to Windows: it sits at
        // the origin and the others are said relative to it.
        let (from_x, from_y) = here.at;
        let mut said = Vec::new();
        let mut written = 0;
        for seat in now.into_iter().filter(|seat| seat.on) {
            let theirs = seat.adapter == here.adapter;
            let wanted = Seat {
                at: (seat.at.0 - from_x, seat.at.1 - from_y),
                main: theirs,
                wide: if theirs { wide } else { seat.wide },
                high: if theirs { high } else { seat.high },
                ..seat
            };
            match ask_for(&wanted) {
                true => written += 1,
                false => said.push(format!("Windows would not place {}", wanted.adapter)),
            }
        }
        if written == 0 {
            said.push(format!(
                "no screen could be placed around {screen}, so the desktop stays where it is"
            ));
            return (false, said);
        }
        // SAFETY: no name and no mode is how this call is told to apply
        // everything written down above, which is why it was written down
        // without being applied.
        let applied = unsafe {
            ChangeDisplaySettingsExW(
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null_mut(),
                0,
                std::ptr::null(),
            )
        };
        if applied != DISP_CHANGE_SUCCESSFUL {
            said.push(format!(
                "Windows refused to move the desktop onto {screen} ({})",
                why(applied)
            ));
            return (false, said);
        }
        said.push(format!(
            "this computer's desktop is on {screen} at {wide}x{high} for the length of the \
             session, and its own screens are still on"
        ));
        (true, said)
    }

    /// Writes one screen down without applying it.
    fn ask_for(seat: &Seat) -> bool {
        let mut mode: DEVMODEW = unsafe { std::mem::zeroed() };
        mode.dmSize = size_of::<DEVMODEW>() as u16;
        mode.dmFields = DM_POSITION | DM_PELSWIDTH | DM_PELSHEIGHT;
        if seat.on {
            mode.dmPelsWidth = seat.wide;
            mode.dmPelsHeight = seat.high;
            mode.dmFields |= DM_DISPLAYFREQUENCY | DM_DISPLAYORIENTATION;
            mode.dmDisplayFrequency = seat.refresh;
            mode.Anonymous1.Anonymous2.dmDisplayOrientation = seat.turned;
            mode.Anonymous1.Anonymous2.dmPosition.x = seat.at.0;
            mode.Anonymous1.Anonymous2.dmPosition.y = seat.at.1;
        }
        // A size of nought is how this call is told a screen is to draw
        // nothing at all, which is what having been switched off means.
        let mut how = CDS_UPDATEREGISTRY | CDS_NORESET;
        if seat.on && seat.main {
            how |= CDS_SET_PRIMARY;
        }
        let name = super::wide(&seat.adapter);
        // SAFETY: the name and the mode are ours and outlive the call,
        // which is told to write this down rather than apply it.
        let answer = unsafe {
            ChangeDisplaySettingsExW(
                name.as_ptr(),
                &mode,
                std::ptr::null_mut(),
                how,
                std::ptr::null(),
            )
        };
        answer == DISP_CHANGE_SUCCESSFUL
    }
}

#[cfg(windows)]
pub use windows_only::{as_it_stands, put_at, put_back, put_the_desktop_on};

/// Nowhere else has screens to arrange, and the shape above stays
/// compiled and tested everywhere: reading and writing an arrangement is
/// ordinary text work with nothing platform-specific about it.
#[cfg(not(windows))]
pub fn as_it_stands() -> Vec<Seat> {
    Vec::new()
}

#[cfg(not(windows))]
pub fn put_back(_seats: &[Seat]) -> (bool, Vec<String>) {
    (
        true,
        vec!["this computer has no screens to put back".to_string()],
    )
}

#[cfg(not(windows))]
pub fn put_the_desktop_on(_screen: &str, _wide: u32, _high: u32) -> (bool, Vec<String>) {
    (
        false,
        vec!["this computer has no screens to put a desktop on".to_string()],
    )
}

#[cfg(not(windows))]
pub fn put_at(screen: &str, _wide: u32, _high: u32) -> String {
    format!("this computer has no {screen} to resize")
}

#[cfg(windows)]
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
fn read_wide(from: &[u16]) -> String {
    let end = from.iter().position(|letter| *letter == 0).unwrap_or(0);
    String::from_utf16_lossy(&from[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seat() -> Seat {
        Seat {
            adapter: r"\\.\DISPLAY1".to_string(),
            screen: r"MONITOR\SAM7180\{4d36e96e-e325-11ce-bfc1-08002be10318}\0002".to_string(),
            on: true,
            wide: 3840,
            high: 2160,
            refresh: 60,
            at: (-3840, 0),
            turned: 0,
            main: false,
            scale: 175,
        }
    }

    #[test]
    fn a_screen_survives_being_written_down_and_read_back() {
        // Ce qui est écrit ici est relu après une session, parfois après
        // un redémarrage : une seule écriture, une seule lecture.
        let said = seat().to_string();
        assert_eq!(said.parse::<Seat>().unwrap(), seat(), "{said}");
    }

    #[test]
    fn a_screen_that_was_off_carries_that_and_nothing_else() {
        // C'est le cas qui a fâché Victor : sa télé est éteinte presque
        // toujours, et une session la rallumait. Éteint est un état qu'on
        // relève et qu'on remet, pas une absence.
        let off = Seat {
            on: false,
            wide: 0,
            high: 0,
            refresh: 0,
            at: (0, 0),
            main: false,
            screen: String::new(),
            ..seat()
        };
        let said = off.to_string();
        assert!(said.contains("on=no"), "{said}");
        assert!(said.contains("screen=none"), "{said}");
        assert_eq!(said.parse::<Seat>().unwrap(), off);
    }

    #[test]
    fn the_place_of_a_screen_is_kept_including_to_the_left() {
        // « Mon écran 1 est à droite et l'écran 2 à gauche » : la place
        // du second est négative, et un relevé qui la perd rend un bureau
        // en miroir de celui qu'on avait.
        let said = seat().to_string();
        assert!(said.contains("at=-3840,0"), "{said}");
        assert_eq!(said.parse::<Seat>().unwrap().at, (-3840, 0));
    }

    #[test]
    fn the_magnification_is_part_of_what_is_noted_and_put_back() {
        // Le cas de Victor, et il a dû le réparer à la main plusieurs
        // fois : un portable 1920x1200 à qui on demande du 3840x2160 à
        // 175 % refuse la taille, garde la sienne, et se retrouve
        // néanmoins à 175 %. Rien n'ayant changé de taille, une remise
        // qui ne remet que les tailles ne remet rien du tout, et
        // l'agrandissement reste où la session l'a mis.
        let said = seat().to_string();
        assert!(said.contains("scale=175"), "{said}");
        assert_eq!(said.parse::<Seat>().unwrap().scale, 175);
    }

    #[test]
    fn a_whole_desk_survives_it_too() {
        let desk = vec![
            seat(),
            Seat {
                adapter: r"\\.\DISPLAY2".to_string(),
                at: (0, 0),
                main: true,
                ..seat()
            },
            Seat {
                adapter: r"\\.\DISPLAY3".to_string(),
                on: false,
                screen: String::new(),
                ..seat()
            },
        ];
        assert_eq!(read(&written(&desk)), desk);
    }

    #[test]
    fn a_line_that_will_not_read_costs_only_that_line() {
        // Trois écrans remis sur quatre valent mieux qu'aucun, et un
        // fichier à moitié écrit par une machine qui s'est éteinte est
        // exactement ce à quoi il faut survivre.
        let text = format!("{}\n\nn'importe quoi\n", seat());
        assert_eq!(read(&text), vec![seat()]);
    }
}
