//! Key combinations the window answers to during a session.
//!
//! A session gives the keyboard to the far computer: every key typed
//! goes there, which is the whole point, and nothing of ours can listen
//! for one the ordinary way. These are registered with the system
//! instead, which is served before any program is, so they work over a
//! picture that owns the screen.
//!
//! What they are aimed at is the session, and there is exactly one, so
//! they do nothing at all when none is open.
//!
//! A combination is remembered as the place of a key on the keyboard and
//! not as the letter printed on it. The key left of the digits carries
//! « ² » in France and « ` » elsewhere, and it is the same key under the
//! same finger; going through the place is what keeps a shortcut where
//! it was put whatever the keyboard.

// Registering a combination with the system is a Windows matter, and a
// session only ever runs there. The reading, the writing and the naming
// stay compiled and tested everywhere.
#![cfg_attr(not(windows), allow(dead_code))]

use std::fmt;
use std::fs;
use std::io;
use std::path::Path;
use std::str::FromStr;

/// What a combination can be asked to do.
///
/// Three, and no more: what a person reaches for without leaving the
/// picture. Everything else is one click away in the floating menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Doing {
    /// Ends the session, and hands the far desktop back.
    End,
    /// Brings the floating button back and opens its menu.
    Menu,
    /// Switches the picture between a window and the whole screen.
    Fullscreen,
}

impl Doing {
    /// All of them, in the order the settings screen shows them.
    pub const ALL: [Doing; 3] = [Doing::End, Doing::Menu, Doing::Fullscreen];

    /// Name this is filed under, in the file and between the window and
    /// the program.
    pub fn name(self) -> &'static str {
        match self {
            Doing::End => "end",
            Doing::Menu => "menu",
            Doing::Fullscreen => "fullscreen",
        }
    }

    fn read(name: &str) -> Option<Self> {
        Doing::ALL.into_iter().find(|doing| doing.name() == name)
    }
}

/// Keys held down alongside the one that is pressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Held {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// The Windows key. Named as the system names it rather than as the
    /// key is engraved, since that engraving changes with the machine.
    pub win: bool,
}

impl Held {
    fn none(self) -> bool {
        !self.ctrl && !self.alt && !self.shift && !self.win
    }
}

/// One combination: what is held, and the place of the key that is
/// pressed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Combination {
    pub held: Held,
    /// The place of the key, named the way a web page names it:
    /// `KeyX`, `Digit1`, `Backquote`, `F5`.
    pub key: String,
}

impl Combination {
    /// Whether this is one the system will take.
    ///
    /// A combination without a single key held down would swallow that
    /// key for every program on the machine, this one included, which is
    /// never what somebody meant to ask for.
    pub fn stands(&self) -> bool {
        !self.held.none() && scan_code_of(&self.key).is_some()
    }
}

impl fmt::Display for Combination {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (held, name) in [
            (self.held.ctrl, "Ctrl"),
            (self.held.alt, "Alt"),
            (self.held.shift, "Shift"),
            (self.held.win, "Win"),
        ] {
            if held {
                write!(f, "{name}+")?;
            }
        }
        f.write_str(&self.key)
    }
}

impl FromStr for Combination {
    type Err = ();

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let mut held = Held::default();
        let mut key = None;
        for piece in text.split('+').map(str::trim).filter(|p| !p.is_empty()) {
            match piece {
                "Ctrl" => held.ctrl = true,
                "Alt" => held.alt = true,
                "Shift" => held.shift = true,
                "Win" => held.win = true,
                other => key = Some(other.to_string()),
            }
        }
        Ok(Combination {
            held,
            key: key.ok_or(())?,
        })
    }
}

/// Every combination in force, one slot per thing that can be asked.
///
/// A slot left empty is a thing nobody has given a key to, which is the
/// state two of the three start in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Bound {
    pub end: Option<Combination>,
    pub menu: Option<Combination>,
    pub fullscreen: Option<Combination>,
}

impl Bound {
    /// What a machine that has never been told anything answers to.
    ///
    /// One only. A shortcut that ends a session, or resizes the
    /// picture, is a matter of taste and is left to be chosen; the one
    /// that brings the floating button back is not, because hiding that
    /// button is otherwise a decision without a way back.
    pub fn out_of_the_box() -> Self {
        Self {
            end: None,
            menu: Some(Combination {
                held: Held {
                    alt: true,
                    ..Held::default()
                },
                key: "Backquote".to_string(),
            }),
            fullscreen: None,
        }
    }

