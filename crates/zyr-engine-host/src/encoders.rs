//! Which pictures this computer's engine can actually make.
//!
//! A codec is asked for by the computer watching and encoded by the
//! computer being watched, and the second one is the only one that knows
//! whether it can. Asking for one it cannot make is not an error
//! anywhere: the engines agree on another between themselves and the
//! session opens perfectly well. What is wrong is that nothing then says
//! so, and the menu goes on showing a choice that has not been honoured
//! for the rest of the session.
//!
//! So it is read, not guessed, and read the way the screens are: the
//! engine tries every encoder its machine might have when it starts,
//! writes down the ones that worked, and that list is what is taken.
//! Working it out here instead would mean copying the engine's own idea
//! of what its graphics card can do, which is a copy that would be wrong
//! on the first machine nobody tested.
//!
//! Read from the log the engine writes itself, which is the same one the
//! screens come out of and is not one of the four the journal gathers.
//! The first try read the console output captured beside it instead, and
//! that was wrong twice over: it is empty on some machines, and it is
//! emptied whenever somebody clears their journal, which is a thing this
//! product now offers to do from the other end of a tunnel.

use std::path::Path;

use zyr_proto::session::Codec;

/// What the engine in that folder of logs says it can encode.
///
/// Read from the log the engine writes itself, and never from the console
/// output this product captures beside it: that capture is empty on some
/// machines, and it is one of the four files the journal empties, so a
/// person clearing their journal would silently take the answer away.
///
/// Read as it comes, accents or not, like every other reading of this
/// log: an engine that wrote a line in another encoding must cost that
/// line and not the whole answer.
pub fn found_for(logs_dir: &Path) -> Vec<Codec> {
    let Ok(bytes) = std::fs::read(crate::config::engine_log_in(logs_dir)) else {
        return Vec::new();
    };
    found_in(&String::from_utf8_lossy(&bytes))
}

/// Where the engine says its trials begin.
///
/// Read from the last one of these and never from the top of the file:
/// the log carries every run of the engine one after another, and a card
/// that could do a codec last week says nothing about the machine today.
const TRIALS_BEGIN: &str = "Testing for available encoders";

/// What it writes down for each encoder that worked, the codec's own
/// name coming between the two.
const FOUND: &str = "Found ";
const ENCODER: &str = " encoder:";

/// The codecs this computer's engine says it can encode.
///
/// Empty when the engine has not said, which is every engine that has
/// not finished starting and every log that was cleared underneath it.
/// Empty means « no answer » and never « none »: a computer that cannot
/// encode anything cannot be watched at all, so that answer would be
/// about the reading and not about the machine.
pub fn found_in(log: &str) -> Vec<Codec> {
    let lines: Vec<&str> = log.lines().collect();
    let from = lines
        .iter()
        .rposition(|line| line.contains(TRIALS_BEGIN))
        .map_or(0, |at| at + 1);

    let mut found = Vec::new();
    for line in &lines[from..] {
        let Some(named) = named_in(line) else {
            continue;
        };
        if !found.contains(&named) {
            found.push(named);
        }
    }
    found
}

/// The codec one line says was found, if it says any.
fn named_in(line: &str) -> Option<Codec> {
    let after = line.rfind(FOUND)? + FOUND.len();
    let upto = line[after..].find(ENCODER)? + after;
    line[after..upto].trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Word for word what the engine writes, taken from the log of a
    /// machine with an Intel card: it can do H.264 and HEVC, not AV1.
    /// That is the case this file exists for.
    const A_RUN: &str = "\
[01:37:57.922]: Info: // Testing for available encoders, this may generate errors. You can safely ignore those errors. //
[01:37:57.922]: Info: Trying encoder [nvenc]
[01:37:58.025]: Info: Encoder [nvenc] is not supported on this GPU
[01:37:58.031]: Info: Trying encoder [quicksync]
[01:37:58.932]: Error: [av1_qsv @ 000001ada3e206c0] Current codec type is unsupported
[01:37:58.934]: Error: Could not open codec [av1_qsv]: Function not implemented
[01:37:59.403]: Info: // Ignore any errors mentioned above, they are not relevant. //
[01:37:59.403]: Info: Found H.264 encoder: h264_qsv [quicksync]
[01:37:59.403]: Info: Found HEVC encoder: hevc_qsv [quicksync]
[01:37:59.434]: Info: Configuration UI available at [https://127.0.0.1:42001]
";

    #[test]
    fn what_the_engine_says_it_found_is_what_is_taken() {
        assert_eq!(found_in(A_RUN), vec![Codec::H264, Codec::Hevc]);
    }

    #[test]
    fn only_the_last_run_counts() {
        // The log carries every start one after another. A card that
        // could do AV1 last week says nothing about the machine today,
        // and it is today that is being asked about.
        let before = "\
Info: // Testing for available encoders //
Info: Found H.264 encoder: h264_nvenc [nvenc]
Info: Found HEVC encoder: hevc_nvenc [nvenc]
Info: Found AV1 encoder: av1_nvenc [nvenc]
";
        let both = format!("{before}{A_RUN}");
        assert_eq!(found_in(&both), vec![Codec::H264, Codec::Hevc]);
    }

    #[test]
    fn an_engine_that_has_not_said_says_nothing_rather_than_none() {
        // Nothing is an absence of answer and never "none": a computer
        // that could encode nothing could not be watched at all, so
        // that answer would be about the reading and not about the
        // machine.
        assert!(found_in("").is_empty());
        assert!(found_in("Info: // Testing for available encoders //\n").is_empty());
        // And what comes from before the current trials does not
        // count.
        assert!(
            found_in(
                "Info: Found AV1 encoder: av1_nvenc\nInfo: // Testing for available encoders //\n"
            )
            .is_empty()
        );
    }

    #[test]
    fn it_is_the_engines_own_log_that_is_read() {
        // And never the console output captured beside it: it is empty
        // on some machines, and it is one of the four files the "Vider"
        // button clears, including from the other end of a tunnel. The
        // answer would have vanished at the first clearing.
        let folder = std::env::temp_dir().join(format!(
            "zyrdesk-encoders-{}",
            zyr_proto::random::alphanumeric_string(8)
        ));
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("engine-console.log"), "").unwrap();
        std::fs::write(crate::config::engine_log_in(&folder), A_RUN).unwrap();

        assert_eq!(found_for(&folder), vec![Codec::H264, Codec::Hevc]);

        // And an engine that has written nothing at all says nothing,
        // rather than making the reading fail.
        let empty = folder.join("vide");
        std::fs::create_dir_all(&empty).unwrap();
        assert!(found_for(&empty).is_empty());

        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_codec_this_product_does_not_know_is_left_out() {
        // Tomorrow the engine may name an encoder this product has
        // never heard of. An unreadable line is skipped; it does not
        // make the ones after it fail.
        let odd = "\
Info: // Testing for available encoders //
Info: Found VP9 encoder: vp9_qsv [quicksync]
Info: Found HEVC encoder: hevc_qsv [quicksync]
";
        assert_eq!(found_in(odd), vec![Codec::Hevc]);
    }
}
