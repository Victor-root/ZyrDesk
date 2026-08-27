//! What Windows wants, and what the person wanted over it.
//!
//! Three choices: follow Windows, force light, force dark. Following is
//! the default, and it has to follow for real, including while the
//! window is open: everything else on the machine switches the moment
//! Windows does, and a product that does not is the odd one out.
//!
//! # Why the page cannot answer this on its own
//!
//! A web page asks its browser `prefers-color-scheme` and is told. Here
//! the browser is a web view embedded in our window, and the toolkit
//! pins that view's colour scheme to one fixed answer when the window is
//! built, taken from the system at that instant. It is right at the
//! first frame and frozen from then on: Windows switching afterwards
//! changes nothing the page can see, and the event a page would listen
//! for never fires.
//!
//! There is one road out of that in the toolkit, and this program had
//! blocked it. The toolkit refreshes the view when it sees Windows
//! switch, unless a theme has been forced on the window; and the window
//! was being forced to a theme on every start, so that its title bar
//! would match the page. The two ends were therefore fighting: matching
//! the frame cost the following.
//!
//! # So Windows is asked directly
//!
//! The same value the toolkit itself reads, from the same place, and
//! watched rather than sampled: Windows raises a hand when it changes,
//! and every window is told. The page then holds no opinion of its own
//! about the system, which is the point. And the window is only ever
//! forced to a theme when somebody actually chose one, so following
//! stays followed, title bar and all.

use tauri::{AppHandle, Theme, Window};

/// Name the pages listen on to be told what Windows wants now.
#[cfg(windows)]
const CHANGED: &str = "system-theme";

/// The three answers the page may give, spelled as it spells them.
const FOLLOW: &str = "systeme";
const LIGHT: &str = "clair";

/// Whether Windows is asking for a light interface right now.
///
/// Asked by the page as it loads, so it never has to trust a colour
/// scheme frozen at some earlier moment.
#[tauri::command]
pub fn system_theme() -> bool {
    windows_wants_light()
}

/// Matches the window to what the person chose.
///
/// The frame belongs to Windows and not to the page, so the page cannot
/// reach it: without this, a light interface would keep a dark title
/// bar, which is exactly the kind of seam a product is judged on.
///
/// « Follow » is handed over as no choice at all rather than as the
/// colour it comes to right now. The two look identical for one second
/// and are opposites afterwards: a window told nothing follows Windows
/// by itself, frame and all, while a window told « light » stays light
/// for ever and, worse, makes the toolkit stop reporting that Windows
/// switched at all.
#[tauri::command]
pub fn set_theme(window: Window, choix: String) {
    let wanted = match choix.as_str() {
        FOLLOW => None,
        chosen => Some(if chosen == LIGHT {
            Theme::Light
        } else {
            Theme::Dark
        }),
    };
    // A window that refuses the change is not worth stopping for: the
    // page is already drawn in the right theme, only its frame is not.
    let _ = window.set_theme(wanted);
}

/// Follows what Windows wants for as long as the program runs, and tells
/// every window each time it changes.
///
/// On a thread of its own, asleep the whole time: Windows wakes it. The
/// alternative is asking the registry on a timer, which is a question
/// asked a thousand times for an answer that changes twice a day.
#[cfg(windows)]
pub fn watch(app: AppHandle) {
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_NOTIFY, REG_NOTIFY_CHANGE_LAST_SET, RegCloseKey,
        RegNotifyChangeKeyValue, RegOpenKeyExW,
    };
    use windows_sys::Win32::System::Threading::{CreateEventW, INFINITE, WaitForSingleObject};

    use tauri::Emitter;

    std::thread::spawn(move || {
        let name = wide(PERSONALIZE);
        let mut key: HKEY = std::ptr::null_mut();
        // SAFETY: the name outlives the call, and the slot for the key it
        // hands back is ours.
        let opened =
            unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, name.as_ptr(), 0, KEY_NOTIFY, &mut key) };
        if opened != 0 {
            note(&format!(
                "le thème de Windows ne sera pas suivi : sa clé ne s'ouvre pas (code {opened})"
            ));
            return;
        }
        // SAFETY: an unnamed event of ours, reset by hand between waits.
        let woken = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
        if woken.is_null() {
            // SAFETY: a key this thread opened, closed once.
            unsafe { RegCloseKey(key) };
            note("le thème de Windows ne sera pas suivi : pas de réveil à poser");
            return;
        }

        let mut said = windows_wants_light();
        loop {
            // SAFETY: the key and the event are both live, and the call
            // arms one notification which the wait below collects.
            let armed =
                unsafe { RegNotifyChangeKeyValue(key, 0, REG_NOTIFY_CHANGE_LAST_SET, woken, 1) };
            if armed != 0 {
                note(&format!(
                    "le thème de Windows n'est plus suivi : {armed} en posant la garde"
                ));
                break;
            }
            // SAFETY: the event is live and the wait has no deadline.
            if unsafe { WaitForSingleObject(woken, INFINITE) } != WAIT_OBJECT_0 {
                break;
            }
            // The key carries more than this one value: a change to any
            // of it wakes this thread, and only a change to the answer is
            // worth a word to anybody.
            let now = windows_wants_light();
            if now != said {
                said = now;
                note(&format!(
                    "Windows demande maintenant une interface {}",
                    if now { "claire" } else { "sombre" }
                ));
                let _ = app.emit(CHANGED, now);
            }
        }

        // SAFETY: both were made by this thread and are closed once.
        unsafe {
            CloseHandle(woken);
            RegCloseKey(key);
        }
    });
}

/// Elsewhere there is no Windows to follow, and no window to match.
#[cfg(not(windows))]
pub fn watch(_app: AppHandle) {}

/// Where Windows keeps what it wants applications to look like.
///
/// The same key and the same value the toolkit reads, deliberately: two
/// programs answering that question differently in the same window is
/// the seam this whole file exists to close. And it is the applications
/// setting, not the system one: Windows lets the two differ, and what a
/// window is asked to look like is the first.
#[cfg(windows)]
const PERSONALIZE: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
#[cfg(windows)]
const APPS_USE_LIGHT: &str = "AppsUseLightTheme";

/// Whether Windows is asking for a light interface.
///
/// Light when it says nothing, which is what Windows itself does with a
/// missing value.
#[cfg(windows)]
fn windows_wants_light() -> bool {
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};

    let key = wide(PERSONALIZE);
    let value = wide(APPS_USE_LIGHT);
    let mut found = 0u32;
    let mut size = size_of::<u32>() as u32;
    // SAFETY: both names outlive the call, and the four bytes written
    // into are those of the number beside them.
    let code = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&raw mut found).cast::<std::ffi::c_void>(),
            &mut size,
        )
    };
    code != 0 || found != 0
}

#[cfg(not(windows))]
fn windows_wants_light() -> bool {
    true
}

/// Zero-terminated string, the way Windows expects them.
#[cfg(windows)]
fn wide(text: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    std::ffi::OsStr::new(text)
        .encode_wide()
        .chain(Some(0))
        .collect()
}

#[cfg(windows)]
fn note(what: &str) {
    crate::journal::note(what);
}
