//! What is on this computer's clipboard, and what it is being given.
//!
//! The same blindness as the pointer beside it, and the same answer. A
//! clipboard belongs to a window station; a service sits on one carrying
//! no screen, no desktop and no clipboard, and the station that has one
//! belongs to another session entirely. It cannot be opened from here,
//! and no right makes it so. So this program is started again in the
//! session that owns the screen, and reads and writes from there.
//!
//! That helper and this service pass two things through files, for the
//! reason everything between these two programs is a file: it can be read
//! with the eyes, and it survives whoever wrote it. What this computer
//! has, written by the helper and read here; and what it is to be given,
//! written here and picked up by the helper.
//!
//! Each of those is two files, a line that names and a file beside it
//! that holds. The service looks several times a second, and a picture
//! weighs a few hundred thousand bytes: what is looked at that often has
//! to be two words. The bytes are written first and the line after them,
//! so a line naming bytes names bytes that are already there, and a
//! reader that arrives between the two writes reads a line that does not
//! match and comes back a moment later.
//!
//! The helper only runs while somebody is asking, exactly like the
//! pointer's. A computer nobody is watching has its clipboard to itself.
//!
//! # Files, and the one file that says so
//!
//! Files are the exception to all of it. What a clipboard holds of a file
//! is a name, so what a helper puts on this one for the far computer's
//! files is a promise to hand them over, and a promise lives inside the
//! program that made it: a helper that made one cannot go home at the end
//! of its ten seconds without taking the files with it.
//!
//! So there is a third file, and it is one word. The helper writes it at
//! every turn while it holds such a promise, and the word says whether
//! anybody has pasted yet. It carries three things at once: the service
//! learns that somebody pasted and starts bringing the bytes in; it knows
//! not to start a second helper beside one that is holding something; and
//! when the last session goes it takes the file away, which is how the
//! helper is told to let go and end.

// Outside Windows nothing calls this module: the service does not exist
// there. The shape of it stays compiled and tested everywhere, and the
// clipboard itself is the one part that cannot be.
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use zyr_proto::clipboard::{Clip, Head, Kind, Stamp};
use zyr_proto::log::Log;
use zyr_proto::paths;

/// What this module's lines are filed under.
///
/// One word to a module, which is the only rule that keeps a tag worth
/// anything: one somebody has to look up is one nobody types.
const TAG: &str = "clipboard";

/// How often the helper looks at the clipboard.
///
/// Slower than the pointer beside it by a good margin, and it is the
/// right way round: a shape is wanted within a drawn frame, while what
/// somebody copied is wanted before their hand reaches the other
/// keyboard. A fifth of a second, on a question that has to answer
/// within half of one.
const LOOK_EVERY: Duration = Duration::from_millis(200);

/// How long a helper lives before it goes home of its own accord.
///
/// It is what stops one outliving the service that started it: nobody
/// ends it, it simply ends.
const HELPER_LIVES: Duration = Duration::from_secs(10);

/// How late is too late to count on the one that is running.
///
/// Another is started before the last has ended, so the reading never
/// stops between two of them.
const START_ANOTHER_AFTER: Duration = Duration::from_secs(7);

/// How long the service goes on keeping a helper after the last question.
const AFTER_THE_LAST_QUESTION: Duration = Duration::from_secs(4);

/// How long something just given is waited on before it is given up as
/// not having landed.
///
/// A helper takes a moment to notice, put it on, and say so. For that
/// moment this computer says nothing about its own clipboard, rather
/// than say what was on it before and have the far computer push that
/// back. Past this, the helper is gone or refused, and saying nothing
/// for ever would be a clipboard that never shares again.
const SETTLES_WITHIN: Duration = Duration::from_secs(3);

/// How old a helper's mark has to be before the helper counts as gone.
///
/// It is written at every turn of a loop that turns five times a second,
/// so a mark this old is a helper that has stopped writing it. Without
/// this, a helper that died while holding the far computer's files would
/// leave a mark nobody ever takes away, and no other helper would be
/// started beside it for the rest of the session.
const A_STAND_GOES_STALE_AFTER: Duration = Duration::from_secs(2);

/// Whether the thread that keeps a helper alive is running.
static KEEPING: AtomicBool = AtomicBool::new(false);

/// When the last question came, so the keeper knows when to stop.
static ASKED: Mutex<Option<Instant>> = Mutex::new(None);

