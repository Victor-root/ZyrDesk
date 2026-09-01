//! L'icône de la fenêtre : celle de la barre des tâches et du bandeau.
//!
//! A window's icon and a program's icon are two different things, and the
//! first one wins. Windows draws the program's icon out of the .ico
//! compiled into the executable, picking whichever of its twenty sizes
//! fits the place it is drawing; but a window that has been given an icon
//! of its own is drawn from that one instead, stretched or squeezed to
//! whatever the taskbar and the title bar happen to want.
//!
//! The toolkit gives every window one, and it builds it from the first
//! entry of our .ico and nothing else (`tauri-codegen`, `image.rs`:
//! `let entry = &icon_dir.entries()[0]`). Sorted smallest first, as icon
//! files are, that is the sixteen pixel drawing, blown up to the
//! forty-two the taskbar draws at on a magnified screen. Blowing a
//! drawing up is far worse than shrinking one, which is why this icon was
//! the only soft one on a bar of sharp ones, and why no amount of care in
//! the file changed anything: the other twenty sizes were never read.
//!
//! So the window is given its icon here, twice, from the compiled
//! resource: once at the size Windows draws a big icon and once at the
//! size it draws a small one. Both come out of the .ico at exactly those
//! sizes, with nothing stretched at all.

use tauri::AppHandle;

/// Puts the right icon on the home window, at the sizes Windows is about
/// to draw it at.
///
/// Asked again whenever the window changes screen: the sizes are counted
/// in real pixels, and a screen magnified differently asks for different
/// ones.
#[cfg(windows)]
pub fn on_the_window(app: &AppHandle) {
    use tauri::Manager;
    use windows_sys::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        ICON_BIG, ICON_SMALL, IDI_APPLICATION, IMAGE_ICON, LoadImageW, SM_CXICON, SM_CXSMICON,
        SendMessageW, WM_SETICON,
    };

    let Some(home) = app
        .get_window(crate::HOME)
        .and_then(|window| window.hwnd().ok())
    else {
        return;
    };
    let home = home.0 as HWND;
    // SAFETY: our own window, and our own module; all three calls only
    // read.
    let (ours, dpi) = unsafe { (GetModuleHandleW(std::ptr::null()), GetDpiForWindow(home)) };
    // SAFETY: no argument beyond the metric and the scale asked for.
    let sides = unsafe {
        [
            (ICON_BIG, GetSystemMetricsForDpi(SM_CXICON, dpi)),
            (ICON_SMALL, GetSystemMetricsForDpi(SM_CXSMICON, dpi)),
        ]
    };

    for (which, side) in sides {
        // SAFETY: our own module and the icon resource compiled into it,
        // asked for at a size the file holds.
        //
        // Never shared: a shared handle comes back at whatever size it
        // was first asked for, whatever size is asked for afterwards,
        // which is the very thing being put right here. Nothing frees
        // these two; they last as long as the window, which lasts as long
        // as the program.
        let icon = unsafe { LoadImageW(ours, IDI_APPLICATION, IMAGE_ICON, side, side, 0) };
        if icon.is_null() {
            crate::journal::note(&format!(
                "icône de la fenêtre : Windows n'a pas rendu le dessin en {side} px"
            ));
            continue;
        }
        // SAFETY: our own window, and a handle it keeps.
        unsafe { SendMessageW(home, WM_SETICON, which as WPARAM, icon as LPARAM) };
    }
    crate::journal::note(&format!(
        "icône de la fenêtre posée en {} et {} px (écran à {} %)",
        sides[0].1,
        sides[1].1,
        dpi * 100 / 96
    ));
}

#[cfg(not(windows))]
pub fn on_the_window(_app: &AppHandle) {}