    pub fn get(&self, doing: Doing) -> Option<&Combination> {
        match doing {
            Doing::End => self.end.as_ref(),
            Doing::Menu => self.menu.as_ref(),
            Doing::Fullscreen => self.fullscreen.as_ref(),
        }
    }

    fn set(&mut self, doing: Doing, combination: Option<Combination>) {
        let slot = match doing {
            Doing::End => &mut self.end,
            Doing::Menu => &mut self.menu,
            Doing::Fullscreen => &mut self.fullscreen,
        };
        *slot = combination;
    }

    /// Every combination actually in force, with what it does.
    pub fn in_force(&self) -> Vec<(Doing, &Combination)> {
        Doing::ALL
            .into_iter()
            .filter_map(|doing| self.get(doing).map(|combination| (doing, combination)))
            .filter(|(_, combination)| combination.stands())
            .collect()
    }
}

/// Reads what was chosen. A file that is not there means nobody has
/// chosen yet, and the shipped combination stands.
pub fn read(path: &Path) -> io::Result<Bound> {
    let contents = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Bound::out_of_the_box()),
        Err(e) => return Err(e),
    };
    Ok(read_lines(&contents))
}

/// The same, standing on the shipped combinations when the file cannot
/// be read at all.
///
/// Falling back to nothing instead, which is what a plain default is,
/// silently took every combination away, the one that brings the hidden
/// floating button back among them: a disk hiccup turned hiding the
/// button into a one-way door.
fn read_or_shipped(path: &Path) -> Bound {
    read(path).unwrap_or_else(|e| {
        crate::journal::note(&format!(
            "raccourcis illisibles ({e}), combinaisons d'origine en attendant"
        ));
        Bound::out_of_the_box()
    })
}

/// One line per thing, `what combination`, and nothing at all for a
/// thing left without one.
///
/// A line nobody can read is skipped rather than raised. These are a
/// convenience; one bad line must not cost the settings screen.
fn read_lines(contents: &str) -> Bound {
    let mut bound = Bound::default();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut pieces = line.splitn(2, char::is_whitespace);
        let Some(doing) = pieces.next().and_then(Doing::read) else {
            continue;
        };
        let combination = pieces
            .next()
            .and_then(|text| text.trim().parse::<Combination>().ok())
            .filter(Combination::stands);
        bound.set(doing, combination);
    }
    bound
}

pub fn write(path: &Path, bound: &Bound) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut text = String::from(
        "# Raccourcis clavier de ZyrDesk, un par ligne.\n\
         # Une ligne absente veut dire qu'aucune touche n'est attribuée.\n",
    );
    for (doing, combination) in bound.in_force() {
        text.push_str(&format!("{} {combination}\n", doing.name()));
    }
    fs::write(path, text)
}

/// Where a key sits on the keyboard, as the keyboard reports it.
///
/// Only the main block and the function row: everything a combination is
/// ever built on, and every one of them at a fixed place on every
/// keyboard. What is not here is refused out loud rather than silently
/// bound to the wrong key.
const KEYS: &[(&str, u16)] = &[
    ("Escape", 0x01),
    ("Digit1", 0x02),
    ("Digit2", 0x03),
    ("Digit3", 0x04),
    ("Digit4", 0x05),
    ("Digit5", 0x06),
    ("Digit6", 0x07),
    ("Digit7", 0x08),
    ("Digit8", 0x09),
    ("Digit9", 0x0A),
    ("Digit0", 0x0B),
    ("Minus", 0x0C),
    ("Equal", 0x0D),
    ("Tab", 0x0F),
    ("KeyQ", 0x10),
    ("KeyW", 0x11),
    ("KeyE", 0x12),
    ("KeyR", 0x13),
    ("KeyT", 0x14),
    ("KeyY", 0x15),
    ("KeyU", 0x16),
    ("KeyI", 0x17),
    ("KeyO", 0x18),
    ("KeyP", 0x19),
    ("BracketLeft", 0x1A),
    ("BracketRight", 0x1B),
    ("Enter", 0x1C),
    ("KeyA", 0x1E),
    ("KeyS", 0x1F),
    ("KeyD", 0x20),
    ("KeyF", 0x21),
    ("KeyG", 0x22),
    ("KeyH", 0x23),
    ("KeyJ", 0x24),
    ("KeyK", 0x25),
    ("KeyL", 0x26),
    ("Semicolon", 0x27),
    ("Quote", 0x28),
    // The one left of the digits: « ² » in France, « ` » elsewhere.
    ("Backquote", 0x29),
    ("Backslash", 0x2B),
    ("KeyZ", 0x2C),
    ("KeyX", 0x2D),
    ("KeyC", 0x2E),
    ("KeyV", 0x2F),
    ("KeyB", 0x30),
    ("KeyN", 0x31),
    ("KeyM", 0x32),
    ("Comma", 0x33),
    ("Period", 0x34),
    ("Slash", 0x35),
    ("Space", 0x39),
    ("F1", 0x3B),
    ("F2", 0x3C),
    ("F3", 0x3D),
    ("F4", 0x3E),
    ("F5", 0x3F),
    ("F6", 0x40),
    ("F7", 0x41),
    ("F8", 0x42),
    ("F9", 0x43),
    ("F10", 0x44),
    ("F11", 0x57),
    ("F12", 0x58),
    // Between the left shift and the Z on the keyboards that have it.
    ("IntlBackslash", 0x56),
];

