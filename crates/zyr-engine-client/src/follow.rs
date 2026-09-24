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
use zyr_proto::session::{Pointer, SessionSettings};

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
    replaced(path, &line(settings))
}

/// Tells the engine what shape to give its own pointer.
///
/// Its own file and not the line above, which is what the stream is to
/// be: a line that differs from the stream makes the engine build it
/// again, and this changes every time a hand crosses a text field. One
/// word, replaced the same way, read by the engine as often as it likes
/// and costing nothing when it has not moved.
///
/// Answers whether anything was written, so that a shape unchanged since
/// the last one costs no disk at all: this is asked several times a
/// second for the length of a session.
pub fn point_like(shape: Pointer) -> io::Result<bool> {
    point_like_at(&paths::session_pointer(), shape)
}

fn point_like_at(path: &Path, shape: Pointer) -> io::Result<bool> {
    if fs::read_to_string(path).is_ok_and(|written| written.trim() == shape.word()) {
        return Ok(false);
    }
    replaced(path, shape.word())?;
    Ok(true)
}

/// Names the ordinary pointer, there being no far one to follow any
/// more.
///
/// Written and not erased, and the difference matters: erasing says
/// nothing to an engine that is still running, and it stays under
/// whatever shape the last answer left it with. That is an hourglass
/// over a machine that is not busy, and, since the far computer can
/// answer that it is drawing its own pointer, it is also a session with
/// no pointer at all. Naming the arrow says it to both: the engine
/// still up, and the next one to read the file.
pub fn point_like_nothing() {
    let _ = point_like_at(&paths::session_pointer(), Pointer::default());
}

/// One line, put in place whole.
fn replaced(path: &Path, line: &str) -> io::Result<()> {
    let beside = path.with_extension("new");
    fs::write(&beside, format!("{line}\n"))?;
    fs::rename(&beside, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zyr_proto::session::Codec;

    #[test]
    fn the_line_says_what_the_engine_reads() {
        // The exact form: it is what the engine reads back, word for
        // word.
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
        // And the codec in the words of the command line, which the
        // engine reads back with the same table.
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

        // Written over, whole, leaving nothing beside it: the engine
        // must never come across a half-written line nor a working
        // file.
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

    #[test]
    fn the_pointer_shape_is_written_only_when_it_changes() {
        // The shape is asked for several times a second for a whole
        // session: rewriting the file every time would wear the disk for
        // an identical word, and make the engine read it again for
        // nothing.
        let folder = std::env::temp_dir().join(format!(
            "zyrdesk-pointer-{}",
            zyr_proto::random::alphanumeric_string(8)
        ));
        fs::create_dir_all(&folder).unwrap();
        let path = folder.join("session-pointer.txt");

        assert!(point_like_at(&path, Pointer::Text).unwrap());
        assert_eq!(fs::read_to_string(&path).unwrap(), "text\n");
        assert!(!point_like_at(&path, Pointer::Text).unwrap());
        assert!(point_like_at(&path, Pointer::Wait).unwrap());
        assert_eq!(fs::read_to_string(&path).unwrap(), "wait\n");
        // And nothing beside it: the engine reads between two
        // writes.
        assert!(!path.with_extension("new").exists());

        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn no_longer_following_the_pointer_gives_back_the_ordinary_arrow() {
        // That is what `point_like_nothing` does, and it writes instead
        // of erasing: an engine still running does not read an absence,
        // it would stay under the last shape received. An hourglass over
        // a machine doing nothing, or, when the far computer had just
        // answered that it was drawing its own pointer, no pointer at
        // all.
        let folder = std::env::temp_dir().join(format!(
            "zyrdesk-pointer-{}",
            zyr_proto::random::alphanumeric_string(8)
        ));
        fs::create_dir_all(&folder).unwrap();
        let path = folder.join("session-pointer.txt");

        assert!(point_like_at(&path, Pointer::Theirs).unwrap());
        assert!(point_like_at(&path, Pointer::default()).unwrap());
        assert_eq!(fs::read_to_string(&path).unwrap(), "arrow\n");

        let _ = fs::remove_dir_all(&folder);
    }
}