/// What was last given to this computer's clipboard, and when, until the
/// helper says it is really there.
static GIVEN: Mutex<Option<(Stamp, Instant)>> = Mutex::new(None);

/// What was last found too heavy to cross, so it is said once and not at
/// every turn of every session.
static TOO_LARGE: Mutex<Option<Stamp>> = Mutex::new(None);

/// What last stopped a paste here from starting at all, for the same
/// reason and to the same end.
static PASTE_REFUSED: Mutex<Option<String>> = Mutex::new(None);

/// What this computer has on its clipboard.
///
/// Never blocks and never fails: what comes back is the last thing the
/// helper wrote, which is at most one look old.
///
/// Nothing means one of three things, and the far computer does the same
/// thing about all three, which is why they are one answer: this
/// clipboard is empty, no helper has read it yet, or something was just
/// put on it and has not landed. In none of those has this computer
/// anything to say, and saying nothing is never read as « empty yours ».
pub fn what_this_computer_has(log: &Log) -> Option<Clip> {
    let log = &log.about(TAG);
    *ASKED.lock().expect("dernière question") = Some(Instant::now());
    if !KEEPING.swap(true, Ordering::SeqCst) {
        keep_a_helper(log.clone());
    }
    let clip =
        written_clip(&paths::clipboard_here()).filter(|clip| !more_than_it_carries(clip, log));
    let mut given = GIVEN.lock().expect("ce qui vient d'être donné");
    match *given {
        // It landed: the helper read back the very thing that was put on,
        // and from here on it is simply what this computer has.
        Some((stamp, _)) if clip.as_ref().is_some_and(|clip| clip.stamp() == stamp) => {
            *given = None;
            clip
        }
        Some((_, when)) if when.elapsed() < SETTLES_WITHIN => None,
        // It did not land in the time a helper takes to notice, put it on
        // and say so. The order is taken away rather than left there: a
        // helper that will not take it would go on trying every fifth of
        // a second for as long as the session lasts, and neither the
        // clipboard nor the journal is any better for that.
        Some(_) => {
            *given = None;
            forget_the_pair(&paths::clipboard_wanted());
            log.write("what came from the far computer never reached this clipboard");
            clip
        }
        None => clip,
    }
}

/// Puts that on this computer's clipboard, through the helper.
///
/// Written down and not waited for: whoever asks is answering the far
/// computer on a channel every other question of the session queues
/// behind, and a clipboard that lands a fifth of a second later lands
/// before any hand reaches for it.
pub fn give_it(clip: &Clip, log: &Log) -> Result<(), String> {
    let log = &log.about(TAG);
    write_the_pair(&paths::clipboard_wanted(), clip)
        .map_err(|e| format!("le presse-papiers n'a pas pu être posé : {e}"))?;
    *GIVEN.lock().expect("ce qui vient d'être donné") = Some((clip.stamp(), Instant::now()));
    log.write(&format!(
        "{} is going on this computer's clipboard",
        clip.in_words()
    ));
    Ok(())
}

/// What a helper is doing about files that live on the far computer.
///
/// There is no third state and no « none »: a helper that is holding
/// nothing writes no mark at all, and no mark is the answer everything
/// here reads as « nobody is holding anything ».
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stand {
    /// They are on this computer's clipboard and nobody has pasted them.
    Held,
    /// Somebody pasted, so their bytes are wanted now.
    Pasting,
}

impl Stand {
    fn spelled(self) -> &'static str {
        match self {
            Self::Held => "standing",
            Self::Pasting => "pasting",
        }
    }

    fn of(said: &str) -> Option<Self> {
        match said.trim() {
            "standing" => Some(Self::Held),
            "pasting" => Some(Self::Pasting),
            _ => None,
        }
    }
}

/// What a helper is doing about the far computer's files right now.
///
/// Nothing when none is holding any, and nothing again when the mark is
/// older than a helper writing it every fifth of a second would leave it:
/// that mark is one a helper died holding, and believing it would be this
/// service never starting another for the rest of the session.
pub fn a_stand_is_up() -> Option<Stand> {
    let mark = paths::clipboard_standing();
    let said = std::fs::read_to_string(&mark).ok()?;
    std::fs::metadata(&mark)
        .and_then(|about| about.modified())
        .is_ok_and(|when| {
            when.elapsed()
                .is_ok_and(|since| since < A_STAND_GOES_STALE_AFTER)
        })
        .then(|| Stand::of(&said))
        .flatten()
}

