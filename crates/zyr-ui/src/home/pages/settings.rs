//! The settings page: what holds for every session and for this
//! computer.
//!
//! Nothing is kept here: what is chosen goes to the service, which writes
//! it down and says it back at the next question, or to the file this
//! window keeps for its theme and its keys. What a session asks for while
//! it is shown, its size, its rate and its codec, lives in the session's
//! own menu, where its effect can be seen; the line at the head of « More
//! settings » only says where they stand.

use zyr_draw::Rect;
use zyr_draw::design;
use zyr_draw::icons;
use zyr_i18n::say;

use crate::home::pages::{Control, Head, line, page_title, section, subheading};
use crate::home::state::{Pick, Target, Toggle};
use crate::home::ui::{Kind, Painter, size};
use crate::home::words;
use crate::shell::shortcuts::Doing;
use crate::shell::theme::Choice;

/// The widest the page's cards go: lines of settings any wider read as a
/// form across a field.
const WIDEST: f32 = 860.0;

/// The codecs and the ways of showing a session the sides offer, as they
/// travel and are written down, in the order the sides show them.
pub const CODECS: [&str; 4] = ["auto", "H.264", "HEVC", "AV1"];
pub const DISPLAYS: [&str; 2] = ["fullscreen", "windowed"];

/// Draws the page within `at`, from its top, and gives back the height it
/// takes.
pub fn draw(painter: &mut Painter, at: Rect) -> f32 {
    let width = size(at).0.min(painter.px(WIDEST));
    let at = Rect {
        right: at.left + width,
        ..at
    };
    let between = painter.px(design::SPACE_5);
    let mut y = at.top;
    y += page_title(painter, at, &say!("nav.settings"), &say!("settings.lead"));
    y += general(painter, Rect { top: y, ..at }) + between;
    y += account(painter, Rect { top: y, ..at }) + between;
    y += shortcuts(painter, Rect { top: y, ..at }) + between;
    y += journal(painter, Rect { top: y, ..at }) + between;
    y += more(painter, Rect { top: y, ..at });
    y - at.top
}

/* ---- The sections ----------------------------------------------------- */

fn general(painter: &mut Painter, at: Rect) -> f32 {
    let head = Head {
        icon: &icons::THEME,
        title: say!("settings.general"),
        lead: None,
        fold: None,
    };
    section(painter, at, &head, |painter, room| {
        let (left, width) = (room.left, size(room).0);
        let mut y = room.top;
        let chosen = Choice::ALL
            .iter()
            .position(|choice| *choice == crate::shell::theme::chosen());
        let words = vec![
            say!("settings.theme_system"),
            say!("settings.theme_light"),
            say!("settings.theme_dark"),
        ];
        let targets = Choice::ALL
            .iter()
            .map(|choice| Target::Theme(*choice))
            .collect();
        y += line(
            painter,
            (left, y, width),
            &say!("settings.theme"),
            &say!("settings.theme_caption"),
            Control::Sides(words, chosen, true, targets),
        );
        y += line(
            painter,
            (left, y, width),
            &say!("settings.at_boot"),
            &say!("settings.at_boot_caption"),
            Control::Switch(Toggle::AtBoot),
        );
        y += line(
            painter,
            (left, y, width),
            &say!("settings.trust"),
            &say!("settings.trust_caption"),
            Control::Switch(Toggle::Trust),
        );
        y - room.top
    })
}

