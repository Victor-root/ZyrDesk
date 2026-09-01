//! Le programme lui-même : ce qu'il garde, ce qui l'attend, et le fil
//! qui possède ses fenêtres.
//!
//! C'était le dernier service que la boîte à outils rendait. Il tient en
//! trois choses.
//!
//! **Une poignée.** Ce que tout ce qui fait partie du programme se passe
//! de main en main pour retrouver ce que le programme garde. Une seule
//! chose derrière, partagée : la copier ne copie rien.
//!
//! **Une boîte aux lettres.** Une fenêtre qui ne montre rien, dont le
//! seul rôle est de porter du travail jusqu'au fil qui possède les
//! autres. Une fenêtre et non un message au fil lui-même, et ce n'est pas
//! un détail : Windows jette les messages adressés à un fil pendant qu'il
//! déplace une fenêtre, et c'est précisément pendant qu'on déplace la
//! fenêtre que l'image d'une session doit la suivre.
//!
//! **Une boucle.** Le fil principal prend les messages du système et les
//! rend à qui ils sont adressés, tant que le programme tourne.

// Une boucle de messages et une boîte aux lettres sont des choses du
// système, et ce produit ne tourne que sous Windows. Ailleurs, chacune
// répond ce que répond un programme sans fenêtre, pour que tout le reste
// reste compilé et vérifié.
#![cfg_attr(not(windows), allow(dead_code, unused_imports))]

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicIsize, Ordering};

use crate::floating::Floating;
use crate::picture::Picture;
use crate::tray::Shown;

/// Le programme, tel que tout ce qui en fait partie le tient.
///
/// Ce qu'une boîte à outils appelait « poignée ». Ce qu'il y a derrière
/// est ce que le produit garde d'une session à l'autre : ce que le bouton
/// flottant suit, ce que l'image tient, et ce que l'icône près de
/// l'horloge a dit la dernière fois.
#[derive(Clone)]
pub struct App(Arc<Dedans>);

#[derive(Default)]
struct Dedans {
    floating: Floating,
    picture: Picture,
    shown: Shown,
}

impl App {
    /// Le programme au premier instant, avant que rien ne tourne.
    pub fn neuf() -> Self {
        App(Arc::new(Dedans::default()))
    }

    pub fn floating(&self) -> &Floating {
        &self.0.floating
    }

    pub fn picture(&self) -> &Picture {
        &self.0.picture
    }

    pub fn shown(&self) -> &Shown {
        &self.0.shown
    }

    /// Fait faire ce travail au fil qui possède les fenêtres.
    ///
    /// Une fenêtre appartient au fil qui l'a faite : la déplacer, la
    /// redimensionner ou lui poser un cadre depuis un autre fil ne marche
    /// pas, ou marche jusqu'au jour où ça ne marche plus.
    pub fn run_on_main_thread(
        &self,
        travail: impl FnOnce() + Send + 'static,
    ) -> Result<(), String> {
        porte(Box::new(travail))
    }
}

/// Ce qui attend le fil principal.
type Travail = Box<dyn FnOnce() + Send>;

static A_FAIRE: Mutex<Vec<Travail>> = Mutex::new(Vec::new());

/// La boîte aux lettres, telle que le système la connaît.
static COURRIER: AtomicIsize = AtomicIsize::new(0);

/// Le message par lequel on lui dit qu'il y a du courrier, et celui par
/// lequel on lui dit que c'est fini.
#[cfg(windows)]
const PORTE: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP;
#[cfg(windows)]
const FINIR: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 1;

#[cfg(windows)]
fn porte(travail: Travail) -> Result<(), String> {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW;

    let courrier = COURRIER.load(Ordering::Relaxed) as HWND;
    if courrier.is_null() {
        return Err("le fil principal n'a pas encore de boîte aux lettres".to_string());
    }
    A_FAIRE
        .lock()
        .expect("travail du fil principal")
        .push(travail);
    // SAFETY: une fenêtre à nous, à qui l'on poste un message qui
    // n'appartient qu'à nous.
    if unsafe { PostMessageW(courrier, PORTE, 0, 0) } == 0 {
        return Err("le fil principal ne prend plus de courrier".to_string());
    }
    Ok(())
}

