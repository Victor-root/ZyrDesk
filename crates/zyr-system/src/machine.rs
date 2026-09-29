//! What this computer is, worded for the journal.
//!
//! The same session runs differently on a laptop on its battery than on
//! a desktop, on Windows 10 than on 11, on a graphics driver from last
//! year than on this month's, and nobody thinks to say so when a journal
//! is sent. This says it once, when a session opens: Windows and its
//! build, the processor and how many of it, the memory, the power plan
//! and whether the mains is on, and every graphics card with the driver
//! it runs on. What is read from Windows is in `inspection.rs`; this
//! only words what was read, so that the wording is tried on any system.

use std::fmt::{self, Write as _};

/// Windows, as its registry names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Windows {
    /// The build number, which is what tells 11 from 10.
    pub build: u32,
    /// The update counter after the build number.
    pub revision: Option<u32>,
    /// The name of the feature update: `23H2`.
    pub version: Option<String>,
    /// `Professional`, `Core`, `Enterprise`...
    pub edition: Option<String>,
}

/// The first build of Windows 11.
const FIRST_WINDOWS_11: u32 = 22_000;

impl fmt::Display for Windows {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(if self.build >= FIRST_WINDOWS_11 {
            "Windows 11"
        } else {
            "Windows 10"
        })?;
        if let Some(version) = &self.version {
            write!(f, " {version}")?;
        }
        write!(f, " (build {}", self.build)?;
        if let Some(revision) = self.revision {
            write!(f, ".{revision}")?;
        }
        if let Some(edition) = &self.edition {
            write!(f, ", {edition}")?;
        }
        f.write_str(")")
    }
}

/// Where the power comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Mains,
    Battery,
}

/// How the computer is told to spend its power.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Power {
    /// The plan in force, in the words of the person's Windows.
    pub plan: Option<String>,
    /// The mode chosen over it on a laptop: best performance, best
    /// power efficiency...
    pub mode: Option<String>,
    pub source: Option<Source>,
}

impl Power {
    fn is_unknown(&self) -> bool {
        self.plan.is_none() && self.mode.is_none() && self.source.is_none()
    }
}

impl fmt::Display for Power {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut told = Vec::new();
        if let Some(plan) = &self.plan {
            told.push(format!("power plan {plan}"));
        }
        if let Some(mode) = &self.mode {
            told.push(format!("{mode} mode"));
        }
        match self.source {
            Some(Source::Mains) => told.push("on the mains".to_string()),
            Some(Source::Battery) => told.push("on battery".to_string()),
            None => {}
        }
        f.write_str(&told.join(", "))
    }
}

/// One graphics card, with the driver installed for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    pub name: String,
    /// The driver's four numbers, as Windows writes them.
    pub driver: Option<String>,
    /// The memory it declares, in bytes.
    pub memory: Option<u64>,
}

impl fmt::Display for Card {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)?;
        let mut about = Vec::new();
        if let Some(driver) = &self.driver {
            about.push(format!("driver {driver}"));
        }
        if let Some(memory) = self.memory {
            about.push(size(memory));
        }
        if about.is_empty() {
            Ok(())
        } else {
            write!(f, " ({})", about.join(", "))
        }
    }
}

/// Bytes as megabytes, or as gigabytes from one up.
fn size(bytes: u64) -> String {
    const MB: u64 = 1024 * 1024;
    const GB: u64 = 1024 * MB;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else {
        format!("{} MB", bytes / MB)
    }
}

/// The letters of a text that Windows wrote as bytes.
pub fn letters(bytes: &[u8]) -> Vec<u16> {
    bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
        .collect()
}

/// Text with its runs of spaces made one: processors are named padded.
pub fn tidy(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The memory a card declares: eight bytes, or four on an older driver.
pub fn memory_declared(bytes: &[u8]) -> Option<u64> {
    match bytes.len() {
        8 => Some(u64::from_le_bytes(bytes.try_into().ok()?)),
        4 => Some(u64::from(u32::from_le_bytes(bytes.try_into().ok()?))),
        _ => None,
    }
    .filter(|memory| *memory > 0)
}

/// The mode of a power overlay, by the identifier Windows keeps it under.
/// Nothing for no overlay at all.
pub fn overlay_name(identifier: &str) -> Option<String> {
    let identifier = identifier
        .trim()
        .trim_matches(['{', '}'])
        .to_ascii_lowercase();
    match identifier.as_str() {
        "" | "00000000-0000-0000-0000-000000000000" => None,
        "961cc777-2547-4f9d-8174-7d86181b8a7a" => Some("Best power efficiency".to_string()),
        "3af9b8d9-7c97-431d-ad78-34a8bfea439f" => Some("Better performance".to_string()),
        "ded574b5-45a0-4f42-8737-46345c09c238" => Some("Best performance".to_string()),
        other => Some(other.to_string()),
    }
}

/// What a computer is.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Machine {
    pub windows: Option<Windows>,
    pub processor: Option<String>,
    /// Logical processors, nought when unknown.
    pub processors: usize,
    /// Memory Windows can use, in bytes.
    pub memory: Option<u64>,
    pub power: Power,
    pub cards: Vec<Card>,
}

