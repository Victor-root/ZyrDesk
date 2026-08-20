//! The one virtual screen driver this product carries.
//!
//! Everything below is true of this driver and of nothing else: the name
//! Windows files it under, the files its package is made of, the folder
//! it reads its settings from and how it is told to read another one,
//! and the shape of the file those settings are written in. Nothing
//! outside this file knows any of it.
//!
//! It is a driver somebody else publishes, under a licence that lets it
//! be carried along, and signed by a foundation that signs open source
//! for free. That last point is the whole reason it is this one: Windows
//! refuses to load a driver nobody vouched for, vouching costs money
//! every year, and this product spends none.
//!
//! What it is told to do is narrow. It grows one screen, it offers the
//! sizes written down for it, and it keeps its own papers in ZyrDesk's
//! folder rather than in one of its own at the root of the disk.

use std::path::{Path, PathBuf};

use crate::driver::{Driver, Guid, Mode};
use crate::{Done, Trouble};

/// Where the driver looks for the folder it should read and write in.
///
/// Left to itself it uses a folder of its own making at the root of the
/// disk, which is neither ours to leave behind nor ours to clean up.
#[cfg(windows)]
const WHERE_IT_LOOKS: &str = r"SOFTWARE\MikeTheTech\VirtualDisplayDriver";

/// Name of the value under that key holding the folder.
#[cfg(windows)]
const THE_FOLDER_VALUE: &str = "VDDPATH";

/// The settings file the driver reads inside that folder.
const SETTINGS_FILE: &str = "vdd_settings.xml";

/// Name the screen introduces itself by among the machine's screens.
///
/// Not the name of the driver and not the name of the device: what a
/// screen is called comes from the little block of identity every screen
/// publishes, and this driver publishes that. It is the only name the
/// host engine ever sees, so it is the one to look for.
const HOW_THE_SCREEN_INTRODUCES_ITSELF: &str = "VDD by MTT";

/// The driver, as one value the whole product shares.
pub const MTT: Mtt = Mtt;

#[derive(Debug, Clone, Copy)]
pub struct Mtt;

impl Driver for Mtt {
    fn name(&self) -> &'static str {
        "Virtual Display Driver"
    }

    fn hardware_id(&self) -> &'static str {
        r"Root\MttVDD"
    }

    fn class(&self) -> (&'static str, Guid) {
        (
            "Display",
            Guid {
                a: 0x4d36_e968,
                b: 0xe325,
                c: 0x11ce,
                d: [0xbf, 0xc1, 0x08, 0x00, 0x2b, 0xe1, 0x03, 0x18],
            },
        )
    }

    fn inf_file(&self) -> &'static str {
        "MttVDD.inf"
    }

    fn catalog_file(&self) -> &'static str {
        "MttVDD.cat"
    }

    fn package_files(&self) -> &'static [&'static str] {
        // The signature covers exactly this set: a package short of one
        // of them, or carrying one that was touched, is a package
        // Windows will not load.
        &["MttVDD.inf", "MttVDD.cat", "MttVDD.dll"]
    }

    fn is_its_screen(&self, friendly_name: &str) -> bool {
        friendly_name
            .trim()
            .eq_ignore_ascii_case(HOW_THE_SCREEN_INTRODUCES_ITSELF)
    }

    #[cfg(windows)]
    fn settle_in(&self, home: &Path, done: &mut Done) -> Result<(), Trouble> {
        std::fs::create_dir_all(home).map_err(|e| Trouble::File {
            path: home.display().to_string(),
            reason: e.to_string(),
        })?;
        crate::place::write_registry_text(WHERE_IT_LOOKS, THE_FOLDER_VALUE, home)?;
        done.step(format!(
            "virtual screen told to keep its papers in {}",
            home.display()
        ));
        Ok(())
    }

    #[cfg(windows)]
    fn move_out(&self, home: &Path, done: &mut Done) -> Result<(), Trouble> {
        crate::place::forget_registry_key(WHERE_IT_LOOKS)?;
        let settings = settings_path(home);
        match std::fs::remove_file(&settings) {
            Ok(()) => done.step(format!(
                "virtual screen settings removed: {}",
                settings.display()
            )),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(Trouble::File {
                    path: settings.display().to_string(),
                    reason: e.to_string(),
                });
            }
        }
        done.step("virtual screen no longer points at any folder of ours");
        Ok(())
    }

    #[cfg(not(windows))]
    fn settle_in(&self, _home: &Path, _done: &mut Done) -> Result<(), Trouble> {
        Err(Trouble::NotHere)
    }

    #[cfg(not(windows))]
    fn move_out(&self, _home: &Path, _done: &mut Done) -> Result<(), Trouble> {
        Err(Trouble::NotHere)
    }

    fn write_modes(&self, home: &Path, modes: &[Mode], done: &mut Done) -> Result<bool, Trouble> {
        let settings = settings_path(home);
        let wanted = settings_file(modes);
        if std::fs::read_to_string(&settings).is_ok_and(|already| already == wanted) {
            return Ok(false);
        }
        if let Some(folder) = settings.parent() {
            std::fs::create_dir_all(folder).map_err(|e| Trouble::File {
                path: folder.display().to_string(),
                reason: e.to_string(),
            })?;
        }
        std::fs::write(&settings, &wanted).map_err(|e| Trouble::File {
            path: settings.display().to_string(),
            reason: e.to_string(),
        })?;
        done.step(format!(
            "virtual screen sizes written to {} : {}",
            settings.display(),
            modes
                .iter()
                .map(Mode::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ));
        Ok(true)
    }
}