#[cfg(not(windows))]
fn porte(_travail: Travail) -> Result<(), String> {
    Err("il n'y a pas de fil de fenêtres hors de Windows".to_string())
}

/// Ouvre la boîte aux lettres. À faire sur le fil principal, avant tout
/// le reste.
#[cfg(windows)]
pub fn ouvre_le_courrier() -> Result<(), String> {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, HWND_MESSAGE, RegisterClassW, WNDCLASSW,
    };

    let classe: Vec<u16> = "ZyrDeskCourrier".encode_utf16().chain(Some(0)).collect();
    // SAFETY: une classe déclarée une fois et une fenêtre bâtie dessus,
    // sur le fil qui pompera ses messages. Elle ne montre rien : une
    // fenêtre dont le parent est celui-ci n'est jamais dessinée.
    let courrier = unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let class = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(recoit),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: std::ptr::null_mut(),
            hCursor: std::ptr::null_mut(),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: classe.as_ptr(),
        };
        RegisterClassW(&class);
        CreateWindowExW(
            0,
            classe.as_ptr(),
            std::ptr::null(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        )
    };
    if courrier.is_null() {
        return Err("le fil principal n'a pas pu ouvrir de boîte aux lettres".to_string());
    }
    COURRIER.store(courrier as isize, Ordering::Relaxed);
    Ok(())
}

#[cfg(not(windows))]
pub fn ouvre_le_courrier() -> Result<(), String> {
    Err("il n'y a pas de fil de fenêtres hors de Windows".to_string())
}

/// SAFETY: appelée par le système sur le fil qui a fait cette fenêtre,
/// avec les arguments qu'il documente.
#[cfg(windows)]
unsafe extern "system" fn recoit(
    window: windows_sys::Win32::Foundation::HWND,
    message: u32,
    holding: windows_sys::Win32::Foundation::WPARAM,
    with: windows_sys::Win32::Foundation::LPARAM,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::UI::WindowsAndMessaging::{DefWindowProcW, PostQuitMessage};

    match message {
        PORTE => {
            // Sorti du verrou avant d'être fait : un travail qui en
            // porterait un autre reprendrait un verrou qu'on tient
            // encore, et le fil principal s'arrêterait pour de bon.
            let travaux = std::mem::take(&mut *A_FAIRE.lock().expect("travail du fil principal"));
            for travail in travaux {
                travail();
            }
            0
        }
        FINIR => {
            // SAFETY: rien d'autre que le mot qui arrête la boucle.
            unsafe { PostQuitMessage(0) };
            0
        }
        // SAFETY: la réponse du système à tout ce qui n'est pas répondu
        // ici.
        _ => unsafe { DefWindowProcW(window, message, holding, with) },
    }
}

