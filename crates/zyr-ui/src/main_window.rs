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
//! écrites ici, et en vrais pixels partout ailleurs : `scale` fait le
//! passage, une fois, au moment de bâtir et à chaque changement d'écran.

// Une fenêtre est une chose du système, et ce produit n'en ouvre que sous
// Windows. Ailleurs, chaque réponse est celle d'une fenêtre qui n'existe
// pas, pour que tout le reste du fichier reste compilé et vérifié.
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

use crate::app::App;

/// Ce sous quoi ce module classe ses lignes du journal.
const TAG: &str = "window";

/// Écrit une ligne sous l'étiquette de ce module.
fn note(what: &str) {
    crate::journal::note_about(TAG, what);
}

/// Ce que la fenêtre fait de large et de haut en s'ouvrant, et ce en
/// dessous de quoi elle ne descend pas, en pixels de page.
///
/// Le plancher n'est pas une préférence : c'est la place qu'il faut pour
/// que les cartes des ordinateurs tiennent en ligne et que le menu d'une
/// session ait où s'ouvrir.
const OPENS_AT: (i32, i32) = (1060, 720);
const NEVER_SMALLER: (i32, i32) = (880, 600);

/// La fenêtre, telle que le système la connaît.
static HANDLE: AtomicIsize = AtomicIsize::new(0);
/// Si elle prend l'écran entier.
///
/// Retenu plutôt que relu sur la fenêtre, parce que les endroits qui le
/// demandent le demandent à des moments où la fenêtre ne peut pas
/// répondre : le système demande quel cadre elle aura pendant qu'elle est
/// encore de la taille qu'elle était, et le compositeur apprend comment
/// arrondir ses coins avant qu'elle ait bougé. La seule porte d'entrée et
/// de sortie du plein écran l'écrit, donc il est juste avant que l'une ou
/// l'autre question soit posée.
static FULL_SCREEN: AtomicBool = AtomicBool::new(false);

/// Où elle était et de quoi elle avait l'air avant de prendre l'écran.
///
/// Les deux ensemble : reprendre sa place sans reprendre son cadre la
/// laisserait sans barre de titre au milieu du bureau.
static BEFORE_FULL_SCREEN: Mutex<Option<(isize, isize, [u8; PLACE])>> = Mutex::new(None);

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
static PROGRAM: Mutex<Option<App>> = Mutex::new(None);

fn program() -> Option<App> {
    PROGRAM.lock().expect("programme de la fenêtre").clone()
}

/// La fenêtre, ou zéro tant qu'elle n'est pas ouverte.
pub fn handle() -> isize {
    HANDLE.load(Ordering::Relaxed)
}

/// Le nom de sa classe, sous lequel un second ZyrDesk la retrouve.
const CLASS_NAME: &str = "ZyrDesk";

/// Le message par lequel un second ZyrDesk demande à celui qui tourne de
/// se montrer, plutôt que d'ouvrir une deuxième fenêtre.
#[cfg(windows)]
const SHOW_YOURSELF: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP;

/// Demande à la fenêtre du ZyrDesk qui tourne déjà de revenir.
#[cfg(windows)]
pub fn show_the_one_running() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW};

    let class_name: Vec<u16> = CLASS_NAME.encode_utf16().chain(Some(0)).collect();
    // SAFETY: un nom qui survit à l'appel, et un message qui n'appartient
    // qu'à nous, posté à une fenêtre de notre propre classe.
    unsafe {
        let already = FindWindowW(class_name.as_ptr(), std::ptr::null());
        if !already.is_null() {
            PostMessageW(already, SHOW_YOURSELF, 0, 0);
        }
    }
}

#[cfg(not(windows))]
pub fn show_the_one_running() {}

/* ---- L'ouvrir ------------------------------------------------------- */