/// The next piece a paste on this computer is still waiting for.
///
/// This is where a paste becomes a transfer. Nothing crosses while the
/// far computer's files merely sit on this clipboard; the moment somebody
/// pastes them, the helper says so, and the bytes start being asked for
/// from here.
///
/// Nothing means the far computer may stop sending, which covers both
/// « nobody here is pasting » and « what was being pasted is all here ».
pub fn what_a_paste_here_wants(log: &Log) -> Option<zyr_tunnel::aside::Wanted> {
    let log = &log.about(TAG);
    if a_stand_is_up() == Some(Stand::Pasting)
        && let Some(clip) = written_clip(&paths::clipboard_here())
        && let Some(listed) = clip.listing()
        && let Err(refused) = crate::transfer::coming_in(clip.stamp(), &listed, log)
    {
        // Said once: what refuses here is a folder that will not open,
        // and a disk does not change its mind between two turns of a loop
        // that turns four times a second.
        let mut said = PASTE_REFUSED.lock().expect("ce qui empêche de coller");
        if said.replace(refused.clone()).as_deref() != Some(refused.as_str()) {
            log.write(&format!("files: nothing can be pasted here: {refused}"));
        }
    }
    crate::transfer::what_is_still_wanted()
}

/// Whether the far computer may come asking for the bytes of files
/// copied on this one.
///
/// The one reason to keep asking when there is nothing to ask for: a far
/// end that pastes can only say so in an answer, and answers only come to
/// questions. False while what is on this clipboard came from the far
/// computer in the first place, since nobody asks for their own files
/// back.
pub fn files_may_be_wanted_here() -> bool {
    a_stand_is_up().is_none()
        && written_clip(&paths::clipboard_here()).is_some_and(|clip| clip.kind() == Kind::Files)
}

/// Whether that is more than a session carries, said once when it is.
///
/// The one rule about weight, and it lives here because here is the one
/// door a clip on this computer goes through on its way out, whether this
/// computer is the one watching or the one being watched. Past the
/// ceiling, what somebody copied would hold up the picture for seconds on
/// a link that has to carry both, to paste something they almost
/// certainly have on the other machine already.
///
/// Said once per thing and not at every turn: this is asked several times
/// a second, and a line a second for as long as a large picture sits on
/// somebody's clipboard is a journal with nothing else in it.
fn more_than_it_carries(clip: &Clip, log: &Log) -> bool {
    if !clip.too_large() {
        return false;
    }
    let mut said = TOO_LARGE.lock().expect("ce qui ne passe pas");
    if said.replace(clip.stamp()) != Some(clip.stamp()) {
        log.write(&format!(
            "{} is more than a session carries, and stays on this computer",
            clip.in_words()
        ));
    }
    true
}

/// Whether the last question is far enough behind to stop.
fn nobody_is_asking() -> bool {
    ASKED
        .lock()
        .expect("dernière question")
        .is_none_or(|asked| asked.elapsed() > AFTER_THE_LAST_QUESTION)
}

/// Keeps a helper running in the session that owns the screen, for as
/// long as anybody is asking.
///
/// A thread of its own because starting a program in another session
/// takes milliseconds, and the threads that answer the far computer are
/// shared with everything else this service does.
#[cfg(windows)]
fn keep_a_helper(log: Log) {
    let log = log.about(TAG);
    std::thread::spawn(move || {
        log.write("a session is sharing this computer's clipboard");
        let mut started: Option<Instant> = None;
        let mut refused = false;
        while !nobody_is_asking() {
            // Not while one of them is holding the far computer's files.
            // That one stays for as long as they are on the clipboard,
            // which can be minutes, and a second beside it would read a
            // clipboard it can make nothing of and say so once every few
            // seconds for the whole of that time.
            if a_stand_is_up().is_none()
                && started.is_none_or(|at| at.elapsed() > START_ANOTHER_AFTER)
            {
                match crate::session::start_carrying_the_clipboard() {
                    Ok(()) => {
                        if started.is_none() {
                            log.write("reading it from the session that owns the screen");
                        }
                        refused = false;
                        started = Some(Instant::now());
                    }
                    // Said once and not every turn: a machine at its
                    // sign-in screen has no session to read from, and
                    // that is a state it can sit in for hours.
                    Err(e) => {
                        if !refused {
                            refused = true;
                            log.write(&format!("nothing can reach the clipboard here: {e}"));
                        }
                        started = None;
                    }
                }
            }
            std::thread::sleep(LOOK_EVERY);
        }
        log.write("nobody is sharing it any more");
        // Both pairs go with the asking. What this computer had copied
        // is no more a session's business once the session has gone, and
        // the next one starts on what is really on the clipboard rather
        // than on what the last one left written down.
        forget_the_pair(&paths::clipboard_here());
        forget_the_pair(&paths::clipboard_wanted());
        let _ = std::fs::remove_file(paths::clipboard_files());
        // Taking the mark away is how a helper holding the far computer's
        // files is told the sessions have gone: it lets go on finding its
        // own mark taken, since what it was holding is files nobody can
        // send any more.
        let _ = std::fs::remove_file(paths::clipboard_standing());
        crate::transfer::forget(&log);
        *GIVEN.lock().expect("ce qui vient d'être donné") = None;
        KEEPING.store(false, Ordering::SeqCst);
    });
}

