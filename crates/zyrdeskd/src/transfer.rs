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

// Outside Windows nothing calls this module: the service does not exist
// there. Its logic has nothing platform-specific about it and stays
// compiled and tested everywhere.
#![cfg_attr(not(windows), allow(dead_code))]

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
    /// And what was last drawn of it, for the same reason and at the
    /// hundredth rather than the tenth: what the button shows is a bar,
    /// and a bar is worth writing again when it has moved by something
    /// somebody can see.
    drawn: u32,
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
    let mut held = COMING.lock().expect("transfert en cours");
    if held.as_ref().is_some_and(|coming| coming.stamp == stamp) {
        return Ok(());
    }

    let where_they_land = paths::pasted();
    // Emptied and not added to. What is in there is what a previous
    // paste left, and a file of the same name half written by that one
    // would be read as this one's.
    let _ = std::fs::remove_dir_all(&where_they_land);
    std::fs::create_dir_all(&where_they_land)
        .map_err(|e| format!("le dossier des fichiers reçus ne s'ouvre pas : {e}"))?;

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
        // A hundred and one, which no hundredth ever is: the first piece
        // to land is what draws the bar, and nought would have it look
        // already drawn.
        drawn: u32::MAX,
    });
    say_how_far(
        held.as_mut().expect("le transfert qui vient d'ouvrir"),
        false,
        log,
    );
    Ok(())
}

/// The next piece this computer is still waiting for, when it is waiting
/// for one.
///
/// Nothing when no transfer is under way and nothing when the one that is
/// has all its bytes, which are the two ways of saying the far computer
/// may stop sending.
pub fn what_is_still_wanted() -> Option<Wanted> {
    let coming = COMING.lock().expect("transfert en cours");
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
    let mut held = COMING.lock().expect("transfert en cours");
    let Some(coming) = held.as_mut() else {
        return Ok(true);
    };
    let rank = given.rank as usize;
    let Some(file) = coming.listed.at(rank) else {
        return Err(format!("un morceau du fichier {rank}, qui n'existe pas"));
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
            .map_err(|e| format!("le dossier de {} ne s'ouvre pas : {e}", file.path()))?;
    }
    let mut writing = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(given.from == 0)
        .open(&lands)
        .map_err(|e| format!("{} ne s'écrit pas : {e}", file.path()))?;
    writing
        .seek(SeekFrom::Start(given.from))
        .and_then(|_| writing.write_all(&given.bytes))
        .map_err(|e| format!("{} ne s'écrit pas : {e}", file.path()))
}

/// How far a transfer has got, off the one in hand.
///
/// Read by nobody here: what watches this is the interface, in another
/// program, and what it reads is the line `say_how_far` writes.
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
        .expect("transfert en cours")
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
    let mut held = COMING.lock().expect("transfert en cours");
    drop_it(&mut held, "goes with the session", log);
}

/// The one way a transfer ends before its time, said in the words of
/// whichever thing ended it.
fn drop_it(held: &mut Option<Coming>, why: &str, log: &Log) {
    // Taken away whether or not anything was on its way. No file rather
    // than a bar left at where it stopped: what the button draws while
    // this is there is a transfer under way, and after this there is
    // none, including the one a session before this left written down.
    let _ = std::fs::remove_file(paths::files_coming());
    if let Some(coming) = held.take() {
        let _ = std::fs::remove_dir_all(&coming.where_they_land);
        log.write(&format!("what was coming in {why}"));
    }
}

