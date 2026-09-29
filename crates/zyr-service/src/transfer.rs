//! Files crossing, a piece at a time, while a session runs beside them.
//!
//! What a clipboard holds of a file is its name. The names cross at once
//! and weigh nothing, however many gigabytes they stand for; the bytes
//! cross here, and only once somebody pastes them somewhere.
//!
//! # What keeps the picture out of it
//!
//! One piece is in flight at a time, and nothing else is needed. A piece
//! is asked for, it arrives, the next one is asked for: the file can
//! never occupy more of the link than one piece, whatever it weighs and
//! however fast the link is. A faster link carries the file faster and
//! leaves the picture exactly where it was, because there was never a
//! queue of pieces to get behind.
//!
//! # Where the bytes land
//!
//! In a folder of the product's own, under its data, and never straight
//! where the person is pasting. Windows does that last copy itself, out
//! of what this product hands it, at the moment and to the place the
//! person chose; a product writing into somebody's Documents folder by
//! itself would be a product deciding something that was never asked of
//! it.
//!
//! The folder goes when the paste is over, and the paste is over when
//! nothing has come for it in a while. Not when the session goes: a link
//! that blinks closes one way and opens another a moment later, and a
//! transfer tied to the first would be four gigabytes thrown away at
//! eighty per cent for a hiccup. What is kept meanwhile is exactly what
//! lets the next way carry on from the piece that was reached.