/// Ouvre la fenêtre, cachée.
///
/// Cachée : ce qui la remplit n'est pas encore dessiné, et une fenêtre
/// montrée avant d'avoir été peinte se voit vide. C'est `show` qui la
/// découvre, une fois l'accueil posé dedans.
#[cfg(windows)]
pub fn open(app: &App) -> Result<(), String> {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::HiDpi::GetDpiForSystem;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CW_USEDEFAULT, CreateWindowExW, GetSystemMetrics, IDC_ARROW, LoadCursorW, RegisterClassW,
        SM_CXSCREEN, SM_CYSCREEN, WNDCLASSW, WS_CLIPCHILDREN, WS_OVERLAPPEDWINDOW,
    };

    if handle() != 0 {
        return Ok(());
    }
    *PROGRAM.lock().expect("programme de la fenêtre") = Some(app.clone());

    let class_name = wide(CLASS_NAME);
    let title = wide(CLASS_NAME);
    // SAFETY: no argument beyond what is asked for.
    let dpi = unsafe { GetDpiForSystem() };
    let (width, height) = (
        scaled(OPENS_AT.0, dpi as i32),
        scaled(OPENS_AT.1, dpi as i32),
    );
    // Au milieu de l'écran principal : c'est là qu'une fenêtre s'ouvre la
    // première fois, et le système ne le fait pas tout seul.
    // SAFETY: no argument beyond the metric asked for.
    let (screen_width, screen_height) =
        unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
    let (x, y) = if screen_width > width && screen_height > height {
        ((screen_width - width) / 2, (screen_height - height) / 2)
    } else {
        (CW_USEDEFAULT, CW_USEDEFAULT)
    };

    // SAFETY: une classe déclarée une fois et une fenêtre bâtie dessus,
    // sur le fil qui pompera ses messages.
    let hwnd = unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let class = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(answers),
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
            lpszClassName: class_name.as_ptr(),
        };
        RegisterClassW(&class);
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            title.as_ptr(),
            // Rognée par ses filles : l'accueil et l'image d'une session
            // en sont, et sans ça le système peindrait dessous avant
            // qu'elles se peignent dessus.
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            x,
            y,
            width,
            height,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        )
    };
    if hwnd.is_null() {
        return Err("la fenêtre de ZyrDesk n'a pas pu s'ouvrir".to_string());
    }
    HANDLE.store(hwnd as isize, Ordering::Relaxed);
    note(&format!(
        "fenêtre ouverte par ZyrDesk, {width}x{height} px à {} %",
        dpi * 100 / 96
    ));
    Ok(())
}

#[cfg(not(windows))]
pub fn open(_app: &App) -> Result<(), String> {
    Err("ZyrDesk n'ouvre de fenêtre que sous Windows".to_string())
}

/// Une longueur de page en vrais pixels, sur un écran de cet
/// agrandissement.
fn scaled(page: i32, dpi: i32) -> i32 {
    page * dpi / 96
}

/// Un mot dans les caractères que Windows compte, fini par le zéro qu'il
/// cherche.
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

/* ---- Ce que la fenêtre répond --------------------------------------- */