/// Says where a transfer has got to, to the two that want to know.
///
/// The button is told whenever the bar it draws would move, which is a
/// hundred times over the whole of a transfer however many pieces it
/// takes: what is being watched is a bar, and one written again without
/// having moved is a file rewritten forty times a second for nothing.
///
/// The journal is told at each tenth, which is less again for the reason
/// it always is: a file of a gigabyte is four thousand pieces, and a
/// journal with four thousand lines of one transfer in it has nothing
/// else in it. Not never either: a transfer is the one thing here that
/// takes minutes, and minutes of silence read as nothing happening.
fn say_how_far(coming: &mut Coming, over: bool, log: &Log) {
    let reached = reached(coming);
    let hundredths = reached.hundredths();
    if over {
        // Taken away rather than left full. What the button draws while
        // this is there is a transfer under way, and the bytes being all
        // here is the end of that: what is left to wait for is Windows'
        // own copying, which has a window of its own.
        let _ = std::fs::remove_file(paths::files_coming());
    } else if hundredths != coming.drawn {
        coming.drawn = hundredths;
        let _ = zyr_proto::files::replace(&paths::files_coming(), &reached.written());
    }

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

    /// Où en est le transfert, lu là où le bouton le lit.
    fn drawn() -> Option<HowFar> {
        HowFar::read(&std::fs::read_to_string(paths::files_coming()).ok()?).ok()
    }

    /// A copy that is not any other copy, which is all these tests ask of
    /// a stamp: what it really is comes from the far computer's clip.
    fn a_copy(named: &str) -> Stamp {
        zyr_proto::clipboard::Clip::text(named).stamp()
    }

    #[test]
    fn un_transfert_avance_fichier_par_fichier_et_se_termine() {
        let (log, folder) = a_log("aller");
        let listed = Listing::of(vec![
            Listed::new("un.txt", 3).unwrap(),
            Listed::new("dossier/deux.bin", 2).unwrap(),
        ]);
        coming_in(a_copy("aller"), &listed, &log).unwrap();
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
        // Le premier est plein, donc c'est le second qu'on veut.
        assert_eq!(what_is_still_wanted().unwrap().rank, 1);
        assert_eq!(drawn().unwrap().done, 3);

        assert!(take(&given(1, 0, b"de".to_vec()), &log).unwrap());
        assert_eq!(what_is_still_wanted(), None, "plus rien à demander");
        assert_eq!(std::fs::read(landed.join("un.txt")).unwrap(), b"abc");
        assert_eq!(
            std::fs::read(landed.join("dossier").join("deux.bin")).unwrap(),
            b"de"
        );

        forget(&log);
        assert!(!landed.exists());
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn le_meme_coller_annonce_deux_fois_ne_repart_pas_de_zero() {
        // Celui qui voit le coller le voit à chaque tour de sa boucle :
        // sans ça, le transfert recommencerait quatre fois par seconde et
        // ne dépasserait jamais son premier morceau.
        let (log, folder) = a_log("deux-fois");
        // Un morceau plein d'un fichier plus long, pour que le premier
        // morceau ne finisse pas le transfert.
        let whole = A_PIECE as u64 + 3;
        let listed = Listing::of(vec![Listed::new("un.txt", whole).unwrap()]);
        let meme = a_copy("deux-fois");

        coming_in(meme, &listed, &log).unwrap();
        take(&given(0, 0, vec![b'a'; A_PIECE]), &log).unwrap();
        coming_in(meme, &listed, &log).unwrap();
        assert_eq!(
            drawn().unwrap().done,
            A_PIECE as u64,
            "le transfert a repris à zéro"
        );

        // Une autre copie, elle, repart : ce n'est plus la même.
        coming_in(a_copy("une-autre"), &listed, &log).unwrap();
        assert_eq!(drawn().unwrap().done, 0);

        forget(&log);
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn un_morceau_qui_ne_tombe_pas_ou_on_attendait_est_jete() {
        // C'est la réponse à une question posée avant que le
        // presse-papiers change : l'écrire mettrait les octets d'une
        // copie dans le fichier d'une autre.
        // Un fichier d'un morceau plein et de trois octets : plein, parce
        // qu'un morceau plus court que ce qu'on a demandé dit la fin du
        // fichier, et qu'on veut ici un premier morceau qui ne la dise
        // pas.
        let (log, folder) = a_log("decale");
        let whole = A_PIECE as u64 + 3;
        let listed = Listing::of(vec![Listed::new("un.txt", whole).unwrap()]);
        coming_in(a_copy("decale"), &listed, &log).unwrap();
        let landed = paths::pasted();

        let first = vec![b'a'; A_PIECE];
        assert!(!take(&given(0, 0, first.clone()), &log).unwrap());
        // On attend l'octet suivant : celui qui repart de zéro est jeté.
        assert!(!take(&given(0, 0, b"zzz".to_vec()), &log).unwrap());
        assert_eq!(drawn().unwrap().done, A_PIECE as u64);
        assert!(take(&given(0, A_PIECE as u64, b"def".to_vec()), &log).unwrap());

        let mut whole_of_it = first;
        whole_of_it.extend_from_slice(b"def");
        assert_eq!(std::fs::read(landed.join("un.txt")).unwrap(), whole_of_it);

        forget(&log);
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn un_fichier_plus_court_que_ce_qu_il_annoncait_ne_se_demande_pas_sans_fin() {
        // Un fichier qui a maigri entre la copie et le coller : le
        // morceau court dit la fin, et sans ça on le redemanderait pour
        // toujours.
        let (log, folder) = a_log("maigri");
        let listed = Listing::of(vec![Listed::new("un.txt", 4_000_000).unwrap()]);
        coming_in(a_copy("maigri"), &listed, &log).unwrap();

        assert!(take(&given(0, 0, b"court".to_vec()), &log).unwrap());
        assert_eq!(what_is_still_wanted(), None);

        forget(&log);
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn un_transfert_qui_bouge_encore_survit_a_la_session_qui_l_a_ouvert() {
        // Une liaison qui cligne ferme une voie et en ouvre une autre.
        // Un transfert accroché à la première, ce sont quatre gigaoctets
        // jetés à quatre-vingts pour cent pour un hoquet : ce qui décide
        // est le dernier morceau reçu, jamais la session.
        let (log, folder) = a_log("hoquet");
        let whole = A_PIECE as u64 + 3;
        let listed = Listing::of(vec![Listed::new("un.txt", whole).unwrap()]);

        assert!(!still_coming(), "rien ne vient encore");
        coming_in(a_copy("hoquet"), &listed, &log).unwrap();
        assert!(still_coming(), "un transfert qui vient d'ouvrir attend");

        take(&given(0, 0, vec![0u8; A_PIECE]), &log).unwrap();
        assert!(still_coming());

        // Sans nouvelle depuis plus longtemps que ce que Windows attend :
        // plus personne ne sert ce coller, et le tenir plus longtemps
        // serait tenir un presse-papiers pour rien.
        COMING.lock().unwrap().as_mut().unwrap().last_piece -= zyr_clipboard::PATIENCE;
        assert!(!still_coming());

        forget(&log);
        assert!(!still_coming());
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn sans_transfert_rien_n_est_voulu_et_rien_ne_se_dessine() {
        // C'est ce que répond un ordinateur où personne ne colle, et
        // c'est ce qui dit à l'autre bout qu'il peut cesser d'envoyer.
        let (log, folder) = a_log("rien");
        forget(&log);
        assert_eq!(what_is_still_wanted(), None);
        assert_eq!(drawn(), None);
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn un_avancement_se_lit_en_centiemes_et_jamais_au_dela() {
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
        // Rien à copier est fini et non à moitié fait : un fichier vide
        // est une vraie chose à copier et il n'a pas de milieu.
        assert_eq!(
            HowFar {
                done: 0,
                whole: 0,
                files: 1
            }
            .hundredths(),
            100
        );
        // Et ce qui déborde ne dépasse pas cent, un fichier ayant pu
        // grossir entre la copie et le coller.
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
