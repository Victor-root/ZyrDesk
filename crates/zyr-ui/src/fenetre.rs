//! La fenêtre de ZyrDesk, celle que le système encadre.
//!
//! Faite par ce programme et non par une boîte à outils. C'était la
//! dernière chose qu'une boîte à outils tenait pour nous, et c'est celle
//! qui compte le plus : c'est **la même fenêtre** qui porte l'accueil et
//! qui porte l'image d'une session, et tout ce que `picture` fait de
//! délicat se joue dans les messages qu'elle reçoit.
//!
//! Ce que ça change, en clair : le cadre, le plein écran, l'agrandi et le
//! suivi de l'écran sont écrits ici, en une page, au lieu d'être
//! reproduits par une couche qui vise autre chose. Ce que `picture` posait
//! par-dessus continue de se poser par-dessus, exactement pareil : un
//! gardien se met devant cette fenêtre-ci comme il se mettait devant
//! l'autre.
//!
//! Les longueurs se comptent en **pixels de page** quand elles sont
//! écrites ici, et en vrais pixels partout ailleurs : `echelle` fait le
//! passage, une fois, au moment de bâtir et à chaque changement d'écran.

// Une fenêtre est une chose du système, et ce produit n'en ouvre que sous
// Windows. Ailleurs, chaque réponse est celle d'une fenêtre qui n'existe
// pas, pour que tout le reste du fichier reste compilé et vérifié.
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

use tauri::AppHandle;

/// Ce que la fenêtre fait de large et de haut en s'ouvrant, et ce en
/// dessous de quoi elle ne descend pas, en pixels de page.
///
/// Le plancher n'est pas une préférence : c'est la place qu'il faut pour
/// que les cartes des ordinateurs tiennent en ligne et que le menu d'une
/// session ait où s'ouvrir.
const OUVERTE: (i32, i32) = (1060, 720);
const JAMAIS_MOINS: (i32, i32) = (880, 600);

/// La fenêtre, telle que le système la connaît.
static ELLE: AtomicIsize = AtomicIsize::new(0);
/// Si elle prend l'écran entier.
///
/// Retenu plutôt que relu sur la fenêtre, parce que les endroits qui le
/// demandent le demandent à des moments où la fenêtre ne peut pas
/// répondre : le système demande quel cadre elle aura pendant qu'elle est
/// encore de la taille qu'elle était, et le compositeur apprend comment
/// arrondir ses coins avant qu'elle ait bougé. La seule porte d'entrée et
/// de sortie du plein écran l'écrit, donc il est juste avant que l'une ou
/// l'autre question soit posée.
static TOUT_L_ECRAN: AtomicBool = AtomicBool::new(false);

/// Où elle était et de quoi elle avait l'air avant de prendre l'écran.
///
/// Les deux ensemble : reprendre sa place sans reprendre son cadre la
/// laisserait sans barre de titre au milieu du bureau.
static AVANT: Mutex<Option<(isize, isize, [u8; PLACE])>> = Mutex::new(None);

/// La taille du bloc où le système écrit la place d'une fenêtre.
///
/// Gardé en octets et non dans son type : cette structure appartient à
/// Windows, et ce fichier n'a rien à en lire, seulement à la rendre telle
/// qu'elle a été prise.
#[cfg(windows)]
const PLACE: usize =
    std::mem::size_of::<windows_sys::Win32::UI::WindowsAndMessaging::WINDOWPLACEMENT>();
#[cfg(not(windows))]
const PLACE: usize = 1;

/// Le programme, gardé ici parce que rien n'en donne un à une fenêtre du
/// système : ce qui arrive à celle-ci arrive du système, pas d'une boucle
/// qui saurait à qui parler.
static PROGRAM: Mutex<Option<AppHandle>> = Mutex::new(None);

fn programme() -> Option<AppHandle> {
    PROGRAM.lock().expect("programme de la fenêtre").clone()
}