fn scan_code_of(key: &str) -> Option<u16> {
    KEYS.iter()
        .find(|(name, _)| *name == key)
        .map(|(_, code)| *code)
}

/// The same the other way: what key sits at this place.
///
/// What the settings screen needs to take a combination from the
/// keyboard. A place this product does not know is no combination at
/// all, and saying so is what stops a shortcut being filed under a key
/// nothing can ever register.
pub fn placed(scan: u16) -> Option<&'static str> {
    KEYS.iter()
        .find(|(_, code)| *code == scan)
        .map(|(name, _)| *name)
}

/// Chaque combinaison, écrite comme elle est gravée sur le clavier
/// branché, et rien pour ce à quoi aucune touche n'est attribuée.
///
/// Le produit retient la place d'une touche et non le signe dessus ; ceci
/// refait le chemin en sens inverse. Lu par le menu de la session, qui
/// dit à côté de chaque ligne la touche qui la déclenche, et par l'écran
/// des réglages, où les trois se choisissent.
pub fn engraved() -> Vec<(Doing, Option<String>)> {
    let bound = read_or_shipped(&zyr_proto::paths::keyboard_shortcuts());
    Doing::ALL
        .into_iter()
        .map(|doing| {
            let said = bound
                .get(doing)
                .filter(|combination| combination.stands())
                .map(spelled);
            (doing, said)
        })
        .collect()
}

/// Une combinaison telle qu'une personne la lit.
///
/// Les touches tenues portent ici le mot du clavier français, quand
/// `Display` porte celui du fichier : l'un se lit, l'autre se relit, et
/// les confondre changerait ce qui est écrit sur le disque.
fn spelled(combination: &Combination) -> String {
    let mut written = String::new();
    for (held, name) in [
        (combination.held.ctrl, "Ctrl"),
        (combination.held.alt, "Alt"),
        (combination.held.shift, "Maj"),
        (combination.held.win, "Win"),
    ] {
        if held {
            written.push_str(name);
            written.push_str(" + ");
        }
    }
    written.push_str(&engraved_key(&combination.key));
    written
}

/// Ce qui est gravé sur cette touche-là, sur le clavier branché.
///
/// Faute de réponse, la place est écrite telle quelle : illisible mais
/// jamais fausse, ce que la page fait déjà.
#[cfg(windows)]
fn engraved_key(key: &str) -> String {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        MAPVK_VK_TO_CHAR, MAPVK_VSC_TO_VK_EX, MapVirtualKeyW,
    };

    let Some(scan) = scan_code_of(key) else {
        return key.to_string();
    };
    // SAFETY: deux questions au système sur le clavier de ce fil, qui ne
    // touchent à rien qui soit à nous.
    let engraved = unsafe {
        match MapVirtualKeyW(u32::from(scan), MAPVK_VSC_TO_VK_EX) {
            0 => 0,
            code => MapVirtualKeyW(code, MAPVK_VK_TO_CHAR),
        }
    };
    // Le bit de tête dit une touche morte, dont le signe est le reste.
    // Les touches sans signe, Entrée ou les touches de fonction, ne
    // répondent rien : c'est leur nom qui est lisible, pas leur gravure.
    match char::from_u32(engraved & 0x7FFF_FFFF) {
        Some(sign) if !sign.is_control() && sign != ' ' => sign.to_uppercase().to_string(),
        _ => key.to_string(),
    }
}

#[cfg(not(windows))]
fn engraved_key(key: &str) -> String {
    key.to_string()
}

/* ---- Ce que la fenêtre demande --------------------------------------- */

