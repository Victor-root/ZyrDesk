fn main() {
    // L'icône du programme est une ressource Windows fabriquée ici, à
    // partir de ce fichier-là. Sans cette ligne, Cargo ne refait pas ce
    // travail quand le dessin change : l'exécutable garde l'ancienne
    // icône, et on cherche longtemps pourquoi le nouveau logo n'arrive
    // pas dans la barre des tâches.
    println!("cargo:rerun-if-changed=../../packaging/brand/zyrdesk.ico");
    tauri_build::build()
}
