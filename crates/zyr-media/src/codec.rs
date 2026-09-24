//! Which video codecs exist, which each side can handle, and which one a
//! session ends up with.
//!
//! The host alone knows what it can encode and the client alone knows what
//! its graphics card can decode, so the choice is made from both lists
//! rather than assumed by either side.

/// A video codec the engine can carry.
///
/// The value is what travels on the wire.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VideoCodec {
    H264 = 1,
    Hevc = 2,
    Av1 = 3,
}

impl VideoCodec {
    /// Every codec, in the order they are tried when nothing else decides.
    pub const ALL: [VideoCodec; 3] = [VideoCodec::H264, VideoCodec::Hevc, VideoCodec::Av1];

    /// The codec a wire value names.
    pub fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(VideoCodec::H264),
            2 => Some(VideoCodec::Hevc),
            3 => Some(VideoCodec::Av1),
            _ => None,
        }
    }

    /// The value written on the wire.
    pub fn wire(self) -> u8 {
        self as u8
    }

    /// The name the rest of the product shows and stores ("H.264", "HEVC", "AV1").
    pub fn name(self) -> &'static str {
        match self {
            VideoCodec::H264 => "H.264",
            VideoCodec::Hevc => "HEVC",
            VideoCodec::Av1 => "AV1",
        }
    }
}

/// A set of codecs, one bit each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CodecSet(u8);

impl CodecSet {
    /// No codec at all.
    pub fn empty() -> Self {
        Self(0)
    }

    /// The set as it travels on the wire.
    pub fn from_wire(bits: u8) -> Self {
        let known = VideoCodec::ALL
            .iter()
            .fold(0u8, |all, codec| all | Self::bit(*codec));
        Self(bits & known)
    }

    /// The bits written on the wire.
    pub fn wire(self) -> u8 {
        self.0
    }

    /// This set with one more codec in it.
    pub fn with(self, codec: VideoCodec) -> Self {
        Self(self.0 | Self::bit(codec))
    }

    pub fn contains(self, codec: VideoCodec) -> bool {
        self.0 & Self::bit(codec) != 0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The codecs in the set, in [`VideoCodec::ALL`] order.
    pub fn iter(self) -> impl Iterator<Item = VideoCodec> {
        VideoCodec::ALL
            .into_iter()
            .filter(move |codec| self.contains(*codec))
    }

    fn bit(codec: VideoCodec) -> u8 {
        1 << codec.wire()
    }
}

impl FromIterator<VideoCodec> for CodecSet {
    fn from_iter<I: IntoIterator<Item = VideoCodec>>(codecs: I) -> Self {
        codecs
            .into_iter()
            .fold(CodecSet::empty(), |set, codec| set.with(codec))
    }
}

/// What the person asked for in the menu.
///
/// The value is what travels on the wire.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CodecChoice {
    #[default]
    Auto = 0,
    H264 = 1,
    Hevc = 2,
    Av1 = 3,
}

impl CodecChoice {
    pub fn from_wire(value: u8) -> Option<Self> {
        match value {
            0 => Some(CodecChoice::Auto),
            1 => Some(CodecChoice::H264),
            2 => Some(CodecChoice::Hevc),
            3 => Some(CodecChoice::Av1),
            _ => None,
        }
    }

    pub fn wire(self) -> u8 {
        self as u8
    }

    /// The codec named, if one is.
    pub fn codec(self) -> Option<VideoCodec> {
        VideoCodec::from_wire(self.wire())
    }
}

/// The codec a session uses, given what was asked and what each side can do.
///
/// A codec both sides handle is always preferred to nothing: an explicit
/// choice that one side cannot honour falls back to what automatic would
/// have picked, and the host says so to the viewer. Automatic picks HEVC
/// when both can, for the picture it gives at the same rate, then H.264.
/// AV1 is only ever used when asked for by name.
pub fn negotiate(choice: CodecChoice, host: CodecSet, client: CodecSet) -> Option<VideoCodec> {
    let both = |codec: VideoCodec| host.contains(codec) && client.contains(codec);
    if let Some(codec) = choice.codec()
        && both(codec)
    {
        return Some(codec);
    }
    [VideoCodec::Hevc, VideoCodec::H264]
        .into_iter()
        .find(|codec| both(*codec))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(codecs: &[VideoCodec]) -> CodecSet {
        codecs.iter().copied().collect()
    }

    #[test]
    fn automatic_prefers_hevc_then_h264_and_never_av1() {
        let everything = set(&VideoCodec::ALL);
        assert_eq!(
            negotiate(CodecChoice::Auto, everything, everything),
            Some(VideoCodec::Hevc)
        );
        let no_hevc_here = set(&[VideoCodec::H264, VideoCodec::Av1]);
        assert_eq!(
            negotiate(CodecChoice::Auto, everything, no_hevc_here),
            Some(VideoCodec::H264)
        );
        let only_av1 = set(&[VideoCodec::Av1]);
        assert_eq!(negotiate(CodecChoice::Auto, only_av1, only_av1), None);
    }

    #[test]
    fn a_named_codec_is_used_when_both_sides_handle_it() {
        let everything = set(&VideoCodec::ALL);
        assert_eq!(
            negotiate(CodecChoice::Av1, everything, everything),
            Some(VideoCodec::Av1)
        );
        assert_eq!(
            negotiate(CodecChoice::H264, everything, everything),
            Some(VideoCodec::H264)
        );
    }

    #[test]
    fn a_named_codec_one_side_lacks_falls_back_like_automatic() {
        let host = set(&[VideoCodec::H264, VideoCodec::Hevc]);
        let client = set(&VideoCodec::ALL);
        assert_eq!(
            negotiate(CodecChoice::Av1, host, client),
            Some(VideoCodec::Hevc)
        );
        assert_eq!(
            negotiate(CodecChoice::H264, CodecSet::empty(), client),
            None
        );
    }

    #[test]
    fn sets_travel_on_the_wire_and_ignore_unknown_bits() {
        let both = set(&[VideoCodec::H264, VideoCodec::Av1]);
        assert_eq!(CodecSet::from_wire(both.wire()), both);
        assert_eq!(CodecSet::from_wire(0xff), set(&VideoCodec::ALL));
        assert_eq!(
            both.iter().collect::<Vec<_>>(),
            vec![VideoCodec::H264, VideoCodec::Av1]
        );
    }

    #[test]
    fn wire_values_name_the_right_codec() {
        for codec in VideoCodec::ALL {
            assert_eq!(VideoCodec::from_wire(codec.wire()), Some(codec));
        }
        assert_eq!(VideoCodec::from_wire(0), None);
        assert_eq!(CodecChoice::from_wire(4), None);
        assert_eq!(CodecChoice::Hevc.codec(), Some(VideoCodec::Hevc));
        assert_eq!(CodecChoice::Auto.codec(), None);
    }
}
