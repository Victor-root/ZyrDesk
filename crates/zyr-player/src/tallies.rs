//! What the player counted: every datagram, picture and sound packet it
//! passed over, and why. Written to the journal now and then and at the
//! end of a session, and readable at any time for a diagnosis.

use std::fmt;

use zyr_media::audio::JitterCounters;
use zyr_media::video::AssemblyCounters;

/// The counters of a session so far.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tallies {
    pub assembly: AssemblyCounters,
    pub pictures: PictureTallies,
    pub jitter: JitterCounters,
    pub sound: SoundTallies,
    pub link: LinkTallies,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PictureTallies {
    pub decoded: u64,
    pub shown: u64,
    /// Decoded, then replaced by a newer picture before being shown.
    pub unshown: u64,
    /// Sent again by the host, its screen unchanged, and replaced by a
    /// newer picture before being shown: nothing the person could see.
    pub gave_way: u64,
    /// Whole frames passed over while waiting for a key frame.
    pub skipped: u64,
    /// Times the player fell so far behind the host that it dropped what
    /// waited for the decoder and asked for a key frame.
    pub behind: u64,
    /// Frames the decoder refused.
    pub broken: u64,
    /// Key frames asked for, the repeated requests included.
    pub recovers: u64,
    /// Pictures the surface could not draw.
    pub undrawn: u64,
    /// The last picture drawn again for a new size of the surface.
    pub redrawn: u64,
    /// Times the graphics card went away and everything was made again.
    pub renewed: u64,
    /// The fingerprint of the last picture shown, when shown on the
    /// processor.
    pub checksum: Option<u64>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SoundTallies {
    /// Datagrams that were not sound packets.
    pub malformed: u64,
    pub decoded: u64,
    /// Packets that never came, played as silence in their place.
    pub concealed: u64,
    /// Packets the decoder refused, played as silence too.
    pub broken: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LinkTallies {
    /// Video datagrams dropped on arrival, the video thread being behind.
    pub video_crowded: u64,
    /// Sound datagrams dropped on arrival, the sound thread being behind.
    pub sound_crowded: u64,
    /// Sound datagrams with no sound card to play them on.
    pub sound_unplayed: u64,
    /// Control messages from the engine that could not be read.
    pub unreadable: u64,
    /// Input events sent to the host.
    pub sent: u64,
    /// Presses of a key or button the host already holds down.
    pub pressed_again: u64,
    /// Events that came before the session was open, with no picture to
    /// aim at yet.
    pub too_early: u64,
}

impl Tallies {
    /// What the counters gained since `before`, for the journal: the
    /// picture and the sound of the seconds between two readings.
    pub fn gained_since(&self, before: &Tallies) -> String {
        let gained = |now: u64, before: u64| now.saturating_sub(before);
        let (a, b) = (&self.assembly, &before.assembly);
        let (p, q) = (&self.pictures, &before.pictures);
        let (l, m) = (&self.link, &before.link);
        format!(
            "frames whole {} (repaired {}), lost {}; pictures decoded {}, shown {}, replaced {}, \
             sent again gave way {}, passed over {}, fallen behind {}, broken {}, undrawn {}, key \
             frames asked {}; datagrams left out on the way to the video thread {}; sound packets \
             concealed {}, underruns {}, left out on the way {}",
            gained(a.frames_complete, b.frames_complete),
            gained(a.frames_recovered_by_fec, b.frames_recovered_by_fec),
            gained(a.frames_lost, b.frames_lost),
            gained(p.decoded, q.decoded),
            gained(p.shown, q.shown),
            gained(p.unshown, q.unshown),
            gained(p.gave_way, q.gave_way),
            gained(p.skipped, q.skipped),
            gained(p.behind, q.behind),
            gained(p.broken, q.broken),
            gained(p.undrawn, q.undrawn),
            gained(p.recovers, q.recovers),
            gained(l.video_crowded, m.video_crowded),
            gained(self.sound.concealed, before.sound.concealed),
            gained(self.jitter.underruns, before.jitter.underruns),
            gained(l.sound_crowded, m.sound_crowded),
        )
    }
}

impl fmt::Display for Tallies {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let a = &self.assembly;
        let p = &self.pictures;
        let j = &self.jitter;
        let s = &self.sound;
        let l = &self.link;
        write!(
            f,
            "video packets {} (duplicate {}, late {}, unneeded {}, malformed {}, overflow {}, \
             crowded {}), frames whole {} (repaired {}, parity used {}), lost {}, superseded {}; \
             pictures decoded {}, shown {}, unshown {}, gave way {}, skipped {}, fallen behind {}, \
             broken {}, undrawn {}, key frames asked {}, redrawn {}, renewed {}; sound packets {} \
             (late {}, duplicate {}, dropped {}, crowded {}, unplayed {}, malformed {}), \
             decoded {}, concealed {}, broken {}, underruns {}; control unreadable {}; input sent \
             {}, pressed again {}, too early {}",
            a.packets,
            a.duplicates,
            a.late,
            a.unneeded,
            a.malformed,
            a.overflow,
            l.video_crowded,
            a.frames_complete,
            a.frames_recovered_by_fec,
            a.parity_used,
            a.frames_lost,
            a.frames_superseded,
            p.decoded,
            p.shown,
            p.unshown,
            p.gave_way,
            p.skipped,
            p.behind,
            p.broken,
            p.undrawn,
            p.recovers,
            p.redrawn,
            p.renewed,
            j.packets,
            j.late,
            j.duplicates,
            j.dropped,
            l.sound_crowded,
            l.sound_unplayed,
            s.malformed,
            s.decoded,
            s.concealed,
            s.broken,
            j.underruns,
            l.unreadable,
            l.sent,
            l.pressed_again,
            l.too_early,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_the_counters_gained_between_two_readings_is_told() {
        let before = Tallies::default();
        let mut now = Tallies::default();
        now.assembly.frames_complete = 60;
        now.assembly.frames_recovered_by_fec = 2;
        now.assembly.frames_lost = 1;
        now.pictures.decoded = 59;
        now.pictures.shown = 55;
        now.pictures.unshown = 3;
        now.pictures.gave_way = 1;
        now.pictures.skipped = 4;
        now.pictures.recovers = 2;
        now.link.video_crowded = 7;
        now.sound.concealed = 5;
        now.jitter.underruns = 1;
        assert_eq!(
            now.gained_since(&before),
            "frames whole 60 (repaired 2), lost 1; pictures decoded 59, shown 55, replaced 3, \
             sent again gave way 1, passed over 4, fallen behind 0, broken 0, undrawn 0, key \
             frames asked 2; datagrams left out on the way to the video thread 7; sound packets \
             concealed 5, underruns 1, left out on the way 0"
        );
        // Counters only grow: a reading older than the other tells of
        // nothing gained rather than of a wrap.
        assert!(
            before
                .gained_since(&now)
                .starts_with("frames whole 0 (repaired 0), lost 0;")
        );
    }
}
