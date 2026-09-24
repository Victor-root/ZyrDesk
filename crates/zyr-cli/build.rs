//! Puts the product's mark and name on the command line tool.
//!
//! The one program of the product that a person starts themselves, by
//! typing its name. So it lives in the list of programs like the others,
//! and has to say there which one it is rather than show the name of its
//! file.
//!
//! Nothing else is set here: what the file says about itself, its name
//! and its version, comes from the package.

fn main() {
    // Redone when the drawing changes, which Cargo cannot know on its
    // own: without this the executable keeps the icon it was built with
    // and a new logo never arrives.
    println!("cargo:rerun-if-changed=../../packaging/brand/zyrdesk.ico");
    #[cfg(windows)]
    winresource::WindowsResource::new()
        .set_icon("../../packaging/brand/zyrdesk.ico")
        .set(
            "FileDescription",
            &std::env::var("CARGO_PKG_DESCRIPTION").expect("CARGO_PKG_DESCRIPTION"),
        )
        .compile()
        .expect("icône de l'outil en ligne de commande");
}
