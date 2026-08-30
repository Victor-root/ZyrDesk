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

use zyr_proto::session::Codec;

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

    /// Mot pour mot ce que le moteur écrit, pris dans le journal d'une
    /// machine à carte Intel : elle sait faire du H.264 et du HEVC, pas
    /// de l'AV1. C'est le cas qui a valu ce fichier.
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
        // Le journal porte tous les démarrages à la suite. Une carte qui
        // savait faire de l'AV1 la semaine dernière ne dit rien de la
        // machine d'aujourd'hui, et c'est aujourd'hui qu'on demande.
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
        // Rien n'est une absence de réponse et jamais « aucun » : un
        // ordinateur qui n'encoderait rien ne pourrait pas être regardé
        // du tout, donc cette réponse-là parlerait de la lecture et non
        // de la machine.
        assert!(found_in("").is_empty());
        assert!(found_in("Info: // Testing for available encoders //\n").is_empty());
        // Et ce qui vient d'avant les essais en cours ne compte pas.
        assert!(
            found_in(
                "Info: Found AV1 encoder: av1_nvenc\nInfo: // Testing for available encoders //\n"
            )
            .is_empty()
        );
    }

    #[test]
    fn a_codec_this_product_does_not_know_is_left_out() {
        // Le moteur peut nommer demain un encodeur dont ce produit n'a
        // jamais entendu parler. Une ligne illisible se saute ; elle ne
        // fait pas rater celles qui suivent.
        let odd = "\
Info: // Testing for available encoders //
Info: Found VP9 encoder: vp9_qsv [quicksync]
Info: Found HEVC encoder: hevc_qsv [quicksync]
";
        assert_eq!(found_in(odd), vec![Codec::Hevc]);
    }
}