impl fmt::Display for Machine {
    /// One line, whatever could not be read left out.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut told = Vec::new();
        if let Some(windows) = &self.windows {
            told.push(windows.to_string());
        }
        match (&self.processor, self.processors) {
            (Some(name), 0) => told.push(name.clone()),
            (Some(name), count) => told.push(format!("{name}, {count} logical processors")),
            (None, 0) => {}
            (None, count) => told.push(format!("{count} logical processors")),
        }
        if let Some(memory) = self.memory {
            told.push(format!("{} of memory", size(memory)));
        }
        if !self.power.is_unknown() {
            told.push(self.power.to_string());
        }
        if !self.cards.is_empty() {
            let mut cards = String::from("graphics ");
            for (at, card) in self.cards.iter().enumerate() {
                if at > 0 {
                    cards.push_str(", ");
                }
                let _ = write!(cards, "{card}");
            }
            told.push(cards);
        }
        f.write_str(&told.join("; "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_laptop() -> Machine {
        Machine {
            windows: Some(Windows {
                build: 22_631,
                revision: Some(4460),
                version: Some("23H2".to_string()),
                edition: Some("Professional".to_string()),
            }),
            processor: Some("Example(R) Core(TM) i5-0000H CPU @ 2.40GHz".to_string()),
            processors: 8,
            memory: Some(17_000_000_000),
            power: Power {
                plan: Some("Balanced".to_string()),
                mode: Some("Best performance".to_string()),
                source: Some(Source::Mains),
            },
            cards: vec![
                Card {
                    name: "Example GTX 0000 Ti".to_string(),
                    driver: Some("32.0.15.6094".to_string()),
                    memory: Some(6 * 1024 * 1024 * 1024),
                },
                Card {
                    name: "Example UHD Graphics".to_string(),
                    driver: Some("31.0.101.2130".to_string()),
                    memory: Some(128 * 1024 * 1024),
                },
            ],
        }
    }

    #[test]
    fn a_computer_is_told_in_one_line() {
        assert_eq!(
            a_laptop().to_string(),
            "Windows 11 23H2 (build 22631.4460, Professional); Example(R) Core(TM) i5-0000H CPU \
             @ 2.40GHz, 8 logical processors; 15.8 GB of memory; power plan Balanced, Best \
             performance mode, on the mains; graphics Example GTX 0000 Ti (driver 32.0.15.6094, \
             6.0 GB), Example UHD Graphics (driver 31.0.101.2130, 128 MB)"
        );
    }

    #[test]
    fn a_card_declares_its_memory_in_eight_bytes_or_four() {
        assert_eq!(
            memory_declared(&6_442_450_944u64.to_le_bytes()),
            Some(6_442_450_944)
        );
        assert_eq!(
            memory_declared(&134_217_728u32.to_le_bytes()),
            Some(134_217_728)
        );
        assert_eq!(memory_declared(&0u64.to_le_bytes()), None);
        assert_eq!(memory_declared(&[1, 2, 3]), None);
    }

    #[test]
    fn the_bytes_of_a_text_are_its_letters_and_a_stray_byte_is_left_out() {
        let mut bytes: Vec<u8> = "Équilibré"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        assert_eq!(
            String::from_utf16_lossy(&letters(&bytes)),
            "Équilibré".to_string()
        );
        bytes.push(7);
        assert_eq!(letters(&bytes).len(), 9);
    }

    #[test]
    fn a_processor_is_named_without_its_padding() {
        assert_eq!(
            tidy("Example(R) Core(TM) i5-0000H CPU @   2.40GHz  "),
            "Example(R) Core(TM) i5-0000H CPU @ 2.40GHz"
        );
    }

    #[test]
    fn a_power_mode_is_named_by_its_identifier_and_none_means_none() {
        assert_eq!(
            overlay_name("ded574b5-45a0-4f42-8737-46345c09c238").as_deref(),
            Some("Best performance")
        );
        assert_eq!(
            overlay_name("{961CC777-2547-4F9D-8174-7D86181B8A7A}").as_deref(),
            Some("Best power efficiency")
        );
        assert_eq!(overlay_name("00000000-0000-0000-0000-000000000000"), None);
        assert_eq!(overlay_name(""), None);
        assert_eq!(
            overlay_name("12345678-0000-0000-0000-000000000000").as_deref(),
            Some("12345678-0000-0000-0000-000000000000")
        );
    }

    #[test]
    fn what_could_not_be_read_is_left_out() {
        let mut machine = a_laptop();
        machine.windows = Some(Windows {
            build: 19_045,
            revision: None,
            version: None,
            edition: None,
        });
        machine.processor = None;
        machine.memory = None;
        machine.power = Power {
            source: Some(Source::Battery),
            ..Power::default()
        };
        machine.cards = vec![Card {
            name: "Example Basic Display".to_string(),
            driver: None,
            memory: None,
        }];
        assert_eq!(
            machine.to_string(),
            "Windows 10 (build 19045); 8 logical processors; on battery; graphics Example Basic \
             Display"
        );
        assert_eq!(Machine::default().to_string(), "");
    }
}
