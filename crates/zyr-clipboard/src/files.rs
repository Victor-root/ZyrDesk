//! Walking what somebody dropped on a clipboard into a list of files.
//!
//! The Explorer hands over a handful of paths, and any of them may be a
//! folder holding a thousand more. What crosses is the flat list of the
//! files inside them, each under a path relative to what was copied:
//! `D:\Photos\2026` copied whole travels as `2026/lac.jpg`,
//! `2026/mer.jpg`, and so on. That is what the Explorer itself does when
//! it pastes a folder, and it is what makes the far computer able to
//! choose where the whole thing lands.
//!
//! Nothing here is Windows'. It is `read_dir` and a weight, so it stays
//! compiled and tested on every machine this product is built on, which
//! is where a mistake about what gets copied is worth catching.

// Only the Windows half calls it, there being no clipboard to read
// anywhere else, and the tests below.
#![cfg_attr(not(windows), allow(dead_code))]

use std::path::{Path, PathBuf};

use zyr_proto::clipboard::{Listed, Listing};

/// The most files one copy may name.
///
/// Somebody who selects a whole disk by mistake is the case this exists
/// for. Past this, the list itself starts to be the thing being sent,
/// and what they meant to copy is almost certainly not a hundred
/// thousand files.
pub const MOST_FILES: usize = 10_000;

/// How deep the walk goes into folders inside folders.
///
/// Windows paths stop long before this on their own; what this really
/// guards is a folder that contains itself, which a junction can make
/// and which no length ever stops.
const DEEPEST: usize = 64;

/// What a folder or a file dropped on a clipboard comes to.
///
/// Two lists of the same length and in the same order: what crosses, and
/// where each of those really is on this computer. The second never
/// crosses and could not mean anything if it did, naming disks and
/// folders of this machine alone.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Walked {
    pub listed: Listing,
    pub really: Vec<PathBuf>,
    /// Whether the walk stopped on the ceiling rather than on the end of
    /// what was copied.
    ///
    /// Said out loud where a listing is used, because the alternative is
    /// a folder that arrives looking whole and is not: what is missing
    /// from a copy of ten thousand files is not something anybody spots
    /// by looking.
    pub cut_short: bool,
}

/// Walks what was dropped, folders and all.
///
/// What cannot be read is left out rather than failing the whole: a
/// folder holding one file this account may not open is still a folder
/// worth copying, and the one that was left out is a file the person can
/// see is missing.
pub fn walked(dropped: &[PathBuf]) -> Walked {
    let mut walk = Walked::default();
    let mut listed = Vec::new();
    for item in dropped {
        // Named by its last piece, which is what the Explorer pastes it
        // under: a folder copied whole arrives as that folder.
        let Some(named) = item.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        into(item, named, 0, &mut listed, &mut walk.really);
    }
    walk.cut_short = listed.len() >= MOST_FILES;
    walk.listed = Listing::of(listed);
    walk
}

fn into(at: &Path, named: &str, deep: usize, listed: &mut Vec<Listed>, really: &mut Vec<PathBuf>) {
    if listed.len() >= MOST_FILES || deep > DEEPEST {
        return;
    }
    // Asked of the name and not of what it points at: a shortcut into a
    // folder above itself is how a walk never ends, and what somebody
    // copied is the shortcut and not the whole disk behind it.
    let Ok(about) = at.symlink_metadata() else {
        return;
    };
    if about.file_type().is_symlink() {
        return;
    }
    if about.is_file() {
        if let Some(file) = Listed::new(named, about.len()) {
            listed.push(file);
            really.push(at.to_path_buf());
        }
        return;
    }
    let Ok(inside) = std::fs::read_dir(at) else {
        return;
    };
    // In the order the disk hands them over, which is the Explorer's own
    // order on Windows: what matters is that the two lists stay in step,
    // not which file comes first.
    for entry in inside.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        into(
            &entry.path(),
            &format!("{named}/{name}"),
            deep + 1,
            listed,
            really,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_tree(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "zyr-clipboard-{}-{name}",
            zyr_proto::random::alphanumeric_string(8)
        ));
        std::fs::create_dir_all(root.join("photos").join("2026")).unwrap();
        std::fs::write(root.join("seul.txt"), b"trois").unwrap();
        std::fs::write(root.join("photos").join("lac.jpg"), b"12345678").unwrap();
        std::fs::write(root.join("photos").join("2026").join("mer.jpg"), b"12").unwrap();
        root
    }

    #[test]
    fn un_fichier_seul_traverse_sous_son_nom() {
        let root = a_tree("un-fichier");
        let walk = walked(&[root.join("seul.txt")]);

        assert_eq!(walk.listed.files().len(), 1);
        assert_eq!(walk.listed.files()[0].path(), "seul.txt");
        assert_eq!(walk.listed.whole(), 5);
        assert_eq!(walk.really, vec![root.join("seul.txt")]);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn un_dossier_traverse_entier_et_sous_son_nom() {
        // C'est ce que fait l'explorateur quand il colle un dossier :
        // le dossier réapparaît, avec ce qu'il y a dedans.
        let root = a_tree("un-dossier");
        let walk = walked(&[root.join("photos")]);

        let mut chemins: Vec<&str> = walk.listed.files().iter().map(|f| f.path()).collect();
        chemins.sort_unstable();
        assert_eq!(chemins, vec!["photos/2026/mer.jpg", "photos/lac.jpg"]);
        assert_eq!(walk.listed.whole(), 10);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn les_deux_listes_restent_en_face_l_une_de_l_autre() {
        // C'est ce qui fait qu'on sait quel fichier ouvrir quand l'autre
        // ordinateur demande le troisième de la liste. Deux listes qui
        // se décalent enverraient le mauvais fichier sous le bon nom.
        let root = a_tree("en-face");
        let walk = walked(&[root.join("photos"), root.join("seul.txt")]);

        assert_eq!(walk.listed.files().len(), walk.really.len());
        for (rank, file) in walk.listed.files().iter().enumerate() {
            let vraiment = &walk.really[rank];
            assert!(
                vraiment.ends_with(file.name()),
                "{vraiment:?} pour {file:?}"
            );
            assert_eq!(std::fs::metadata(vraiment).unwrap().len(), file.bytes());
        }
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn ce_qui_n_existe_pas_est_laisse_de_cote_sans_faire_tomber_le_reste() {
        // Un fichier effacé entre la copie et la lecture est la moitié
        // d'une seconde d'écart, et il ne doit pas coûter les autres.
        let root = a_tree("manquant");
        let walk = walked(&[root.join("nulle-part.txt"), root.join("seul.txt")]);

        assert_eq!(walk.listed.files().len(), 1);
        assert_eq!(walk.listed.files()[0].path(), "seul.txt");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn une_copie_coupee_au_plafond_le_dit() {
        // Sans ça, un dossier arrive en ayant l'air entier sans l'être,
        // et ce qui manque d'une copie de dix mille fichiers n'est pas
        // une chose qu'on repère en regardant.
        let root = a_tree("plafond");
        assert!(!walked(&[root.join("photos")]).cut_short);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn rien_de_depose_ne_donne_rien() {
        let walk = walked(&[]);
        assert!(walk.listed.is_empty());
        assert!(walk.really.is_empty());
    }
}