use std::io::{Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

use zyr_proto::clipboard::{HowFar, Listed, Listing, Stamp, weighed};
use zyr_proto::log::Log;
use zyr_proto::paths;
use zyr_tunnel::aside::{A_PIECE, Given, Wanted};

/// What this module's lines are filed under.
const TAG: &str = "files";

/// The one transfer coming into this computer, when there is one.
///
/// One at a time, because one clipboard holds one thing: pasting
/// something else replaces what was on it, and a transfer of what is no
/// longer on the clipboard is a transfer nobody is waiting for.
static COMING: Mutex<Option<Coming>> = Mutex::new(None);

/// A transfer under way into this computer.
struct Coming {
    /// Which copy this is bringing in.
    ///
    /// Kept so that being told again about the same paste is nothing:
    /// whoever notices a paste notices it at every turn of a loop, and
    /// starting over each time would be a transfer that never got past
    /// its first piece.
    stamp: Stamp,
    listed: Listing,
    /// How far each file has been written, in the order the listing names
    /// them. A file that is done sits at its own weight.
    done: Vec<u64>,
    /// Which file is being asked for right now.
    ///
    /// Kept rather than worked out at every turn: the alternative is
    /// walking the whole list a few times a second to find the first file
    /// that is not finished, which for ten thousand files is a walk
    /// nobody needs to take.
    at: usize,
    where_they_land: PathBuf,
    since: Instant,
    /// When a piece last landed.
    ///
    /// What says whether anybody is still serving this paste. The session
    /// that started it is not the answer: a link that blinks closes the
    /// way and opens another, and a transfer tied to the first would be
    /// four gigabytes thrown away for a hiccup.
    last_piece: Instant,
    /// What was last said out loud about it, so that a line is written
    /// when it starts, when it ends, and at each tenth of the way, and
    /// not four times a second.
    said: u8,
}

/// Starts bringing those files in, unless they are already coming.
///
/// Said again at every turn by whoever is watching for a paste, and doing
/// nothing the second time is the whole point: a transfer started afresh
/// four times a second would never get past its first piece.
///
/// What was on its way before is dropped: one clipboard holds one thing,
/// and what is no longer on it is not being pasted by anybody.
pub fn coming_in(stamp: Stamp, listed: &Listing, log: &Log) -> Result<(), String> {
    let log = &log.about(TAG);
    let mut held = COMING.lock().expect("transfer under way");
    if held.as_ref().is_some_and(|coming| coming.stamp == stamp) {
        return Ok(());
    }

    let where_they_land = paths::pasted();
    // Emptied and not added to. What is in there is what a previous
    // paste left, and a file of the same name half written by that one
    // would be read as this one's.
    let _ = std::fs::remove_dir_all(&where_they_land);
    std::fs::create_dir_all(&where_they_land)
        .map_err(|e| format!("the folder of received files does not open: {e}"))?;

    log.write(&format!(
        "bringing in {} from the far computer",
        listed.in_words()
    ));
    *held = Some(Coming {
        stamp,
        done: vec![0; listed.files().len()],
        listed: listed.clone(),
        at: 0,
        where_they_land,
        since: Instant::now(),
        last_piece: Instant::now(),
        said: 0,
    });
    say_how_far(held.as_mut().expect("the transfer just opened"), false, log);
    Ok(())
}

/// The next piece this computer is still waiting for, when it is waiting
/// for one.
///
/// Nothing when no transfer is under way and nothing when the one that is
/// has all its bytes, which are the two ways of saying the far computer
/// may stop sending.
pub fn what_is_still_wanted() -> Option<Wanted> {
    let coming = COMING.lock().expect("transfer under way");
    let coming = coming.as_ref()?;
    let rank = coming.at;
    // Nothing past the end of the list, which is a transfer with all its
    // bytes: the far computer reads that as « you may stop sending ».
    coming.listed.at(rank)?;
    Some(Wanted {
        rank: rank as u32,
        from: coming.done[rank],
        how_many: A_PIECE as u32,
    })
}

/// Writes a piece that arrived, and says whether the transfer is done.
///
/// A piece that does not start where this computer was waiting is
/// dropped rather than written: it is an answer to a question asked
/// before the clipboard changed, and writing it would put the bytes of
/// one copy inside the file of another.
///
/// A refusal here is the end of the transfer and not the end of that one
/// piece. What refuses is a disk, and a disk does not change its mind
/// between two turns of a loop that turns four times a second: asking
/// again would be the same refusal written in the journal for as long as
/// the session lasts. So what was coming in is dropped, and the far
/// computer is told to stop sending on the very next answer.
pub fn take(given: &Given, log: &Log) -> Result<bool, String> {
    let log = &log.about(TAG);
    let mut held = COMING.lock().expect("transfer under way");
    let Some(coming) = held.as_mut() else {
        return Ok(true);
    };
    let rank = given.rank as usize;
    let Some(file) = coming.listed.at(rank) else {
        return Err(format!("a piece of file {rank}, which does not exist"));
    };
    if coming.done.get(rank) != Some(&given.from) {
        return Ok(false);
    }

    if let Err(refused) = write_it(coming, file, given) {
        drop_it(&mut held, "cannot be written down, and stops here", log);
        return Err(refused);
    }
    coming.done[rank] = given.from + given.bytes.len() as u64;
    coming.last_piece = Instant::now();
    // A piece shorter than one asked for is the end of that file, and an
    // empty one is a file that was already whole: both move on, and
    // neither is a fault. Without the second, a file whose weight changed
    // between the copy and the paste would be asked for for ever.
    if coming.done[rank] >= file.bytes() || given.bytes.len() < A_PIECE {
        coming.at = rank + 1;
    }
    let over = coming.at >= coming.listed.files().len();
    say_how_far(coming, over, log);
    Ok(over)
}

/// Puts a piece where its file lands, making the folders above it.
fn write_it(coming: &Coming, file: &Listed, given: &Given) -> Result<(), String> {
    let lands = coming.where_they_land.join(file.path());
    if let Some(folder) = lands.parent() {
        std::fs::create_dir_all(folder)
            .map_err(|e| format!("the folder of {} does not open: {e}", file.path()))?;
    }
    let mut writing = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(given.from == 0)
        .open(&lands)
        .map_err(|e| format!("{} cannot be written: {e}", file.path()))?;
    writing
        .seek(SeekFrom::Start(given.from))
        .and_then(|_| writing.write_all(&given.bytes))
        .map_err(|e| format!("{} cannot be written: {e}", file.path()))
}

/// How far the files being pasted here have got, while their bytes are
/// still on their way.
///
/// What the interface draws its bar from, asking for it on the control
/// channel. Nothing once every byte is here: what is left to wait for is
/// Windows' own copying, which has a window of its own, and a bar left
/// full would say a transfer is still under way.
pub fn how_far() -> Option<HowFar> {
    let coming = COMING.lock().expect("transfer under way");
    let coming = coming.as_ref()?;
    coming.listed.at(coming.at)?;
    Some(reached(coming))
}

/// How far a transfer has got, off the one in hand.
fn reached(coming: &Coming) -> HowFar {
    HowFar {
        done: coming.done.iter().sum(),
        whole: coming.listed.whole(),
        files: coming.listed.files().len(),
    }
}

/// Whether a paste coming in here may still be served.
///
/// A transfer is under way and its last piece is recent enough that
/// something is plainly still sending. This is what outlives a session:
/// a link that blinks closes one way and opens another a moment later,
/// and everything about the paste, the bytes already written and the
/// place kept on the clipboard, is worth holding across that gap rather
/// than throwing four gigabytes away for a hiccup.
///
/// The same patience as the one Windows is made to wait, read from
/// there rather than written again here: whoever holds the bytes and
/// whoever waits for them must give up at the same moment, or one of the
/// two is serving a paste the other has already abandoned.
pub fn still_coming() -> bool {
    COMING
        .lock()
        .expect("transfer under way")
        .as_ref()
        .is_some_and(|coming| coming.last_piece.elapsed() < zyr_clipboard::PATIENCE)
}

/// Drops whatever was on its way and takes the folder with it.
///
/// Called when the session goes and nothing is coming any more. What was
/// pasted is somewhere else by then, Windows having copied it out of what
/// this product handed it, and what was not pasted is a transfer nobody
/// finished.
pub fn forget(log: &Log) {
    let log = &log.about(TAG);
    let mut held = COMING.lock().expect("transfer under way");
    drop_it(&mut held, "goes with the session", log);
}

/// The one way a transfer ends before its time, said in the words of
/// whichever thing ended it.
fn drop_it(held: &mut Option<Coming>, why: &str, log: &Log) {
    if let Some(coming) = held.take() {
        let _ = std::fs::remove_dir_all(&coming.where_they_land);
        log.write(&format!("what was coming in {why}"));
    }
}

/// Says in the journal where a transfer has got to.
///
/// At each tenth, and no more often: a file of a gigabyte is four
/// thousand pieces, and a journal with four thousand lines of one
/// transfer in it has nothing else in it. Not never either: a transfer
/// is the one thing here that takes minutes, and minutes of silence read
/// as nothing happening.
fn say_how_far(coming: &mut Coming, over: bool, log: &Log) {
    let reached = reached(coming);
    let done = reached.done;
    let whole = reached.whole;
    let tenths = if whole == 0 {
        10
    } else {
        ((done.min(whole) as u128 * 10) / whole as u128) as u8
    };
    if !over && tenths <= coming.said {
        return;
    }
    coming.said = tenths;
    let took = coming.since.elapsed();
    if over {
        log.write(&format!(
            "{} came in, in {:.1} s",
            weighed(done),
            took.as_secs_f32()
        ));
        return;
    }
    log.write(&format!(
        "{} of {} in, {} a second",
        weighed(done),
        weighed(whole),
        weighed((done as f64 / took.as_secs_f64().max(0.001)) as u64)
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The transfer under way is one for the whole service, and so is
    /// the folder it lands in: two of these tests at once are two pastes
    /// fighting over one transfer, and each fails the other.
    static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

    fn alone() -> std::sync::MutexGuard<'static, ()> {
        ONE_AT_A_TIME
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn a_log(name: &str) -> (Log, PathBuf) {
        let folder = std::env::temp_dir().join(format!(
            "zyrdeskd-transfer-{}-{name}",
            zyr_proto::random::alphanumeric_string(8)
        ));
        std::fs::create_dir_all(&folder).unwrap();
        let log = Log::open(&folder.join("service.log")).unwrap();
        (log, folder)
    }

    fn given(rank: u32, from: u64, bytes: Vec<u8>) -> Given {
        Given { rank, from, bytes }
    }

    /// A copy that is not any other copy, which is all these tests ask of
    /// a stamp: what it really is comes from the far computer's clip.
    fn a_copy(named: &str) -> Stamp {
        zyr_proto::clipboard::Clip::text(named).stamp()
    }

    #[test]
    fn a_transfer_moves_file_by_file_and_finishes() {
        let _alone = alone();
        let (log, folder) = a_log("going");
        let listed = Listing::of(vec![
            Listed::new("one.txt", 3).unwrap(),
            Listed::new("folder/two.bin", 2).unwrap(),
        ]);
        coming_in(a_copy("going"), &listed, &log).unwrap();
        let landed = paths::pasted();

        assert_eq!(
            what_is_still_wanted(),
            Some(Wanted {
                rank: 0,
                from: 0,
                how_many: A_PIECE as u32
            })
        );
        assert!(!take(&given(0, 0, b"abc".to_vec()), &log).unwrap());
        // The first one is full, so the second one is what is
        // wanted.
        assert_eq!(what_is_still_wanted().unwrap().rank, 1);
        assert_eq!(how_far().unwrap().done, 3);

        assert!(take(&given(1, 0, b"de".to_vec()), &log).unwrap());
        assert_eq!(what_is_still_wanted(), None, "nothing left to ask for");
        // And no bar: a bar left full would say a transfer is still under
        // way, when what is left is Windows' own copying.
        assert_eq!(how_far(), None);
        assert_eq!(std::fs::read(landed.join("one.txt")).unwrap(), b"abc");
        assert_eq!(
            std::fs::read(landed.join("folder").join("two.bin")).unwrap(),
            b"de"
        );

        forget(&log);
        assert!(!landed.exists());
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn the_same_paste_announced_twice_does_not_start_over() {
        // Whoever notices the paste notices it at every turn of their
        // loop: without this, the transfer would start over four times a
        // second and never get past its first piece.
        let _alone = alone();
        let (log, folder) = a_log("twice");
        // A full piece of a longer file, so that the first piece does
        // not finish the transfer.
        let whole = A_PIECE as u64 + 3;
        let listed = Listing::of(vec![Listed::new("one.txt", whole).unwrap()]);
        let same = a_copy("twice");

        coming_in(same, &listed, &log).unwrap();
        take(&given(0, 0, vec![b'a'; A_PIECE]), &log).unwrap();
        coming_in(same, &listed, &log).unwrap();
        assert_eq!(
            how_far().unwrap().done,
            A_PIECE as u64,
            "the transfer started over from zero"
        );

        // Another copy, though, starts over: it is no longer the
        // same one.
        coming_in(a_copy("another"), &listed, &log).unwrap();
        assert_eq!(how_far().unwrap().done, 0);

        forget(&log);
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn a_piece_that_does_not_land_where_expected_is_dropped() {
        // It is the answer to a question asked before the clipboard
        // changed: writing it would put the bytes of one copy into the
        // file of another.
        //
        // A file of one full piece and three bytes: full, because a piece
        // shorter than what was asked for says the file has ended, and
        // what is wanted here is a first piece that does not say so.
        let _alone = alone();
        let (log, folder) = a_log("shifted");
        let whole = A_PIECE as u64 + 3;
        let listed = Listing::of(vec![Listed::new("one.txt", whole).unwrap()]);
        coming_in(a_copy("shifted"), &listed, &log).unwrap();
        let landed = paths::pasted();

        let first = vec![b'a'; A_PIECE];
        assert!(!take(&given(0, 0, first.clone()), &log).unwrap());
        // The next byte is what is waited for: the piece that starts
        // again from zero is dropped.
        assert!(!take(&given(0, 0, b"zzz".to_vec()), &log).unwrap());
        assert_eq!(how_far().unwrap().done, A_PIECE as u64);
        assert!(take(&given(0, A_PIECE as u64, b"def".to_vec()), &log).unwrap());

        let mut whole_of_it = first;
        whole_of_it.extend_from_slice(b"def");
        assert_eq!(std::fs::read(landed.join("one.txt")).unwrap(), whole_of_it);

        forget(&log);
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn a_file_shorter_than_it_announced_is_not_asked_for_forever() {
        // A file that shrank between the copy and the paste: the short
        // piece says it has ended, and without that it would be asked
        // for again forever.
        let _alone = alone();
        let (log, folder) = a_log("shrunk");
        let listed = Listing::of(vec![Listed::new("one.txt", 4_000_000).unwrap()]);
        coming_in(a_copy("shrunk"), &listed, &log).unwrap();

        assert!(take(&given(0, 0, b"short".to_vec()), &log).unwrap());
        assert_eq!(what_is_still_wanted(), None);

        forget(&log);
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn a_transfer_still_moving_outlives_the_session_that_opened_it() {
        // A link that blinks closes one way and opens another. A
        // transfer tied to the first is four gigabytes thrown away at
        // eighty per cent for a hiccup: what decides is the last piece
        // received, never the session.
        let _alone = alone();
        let (log, folder) = a_log("hiccup");
        let whole = A_PIECE as u64 + 3;
        let listed = Listing::of(vec![Listed::new("one.txt", whole).unwrap()]);

        assert!(!still_coming(), "nothing is coming yet");
        coming_in(a_copy("hiccup"), &listed, &log).unwrap();
        assert!(still_coming(), "a transfer that just opened is waiting");

        take(&given(0, 0, vec![0u8; A_PIECE]), &log).unwrap();
        assert!(still_coming());

        // No news for longer than Windows waits: nobody is serving this
        // paste any more, and holding it any longer would be holding a
        // clipboard for nothing.
        COMING.lock().unwrap().as_mut().unwrap().last_piece -= zyr_clipboard::PATIENCE;
        assert!(!still_coming());

        forget(&log);
        assert!(!still_coming());
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn without_a_transfer_nothing_is_wanted_and_nothing_is_on_its_way() {
        // This is what a computer where nobody is pasting answers, and
        // it is what tells the other end it may stop sending.
        let _alone = alone();
        let (log, folder) = a_log("nothing");
        forget(&log);
        assert_eq!(what_is_still_wanted(), None);
        assert_eq!(how_far(), None);
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn progress_reads_in_hundredths_and_never_beyond() {
        assert_eq!(
            HowFar {
                done: 0,
                whole: 200,
                files: 1
            }
            .hundredths(),
            0
        );
        assert_eq!(
            HowFar {
                done: 50,
                whole: 200,
                files: 1
            }
            .hundredths(),
            25
        );
        // Nothing to copy is finished and not half done: an empty file
        // is a real thing to copy and it has no middle.
        assert_eq!(
            HowFar {
                done: 0,
                whole: 0,
                files: 1
            }
            .hundredths(),
            100
        );
        // And what overflows does not go past a hundred, since a
        // file may have grown between the copy and the paste.
        assert_eq!(
            HowFar {
                done: 400,
                whole: 200,
                files: 1
            }
            .hundredths(),
            100
        );
    }
}
