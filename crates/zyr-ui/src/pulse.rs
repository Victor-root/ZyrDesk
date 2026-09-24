//! Le battement de ce qui bouge à l'écran.
//!
//! Une horloge du système ne bat pas plus fin que son tic, quinze
//! millisecondes et demie, et elle arrondit au tic suivant : un battement
//! demandé toutes les seize millisecondes tombe en réalité à trente et
//! une, soit trente-deux images par seconde sur un écran qui en montre
//! soixante. C'est visible à l'oeil nu sur tout ce qui glisse.
//!
//! Ce qui bouge bat donc au rythme du compositeur de Windows, qui est
//! celui de l'écran : un fil attend la fin de chaque composition et
//! réveille les fenêtres qui animent quelque chose. Demander une image de
//! plus que ce que l'écran montre ne se verrait pas et coûterait pour
//! rien ; en demander moins se voit tout de suite.
//!
//! Un seul fil pour tout le produit : le compositeur bat pour tout le
//! monde à la fois, et deux fils qui l'attendent attendraient le même
//! instant.

use std::sync::{Condvar, Mutex, OnceLock};

use windows_sys::Win32::Foundation::HWND;

/// Les fenêtres qui bougent en ce moment, et le message qui redessine
/// chacune. La poignée est retenue en nombre : c'est ce qui traverse un
/// fil.
static MOVING: Mutex<Vec<(isize, u32)>> = Mutex::new(Vec::new());

/// De quoi rendormir le fil quand plus rien ne bouge : sans lui, il
/// tournerait à vide au rythme de l'écran pendant que le produit ne fait
/// rien.
static WAKE: Condvar = Condvar::new();

/// Le temps d'une image quand le compositeur ne répond pas. Il ne se
/// laisse plus arrêter depuis Windows 8, mais un fil qui tournerait sans
/// jamais attendre prendrait un coeur entier.
const FRAME: std::time::Duration = std::time::Duration::from_millis(16);

/// Fait battre cette fenêtre-là, qui recevra ce message à chaque image
/// jusqu'à ce qu'elle demande à s'arrêter.
///
/// Redemander pour une fenêtre qui bat déjà ne fait rien.
pub fn beat(window: HWND, message: u32) {
    let mut moving = MOVING.lock().expect("rythme");
    let this_one = window as isize;
    if moving.iter().any(|(w, _)| *w == this_one) {
        return;
    }
    moving.push((this_one, message));
    start();
    WAKE.notify_one();
}

/// Arrête le battement de cette fenêtre. Rien si elle ne battait pas.
pub fn stop(window: HWND) {
    let this_one = window as isize;
    MOVING
        .lock()
        .expect("rythme")
        .retain(|(w, _)| *w != this_one);
}

/// Le fil qui attend le compositeur, lancé à la première chose qui bouge
/// et gardé ensuite : il dort tant que rien ne bouge.
fn start() {
    static THREAD: OnceLock<()> = OnceLock::new();
    THREAD.get_or_init(|| {
        std::thread::Builder::new()
            .name("rythme".to_string())
            .spawn(run)
            .expect("fil du rythme");
    });
}

fn run() {
    use windows_sys::Win32::Graphics::Dwm::DwmFlush;
    use windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW;

    loop {
        let beating = {
            let mut moving = MOVING.lock().expect("rythme");
            while moving.is_empty() {
                moving = WAKE.wait(moving).expect("rythme");
            }
            moving.clone()
        };
        // SAFETY: rien à lui passer, et l'attente n'appartient à aucune
        // fenêtre.
        if unsafe { DwmFlush() } < 0 {
            std::thread::sleep(FRAME);
        }
        for (window, message) in beating {
            // SAFETY: un message déposé dans la file d'une fenêtre. Elle
            // peut avoir disparu entre-temps, et le système répond alors
            // que non sans que rien ne soit touché.
            unsafe { PostMessageW(window as HWND, message, 0, 0) };
        }
    }
}
