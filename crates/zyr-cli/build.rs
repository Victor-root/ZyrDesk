//! Puts the product's mark and name on the command line tool.
//!
//! Le seul programme du produit qu'une personne lance elle-même en
//! tapant son nom. Il vit donc dans la liste des programmes comme les
//! autres, et doit y dire lequel il est plutôt que d'y montrer le nom de
//! son fichier.
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
        .expect("icône de l'outil en ligne de commande");
}