/// La fenêtre, ou zéro tant qu'elle n'est pas ouverte.
pub fn sienne() -> isize {
    ELLE.load(Ordering::Relaxed)
}

/* ---- L'ouvrir ------------------------------------------------------- */

/// Ouvre la fenêtre, cachée.
///
/// Cachée : ce qui la remplit n'est pas encore dessiné, et une fenêtre
/// montrée avant d'avoir été peinte se voit vide. C'est `montre` qui la
/// découvre, une fois l'accueil posé dedans.
#[cfg(windows)]
pub fn ouvre(app: &AppHandle) -> Result<(), String> {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::HiDpi::GetDpiForSystem;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CW_USEDEFAULT, CreateWindowExW, GetSystemMetrics, IDC_ARROW, LoadCursorW, RegisterClassW,
        SM_CXSCREEN, SM_CYSCREEN, WNDCLASSW, WS_CLIPCHILDREN, WS_OVERLAPPEDWINDOW,
    };

    if sienne() != 0 {
        return Ok(());
    }
    *PROGRAM.lock().expect("programme de la fenêtre") = Some(app.clone());

    let classe = wide("ZyrDesk");
    let titre = wide("ZyrDesk");
    // SAFETY: no argument beyond what is asked for.
    let dpi = unsafe { GetDpiForSystem() };
    let (large, haute) = (pour(OUVERTE.0, dpi as i32), pour(OUVERTE.1, dpi as i32));
    // Au milieu de l'écran principal : c'est là qu'une fenêtre s'ouvre la
    // première fois, et le système ne le fait pas tout seul.
    // SAFETY: no argument beyond the metric asked for.
    let (ecran_large, ecran_haut) =
        unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
    let (x, y) = if ecran_large > large && ecran_haut > haute {
        ((ecran_large - large) / 2, (ecran_haut - haute) / 2)
    } else {
        (CW_USEDEFAULT, CW_USEDEFAULT)
    };

    // SAFETY: une classe déclarée une fois et une fenêtre bâtie dessus,
    // sur le fil qui pompera ses messages.
    let elle = unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let class = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(repond),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: std::ptr::null_mut(),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            // Aucun fond : tout son dedans est une fenêtre fille qui se
            // peint elle-même, et un fond posé par le système serait une
            // couleur de plus, vue le temps d'une image à chaque
            // redimensionnement.
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: classe.as_ptr(),
        };
        RegisterClassW(&class);
        CreateWindowExW(
            0,
            classe.as_ptr(),
            titre.as_ptr(),
            // Rognée par ses filles : l'accueil et l'image d'une session
            // en sont, et sans ça le système peindrait dessous avant
            // qu'elles se peignent dessus.
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            x,
            y,
            large,
            haute,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        )
    };
    if elle.is_null() {
        return Err("la fenêtre de ZyrDesk n'a pas pu s'ouvrir".to_string());
    }
    ELLE.store(elle as isize, Ordering::Relaxed);
    crate::journal::note(&format!(
        "fenêtre ouverte par ZyrDesk, {large}x{haute} px à {} %",
        dpi * 100 / 96
    ));
    Ok(())
}

#[cfg(not(windows))]
pub fn ouvre(_app: &AppHandle) -> Result<(), String> {
    Err("ZyrDesk n'ouvre de fenêtre que sous Windows".to_string())
}

/// Une longueur de page en vrais pixels, sur un écran de cet
/// agrandissement.
fn pour(page: i32, dpi: i32) -> i32 {
    page * dpi / 96
}

/// Un mot dans les caractères que Windows compte, fini par le zéro qu'il
/// cherche.
fn wide(mot: &str) -> Vec<u16> {
    mot.encode_utf16().chain(Some(0)).collect()
}

/* ---- Ce que la fenêtre répond --------------------------------------- */