/// The account: the link as it stands, what makes or undoes it, and the
/// devices on it.
fn account(painter: &mut Painter, at: Rect) -> f32 {
    let head = Head {
        icon: &icons::SERVER,
        title: say!("settings.account"),
        lead: Some(say!("settings.account_lead")),
        fold: None,
    };
    section(painter, at, &head, |painter, room| {
        let seen = painter.seen;
        let (left, width) = (room.left, size(room).0);
        let mut y = room.top;
        let Some(account) = seen.account.as_ref() else {
            return line(
                painter,
                (left, y, width),
                &say!("settings.account_unknown"),
                &say!("settings.account_unknown_caption"),
                Control::Nothing,
            );
        };
        let Some(link) = account.link.as_ref() else {
            return line(
                painter,
                (left, y, width),
                &say!("settings.no_account"),
                &say!("settings.no_account_caption"),
                Control::Buttons(vec![(
                    say!("signin.sign_in"),
                    Kind::Primary,
                    Target::SignIn,
                    Some(&icons::LOGIN),
                )]),
            );
        };
        let server = if link.name.is_empty() {
            link.server.clone()
        } else {
            link.name.clone()
        };
        let state = if link.connected {
            say!("settings.linked", server = server)
        } else {
            match link.trouble.as_ref() {
                Some(why) => say!(
                    "settings.unreachable_because",
                    server = server,
                    why = zyr_i18n::fact(why).replace('\n', " ")
                ),
                None => say!("settings.unreachable", server = server),
            }
        };
        let armed = painter.state.is_armed(&Target::Detach);
        y += line(
            painter,
            (left, y, width),
            &link.username,
            &state,
            Control::Buttons(vec![(
                if armed {
                    say!("dialog.confirm")
                } else {
                    say!("settings.detach")
                },
                if armed { Kind::Warning } else { Kind::Quiet },
                Target::Detach,
                Some(&icons::LOGOUT),
            )]),
        );
        let colours = painter.colours;
        y += subheading(painter, (left, y, width), &say!("settings.devices"));
        if account.devices.is_empty() {
            let said = if link.connected {
                say!("settings.no_device")
            } else {
                say!("settings.devices_later")
            };
            y += painter.paragraph(
                &said,
                painter.caption(),
                colours.text_soft,
                (left, y, width),
            ) + painter.px(design::SPACE_3);
        }
        for device in &account.devices {
            let label = if device.this {
                say!("settings.this_device", name = device.name)
            } else {
                device.name.clone()
            };
            let rename = Target::RenameDevice(device.id.clone());
            let revoke = Target::Revoke(device.id.clone());
            let mut buttons = vec![(say!("renaming.rename"), Kind::Quiet, rename, None)];
            if !device.this {
                let armed = painter.state.is_armed(&revoke);
                buttons.insert(
                    0,
                    (
                        if armed {
                            say!("dialog.confirm")
                        } else {
                            say!("settings.revoke")
                        },
                        if armed { Kind::Warning } else { Kind::Ghost },
                        revoke,
                        None,
                    ),
                );
            }
            y += line(
                painter,
                (left, y, width),
                &label,
                &words::device_presence(device),
                Control::Buttons(buttons),
            );
        }
        y - room.top
    })
}

/// The keys that work during a session, over its picture.
fn shortcuts(painter: &mut Painter, at: Rect) -> f32 {
    let head = Head {
        icon: &icons::SHORTCUTS,
        title: say!("settings.shortcuts"),
        lead: Some(say!("settings.shortcuts_lead")),
        fold: None,
    };
    section(painter, at, &head, |painter, room| {
        let (left, width) = (room.left, size(room).0);
        let mut y = room.top;
        for (doing, label, caption) in [
            (
                Doing::End,
                say!("settings.key_end"),
                say!("settings.key_end_caption"),
            ),
            (
                Doing::Menu,
                say!("settings.key_menu"),
                say!("settings.key_menu_caption"),
            ),
            (
                Doing::Fullscreen,
                say!("settings.key_fullscreen"),
                say!("settings.key_fullscreen_caption"),
            ),
            (
                Doing::NextScreen,
                say!("settings.key_next_screen"),
                say!("settings.key_next_screen_caption"),
            ),
        ] {
            y += line(
                painter,
                (left, y, width),
                &label,
                &caption,
                Control::Key(doing),
            );
        }
        y - room.top
    })
}

/// The journals, which are what a problem is reported with.
fn journal(painter: &mut Painter, at: Rect) -> f32 {
    let head = Head {
        icon: &icons::JOURNAL,
        title: say!("settings.journal"),
        lead: None,
        fold: None,
    };
    section(painter, at, &head, |painter, room| {
        let folder = painter.seen.folder.clone();
        line(
            painter,
            (room.left, room.top, size(room).0),
            &say!("settings.journal_line"),
            &say!("settings.journal_caption", folder = folder),
            Control::Buttons(vec![
                (
                    say!("settings.open_journal"),
                    Kind::Quiet,
                    Target::OpenJournal,
                    Some(&icons::JOURNAL),
                ),
                (
                    say!("journal.open_folder"),
                    Kind::Ghost,
                    Target::OpenLogs,
                    Some(&icons::FOLDER),
                ),
            ]),
        )
    })
}