#[cfg(not(windows))]
fn keep_a_helper(_log: Log) {
    KEEPING.store(false, Ordering::SeqCst);
}

/// Reads and writes this computer's clipboard, until its time is up.
///
/// This is the helper, and it only ever runs in the session that owns the
/// screen: started anywhere else it reads a window station with no
/// clipboard on it. It ends by itself so that nothing has to end it.
///
/// Files are the one thing that keeps it past its hour. What stands in
/// for the far computer's files lives inside whoever put it on the
/// clipboard, so a helper that has put one there cannot go home: it would
/// take the files with it. It stays until somebody copies something else,
/// or until the service takes its mark away.
#[cfg(windows)]
pub fn carry_the_clipboard_here() {
    // Taken before anything else and held to the end. Without it nothing
    // can be offered on this computer's clipboard for other programs, so
    // a helper that could not take its place is a helper with half a job
    // and no way of saying which half.
    let _attending = match zyr_clipboard::attend() {
        Ok(attending) => attending,
        Err(e) => {
            said(&e.to_string());
            return;
        }
    };
    // Under whose name, because that decides what it is allowed to see:
    // a helper under the wrong one reads a clipboard that looks empty and
    // has no way at all of saying why.
    hunted(|| format!("this helper is running as {}", whoever_this_is()));
    let until = Instant::now() + HELPER_LIVES;
    let mut counted: Option<u32> = None;
    // Whether this helper is the one holding the far computer's files.
    let mut standing = false;
    // What is already written down, and not nothing. A helper takes over
    // from another every few seconds for the whole of a session: one that
    // started from nothing would write the same clipboard down again, and
    // say so again, every time one of them started.
    let mut written = written_clip(&paths::clipboard_here()).map(|clip| clip.stamp());
    // Whether this is the first thing this helper has looked at. A
    // clipboard found the way it was left is not news, and saying it is
    // would fill a journal with one line per helper rather than one line
    // per thing somebody copied.
    let mut taking_over = true;
    // What went wrong last, so that a clipboard held by another program,
    // or a picture this machine's imaging will not take, is one line and
    // not five a second for as long as the session lasts.
    let mut complained: Option<String> = None;
    while standing || Instant::now() < until {
        if let Some(wanted) = written_clip(&paths::clipboard_wanted()) {
            // Files are the odd one and always were: a clipboard never
            // holds a file, so there is nothing here to put on one. What
            // goes on instead is a promise to hand them over, and that
            // promise is what keeps this helper alive afterwards.
            let put = if wanted.kind() == Kind::Files {
                stand_in_for_them(&wanted).inspect(|()| {
                    standing = true;
                    counted = Some(zyr_clipboard::times_it_changed());
                    written = Some(wanted.stamp());
                    hold_the_mark(Stand::Held);
                    said(&format!(
                        "this computer now offers {}, and their bytes will cross when \
                         somebody pastes them",
                        wanted.in_words()
                    ));
                })
            } else {
                zyr_clipboard::hold_this(&wanted)
                    .map(|missing| {
                        for what in missing {
                            say_once(&mut complained, &what);
                        }
                    })
                    .map_err(|e| e.to_string())
            };
            match put {
                // Read back on the very next turn, because putting
                // something on a clipboard is what changes it: the line
                // this helper then writes is what tells the service the
                // giving landed.
                Ok(()) => forget_the_pair(&paths::clipboard_wanted()),
                // Left where it is, so the next turn tries again: a
                // clipboard held by another program for a moment is the
                // ordinary case and not a fault. The service takes the
                // order away when it has waited long enough, which is
                // what bounds this.
                Err(e) => say_once(
                    &mut complained,
                    &format!("it would not take what it was given: {e}"),
                ),
            }
        }

        if standing {
            if !paths::clipboard_standing().exists() {
                // The service has taken the mark away, which is the one
                // way it has of saying the sessions have gone. What is
                // being held is files nobody can send any more.
                zyr_clipboard::let_go();
                said("the far computer's files are no longer offered here");
                return;
            }
            if zyr_clipboard::still_standing() {
                hold_the_mark(if zyr_clipboard::somebody_pasted() {
                    Stand::Pasting
                } else {
                    Stand::Held
                });
                zyr_clipboard::answer_for(LOOK_EVERY);
                continue;
            }
            // Somebody copied something else on this computer, which is
            // what takes the promise off the clipboard. The turn goes on
            // to the ordinary reading, which picks up whatever took its
            // place.
            zyr_clipboard::let_go();
            standing = false;
            let _ = std::fs::remove_file(paths::clipboard_standing());
        }

        // The counter is the cheap half of this: reading a picture every
        // fifth of a second to discover it has not changed would cost a
        // few million bytes each time. Nought is a system that would not
        // say, and then the clipboard itself is read every turn, which is
        // what it costs to be right rather than fast.
        let counter = zyr_clipboard::times_it_changed();
        if counter != 0 && counted == Some(counter) {
            zyr_clipboard::answer_for(LOOK_EVERY);
            continue;
        }
        counted = Some(counter);
        let news = !std::mem::take(&mut taking_over);
        match zyr_clipboard::what_it_holds() {
            Ok(Some(found)) if written != Some(found.clip.stamp()) => {
                match write_it_down(&found) {
                    Ok(()) => {
                        written = Some(found.clip.stamp());
                        complained = None;
                        // One line per thing copied, which is the right
                        // rate: a clipboard changes a few times an hour.
                        // Without it, a picture that never crossed and a
                        // clipboard nobody touched leave exactly the same
                        // trace, which is none.
                        if news {
                            said(&format!(
                                "this computer now holds {}",
                                found.clip.in_words()
                            ));
                            if found.cut_short {
                                said(&format!(
                                    "more was copied than one copy carries, so only \
                                     the first {} files of it cross",
                                    zyr_clipboard::MOST_FILES
                                ));
                            }
                        }
                    }
                    Err(e) => say_once(
                        &mut complained,
                        &format!("what is on it could not be written down: {e}"),
                    ),
                }
            }
            // Nothing this product carries. Said with what the clipboard
            // was really offering, because that is the one thing that
            // tells « nobody copied anything » from « somebody copied
            // something this product does not take », and the two look
            // identical from everywhere else.
            Ok(None) if news => say_once(
                &mut complained,
                &format!(
                    "nothing on it that crosses ; it holds {}",
                    zyr_clipboard::what_is_offered()
                ),
            ),
            Err(e) if news => say_once(
                &mut complained,
                &format!(
                    "it would not be read: {e} ; it holds {}",
                    zyr_clipboard::what_is_offered()
                ),
            ),
            _ => {}
        }
        zyr_clipboard::answer_for(LOOK_EVERY);
    }
}

