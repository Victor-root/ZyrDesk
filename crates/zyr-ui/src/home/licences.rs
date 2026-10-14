//! What ZyrDesk is made with, under which licences, and their texts.
//!
//! The « About » page lists it, and part of it is an obligation: the
//! licences of what ships inside the product ask for their text to go
//! with it and to be readable (`docs/COMPLIANCE.md`). The texts are taken
//! from the files the repository carries beside each component, whole and
//! untouched, so they cannot drift from what ships.

/// One component the product is made with.
pub struct Credit {
    pub name: &'static str,
    /// What it does for the product, the key of its words.
    pub role: &'static str,
    /// Its licence, as it is named.
    pub licence: &'static str,
    pub link: &'static str,
    /// The full text of its licence.
    pub text: &'static str,
}

/// Where the product's own code is read.
pub const SOURCE: &str = "https://github.com/Victor-root/ZyrDesk";

/// The product's own licence.
pub const OWN_LICENCE: &str = include_str!("../../../../LICENSE");

/// Everything shipped inside the product, or that its pages are drawn
/// with, in the order the page lists it.
pub const CREDITS: [Credit; 13] = [
    Credit {
        name: "FFmpeg",
        role: zyr_i18n::key!("about.role_ffmpeg"),
        licence: "GPL 2.0+",
        link: "https://ffmpeg.org",
        text: include_str!("../../../../vendor/ffmpeg/licenses/FFmpeg-LICENSE.md"),
    },
    Credit {
        name: "x264",
        role: zyr_i18n::key!("about.role_x264"),
        licence: "GPL 2.0+",
        link: "https://www.videolan.org/developers/x264.html",
        text: include_str!("../../../../vendor/ffmpeg/licenses/x264-COPYING"),
    },
    Credit {
        name: "Opus",
        role: zyr_i18n::key!("about.role_opus"),
        licence: "BSD",
        link: "https://opus-codec.org",
        text: include_str!("../../../../vendor/ffmpeg/licenses/opus-COPYING"),
    },
    Credit {
        name: "NVIDIA Video Codec SDK",
        role: zyr_i18n::key!("about.role_nvidia"),
        licence: "MIT",
        link: "https://github.com/FFmpeg/nv-codec-headers",
        text: include_str!("../../../../vendor/ffmpeg/licenses/nv-codec-headers-LICENSE.txt"),
    },
    Credit {
        name: "AMD AMF",
        role: zyr_i18n::key!("about.role_amd"),
        licence: "MIT",
        link: "https://github.com/GPUOpen-LibrariesAndSDKs/AMF",
        text: include_str!("../../../../vendor/ffmpeg/licenses/AMF-LICENSE.txt"),
    },
    Credit {
        name: "Intel VPL",
        role: zyr_i18n::key!("about.role_intel"),
        licence: "MIT",
        link: "https://github.com/intel/libvpl",
        text: include_str!("../../../../vendor/ffmpeg/licenses/libvpl-LICENSE"),
    },
    Credit {
        name: "Virtual Display Driver",
        role: zyr_i18n::key!("about.role_virtual_screen"),
        licence: "MIT",
        link: "https://github.com/VirtualDrivers/Virtual-Display-Driver",
        text: include_str!("../../../../vendor/ecran-virtuel/LICENSE"),
    },
    Credit {
        name: "Tabler Icons",
        role: zyr_i18n::key!("about.role_tabler"),
        licence: "MIT",
        link: "https://tabler.io/icons",
        text: include_str!("../../licenses/tabler-icons.txt"),
    },
    Credit {
        name: "quinn",
        role: zyr_i18n::key!("about.role_quinn"),
        licence: "MIT",
        link: "https://github.com/quinn-rs/quinn",
        text: include_str!("../../licenses/quinn.txt"),
    },
    Credit {
        name: "rustls",
        role: zyr_i18n::key!("about.role_rustls"),
        licence: "MIT",
        link: "https://github.com/rustls/rustls",
        text: include_str!("../../licenses/rustls.txt"),
    },
    Credit {
        name: "ring",
        role: zyr_i18n::key!("about.role_ring"),
        licence: "ISC, Apache 2.0",
        link: "https://github.com/briansmith/ring",
        text: include_str!("../../licenses/ring.txt"),
    },
    Credit {
        name: "mdns-sd",
        role: zyr_i18n::key!("about.role_mdns"),
        licence: "MIT",
        link: "https://github.com/keepsimple1/mdns-sd",
        text: include_str!("../../licenses/mdns-sd.txt"),
    },
    Credit {
        name: "Tokio",
        role: zyr_i18n::key!("about.role_tokio"),
        licence: "MIT",
        link: "https://tokio.rs",
        text: include_str!("../../licenses/tokio.txt"),
    },
];

/// What the engine's design learned from, credited for what it taught:
/// none of their code is in the product (`docs/COMPLIANCE.md`, §4).
pub const STUDIED: [(&str, &str); 2] = [
    ("Sunshine", "https://github.com/LizardByte/Sunshine"),
    ("Moonlight", "https://moonlight-stream.org"),
];

/// The full text of one licence, by its place: the product's own first,
/// then each component's in the order of the list.
pub fn text_of(rank: usize) -> Option<(&'static str, &'static str)> {
    match rank {
        0 => Some(("ZyrDesk", OWN_LICENCE)),
        rank => CREDITS
            .get(rank - 1)
            .map(|credit| (credit.name, credit.text)),
    }
}

/// The addresses a click may open, and no other: an address arriving from
/// anywhere else and handed to the system as it came would open anything
/// at all.
pub fn is_ours(link: &str) -> bool {
    link == SOURCE
        || CREDITS.iter().any(|credit| credit.link == link)
        || STUDIED.iter().any(|(_, studied)| *studied == link)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_licence_text_is_there_and_every_address_is_a_secure_one() {
        assert!(OWN_LICENCE.contains("GNU GENERAL PUBLIC LICENSE"));
        for credit in &CREDITS {
            assert!(!credit.text.trim().is_empty(), "{}", credit.name);
            assert!(credit.link.starts_with("https://"), "{}", credit.link);
        }
        for (name, link) in STUDIED {
            assert!(link.starts_with("https://"), "{name}");
        }
        assert!(SOURCE.starts_with("https://"));
    }

    #[test]
    fn only_the_product_s_own_addresses_may_be_opened() {
        assert!(is_ours(SOURCE));
        assert!(is_ours("https://ffmpeg.org"));
        assert!(!is_ours("https://example.com"));
        assert!(!is_ours("file:///C:/Windows"));
    }

    #[test]
    fn every_licence_is_found_by_its_place() {
        assert_eq!(text_of(0).map(|(name, _)| name), Some("ZyrDesk"));
        assert_eq!(text_of(1).map(|(name, _)| name), Some("FFmpeg"));
        assert!(text_of(CREDITS.len()).is_some());
        assert!(text_of(CREDITS.len() + 1).is_none());
    }
}