/// SAFETY: appelée par le système sur le fil qui a fait cette fenêtre,
/// avec les arguments qu'il documente.
#[cfg(windows)]
unsafe extern "system" fn answers(
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
            if let Some(app) = program() {
                if crate::floating::a_session_is_up(&app) || crate::session::opening() {
                    // Pendant qu'une session ne fait que s'ouvrir il n'y a
                    // parfois rien à terminer ; la demande n'atteint alors
                    // que le journal, et la fenêtre reste. La ranger
                    // laissait l'ouverture continuer sans être vue, et la
                    // session arrivait en rectangle nu sur le bureau.
                    crate::session::end_it(&app);
                } else {
                    hide();
                }
            }
            0
        }
        // Sa taille a changé : ce qu'elle porte suit, ici et maintenant.
        // Ce qu'une boucle d'événements en raconterait arriverait une file
        // plus tard, et un dedans en retard sur son cadre se voit pendant
        // tout un redimensionnement.
        WM_SIZE => {
            say_whether_it_goes_down_or_up(holding);
            let (width, height) = ((with & 0xFFFF) as i32, ((with >> 16) & 0xFFFF) as i32);
            let inside = crate::home::its_canvas();
            if inside != 0 {
                // SAFETY: une fenêtre à nous, posée sur le dedans de
                // celle qui vient de changer de taille.
                unsafe {
                    SetWindowPos(
                        inside as windows_sys::Win32::Foundation::HWND,
                        std::ptr::null_mut(),
                        0,
                        0,
                        width,
                        height,
                        SWP_NOACTIVATE | SWP_NOZORDER,
                    )
                };
            }
            // Remise au tour suivant et non faite ici : la tenir à sa
            // forme la redimensionne, ce qui ferait revenir ce
            // message-ci pendant qu'on y répond.
            if let Some(app) = program() {
                let held = app.clone();
                let _ = app.run_on_main_thread(move || crate::picture::hold_the_shape(&held));
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
                let info = with as *mut MINMAXINFO;
                (*info).ptMinTrackSize.x = scaled(NEVER_SMALLER.0, dpi);
                (*info).ptMinTrackSize.y = scaled(NEVER_SMALLER.1, dpi);
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
            let wanted = unsafe { *(with as *const RECT) };
            // SAFETY: une fenêtre à nous, posée où le système la veut.
            unsafe {
                SetWindowPos(
                    window,
                    std::ptr::null_mut(),
                    wanted.left,
                    wanted.top,
                    wanted.right - wanted.left,
                    wanted.bottom - wanted.top,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                )
            };
            crate::icon::on_the_window();
            if let Some(app) = program() {
                crate::home::measure_the_screen(&app);
            }
            0
        }
        // Le clavier va à ce qui est dessiné dedans : cette fenêtre-ci ne
        // dessine rien et n'a rien à lire.
        // Sauf pendant une session : le clavier appartient alors à
        // l'image, et le lui reprendre en revenant sur la fenêtre serait
        // le retirer à l'ordinateur d'en face.
        WM_SETFOCUS => {
            let inside = crate::home::its_canvas();
            if inside != 0 && crate::picture::the_engines_window().is_none() {
                // SAFETY: une fenêtre à nous, sur le fil qui la possède.
                unsafe { SetFocus(inside as windows_sys::Win32::Foundation::HWND) };
            }
            0
        }
        _ => {
            // Un second ZyrDesk vient d'être lancé : celui qui tourne se
            // montre, et l'autre s'arrête sans rien ouvrir.
            if message == SHOW_YOURSELF {
                show();
                return 0;
            }
            // SAFETY: la réponse du système à tout ce qui n'est pas
            // répondu ici.
            unsafe { DefWindowProcW(window, message, holding, with) }
        }
    }
}

/// Où la fenêtre en était la dernière fois qu'on l'a dit.
#[cfg(windows)]
static MINIMIZED: AtomicBool = AtomicBool::new(false);

/// Dit quand la fenêtre descend dans la barre des tâches et quand elle en
/// remonte, avec ce qui tient le premier plan à cet instant.
///
/// Deux lignes par aller-retour et pas une de plus. Une fenêtre qui ne
/// remonte pas est le genre d'ennui qu'on ne peut pas photographier, et
/// ces deux lignes disent les deux seules choses qui le départagent : si
/// l'ordre de remonter est seulement arrivé jusqu'ici, et à qui
/// appartenait le premier plan quand elle est descendue. Le second
/// répond à lui seul du cas où le système ne redemande rien parce qu'il
/// nous croit déjà devant.
#[cfg(windows)]
fn say_whether_it_goes_down_or_up(what: usize) {
    use windows_sys::Win32::UI::WindowsAndMessaging::SIZE_MINIMIZED;

    let minimized = what as u32 == SIZE_MINIMIZED;
    if MINIMIZED.swap(minimized, Ordering::Relaxed) == minimized {
        return;
    }
    note(&format!(
        "fenêtre {} ; le premier plan est {}",
        if minimized {
            "rangée dans la barre des tâches"
        } else {
            "ressortie de la barre des tâches"
        },
        crate::picture::the_front_in_words()
    ));
}

/* ---- La montrer, la ranger ------------------------------------------ */

/// La ramène, où qu'elle ait été laissée.
#[cfg(windows)]
pub fn show() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        IsIconic, SW_RESTORE, SW_SHOW, SetForegroundWindow, ShowWindow,
    };

    let hwnd = handle() as windows_sys::Win32::Foundation::HWND;
    if hwnd.is_null() {
        return;
    }
    // SAFETY: une fenêtre à nous.
    unsafe {
        ShowWindow(
            hwnd,
            if IsIconic(hwnd) != 0 {
                SW_RESTORE
            } else {
                SW_SHOW
            },
        );
        SetForegroundWindow(hwnd);
    }
}

