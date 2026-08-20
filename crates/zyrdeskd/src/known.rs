//! The computers written down by hand.
//!
//! Everything else on the home screen comes from the local network
//! announcing itself. On a network that carries no announcement, a
//! computer named once by hand would have to be named again at every
//! session, address and fingerprint both, which is exactly the copying
//! this product exists to remove. Written down here, it stays on the
//! screen like any other and is one click away for good.
//!
//! What is kept is only what a person cannot be asked to remember: where
//! the computer is, what it is called, and what it is recognised by.
//! Nothing here decides who may come in; that stays in the list of
//! authorised devices, which this one never touches.
//!
//! The file is meant to be readable and correctable in a text editor. It
//! holds no secret: an address and a fingerprint are both public, and
//! knowing them opens nothing.

use std::fs;
use std::io;
use std::path::Path;

use zyr_transport::Fingerprint;

/// A computer somebody wrote down.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Known {
    pub fingerprint: Fingerprint,
    /// Where to reach it, exactly as it was written.
    pub host: String,
    /// What to call it on screen.
    pub name: String,
}

/// Reads the computers written down. A missing file simply means none.
pub fn read(path: &Path) -> io::Result<Vec<Known>> {
    let contents = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    Ok(contents.lines().filter_map(read_one).collect())
}

/// One line, or nothing when it says nothing usable.
///
/// A line nobody can read is skipped rather than raised: this list is a
/// convenience, and one bad line in it must not cost the whole screen.
/// The list of authorised devices is the one that refuses what it cannot
/// read, because being wrong there means letting somebody in.
fn read_one(line: &str) -> Option<Known> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let mut pieces = line.splitn(3, char::is_whitespace);
    let fingerprint: Fingerprint = pieces.next()?.parse().ok()?;
    let host = pieces.next()?.trim().to_string();
    if host.is_empty() {
        return None;
    }
    // The name takes whatever is left, spaces and all: computers are
    // called « PC de Victor » far more often than anyone expects.
    let name = pieces.next().unwrap_or("").trim().to_string();
    let name = if name.is_empty() { host.clone() } else { name };
    Some(Known {
        fingerprint,
        host,
        name,
    })
}

/// Writes a computer down, replacing whatever was known of it.
///
/// Replaced rather than added twice: the same computer written down
/// again is a correction, most often an address that has changed.
pub fn add(path: &Path, computer: Known) -> io::Result<()> {
    let mut computers = read(path)?;
    computers.retain(|known| known.fingerprint != computer.fingerprint);
    computers.push(computer);
    write(path, &computers)
}

/// Takes a computer off the list. `false` when it was not on it.
pub fn remove(path: &Path, fingerprint: Fingerprint) -> io::Result<bool> {
    let mut computers = read(path)?;
    let before = computers.len();
    computers.retain(|known| known.fingerprint != fingerprint);
    if computers.len() == before {
        return Ok(false);
    }
    write(path, &computers)?;
    Ok(true)
}

fn write(path: &Path, computers: &[Known]) -> io::Result<()> {
    let mut text = String::from(
        "# Ordinateurs ajoutés à la main, pour les réseaux qui ne portent\n\
         # pas les annonces. Une ligne par ordinateur :\n\
         #   empreinte adresse nom\n",
    );
    for known in computers {
        // One computer per line, whatever its name was pasted with: a
        // line break in a name would split it into a line nobody can
        // read and cost the computers after it.
        text.push_str(&format!(
            "{} {} {}\n",
            known.fingerprint,
            known.host,
            known.name.replace(['\n', '\r'], " ")
        ));
    }
    // Whole or not at all: this list and the door it mirrors must not be
    // resettable by a power cut.
    zyr_proto::files::replace(path, &text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fingerprint(seed: u8) -> Fingerprint {
        format!("{seed:02x}").repeat(32).parse().unwrap()
    }

    fn somewhere(what: &str) -> std::path::PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "zyrdeskd-known-{}-{what}",
            zyr_proto::random::alphanumeric_string(8)
        ));
        folder.join("known-computers.conf")
    }

    #[test]
    fn a_computer_written_down_comes_back_whole() {
        let path = somewhere("aller-retour");
        assert!(read(&path).unwrap().is_empty());

        let computer = Known {
            fingerprint: fingerprint(1),
            host: "192.168.1.20".to_string(),
            // Un nom d'ordinateur porte des espaces bien plus souvent
            // qu'on ne le croit, et il est écrit en dernier pour cela.
            name: "PC de Victor".to_string(),
        };
        add(&path, computer.clone()).unwrap();
        assert_eq!(read(&path).unwrap(), vec![computer]);

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn writing_the_same_computer_again_corrects_it() {
        // Une adresse qui change est le cas courant : deux lignes pour la
        // même machine donneraient deux cartes dont une ne marche plus.
        let path = somewhere("correction");
        add(
            &path,
            Known {
                fingerprint: fingerprint(1),
                host: "192.168.1.20".to_string(),
                name: "PC fixe".to_string(),
            },
        )
        .unwrap();
        add(
            &path,
            Known {
                fingerprint: fingerprint(1),
                host: "192.168.1.42".to_string(),
                name: "PC fixe".to_string(),
            },
        )
        .unwrap();

        let computers = read(&path).unwrap();
        assert_eq!(computers.len(), 1);
        assert_eq!(computers[0].host, "192.168.1.42");

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_computer_can_be_taken_off_and_saying_so_twice_is_not_an_error() {
        let path = somewhere("oubli");
        add(
            &path,
            Known {
                fingerprint: fingerprint(1),
                host: "192.168.1.20".to_string(),
                name: "PC fixe".to_string(),
            },
        )
        .unwrap();
        assert!(remove(&path, fingerprint(1)).unwrap());
        assert!(read(&path).unwrap().is_empty());
        // Déjà oublié : l'état demandé, atteint.
        assert!(!remove(&path, fingerprint(1)).unwrap());

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_line_nobody_can_read_is_skipped_and_the_others_stay() {
        // Ce fichier se corrige à la main : une ligne ratée ne doit pas
        // faire disparaître l'écran d'accueil.
        let text = format!(
            "# un commentaire\n\
             \n\
             pas-une-empreinte 192.168.1.1 Machin\n\
             {} 192.168.1.20 PC de Victor\n\
             {}\n\
             {} 192.168.1.30\n",
            fingerprint(1),
            fingerprint(2),
            fingerprint(3)
        );
        let path = somewhere("lignes");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, text).unwrap();

        let computers = read(&path).unwrap();
        assert_eq!(computers.len(), 2, "{computers:?}");
        assert_eq!(computers[0].name, "PC de Victor");
        // Sans nom, l'adresse fait l'affaire : une carte sans titre ne se
        // reconnaît pas.
        assert_eq!(computers[1].name, "192.168.1.30");

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }
}