/// Gives a key to one thing, or takes its key away when nothing is
/// given.
pub fn bind(doing: Doing, wanted: Option<Combination>) -> Result<(), String> {
    if let Some(read) = &wanted
        && !read.stands()
    {
        return Err(
            "cette combinaison ne peut pas être prise : il faut au moins une touche \
                    tenue, et une touche que ZyrDesk sait placer sur un clavier."
                .to_string(),
        );
    }

    let path = zyr_proto::paths::keyboard_shortcuts();
    let mut bound = read(&path).map_err(|e| e.to_string())?;
    bound.set(doing, wanted);
    write(&path, &bound).map_err(|e| e.to_string())?;
    listen_again();
    Ok(())
}

/* ---- Ce qui appartient à Windows ------------------------------------- */

/// The thread the combinations belong to.
///
/// The system files a combination under the thread that asked for it and
/// hands it back to that same thread, so one thread owns all of them for
/// as long as the program runs. Its name is kept here to be able to tell
/// it that the file has changed.
#[cfg(windows)]
static BOARD: std::sync::Mutex<Option<u32>> = std::sync::Mutex::new(None);

/// What that thread is told when the combinations change.
#[cfg(windows)]
const AGAIN: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP;

/// Registers what is in the file, and keeps doing it for as long as the
/// program runs.
#[cfg(windows)]
pub fn listen(app: tauri::AppHandle) {
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;

    std::thread::spawn(move || {
        // The thread's message queue exists from its first look at it,
        // and not before. A change posted in the gap between this
        // thread's name being written down and its first wait would be
        // refused by the system and lost; looking once, at nothing, is
        // the documented way to make the queue exist now.
        let mut message = std::mem::MaybeUninit::uninit();
        // SAFETY: the slot is ours, and peeking removes nothing.
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::PeekMessageW(
                message.as_mut_ptr(),
                std::ptr::null_mut(),
                0,
                0,
                windows_sys::Win32::UI::WindowsAndMessaging::PM_NOREMOVE,
            )
        };
        // SAFETY: no argument, and the answer is this thread's own name.
        *BOARD.lock().expect("fil des raccourcis") = Some(unsafe { GetCurrentThreadId() });
        while hold_them(&app) {}
    });
}

/// Tells the thread to read the file again.
#[cfg(windows)]
pub fn listen_again() {
    use windows_sys::Win32::UI::WindowsAndMessaging::PostThreadMessageW;

    let Some(thread) = *BOARD.lock().expect("fil des raccourcis") else {
        return;
    };
    // SAFETY: the name is one this program gave itself above, and the
    // message carries nothing.
    unsafe { PostThreadMessageW(thread, AGAIN, 0, 0) };
}

/// Holds every combination until told to look again, and says whether
/// there is a reason to come back.
#[cfg(windows)]
fn hold_them(app: &tauri::AppHandle) -> bool {
    use std::ptr::null_mut;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        MAPVK_VSC_TO_VK, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, MapVirtualKeyW,
        RegisterHotKey, UnregisterHotKey,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetMessageW, MSG, WM_HOTKEY};

    let bound = read_or_shipped(&zyr_proto::paths::keyboard_shortcuts());
    let mut taken: Vec<(i32, Doing)> = Vec::new();
    for (rank, (doing, combination)) in bound.in_force().into_iter().enumerate() {
        let Some(scan) = scan_code_of(&combination.key) else {
            continue;
        };
        // SAFETY: a scan code and a well known conversion, both plain
        // numbers.
        let key = unsafe { MapVirtualKeyW(u32::from(scan), MAPVK_VSC_TO_VK) };
        if key == 0 {
            crate::journal::note(&format!(
                "raccourci {combination} : ce clavier n'a pas cette touche"
            ));
            continue;
        }

        let mut modifiers = MOD_NOREPEAT;
        for (held, flag) in [
            (combination.held.ctrl, MOD_CONTROL),
            (combination.held.alt, MOD_ALT),
            (combination.held.shift, MOD_SHIFT),
            (combination.held.win, MOD_WIN),
        ] {
            if held {
                modifiers |= flag;
            }
        }

        let id = rank as i32;
        // SAFETY: no window, so the combination belongs to this thread,
        // and the identifier is ours and unique within it.
        if unsafe { RegisterHotKey(null_mut(), id, modifiers, key) } != 0 {
            crate::journal::note(&format!(
                "raccourci {combination} tenu pour {}",
                doing.name()
            ));
            taken.push((id, doing));
        } else {
            // Said out loud: another program holding the same
            // combination is the ordinary reason, and from the outside
            // it looks exactly like a shortcut that does nothing.
            crate::journal::note(&format!(
                "raccourci {combination} refusé par Windows, sans doute déjà pris ailleurs"
            ));
        }
    }

    let mut message = MSG {
        hwnd: null_mut(),
        message: 0,
        wParam: 0,
        lParam: 0,
        time: 0,
        pt: windows_sys::Win32::Foundation::POINT { x: 0, y: 0 },
    };
    let mut again = false;
    // SAFETY: the slot is ours, and no window means every message of
    // this thread, which is where the combinations land.
    while unsafe { GetMessageW(&mut message, null_mut(), 0, 0) } > 0 {
        if message.message == AGAIN {
            again = true;
            break;
        }
        if message.message == WM_HOTKEY
            && let Some((_, doing)) = taken.iter().find(|(id, _)| *id as usize == message.wParam)
        {
            do_it(app, *doing);
        }
    }

    for (id, _) in taken {
        // SAFETY: the identifier is one this thread registered above.
        unsafe { UnregisterHotKey(null_mut(), id) };
    }
    again
}

