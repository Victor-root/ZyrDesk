//! Which brick of ZyrDesk may use which.
//!
//! The workspace is built in layers, and a brick only ever uses bricks of
//! its own layer or of the layers below it:
//!
//! - the base: `zyr-proto`, the values every brick shares, and
//!   `zyr-i18n`, the words the person reads;
//! - the platform, `zyr-clipboard`, `zyr-draw`, `zyr-screen`,
//!   `zyr-sound`, `zyr-system` and `zyr-win32`: what Windows does for the
//!   product, what the product is drawn with, and Windows' own way of
//!   saying things;
//! - the engine, `zyr-media`, `zyr-codec`, `zyr-link`, `zyr-host` and
//!   `zyr-player`: the picture, the sound and the input, from the screen
//!   filmed to the picture drawn, and the one link each half of the
//!   engine has to the service of its computer;
//! - the network and the accounts: `zyr-transport`, `zyr-tunnel`,
//!   `zyr-lan`, `zyr-broker` and `zyr-account`;
//! - the product speaking to itself: `zyr-control`, the conversation
//!   between the window and the service, `zyr-session`, which opens a
//!   session, `zyr-launch`, how the product's programs
//!   are started, and `zyr-service`, what the service does;
//! - the programs, which assemble the rest and are used by nothing: the
//!   window `zyr-ui`, the service `zyrdeskd`, the command line `zyr-cli`
//!   and the server `zyr-server`.
//!
//! The engine is the part held apart. It uses nothing but the engine, the
//! base and the platform, so that nothing done to the window, the service
//! or the network reaches a line of it, and FFmpeg is known to its two
//! halves alone: everyone else asks them.
//!
//! Words are chosen by the programs a person reads, the window and the
//! command line, and by nothing else: the engines, the service and the
//! server tell facts, and never write a sentence for anybody.
//!
//! The map at the top of the tests below is the whole of it, brick by
//! brick. The tests read every manifest of the workspace, and fail when a
//! brick uses one the map does not give it, when the map gives one a
//! brick no longer uses, or when a brick is missing from it. The map is
//! therefore always the workspace as it is: a new dependency is a
//! decision written here, in the open, and never one that slips in.
//!
//! What a brick uses for its tests alone is not part of the map: a test
//! may assemble whatever it checks from end to end.

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::Path;

    use Layer::*;

    /// Where a brick stands. A brick uses bricks of its own layer or of
    /// the layers below it, never of one above.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    enum Layer {
        /// The values every brick shares.
        Base,
        /// What Windows does for the product.
        Platform,
        /// The picture, the sound and the input, and the link each half
        /// of the engine has to its service.
        Engine,
        /// The connection between two computers, and the accounts.
        Network,
        /// The product speaking to itself.
        Product,
        /// What assembles the rest, and is used by nothing.
        Program,
    }

    /// Every brick, its layer, and the bricks of ours it is built with.
    const MAP: &[(&str, Layer, &[&str])] = &[
        ("zyr-proto", Base, &[]),
        ("zyr-i18n", Base, &["zyr-proto"]),
        ("zyr-clipboard", Platform, &["zyr-proto"]),
        ("zyr-draw", Platform, &[]),
        ("zyr-screen", Platform, &["zyr-proto", "zyr-win32"]),
        ("zyr-sound", Platform, &[]),
        ("zyr-system", Platform, &["zyr-proto", "zyr-win32"]),
        ("zyr-win32", Platform, &[]),
        ("zyr-media", Engine, &["zyr-proto"]),
        ("zyr-codec", Engine, &["zyr-media", "zyr-proto"]),
        ("zyr-link", Engine, &["zyr-proto"]),
        (
            "zyr-host",
            Engine,
            &[
                "zyr-codec",
                "zyr-link",
                "zyr-media",
                "zyr-proto",
                "zyr-screen",
            ],
        ),
        (
            "zyr-player",
            Engine,
            &["zyr-codec", "zyr-link", "zyr-media", "zyr-proto"],
        ),
        ("zyr-transport", Network, &["zyr-proto"]),
        (
            "zyr-tunnel",
            Network,
            &["zyr-link", "zyr-proto", "zyr-transport"],
        ),
        ("zyr-lan", Network, &["zyr-proto"]),
        ("zyr-broker", Network, &["zyr-proto"]),
        (
            "zyr-account",
            Network,
            &["zyr-broker", "zyr-proto", "zyr-transport"],
        ),
        ("zyr-launch", Product, &["zyr-proto", "zyr-system"]),
        (
            "zyr-control",
            Product,
            &["zyr-broker", "zyr-link", "zyr-proto"],
        ),
        (
            "zyr-session",
            Product,
            &["zyr-control", "zyr-player", "zyr-proto", "zyr-sound"],
        ),
        (
            "zyr-service",
            Product,
            &[
                "zyr-account",
                "zyr-broker",
                "zyr-clipboard",
                "zyr-control",
                "zyr-lan",
                "zyr-link",
                "zyr-media",
                "zyr-proto",
                "zyr-screen",
                "zyr-sound",
                "zyr-system",
                "zyr-transport",
                "zyr-tunnel",
                "zyr-win32",
            ],
        ),
        (
            "zyr-ui",
            Program,
            &[
                "zyr-broker",
                "zyr-clipboard",
                "zyr-control",
                "zyr-draw",
                "zyr-i18n",
                "zyr-launch",
                "zyr-player",
                "zyr-proto",
                "zyr-session",
                "zyr-win32",
            ],
        ),
        (
            "zyrdeskd",
            Program,
            &[
                "zyr-host",
                "zyr-lan",
                "zyr-proto",
                "zyr-screen",
                "zyr-service",
                "zyr-system",
            ],
        ),
        (
            "zyr-cli",
            Program,
            &[
                "zyr-broker",
                "zyr-control",
                "zyr-host",
                "zyr-i18n",
                "zyr-link",
                "zyr-player",
                "zyr-proto",
                "zyr-session",
                "zyr-transport",
                "zyr-tunnel",
            ],
        ),
        (
            "zyr-server",
            Program,
            &["zyr-broker", "zyr-proto", "zyr-transport"],
        ),
    ];

    /// The brick that holds FFmpeg: only the two halves of the engine
    /// may use it.
    const FFMPEG: &str = "zyr-codec";

    /// The brick that holds the words: only the programs a person reads
    /// may use it.
    const WORDS: &str = "zyr-i18n";

    fn layer_of(brick: &str) -> Layer {
        MAP.iter()
            .find(|(name, _, _)| *name == brick)
            .map(|(_, layer, _)| *layer)
            .unwrap_or_else(|| panic!("{brick} is used but missing from the map"))
    }

    #[test]
    fn the_map_is_the_workspace_as_it_is() {
        let mapped: BTreeMap<String, BTreeSet<String>> = MAP
            .iter()
            .map(|(brick, _, uses)| {
                let uses = uses.iter().map(|used| used.to_string()).collect();
                (brick.to_string(), uses)
            })
            .collect();
        assert_eq!(mapped.len(), MAP.len(), "the map names a brick twice");

        let found = workspace();
        let missing: Vec<_> = found.keys().filter(|b| !mapped.contains_key(*b)).collect();
        assert!(
            missing.is_empty(),
            "bricks missing from the map: {missing:?}"
        );
        let gone: Vec<_> = mapped.keys().filter(|b| !found.contains_key(*b)).collect();
        assert!(
            gone.is_empty(),
            "bricks of the map no longer in the workspace: {gone:?}"
        );

        for (brick, uses) in &found {
            let given = &mapped[brick];
            let taken: Vec<_> = uses.difference(given).collect();
            assert!(
                taken.is_empty(),
                "{brick} uses {taken:?}, which the map does not give it"
            );
            let unused: Vec<_> = given.difference(uses).collect();
            assert!(
                unused.is_empty(),
                "the map gives {brick} {unused:?}, which it no longer uses"
            );
        }
    }

    #[test]
    fn a_brick_uses_nothing_above_its_own_layer() {
        for (brick, layer, uses) in MAP {
            for used in *uses {
                assert!(
                    layer_of(used) <= *layer,
                    "{brick} ({layer:?}) uses {used} ({:?}), which stands above it",
                    layer_of(used)
                );
            }
        }
    }

    #[test]
    fn the_engine_uses_nothing_but_the_engine_the_base_and_the_platform() {
        for (brick, layer, uses) in MAP {
            if *layer != Engine {
                continue;
            }
            for used in *uses {
                assert!(
                    matches!(layer_of(used), Engine | Base | Platform),
                    "the engine's {brick} uses {used}, from outside the engine"
                );
            }
        }
    }

    #[test]
    fn only_the_two_halves_of_the_engine_know_ffmpeg() {
        let knowing: Vec<&str> = MAP
            .iter()
            .filter(|(_, _, uses)| uses.contains(&FFMPEG))
            .map(|(brick, _, _)| *brick)
            .collect();
        assert_eq!(knowing, ["zyr-host", "zyr-player"]);
    }

    #[test]
    fn only_the_window_and_the_command_line_choose_words() {
        for (brick, _, uses) in MAP {
            if uses.contains(&WORDS) {
                assert!(
                    ["zyr-ui", "zyr-cli"].contains(brick),
                    "{brick} uses {WORDS}: below the programs a person reads, a brick tells \
                     facts and leaves the words to them"
                );
            }
        }
    }

    #[test]
    fn nothing_is_built_on_a_program() {
        for (brick, _, uses) in MAP {
            for used in *uses {
                assert!(
                    layer_of(used) != Program,
                    "{brick} uses {used}, a program: a program assembles the rest and \
                     is used by nothing"
                );
            }
        }
    }

    /// The bricks of the workspace, each with the bricks of ours it is
    /// built with, read from their manifests. This brick is left out: it
    /// uses nothing, and only reads the others.
    fn workspace() -> BTreeMap<String, BTreeSet<String>> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        members(&read(&root.join("Cargo.toml")))
            .iter()
            .map(|member| brick(&read(&root.join(member).join("Cargo.toml"))))
            .filter(|(name, _)| name != env!("CARGO_PKG_NAME"))
            .collect()
    }

    fn read(path: &Path) -> String {
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }

    /// The folders of the workspace's bricks, as its manifest lists them.
    fn members(manifest: &str) -> Vec<String> {
        let (_, list) = manifest
            .split_once("members = [")
            .expect("the workspace lists its members");
        let (list, _) = list.split_once(']').expect("the list of members ends");
        list.split(',')
            .map(|member| member.trim().trim_matches('"'))
            .filter(|member| !member.is_empty())
            .map(String::from)
            .collect()
    }

    /// A brick's name, and the bricks of ours it is built with: for every
    /// system, for one system alone and for its build script, and not for
    /// its tests alone.
    fn brick(manifest: &str) -> (String, BTreeSet<String>) {
        let mut name = None;
        let mut section = "";
        let mut uses = BTreeSet::new();
        for line in manifest.lines().map(str::trim) {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line.starts_with('[') {
                section = line.trim_matches(|c| c == '[' || c == ']');
                // A dependency may also be written as a table of its own.
                if let Some((list, dependency)) = section.rsplit_once('.')
                    && builds_with(list)
                    && dependency.starts_with("zyr")
                {
                    uses.insert(dependency.to_string());
                }
                continue;
            }
            if section == "package"
                && let Some(("name", value)) = line.split_once('=').map(|(k, v)| (k.trim(), v))
            {
                name.get_or_insert_with(|| value.trim().trim_matches('"').to_string());
            }
            if builds_with(section) {
                let key = line.split(['.', '=', ' ']).next().unwrap_or_default();
                if key.starts_with("zyr") {
                    uses.insert(key.to_string());
                } else {
                    // Anything else naming one of ours is written in a way
                    // this reader does not know, and would slip past it.
                    assert!(
                        !line.contains("zyr"),
                        "a line of a manifest this reader cannot tell apart: {line}"
                    );
                }
            }
        }
        (name.expect("every brick has a name"), uses)
    }

    /// Whether a section of a manifest lists what the brick is built
    /// with: its dependencies, for every system or for one, and those of
    /// its build script, but not those of its tests.
    fn builds_with(section: &str) -> bool {
        section.ends_with("dependencies") && !section.ends_with("dev-dependencies")
    }

    #[test]
    fn the_reader_sees_every_way_a_manifest_names_a_brick() {
        let manifest = r#"
[package]
name = "zyr-example"
description = "What zyr-proto is not"

[dependencies]
tokio = { version = "1", features = [
    "rt",
] }
zyr-proto.workspace = true
# zyr-media is only named in this comment.

[target.'cfg(windows)'.dependencies]
zyr-screen = { workspace = true }

[target.'cfg(windows)'.build-dependencies]
zyr-sound.workspace = true

[dependencies.zyr-link]
workspace = true

[dev-dependencies]
zyr-host = { workspace = true, features = ["fake"] }

[dev-dependencies.zyr-player]
workspace = true
"#;
        let (name, uses) = brick(manifest);
        assert_eq!(name, "zyr-example");
        assert_eq!(
            uses.into_iter().collect::<Vec<_>>(),
            ["zyr-link", "zyr-proto", "zyr-screen", "zyr-sound"]
        );
    }
}