#[cfg(not(windows))]
pub fn show() {}

/// La range sans rien arrêter.
#[cfg(windows)]
pub fn hide() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SW_HIDE, ShowWindow};

    let hwnd = handle() as windows_sys::Win32::Foundation::HWND;
    if !hwnd.is_null() {
        // SAFETY: une fenêtre à nous.
        unsafe { ShowWindow(hwnd, SW_HIDE) };
    }
}

#[cfg(not(windows))]
pub fn hide() {}

/// Si elle est à l'écran : montrée, et pas rangée dans la barre des
/// tâches.
///
/// Les deux ensemble parce que les deux comptent pour la même chose : une
/// fenêtre réduite se dit encore visible, et le bouton flottant posé
/// dessus serait alors la seule chose à l'écran, accroché dans un coin
/// par-dessus le travail de quelqu'un d'autre.
#[cfg(windows)]
pub fn on_screen() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{IsIconic, IsWindowVisible};

    let hwnd = handle() as windows_sys::Win32::Foundation::HWND;
    // SAFETY: un numéro de fenêtre, que les appels sont faits pour peser.
    !hwnd.is_null() && unsafe { IsWindowVisible(hwnd) != 0 && IsIconic(hwnd) == 0 }
}

#[cfg(not(windows))]
pub fn on_screen() -> bool {
    false
}

/* ---- Ce qu'elle mesure ---------------------------------------------- */

/// De combien un pixel de page compte sur l'écran où elle est.
#[cfg(windows)]
pub fn scale() -> f32 {
    use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;

    let hwnd = handle() as windows_sys::Win32::Foundation::HWND;
    if hwnd.is_null() {
        return 1.0;
    }
    // SAFETY: une fenêtre à nous, dont on ne lit qu'une mesure.
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    if dpi == 0 { 1.0 } else { dpi as f32 / 96.0 }
}

#[cfg(not(windows))]
pub fn scale() -> f32 {
    1.0
}

/// Ce que son dedans mesure, en vrais pixels.
#[cfg(windows)]
pub fn inside() -> (u32, u32) {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect;

    let hwnd = handle() as windows_sys::Win32::Foundation::HWND;
    let mut place = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: une fenêtre à nous, dont le rectangle est lu dans le nôtre.
    if hwnd.is_null() || unsafe { GetClientRect(hwnd, &mut place) } == 0 {
        return (0, 0);
    }
    (place.right.max(0) as u32, place.bottom.max(0) as u32)
}

#[cfg(not(windows))]
pub fn inside() -> (u32, u32) {
    (0, 0)
}

/// Donne à son dedans cette taille-là, le cadre venant en plus.
#[cfg(windows)]
pub fn set_the_inside(width: u32, height: u32) {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GWL_STYLE, GetWindowLongPtrW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER,
        SetWindowPos,
    };

    let hwnd = handle() as windows_sys::Win32::Foundation::HWND;
    if hwnd.is_null() {
        return;
    }
    let mut wanted = RECT {
        left: 0,
        top: 0,
        right: width as i32,
        bottom: height as i32,
    };
    // SAFETY: une fenêtre à nous, dont on lit les deux styles pour que le
    // système compte le cadre qu'ils demandent autour du dedans voulu.
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        let others = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        let dpi = GetDpiForWindow(hwnd).max(96);
        AdjustWindowRectExForDpi(&mut wanted, style, 0, others, dpi);
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            0,
            0,
            wanted.right - wanted.left,
            wanted.bottom - wanted.top,
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

#[cfg(not(windows))]
pub fn set_the_inside(_large: u32, _height: u32) {}

/* ---- L'agrandir, lui donner l'écran --------------------------------- */

/// L'agrandit à ce que le bureau laisse.
#[cfg(windows)]
pub fn maximize() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SW_MAXIMIZE, ShowWindow};

    let hwnd = handle() as windows_sys::Win32::Foundation::HWND;
    if !hwnd.is_null() {
        // SAFETY: une fenêtre à nous.
        unsafe { ShowWindow(hwnd, SW_MAXIMIZE) };
    }
}