/// SAFETY: appelée par le système sur le fil qui a fait cette fenêtre,
/// avec les arguments qu'il documente.
#[cfg(windows)]
unsafe extern "system" fn repond(
    window: windows_sys::Win32::Foundation::HWND,
    message: u32,
    holding: windows_sys::Win32::Foundation::WPARAM,
    with: windows_sys::Win32::Foundation::LPARAM,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DefWindowProcW, MINMAXINFO, SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos, WM_CLOSE,
        WM_DPICHANGED, WM_GETMINMAXINFO, WM_SETFOCUS, WM_SIZE,
    };

    match message {
        // La croix veut dire deux choses, et laquelle dépend de ce que la
        // fenêtre montre.
        //
        // Sur une session, elle termine la session et la fenêtre reste :
        // l'image est dedans, et une croix qui ne ferait que ranger la
        // fenêtre laisserait l'ordinateur distant tenu par quelque chose
        // qui n'a plus rien à l'écran pour le rendre.
        //
        // Sur l'accueil, elle range la fenêtre sans rien arrêter. Cet
        // ordinateur peut être joignable sans que personne ne regarde une
        // fenêtre, l'icône près de l'horloge le dit, et « Quitter » là-bas
        // est la seule chose qui arrête le produit.
        WM_CLOSE => {
            if let Some(app) = programme() {
                if crate::floating::a_session_is_up(&app) || crate::session::opening() {
                    // Pendant qu'une session ne fait que s'ouvrir il n'y a
                    // parfois rien à terminer ; la demande n'atteint alors
                    // que le journal, et la fenêtre reste. La ranger
                    // laissait l'ouverture continuer sans être vue, et la
                    // session arrivait en rectangle nu sur le bureau.
                    crate::session::end_it(&app);
                } else {
                    cache();
                }
            }
            0
        }
        // Sa taille a changé : ce qu'elle porte suit, ici et maintenant.
        // Ce qu'une boucle d'événements en raconterait arriverait une file
        // plus tard, et un dedans en retard sur son cadre se voit pendant
        // tout un redimensionnement.
        WM_SIZE => {
            let (large, haute) = ((with & 0xFFFF) as i32, ((with >> 16) & 0xFFFF) as i32);
            let dedans = crate::accueil::sa_toile();
            if dedans != 0 {
                // SAFETY: une fenêtre à nous, posée sur le dedans de
                // celle qui vient de changer de taille.
                unsafe {
                    SetWindowPos(
                        dedans as windows_sys::Win32::Foundation::HWND,
                        std::ptr::null_mut(),
                        0,
                        0,
                        large,
                        haute,
                        SWP_NOACTIVATE | SWP_NOZORDER,
                    )
                };
            }
            if let Some(app) = programme() {
                crate::picture::hold_the_shape(&app);
            }
            0
        }
        // Ce en dessous de quoi elle ne descend pas, compté sur l'écran
        // qu'elle occupe : le plancher est en pixels de page.
        WM_GETMINMAXINFO => {
            // SAFETY: le système passe ici un bloc à lui, vivant le temps
            // de l'appel, dont on n'écrit qu'un champ.
            unsafe {
                let dpi = GetDpiForWindow(window).max(96) as i32;
                let bloc = with as *mut MINMAXINFO;
                (*bloc).ptMinTrackSize.x = pour(JAMAIS_MOINS.0, dpi);
                (*bloc).ptMinTrackSize.y = pour(JAMAIS_MOINS.1, dpi);
            }
            0
        }
        // Elle a changé d'écran, ou son écran a changé d'agrandissement.
        // Le système dit où la poser pour qu'elle garde sa taille
        // apparente, et tout ce qui est compté en vrais pixels se
        // recompte.
        WM_DPICHANGED => {
            // SAFETY: le système passe ici un rectangle à lui, vivant le
            // temps de l'appel.
            let veut = unsafe { *(with as *const RECT) };
            // SAFETY: une fenêtre à nous, posée où le système la veut.
            unsafe {
                SetWindowPos(
                    window,
                    std::ptr::null_mut(),
                    veut.left,
                    veut.top,
                    veut.right - veut.left,
                    veut.bottom - veut.top,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                )
            };
            if let Some(app) = programme() {
                crate::icon::on_the_window(&app);
                crate::accueil::mesure_l_ecran(&app);
            }
            0
        }
        // Le clavier va à ce qui est dessiné dedans : cette fenêtre-ci ne
        // dessine rien et n'a rien à lire.
        // Sauf pendant une session : le clavier appartient alors à
        // l'image, et le lui reprendre en revenant sur la fenêtre serait
        // le retirer à l'ordinateur d'en face.
        WM_SETFOCUS => {
            let dedans = crate::accueil::sa_toile();
            if dedans != 0 && crate::picture::the_engines_window().is_none() {
                // SAFETY: une fenêtre à nous, sur le fil qui la possède.
                unsafe { SetFocus(dedans as windows_sys::Win32::Foundation::HWND) };
            }
            0
        }
        // SAFETY: la réponse du système à tout ce qui n'est pas répondu
        // ici.
        _ => unsafe { DefWindowProcW(window, message, holding, with) },
    }
}