/// Offers the far computer's files on this computer's clipboard.
///
/// Nothing of them is read here and nothing has to be: what goes on the
/// clipboard is their names and a promise, and the promise is only called
/// in when somebody pastes.
#[cfg(windows)]
fn stand_in_for_them(wanted: &Clip) -> Result<(), String> {
    let listed = wanted
        .listing()
        .ok_or_else(|| "cette liste de fichiers ne se lit pas".to_string())?;
    zyr_clipboard::stand_in_for(&listed, &paths::pasted()).map_err(|e| e.to_string())?;
    // Where this computer's copied files really are stops being true of
    // anything the moment what it holds lives on the other computer.
    // Left there, it would have this computer hand over the files of a
    // copy that is over, to a far end asking about this one.
    let _ = std::fs::remove_file(paths::clipboard_files());
    write_the_pair(&paths::clipboard_here(), wanted).map_err(|e| e.to_string())
}

/// Says, to the service, that this helper is holding the far computer's
/// files and how far that has got.
///
/// Written at every turn rather than when the word changes: the same file
/// is also how the service knows a helper is still there, and a file only
/// says that while it is being written.
#[cfg(windows)]
fn hold_the_mark(what: Stand) {
    let _ = zyr_proto::files::replace(
        &paths::clipboard_standing(),
        &format!("{}\n", what.spelled()),
    );
}

