//! Puts the product's mark on the service.
//!
//! The service is the one binary of the product that never draws a
//! window, so its icon is only ever seen in one place: the list of
//! running programs, where somebody is looking for what ZyrDesk is doing
//! on their machine. Without one it shows there as an anonymous
//! executable among the product's own, which is exactly the moment a name
//! and a mark are worth having.
//!
//! Nothing else of the resource is set here. What the file says about
//! itself, its name and its version, comes from the package, and the
//! description beside it is the one written there.

fn main() {
    // Redone when the drawing changes, which Cargo cannot know on its
    // own: without this the executable keeps the icon it was built with
    // and a new logo never arrives.
    println!("cargo:rerun-if-changed=../../packaging/brand/zyrdesk.ico");
    #[cfg(windows)]
    winresource::WindowsResource::new()
        .set_icon("../../packaging/brand/zyrdesk.ico")
        .compile()
        .expect("icône du service");
}