/* ---- La montrer, la ranger ------------------------------------------ */

/// La ramène, où qu'elle ait été laissée.
#[cfg(windows)]
pub fn montre() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        IsIconic, SW_RESTORE, SW_SHOW, SetForegroundWindow, ShowWindow,
    };

    let elle = sienne() as windows_sys::Win32::Foundation::HWND;
    if elle.is_null() {
        return;
    }
    // SAFETY: une fenêtre à nous.
    unsafe {
        ShowWindow(
            elle,
            if IsIconic(elle) != 0 {
                SW_RESTORE
            } else {
                SW_SHOW
            },
        );
        SetForegroundWindow(elle);
    }
}

#[cfg(not(windows))]
pub fn montre() {}

/// La range sans rien arrêter.
#[cfg(windows)]
pub fn cache() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SW_HIDE, ShowWindow};

    let elle = sienne() as windows_sys::Win32::Foundation::HWND;
    if !elle.is_null() {
        // SAFETY: une fenêtre à nous.
        unsafe { ShowWindow(elle, SW_HIDE) };
    }
}

#[cfg(not(windows))]
pub fn cache() {}

/// Si elle est à l'écran : montrée, et pas rangée dans la barre des
/// tâches.
///
/// Les deux ensemble parce que les deux comptent pour la même chose : une
/// fenêtre réduite se dit encore visible, et le bouton flottant posé
/// dessus serait alors la seule chose à l'écran, accroché dans un coin
/// par-dessus le travail de quelqu'un d'autre.
#[cfg(windows)]
pub fn a_l_ecran() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{IsIconic, IsWindowVisible};

    let elle = sienne() as windows_sys::Win32::Foundation::HWND;
    // SAFETY: un numéro de fenêtre, que les appels sont faits pour peser.
    !elle.is_null() && unsafe { IsWindowVisible(elle) != 0 && IsIconic(elle) == 0 }
}

#[cfg(not(windows))]
pub fn a_l_ecran() -> bool {
    false
}

/* ---- Ce qu'elle mesure ---------------------------------------------- */

/// De combien un pixel de page compte sur l'écran où elle est.
#[cfg(windows)]
pub fn echelle() -> f32 {
    use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;

    let elle = sienne() as windows_sys::Win32::Foundation::HWND;
    if elle.is_null() {
        return 1.0;
    }
    // SAFETY: une fenêtre à nous, dont on ne lit qu'une mesure.
    let dpi = unsafe { GetDpiForWindow(elle) };
    if dpi == 0 { 1.0 } else { dpi as f32 / 96.0 }
}

#[cfg(not(windows))]
pub fn echelle() -> f32 {
    1.0
}

/// Ce que son dedans mesure, en vrais pixels.
#[cfg(windows)]
pub fn dedans() -> (u32, u32) {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect;

    let elle = sienne() as windows_sys::Win32::Foundation::HWND;
    let mut place = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: une fenêtre à nous, dont le rectangle est lu dans le nôtre.
    if elle.is_null() || unsafe { GetClientRect(elle, &mut place) } == 0 {
        return (0, 0);
    }
    (place.right.max(0) as u32, place.bottom.max(0) as u32)
}