/// Writes that down unless it is word for word what was written last.
///
/// Everything in the loop above can go wrong at every turn, five times a
/// second, for as long as whatever is wrong lasts. What is worth reading
/// is that it went wrong and what it said, once.
#[cfg(windows)]
fn say_once(last: &mut Option<String>, what: &str) {
    if last.as_deref() == Some(what) {
        return;
    }
    *last = Some(what.to_string());
    said(what);
}

#[cfg(not(windows))]
pub fn carry_the_clipboard_here() {}

/// Writes into the service's own journal from the helper, which has no
/// journal of its own.
#[cfg(windows)]
fn said(what: &str) {
    if let Ok(log) = Log::open(&crate::service::log_path()) {
        log.about(TAG).write(what);
    }
}

/// The same, in the voice only a hunt wants.
///
/// The journal is not even opened otherwise: a helper starts every few
/// seconds for the whole of a session, and a file opened for a line
/// nobody is going to write is a file opened for nothing.
#[cfg(windows)]
fn hunted(what: impl FnOnce() -> String) {
    if !zyr_proto::FOR_HUNTING {
        return;
    }
    if let Ok(log) = Log::open(&crate::service::log_path()) {
        log.about(TAG).debug(what);
    }
}

/// Who this helper is, as Windows names them.
#[cfg(windows)]
fn whoever_this_is() -> String {
    use windows_sys::Win32::System::WindowsProgramming::GetUserNameW;

    let mut spelled = [0u16; 256];
    let mut room = spelled.len() as u32;
    // SAFETY: a buffer of ours, whose length is handed over and written
    // back as the length of what was put in it, the nought counted.
    if unsafe { GetUserNameW(spelled.as_mut_ptr(), &mut room) } == 0 {
        return "a name Windows would not give".to_string();
    }
    String::from_utf16_lossy(&spelled[..room.saturating_sub(1) as usize])
}

/// Reads a clip from the pair of files that name it and hold it.
///
/// Nothing when the two do not agree, which is a reading taken between
/// the two writes and not a fault: the turn after has both halves.
fn written_clip(named: &std::path::Path) -> Option<Clip> {
    let head: Head = std::fs::read_to_string(named).ok()?.parse().ok()?;
    let bytes = std::fs::read(paths::beside(named)).ok()?;
    head.matches(&bytes).then(|| Clip::new(head.kind, bytes))
}

/// Writes down what the clipboard was found to hold.
///
/// Three files for files and two for everything else, and the third goes
/// before the line that names the other two: the same rule as the bytes,
/// since a line that names a listing names places that have to be written
/// down before anybody reads it.
#[cfg(windows)]
fn write_it_down(found: &zyr_clipboard::Found) -> std::io::Result<()> {
    if !found.really.is_empty() {
        let mut where_they_are = String::new();
        for path in &found.really {
            where_they_are.push_str(&path.to_string_lossy());
            where_they_are.push('\n');
        }
        zyr_proto::files::replace(&paths::clipboard_files(), &where_they_are)?;
    }
    write_the_pair(&paths::clipboard_here(), &found.clip)
}

/// Where the file of that rank really is on this computer.
///
/// By rank and never by the path that crossed: a rank is a number that
/// cannot be made to name another file, where a path handed over by the
/// far computer would be a path this one has to check all over again.
///
/// Nothing when this computer's clipboard has moved on since, which is
/// the far computer asking for a file of a copy that is over.
pub fn where_the_file_is(rank: usize) -> Option<std::path::PathBuf> {
    std::fs::read_to_string(paths::clipboard_files())
        .ok()?
        .lines()
        .nth(rank)
        .filter(|line| !line.is_empty())
        .map(std::path::PathBuf::from)
}