/// Prend les messages du système et les rend à qui ils sont adressés,
/// tant que le programme tourne.
#[cfg(windows)]
pub fn tourne() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, MSG, TranslateMessage,
    };

    // SAFETY: un bloc à nous, que le système remplit à chaque tour.
    let mut message: MSG = unsafe { std::mem::zeroed() };
    // SAFETY: le bloc ci-dessus, et rien d'autre. Un zéro dit que le
    // programme s'arrête, un moins un que la file est cassée : dans les
    // deux cas il n'y a plus rien à attendre.
    while unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) } > 0 {
        // SAFETY: le message qui vient d'arriver, traduit puis rendu.
        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

#[cfg(not(windows))]
pub fn tourne() {}

/// Arrête le programme.
///
/// Demandé au fil principal, seul à pouvoir arrêter sa propre boucle.
#[cfg(windows)]
pub fn quitte() {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW;

    let courrier = COURRIER.load(Ordering::Relaxed) as HWND;
    if !courrier.is_null() {
        // SAFETY: une fenêtre à nous, à qui l'on poste un message qui
        // n'appartient qu'à nous.
        unsafe { PostMessageW(courrier, FINIR, 0, 0) };
    }
}

#[cfg(not(windows))]
pub fn quitte() {}

/* ---- Un seul ZyrDesk à la fois -------------------------------------- */

/// Le verrou qui dit qu'un ZyrDesk tourne déjà, tenu tant qu'il tourne.
static VERROU: AtomicIsize = AtomicIsize::new(0);

/// Le nom du verrou. Local et non global : c'est une fenêtre par personne
/// connectée, pas une par machine.
const UN_SEUL: &str = r"Local\ZyrDesk";

/// Si un ZyrDesk tourne déjà, et alors lui demande de se montrer.
///
/// Deux ZyrDesk à la fois, ce sont deux boutons flottants sur la même
/// session. Celui qui en lance un deuxième voulait revoir sa fenêtre :
/// c'est ce qu'il obtient.
#[cfg(windows)]
pub fn deja_ouvert() -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError};
    use windows_sys::Win32::System::Threading::CreateMutexW;

    let nom: Vec<u16> = UN_SEUL.encode_utf16().chain(Some(0)).collect();
    // SAFETY: un nom qui survit à l'appel, et un verrou tenu jusqu'à la
    // fin du programme, qui le rend en s'arrêtant.
    let (verrou, deja) = unsafe {
        let verrou = CreateMutexW(std::ptr::null(), 1, nom.as_ptr());
        (verrou, GetLastError() == ERROR_ALREADY_EXISTS)
    };
    if verrou.is_null() {
        // Rien ne répond : plutôt démarrer que ne pas démarrer.
        return false;
    }
    if !deja {
        VERROU.store(verrou as isize, Ordering::Relaxed);
        return false;
    }
    // SAFETY: un verrou que cet appel vient de rendre, refermé une fois.
    unsafe { CloseHandle(verrou) };
    crate::fenetre::montre_celle_qui_tourne();
    true
}

#[cfg(not(windows))]
pub fn deja_ouvert() -> bool {
    false
}

/* ---- Les écrans ----------------------------------------------------- */

/// Dit au système que ce programme compte en vrais pixels, sur chaque
/// écran.
///
/// Avant qu'une seule fenêtre existe, parce que c'est à ce moment-là que
/// le système décide : sans ça il agrandirait lui-même ce que nous
/// dessinons déjà à la bonne taille, et tout serait flou.
#[cfg(windows)]
pub fn compte_en_vrais_pixels() {
    use windows_sys::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
    };

    // SAFETY: rien qu'un mot au système sur ce programme-ci. Un refus
    // veut dire qu'il a déjà été dit, ce qui est la même chose.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
}

#[cfg(not(windows))]
pub fn compte_en_vrais_pixels() {}

/* ---- Ce qui tourne sans bloquer le fil des fenêtres ------------------ */

/// Le moteur des tâches, fait une fois et gardé.
///
/// Tout ce qui parle au service passe par un tuyau, et un tuyau
/// s'attend : rien de tout ça n'a le droit d'arrêter le fil qui dessine.
static MOTEUR: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();

fn moteur() -> &'static tokio::runtime::Runtime {
    MOTEUR.get_or_init(|| {
        tokio::runtime::Runtime::new().expect("le moteur des tâches n'a pas pu démarrer")
    })
}

/// Lance une tâche, qui vivra sa vie.
pub fn spawn<F>(tache: F) -> tokio::task::JoinHandle<F::Output>
where
    F: std::future::Future + Send + 'static,
    F::Output: Send + 'static,
{
    moteur().spawn(tache)
}

/// Lance sur un fil où l'attente est permise ce qui attend pour de bon.
pub fn spawn_blocking<F, R>(travail: F) -> tokio::task::JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    moteur().spawn_blocking(travail)
}

/// Attend une tâche depuis un fil qui n'en est pas une.
pub fn block_on<F: std::future::Future>(tache: F) -> F::Output {
    moteur().block_on(tache)
}