fn settings_path(home: &Path) -> PathBuf {
    home.join(SETTINGS_FILE)
}

/// The whole settings file, written out from nothing every time.
///
/// Written whole and not edited in place, so what the driver reads is
/// only ever what this function says. An edited file would carry
/// whatever a previous version of this product, or a person with a text
/// editor, had left in it, and a size nobody asked for is a size the
/// far desktop can end up wearing.
///
/// The driver reads this file by element name wherever it finds one, so
/// the nesting is for the eye alone. Its own order matters though: a
/// height belongs to the width above it, and a rate to the two above it.
fn settings_file(modes: &[Mode]) -> String {
    let mut out = String::from(
        "<?xml version='1.0' encoding='utf-8'?>\n\
         <!-- Written by ZyrDesk. Every change here is overwritten. -->\n\
         <vdd_settings>\n\
         \x20   <monitors>\n\
         \x20       <count>1</count>\n\
         \x20   </monitors>\n\
         \x20   <gpu>\n\
         \x20       <friendlyname>default</friendlyname>\n\
         \x20   </gpu>\n\
         \x20   <resolutions>\n",
    );
    for mode in modes {
        out.push_str(&format!(
            "\x20       <resolution>\n\
             \x20           <width>{}</width>\n\
             \x20           <height>{}</height>\n\
             \x20           <refresh_rate>{}</refresh_rate>\n\
             \x20       </resolution>\n",
            mode.width, mode.height, mode.hz
        ));
    }
    // Said out loud rather than left out. Absent, each of these falls to
    // whatever the driver's own default happens to be that release, and
    // a screen that quietly turns up in ten bits or in a colour shape
    // the encoder has to convert costs a session either sharpness or
    // time, with nothing to show where it went.
    out.push_str(
        "\x20   </resolutions>\n\
         \x20   <colour>\n\
         \x20       <SDR10bit>false</SDR10bit>\n\
         \x20       <HDRPlus>false</HDRPlus>\n\
         \x20       <ColourFormat>RGB</ColourFormat>\n\
         \x20   </colour>\n\
         \x20   <logging>\n\
         \x20       <logging>true</logging>\n\
         \x20       <debuglogging>false</debuglogging>\n\
         \x20   </logging>\n\
         </vdd_settings>\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_screen_is_recognised_by_the_name_it_publishes() {
        assert!(MTT.is_its_screen("VDD by MTT"));
        assert!(MTT.is_its_screen("  vdd by mtt  "));
        assert!(!MTT.is_its_screen("ROG PG279Q"));
        assert!(!MTT.is_its_screen(""));
    }

    #[test]
    fn every_size_asked_for_lands_in_the_file() {
        let modes = [Mode::new(3840, 2160, 60), Mode::new(3440, 1440, 60)];
        let written = settings_file(&modes);
        for mode in modes {
            assert!(
                written.contains(&format!("<width>{}</width>", mode.width)),
                "{written}"
            );
            assert!(
                written.contains(&format!("<height>{}</height>", mode.height)),
                "{written}"
            );
        }
        assert_eq!(written.matches("<resolution>").count(), 2);
    }

    #[test]
    fn no_size_arrives_that_nobody_asked_for() {
        // Le pilote croise toute fréquence globale avec toutes les
        // tailles : une seule ligne oubliée ici et l'écran offre des
        // tailles que personne n'a demandées, dont le bureau distant
        // peut se retrouver habillé.
        let written = settings_file(&[Mode::new(1920, 1080, 60)]);
        assert!(!written.contains("g_refresh_rate"), "{written}");
        assert_eq!(written.matches("<width>").count(), 1);
    }

    #[test]
    fn the_same_sizes_write_the_same_file() {
        // Ce qui permet de ne rien toucher quand rien ne change, et donc
        // de ne pas redémarrer un écran pour rien.
        let modes = [Mode::new(2560, 1440, 60)];
        assert_eq!(settings_file(&modes), settings_file(&modes));
        assert_ne!(
            settings_file(&modes),
            settings_file(&[Mode::new(2560, 1440, 120)])
        );
    }

    #[test]
    fn the_settings_sit_in_the_folder_we_gave_it() {
        let home = Path::new("C:\\ZyrDesk\\data\\screen");
        assert_eq!(settings_path(home), home.join("vdd_settings.xml"));
    }
}
