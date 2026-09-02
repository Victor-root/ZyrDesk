//! What the client engine follows while it streams.
//!
//! Everything a stream is made of reaches the engine on its command line
//! and is read once. Our own build follows a file as well
//! (`patches/MANIFEST.md`): one line saying what the stream should be,
//! replaced whole when a setting changes, and read a few times a second
//! from its own loop. A line that differs from what the stream is makes
//! the engine make its stream over where it stands, same window and same
//! process, which is what lets a session change size or codec without
//! anybody reopening anything. The rate alone changes nothing over here:
//! the far engine is told it directly, and the line only says what the
//! next stream is to announce.
//!
//! The same shape as the line of statistics the engine writes: `key=value`
//! fields with spaces between them. Written here and read there, and
//! nowhere else.

use std::fs;
use std::io;
use std::path::Path;

use zyr_proto::paths;
use zyr_proto::session::SessionSettings;

/// The line, as the engine reads it.
///
/// The codec in the words the command line takes, because the engine
/// reads it with the same table it reads the command line with.
pub fn line(settings: &SessionSettings) -> String {
    format!(
        "width={} height={} fps={} bitrate={} codec={}",
        settings.width,
        settings.height,
        settings.fps,
        settings.bitrate_kbps,
        settings.codec.engine_value()
    )
}

/// Writes it where the engine reads it.
pub fn write(settings: &SessionSettings) -> io::Result<()> {
    write_at(&paths::session_wanted(), settings)
}

/// Replaced whole, and never written in place: the engine reads between
/// two writes, and a line caught half written would be a stream made over
/// on half a description. Written beside and moved over, which the system
/// does in one go.
fn write_at(path: &Path, settings: &SessionSettings) -> io::Result<()> {
    let beside = path.with_extension("new");
    fs::write(&beside, format!("{}\n", line(settings)))?;
    fs::rename(&beside, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zyr_proto::session::Codec;

    #[test]
    fn the_line_says_what_the_engine_reads() {
        // La forme exacte : c'est ce que le moteur relit, mot pour mot.
        let settings = SessionSettings {
            width: 2560,
            height: 1440,
            fps: 120,
            bitrate_kbps: 30_000,
            codec: Codec::Hevc,
            ..SessionSettings::default()
        };
        assert_eq!(
            line(&settings),
            "width=2560 height=1440 fps=120 bitrate=30000 codec=HEVC"
        );
        // Et le codec dans les mots de la ligne de commande, que le
        // moteur relit avec la même table.
        for codec in [Codec::Auto, Codec::H264, Codec::Hevc, Codec::Av1] {
            let said = line(&SessionSettings {
                codec,
                ..SessionSettings::default()
            });
            assert!(
                said.ends_with(&format!("codec={}", codec.engine_value())),
                "{said}"
            );
        }
    }

    #[test]
    fn the_file_is_replaced_whole() {
        let folder = std::env::temp_dir().join(format!(
            "zyrdesk-follow-{}",
            zyr_proto::random::alphanumeric_string(8)
        ));
        fs::create_dir_all(&folder).unwrap();
        let path = folder.join("session-wanted.txt");

        let first = SessionSettings::default();
        write_at(&path, &first).unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            format!("{}\n", line(&first))
        );

        // Réécrit par-dessus, entier, sans rien laisser à côté : le
        // moteur ne doit jamais tomber sur une ligne à moitié écrite ni
        // sur un fichier de travail.
        let second = SessionSettings {
            bitrate_kbps: 45_000,
            ..first
        };
        write_at(&path, &second).unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            format!("{}\n", line(&second))
        );
        assert!(!path.with_extension("new").exists());

        let _ = fs::remove_dir_all(&folder);
    }
}