#[cfg(windows)]
fn do_it(app: &tauri::AppHandle, doing: Doing) {
    match doing {
        Doing::Menu => {
            if let Err(e) = crate::floating::show_the_menu(app) {
                crate::journal::note(&format!("raccourci du menu sans effet : {e}"));
            }
        }
        Doing::End => on_the_session(app, crate::floating::Act::End),
        Doing::Fullscreen => on_the_session(app, crate::floating::Act::Fullscreen),
    }
}

/// Asks the session for something, without holding the thread that owns
/// the combinations: it has to be back waiting for the next one.
#[cfg(windows)]
fn on_the_session(app: &tauri::AppHandle, act: crate::floating::Act) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = crate::floating::ask(&app, act).await {
            crate::journal::note(&format!("raccourci sans effet : {e}"));
        }
    });
}

#[cfg(not(windows))]
pub fn listen(_app: tauri::AppHandle) {}

#[cfg(not(windows))]
pub fn listen_again() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_combination_is_the_one_that_brings_the_button_back() {
        let bound = Bound::out_of_the_box();
        let menu = bound.menu.expect("le menu a une combinaison d'origine");
        assert_eq!(menu.to_string(), "Alt+Backquote");
        assert!(menu.stands());
        assert!(bound.end.is_none());
        assert!(bound.fullscreen.is_none());
    }

    #[test]
    fn a_combination_survives_being_written_down_and_read_back() {
        for text in ["Alt+Backquote", "Ctrl+Alt+Shift+KeyQ", "Win+F5"] {
            let combination: Combination = text.parse().expect(text);
            assert_eq!(combination.to_string(), text);
            assert!(combination.stands(), "sur « {text} »");
        }
    }

    #[test]
    fn a_combination_without_anything_held_is_refused() {
        // Elle avalerait cette touche pour toute la machine, y compris
        // pour ce qu'on est en train de taper à l'autre bout.
        let alone: Combination = "KeyQ".parse().expect("KeyQ");
        assert!(!alone.stands());
    }

    #[test]
    fn a_key_we_cannot_place_on_a_keyboard_is_refused() {
        let unknown: Combination = "Alt+Teleport".parse().expect("Alt+Teleport");
        assert!(!unknown.stands());
    }

    #[test]
    fn what_is_written_comes_back_the_same() {
        let mut bound = Bound::out_of_the_box();
        bound.end = Some("Ctrl+Alt+Shift+KeyQ".parse().expect("combinaison"));
        let folder = std::env::temp_dir().join(format!(
            "zyrdesk-raccourcis-{}",
            zyr_proto::random::alphanumeric_string(8)
        ));
        let path = folder.join("keyboard-shortcuts.conf");
        write(&path, &bound).expect("écriture");
        assert_eq!(read(&path).expect("lecture"), bound);
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_thing_left_without_a_key_stays_without_one() {
        let bound = read_lines("menu Alt+Backquote\nfullscreen\n");
        assert!(bound.menu.is_some());
        assert!(bound.fullscreen.is_none());
        assert!(bound.end.is_none());
    }

    #[test]
    fn a_line_nobody_can_read_costs_only_that_line() {
        let bound = read_lines("bidule Alt+KeyA\nmenu Alt+KeyM\n");
        assert_eq!(bound.menu.expect("menu").to_string(), "Alt+KeyM");
    }

    #[test]
    fn only_what_the_system_would_take_is_handed_to_it() {
        let bound = read_lines("menu KeyM\nleave Alt+Teleport\nfullscreen Ctrl+F1\n");
        let in_force = bound.in_force();
        assert_eq!(in_force.len(), 1);
        assert_eq!(in_force[0].0, Doing::Fullscreen);
    }
}