#[cfg(not(windows))]
pub fn dedans() -> (u32, u32) {
    (0, 0)
}

/// Donne à son dedans cette taille-là, le cadre venant en plus.
#[cfg(windows)]
pub fn pose_le_dedans(large: u32, haute: u32) {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GWL_STYLE, GetWindowLongPtrW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER,
        SetWindowPos,
    };

    let elle = sienne() as windows_sys::Win32::Foundation::HWND;
    if elle.is_null() {
        return;
    }
    let mut veut = RECT {
        left: 0,
        top: 0,
        right: large as i32,
        bottom: haute as i32,
    };
    // SAFETY: une fenêtre à nous, dont on lit les deux styles pour que le
    // système compte le cadre qu'ils demandent autour du dedans voulu.
    unsafe {
        let style = GetWindowLongPtrW(elle, GWL_STYLE) as u32;
        let autres = GetWindowLongPtrW(elle, GWL_EXSTYLE) as u32;
        let dpi = GetDpiForWindow(elle).max(96);
        AdjustWindowRectExForDpi(&mut veut, style, 0, autres, dpi);
        SetWindowPos(
            elle,
            std::ptr::null_mut(),
            0,
            0,
            veut.right - veut.left,
            veut.bottom - veut.top,
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

#[cfg(not(windows))]
pub fn pose_le_dedans(_large: u32, _haute: u32) {}

/* ---- L'agrandir, lui donner l'écran --------------------------------- */

/// L'agrandit à ce que le bureau laisse.
#[cfg(windows)]
pub fn agrandis() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SW_MAXIMIZE, ShowWindow};

    let elle = sienne() as windows_sys::Win32::Foundation::HWND;
    if !elle.is_null() {
        // SAFETY: une fenêtre à nous.
        unsafe { ShowWindow(elle, SW_MAXIMIZE) };
    }
}

#[cfg(not(windows))]
pub fn agrandis() {}

#[cfg(windows)]
pub fn est_agrandie() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::IsZoomed;

    let elle = sienne() as windows_sys::Win32::Foundation::HWND;
    // SAFETY: un numéro de fenêtre, que l'appel est fait pour peser.
    !elle.is_null() && unsafe { IsZoomed(elle) != 0 }
}

#[cfg(not(windows))]
pub fn est_agrandie() -> bool {
    false
}

/// Si elle prend l'écran entier.
pub fn tient_l_ecran() -> bool {
    TOUT_L_ECRAN.load(Ordering::Relaxed)
}

