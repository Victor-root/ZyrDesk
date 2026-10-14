//! Puts the program's Windows resource in the window's executable.
//!
//! The manifest, under the number Windows reads in a program: the modern
//! controls, real pixels on every screen, and the rights of whoever
//! launches it, no more. The icon under 32512, the number of an
//! application icon: it is under that one that the system looks for it
//! for the taskbar, and under that one that `icon.rs` asks for it again
//! at the exact size it needs. And the name the Task Manager shows,
//! which is the package's description: ZyrDesk runs several programs on
//! one machine, and each one has to say which it is.
//!
//! The version comes from the package, and the same tool as the other
//! programs of the product writes all of it.

fn main() {
    // Redone when the drawing or the manifest changes, which Cargo
    // cannot know on its own: without these lines the executable keeps
    // what it was built with, and a new logo never reaches the taskbar.
    println!("cargo:rerun-if-changed=../../packaging/brand/zyrdesk.ico");
    println!("cargo:rerun-if-changed=zyrdesk.manifest");
    #[cfg(windows)]
    winresource::WindowsResource::new()
        .set_icon_with_id("../../packaging/brand/zyrdesk.ico", "32512")
        .set_manifest_file("zyrdesk.manifest")
        .set_language(0x0409)
        .set(
            "FileDescription",
            &std::env::var("CARGO_PKG_DESCRIPTION").expect("CARGO_PKG_DESCRIPTION"),
        )
        .set("CompanyName", "ZyrDesk")
        .set("ProductName", "ZyrDesk")
        .set("InternalName", "ZyrDesk")
        .set("OriginalFilename", "ZyrDesk.exe")
        .compile()
        .expect("the window's resource is embedded");
}
