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
    /// Whole frames passed over while waiting for a key frame.
    pub skipped: u64,
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
             pictures decoded {}, shown {}, unshown {}, skipped {}, broken {}, undrawn {}, \
             key frames asked {}, redrawn {}, renewed {}; sound packets {} (late {}, \
             duplicate {}, dropped {}, crowded {}, unplayed {}, malformed {}), decoded {}, \
             concealed {}, broken {}, underruns {}; control unreadable {}; input sent {}, \
             pressed again {}, too early {}",
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
            p.skipped,
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
