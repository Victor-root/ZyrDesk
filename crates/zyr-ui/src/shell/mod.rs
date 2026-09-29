//! The program around the windows.
//!
//! What every other part of the window stands on: the program itself and
//! its main window, the icon by the clock, the journal, the theme, what
//! the person chose and the keyboard shortcuts they set, the folders the
//! window opens, and the questions every part asks of the service.

pub mod app;
pub mod desk;
pub mod folders;
pub mod icon;
pub mod journal;
// The window itself, opened by this program.
pub mod main_window;
pub mod service;
pub mod settings;
pub mod shortcuts;
pub mod theme;
pub mod tray;
// What every window of this program says to Windows the same way.
#[cfg(windows)]
pub mod win32;