/// Lui donne l'écran entier, ou le lui reprend.
///
/// Sa place et son cadre sont mis de côté ensemble et repris ensemble :
/// une fenêtre qui retrouve sa place sans retrouver sa barre de titre est
/// une fenêtre qu'on ne peut plus attraper.
#[cfg(windows)]
pub fn prend_l_ecran(tout: bool) {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GWL_STYLE, GetWindowLongPtrW, GetWindowPlacement, HWND_TOP, SWP_FRAMECHANGED,
        SWP_NOACTIVATE, SetWindowLongPtrW, SetWindowPlacement, SetWindowPos, WINDOWPLACEMENT,
        WS_CAPTION, WS_EX_CLIENTEDGE, WS_EX_DLGMODALFRAME, WS_EX_STATICEDGE, WS_EX_WINDOWEDGE,
        WS_THICKFRAME,
    };

    let elle = sienne() as windows_sys::Win32::Foundation::HWND;
    if elle.is_null() || TOUT_L_ECRAN.swap(tout, Ordering::Relaxed) == tout {
        return;
    }
    let mut avant = AVANT.lock().expect("place de la fenêtre");
    if tout {
        let mut place: WINDOWPLACEMENT = unsafe { std::mem::zeroed() };
        place.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
        let mut about: MONITORINFO = unsafe { std::mem::zeroed() };
        about.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        // SAFETY: une fenêtre à nous, et deux blocs à nous dont la taille
        // est écrite dedans comme les appels le demandent.
        let (style, autres, lu) = unsafe {
            (
                GetWindowLongPtrW(elle, GWL_STYLE),
                GetWindowLongPtrW(elle, GWL_EXSTYLE),
                GetWindowPlacement(elle, &mut place) != 0
                    && GetMonitorInfoW(
                        MonitorFromWindow(elle, MONITOR_DEFAULTTONEAREST),
                        &mut about,
                    ) != 0,
            )
        };
        if !lu {
            TOUT_L_ECRAN.store(false, Ordering::Relaxed);
            return;
        }
        // SAFETY: la structure du système, recopiée telle quelle pour être
        // rendue telle quelle : ce fichier n'en lit rien.
        let garde: [u8; PLACE] = unsafe { std::mem::transmute(place) };
        *avant = Some((style, autres, garde));

        let sans_cadre = style & !((WS_CAPTION | WS_THICKFRAME) as isize);
        let sans_bord = autres
            & !((WS_EX_DLGMODALFRAME | WS_EX_WINDOWEDGE | WS_EX_CLIENTEDGE | WS_EX_STATICEDGE)
                as isize);
        let ou: RECT = about.rcMonitor;
        // SAFETY: une fenêtre à nous, à qui l'on donne son cadre et sa
        // place, en demandant que le cadre soit recompté.
        unsafe {
            SetWindowLongPtrW(elle, GWL_STYLE, sans_cadre);
            SetWindowLongPtrW(elle, GWL_EXSTYLE, sans_bord);
            SetWindowPos(
                elle,
                HWND_TOP,
                ou.left,
                ou.top,
                ou.right - ou.left,
                ou.bottom - ou.top,
                SWP_FRAMECHANGED | SWP_NOACTIVATE,
            );
        }
        return;
    }

    let Some((style, autres, garde)) = avant.take() else {
        return;
    };
    // SAFETY: la structure du système, rendue telle qu'elle a été prise.
    let place: WINDOWPLACEMENT = unsafe { std::mem::transmute(garde) };
    // SAFETY: une fenêtre à nous, à qui l'on rend son cadre puis sa place.
    unsafe {
        SetWindowLongPtrW(elle, GWL_STYLE, style);
        SetWindowLongPtrW(elle, GWL_EXSTYLE, autres);
        SetWindowPlacement(elle, &place);
        SetWindowPos(
            elle,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED
                | SWP_NOACTIVATE
                | windows_sys::Win32::UI::WindowsAndMessaging::SWP_NOMOVE
                | windows_sys::Win32::UI::WindowsAndMessaging::SWP_NOSIZE
                | windows_sys::Win32::UI::WindowsAndMessaging::SWP_NOZORDER,
        );
    }
}

#[cfg(not(windows))]
pub fn prend_l_ecran(_tout: bool) {}

/* ---- Son cadre ------------------------------------------------------ */

/// Accorde le cadre de la fenêtre au thème.
///
/// Le cadre appartient à Windows et non à nous : c'est la seule partie de
/// la fenêtre que ce programme ne dessine pas, et sans ça une interface
/// claire garderait une barre de titre sombre.
#[cfg(windows)]
pub fn habille(clair: bool) {
    use windows_sys::Win32::Graphics::Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute};

    let elle = sienne() as windows_sys::Win32::Foundation::HWND;
    if elle.is_null() {
        return;
    }
    let sombre: i32 = i32::from(!clair);
    // SAFETY: une fenêtre à nous, et quatre octets à nous dont la taille
    // est dite. Un refus est la réponse d'un Windows trop ancien pour
    // cette barre-là, et n'empêche rien d'autre.
    unsafe {
        DwmSetWindowAttribute(
            elle,
            DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
            (&raw const sombre).cast(),
            std::mem::size_of::<i32>() as u32,
        )
    };
}

#[cfg(not(windows))]
pub fn habille(_clair: bool) {}