#[cfg(not(windows))]
pub fn maximize() {}

/// La rend à la taille qu'elle avait avant d'être agrandie.
///
/// Seulement si elle l'est : autrement l'appel ne ferait que la remettre
/// au premier plan, pour rien.
///
/// Et sans que le système le joue, parce que ce n'est pas un état où
/// l'on s'arrête : la fenêtre prend l'écran dans la foulée, et cette
/// taille-là n'est qu'un passage. Or le compositeur joue les changements
/// d'état à son rythme et non au nôtre. ShowWindow rend la main tout de
/// suite, l'animation continue derrière, et la fenêtre a déjà pris
/// l'écran pendant qu'il la rapetisse encore. Le bureau distant, qui est
/// une fenêtre portée par celle-ci et donc dessinée dans sa composition,
/// s'en va avec elle : c'est l'agrandissement bizarre à l'intérieur du
/// flux, au premier plein écran d'une session.
///
/// Rendu juste après : « agrandir » depuis la barre de titre est un
/// geste où l'animation du système est voulue, et où tout un pan de
/// picture.rs compte dessus.
#[cfg(windows)]
fn restore_its_size() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SW_RESTORE, ShowWindow};

    // Agrandie veut déjà dire qu'elle existe.
    if is_maximized() {
        let hwnd = handle() as windows_sys::Win32::Foundation::HWND;
        play_the_transitions(hwnd, false);
        // SAFETY: une fenêtre à nous.
        unsafe { ShowWindow(hwnd, SW_RESTORE) };
        play_the_transitions(hwnd, true);
    }
}

/// Demande au compositeur de jouer, ou de ne pas jouer, ce que cette
/// fenêtre change d'état.
///
/// Un refus est la réponse d'un Windows qui n'a pas ce réglage, et ne
/// coûte que l'animation qu'on voulait éviter.
#[cfg(windows)]
fn play_the_transitions(hwnd: windows_sys::Win32::Foundation::HWND, yes: bool) {
    use windows_sys::Win32::Graphics::Dwm::{
        DWMWA_TRANSITIONS_FORCEDISABLED, DwmSetWindowAttribute,
    };

    let disabled: i32 = i32::from(!yes);
    // SAFETY: une fenêtre à nous, et quatre octets à nous dont la taille
    // est dite.
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_TRANSITIONS_FORCEDISABLED as u32,
            (&raw const disabled).cast(),
            std::mem::size_of::<i32>() as u32,
        )
    };
}

#[cfg(windows)]
pub fn is_maximized() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::IsZoomed;

    let hwnd = handle() as windows_sys::Win32::Foundation::HWND;
    // SAFETY: un numéro de fenêtre, que l'appel est fait pour peser.
    !hwnd.is_null() && unsafe { IsZoomed(hwnd) != 0 }
}

#[cfg(not(windows))]
pub fn is_maximized() -> bool {
    false
}

/// Si elle prend l'écran entier.
pub fn holds_the_screen() -> bool {
    FULL_SCREEN.load(Ordering::Relaxed)
}

