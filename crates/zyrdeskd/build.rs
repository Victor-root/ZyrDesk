//! Puts the product's mark on the service.
//!
//! The service is the one binary of the product that never draws a
//! window, so its icon is only ever seen in one place: the list of
//! running programs, where somebody is looking for what ZyrDesk is doing
//! on their machine. Without one it shows there as an anonymous
//! executable among the product's own, which is exactly the moment a name
//! and a mark are worth having.
//!
//! Et le nom que la liste des programmes affiche à côté, qui est la
//! description du paquet : ZyrDesk fait tourner plusieurs programmes sur
//! une machine, et chacun doit dire lequel il est. Sans ce nom-là,
//! Windows montre celui du fichier, qui ne veut rien dire pour la
//! personne qui le lit.
//!
//! Rien d'autre n'est posé ici : ce que le fichier dit de lui-même, son
//! nom et sa version, vient du paquet.

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
        .expect("icône du service");
}