/// What only matters to someone trying things: folded until asked for,
/// since it is the one place jargon is allowed.
fn more(painter: &mut Painter, at: Rect) -> f32 {
    let head = Head {
        icon: &icons::ADJUSTMENTS,
        title: say!("settings.more"),
        lead: Some(say!("settings.more_lead")),
        fold: Some(painter.state.advanced),
    };
    section(painter, at, &head, |painter, room| {
        if painter.state.advanced {
            advanced(painter, room)
        } else {
            0.0
        }
    })
}

fn advanced(painter: &mut Painter, room: Rect) -> f32 {
    let seen = painter.seen;
    let (left, width) = (room.left, size(room).0);
    let mut y = room.top;
    let asked = seen.settings.as_ref().map_or_else(String::new, |said| {
        say!(
            "settings.asked_caption",
            width = said.width,
            height = said.height,
            fps = said.fps,
            rate = (said.bitrate_kbps as f32 / 1000.0).round() as u32
        )
    });
    y += line(
        painter,
        (left, y, width),
        &say!("settings.asked"),
        &asked,
        Control::Nothing,
    );

    let settings = seen.settings.as_ref();
    let enabled = settings.is_some();
    let codec = settings.map(|said| said.codec.as_str());
    y += line(
        painter,
        (left, y, width),
        &say!("settings.codec"),
        &say!("settings.codec_caption"),
        Control::Sides(
            vec![
                say!("menu.codec_auto"),
                "H.264".to_string(),
                "HEVC".to_string(),
                "AV1".to_string(),
            ],
            codec.and_then(|codec| CODECS.iter().position(|offered| *offered == codec)),
            enabled,
            (0..CODECS.len())
                .map(|rank| Target::Side(Pick::Codec, rank))
                .collect(),
        ),
    );
    let display = settings.map(|said| said.display.as_str());
    y += line(
        painter,
        (left, y, width),
        &say!("settings.display"),
        &say!("settings.display_caption"),
        Control::Sides(
            vec![
                say!("settings.display_fullscreen"),
                say!("settings.display_windowed"),
            ],
            display.and_then(|display| DISPLAYS.iter().position(|offered| *offered == display)),
            enabled,
            (0..DISPLAYS.len())
                .map(|rank| Target::Side(Pick::Display, rank))
                .collect(),
        ),
    );
    y += line(
        painter,
        (left, y, width),
        &say!("menu.mouse"),
        &say!("settings.mouse_caption"),
        Control::Sides(
            vec![say!("menu.mouse_desktop"), say!("menu.mouse_game")],
            settings.map(|said| usize::from(!said.absolute_mouse)),
            enabled,
            vec![Target::Side(Pick::Mouse, 0), Target::Side(Pick::Mouse, 1)],
        ),
    );
    y += line(
        painter,
        (left, y, width),
        &say!("settings.far_speakers"),
        &say!("settings.far_speakers_caption"),
        Control::Switch(Toggle::FarSpeakers),
    );
    y += line(
        painter,
        (left, y, width),
        &say!("settings.statistics"),
        &say!("settings.statistics_caption"),
        Control::Switch(Toggle::Statistics),
    );

    let colours = painter.colours;
    y += subheading(painter, (left, y, width), &say!("settings.network_trials"));
    y += painter.paragraph(
        &say!("settings.network_trials_caption"),
        painter.caption(),
        colours.text_soft,
        (left, y, width),
    ) + painter.px(design::SPACE_3);
    y += line(
        painter,
        (left, y, width),
        &say!("settings.marking"),
        &say!("settings.marking_caption"),
        Control::Switch(Toggle::Marking),
    );
    y += line(
        painter,
        (left, y, width),
        &say!("settings.fixed_port", port = zyr_proto::net::TUNNEL_PORT),
        &say!("settings.fixed_port_caption"),
        Control::Switch(Toggle::FixedPort),
    );
    y - room.top
}