/// Lui donne l'écran entier, ou le lui reprend.
///
/// Sa place et son cadre sont mis de côté ensemble et repris ensemble :
/// une fenêtre qui retrouve sa place sans retrouver sa barre de titre est
/// une fenêtre qu'on ne peut plus attraper.
#[cfg(windows)]
pub fn take_the_screen(whole: bool) {
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

    let hwnd = handle() as windows_sys::Win32::Foundation::HWND;
    if hwnd.is_null() || FULL_SCREEN.swap(whole, Ordering::Relaxed) == whole {
        return;
    }
    let mut before = BEFORE_FULL_SCREEN.lock().expect("place de la fenêtre");
    if whole {
        let mut place: WINDOWPLACEMENT = unsafe { std::mem::zeroed() };
        place.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
        let mut about: MONITORINFO = unsafe { std::mem::zeroed() };
        about.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        // SAFETY: une fenêtre à nous, et deux blocs à nous dont la taille
        // est écrite dedans comme les appels le demandent.
        let (style, others, read) = unsafe {
            (
                GetWindowLongPtrW(hwnd, GWL_STYLE),
                GetWindowLongPtrW(hwnd, GWL_EXSTYLE),
                GetWindowPlacement(hwnd, &mut place) != 0
                    && GetMonitorInfoW(
                        MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST),
                        &mut about,
                    ) != 0,
            )
        };
        if !read {
            FULL_SCREEN.store(false, Ordering::Relaxed);
            return;
        }
        // SAFETY: la structure du système, recopiée telle quelle pour être
        // rendue telle quelle : ce fichier n'en lit rien.
        let kept: [u8; PLACE] = unsafe { std::mem::transmute(place) };
        *before = Some((style, others, kept));

        // Une fenêtre agrandie l'est encore une fois son cadre retiré, et
        // le système la tient à la place qu'il lui a donnée : elle y
        // revient au premier recompte de cadre qui suit, lequel arrive
        // aussitôt puisque retirer le cadre consiste à le demander. On
        // voit alors une fenêtre sans bordure aux mesures de l'agrandi,
        // débordant de l'écran des quelques pixels que Windows réserve à
        // la bordure d'une fenêtre agrandie, et amputée en bas de la
        // hauteur de la barre des tâches. Elle est donc rendue à sa
        // taille avant de prendre l'écran ; l'agrandi est déjà dans le
        // relevé qui la lui rendra tout à l'heure.
        restore_its_size();

        // Relu après ça, et non repris de plus haut : l'agrandi se lit
        // dans le style lui-même, et réécrire celui d'avant redirait au
        // système qu'elle est agrandie alors qu'elle ne l'est plus.
        // SAFETY: une fenêtre à nous, dont on relit le style.
        let current_style = unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) };

        let without_frame = current_style & !((WS_CAPTION | WS_THICKFRAME) as isize);
        let without_border = others
            & !((WS_EX_DLGMODALFRAME | WS_EX_WINDOWEDGE | WS_EX_CLIENTEDGE | WS_EX_STATICEDGE)
                as isize);
        let area: RECT = about.rcMonitor;
        // SAFETY: une fenêtre à nous, à qui l'on donne son cadre et sa
        // place, en demandant que le cadre soit recompté.
        unsafe {
            SetWindowLongPtrW(hwnd, GWL_STYLE, without_frame);
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, without_border);
            SetWindowPos(
                hwnd,
                HWND_TOP,
                area.left,
                area.top,
                area.right - area.left,
                area.bottom - area.top,
                SWP_FRAMECHANGED | SWP_NOACTIVATE,
            );
        }
        return;
    }

    let Some((style, others, kept)) = before.take() else {
        return;
    };
    // SAFETY: la structure du système, rendue telle qu'elle a été prise.
    let place: WINDOWPLACEMENT = unsafe { std::mem::transmute(kept) };
    // SAFETY: une fenêtre à nous, à qui l'on rend son cadre puis sa place.
    unsafe {
        SetWindowLongPtrW(hwnd, GWL_STYLE, style);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, others);
        SetWindowPlacement(hwnd, &place);
        SetWindowPos(
            hwnd,
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
pub fn take_the_screen(_whole: bool) {}

/* ---- Son cadre ------------------------------------------------------ */

/// Accorde le cadre de la fenêtre au thème.
///
/// Le cadre appartient à Windows et non à nous : c'est la seule partie de
/// la fenêtre que ce programme ne dessine pas, et sans ça une interface
/// claire garderait une barre de titre sombre.
#[cfg(windows)]
pub fn dress_the_frame(light: bool) {
    use windows_sys::Win32::Graphics::Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute};

    let hwnd = handle() as windows_sys::Win32::Foundation::HWND;
    if hwnd.is_null() {
        return;
    }
    let dark: i32 = i32::from(!light);
    // SAFETY: une fenêtre à nous, et quatre octets à nous dont la taille
    // est dite. Un refus est la réponse d'un Windows trop ancien pour
    // cette barre-là, et n'empêche rien d'autre.
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
            (&raw const dark).cast(),
            std::mem::size_of::<i32>() as u32,
        )
    };
}

#[cfg(not(windows))]
pub fn dress_the_frame(_light: bool) {}