/// A piece of one of the files this computer's clipboard named.
///
/// Read straight off the disk each time rather than held open: a paste
/// can be minutes apart from the copy that started it, and a file held
/// open all that while is a file nobody else may move or delete.
pub fn a_piece_of(asked: zyr_tunnel::aside::Wanted) -> Result<zyr_tunnel::aside::Given, String> {
    use std::io::{Read, Seek, SeekFrom};

    let rank = asked.rank as usize;
    let path = where_the_file_is(rank)
        .ok_or_else(|| format!("le fichier {rank} n'est plus celui qui est copié ici"))?;
    let mut open = std::fs::File::open(&path)
        .map_err(|e| format!("{} ne s'ouvre pas : {e}", path.display()))?;
    open.seek(SeekFrom::Start(asked.from))
        .map_err(|e| format!("{} ne se lit pas : {e}", path.display()))?;

    // Read to whatever really comes, which is what says a file ended:
    // a piece shorter than the one asked for is the end of it, and an
    // empty one is a file that had nothing left.
    let mut bytes = vec![0u8; asked.how_many as usize];
    let mut taken = 0;
    while taken < bytes.len() {
        match open.read(&mut bytes[taken..]) {
            Ok(0) => break,
            Ok(read) => taken += read,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(format!("{} ne se lit pas : {e}", path.display())),
        }
    }
    bytes.truncate(taken);
    Ok(zyr_tunnel::aside::Given {
        rank: asked.rank,
        from: asked.from,
        bytes,
    })
}

/// Writes a clip as that pair of files.
///
/// The bytes first and the line naming them after, which is the whole of
/// what makes the reading above safe: a line is never there before what
/// it names.
fn write_the_pair(named: &std::path::Path, clip: &Clip) -> std::io::Result<()> {
    zyr_proto::files::replace_bytes(&paths::beside(named), clip.bytes())?;
    zyr_proto::files::replace(named, &format!("{}\n", clip.named()))
}

/// Takes both files away, in the order that leaves nothing readable
/// behind: the line first, since it is the line that says there is
/// anything to read.
fn forget_the_pair(named: &std::path::Path) {
    let _ = std::fs::remove_file(named);
    let _ = std::fs::remove_file(paths::beside(named));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_folder(name: &str) -> std::path::PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "zyrdeskd-clipboard-{}-{name}",
            zyr_proto::random::alphanumeric_string(8)
        ));
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn un_clip_ecrit_se_relit() {
        let folder = fresh_folder("aller-retour");
        let named = folder.join("clipboard-here.txt");
        let clip = Clip::picture(vec![0x89, b'P', b'N', b'G', 0, 255]);

        assert_eq!(written_clip(&named), None);
        write_the_pair(&named, &clip).unwrap();
        assert_eq!(written_clip(&named), Some(clip));

        forget_the_pair(&named);
        assert_eq!(written_clip(&named), None);
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn une_ligne_qui_ne_correspond_pas_a_ses_octets_est_sautee() {
        // Le service lit entre les deux écritures : ce moment-là doit
        // être « rien à dire ce tour-ci » et jamais la moitié de deux
        // choses collée au presse-papiers d'en face.
        let folder = fresh_folder("entre-deux");
        let named = folder.join("clipboard-here.txt");
        write_the_pair(&named, &Clip::text("le nouveau")).unwrap();
        std::fs::write(paths::beside(&named), b"l'ancien").unwrap();

        assert_eq!(written_clip(&named), None);
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn personne_ne_demande_tant_que_personne_n_a_demande() {
        // C'est ce qui décide qu'un ordinateur que personne ne regarde
        // garde son presse-papiers pour lui : sans question, plus aucun
        // assistant n'est relancé et le dernier s'éteint tout seul.
        *ASKED.lock().unwrap() = None;
        assert!(nobody_is_asking());
        *ASKED.lock().unwrap() = Some(Instant::now());
        assert!(!nobody_is_asking());
        *ASKED.lock().unwrap() = Instant::now().checked_sub(AFTER_THE_LAST_QUESTION * 2);
        assert!(nobody_is_asking());
    }

    #[test]
    fn un_assistant_est_relance_avant_que_le_precedent_ne_meure() {
        // Sans ce recouvrement, la lecture s'arrêterait entre deux
        // assistants et ce qu'on copie pendant ce temps-là ne partirait
        // jamais.
        assert!(
            START_ANOTHER_AFTER < HELPER_LIVES,
            "un assistant doit être relancé avant la fin du précédent"
        );
    }
}
