//! « See the details »: everything about this computer, its fingerprint
//! first.
//!
//! The fingerprint lives here and in « Add a computer », not on the home
//! page: it is only ever needed to write this computer down on another
//! one (`docs/INTERFACE.md`). The rest is sorted in three groups, each a
//! framed table of facts with an icon, so that the eye finds a value by
//! its icon before reading its label.

use zyr_draw::Rect;
use zyr_draw::design;
use zyr_draw::icons::{self, Icon};
use zyr_i18n::say;
use zyr_system::Source;

use super::{Action, Shape, callout, fingerprint};
use crate::home::pages::home::tone_colour;
use crate::home::seen::Seen;
use crate::home::state::Target;
use crate::home::ui::{Kind, Painter, middle, size, square};
use crate::home::words::{self, Tone};

/// One fact of this computer: its icon, its label, and what it says,
/// each entry a value and what precedes it in smaller words under it.
struct Detail {
    icon: &'static Icon,
    label: String,
    said: Vec<(String, Option<String>)>,
    /// A state, whose dot is drawn before the value in its colour.
    tone: Option<Tone>,
    /// Taking the whole width of its table: a list that would be cut in
    /// a column.
    wide: bool,
}

impl Detail {
    fn new(icon: &'static Icon, label: String, value: String, below: Option<String>) -> Self {
        Detail {
            icon,
            label,
            said: vec![(value, below)],
            tone: None,
            wide: false,
        }
    }

    fn toned(self, tone: Tone) -> Self {
        Detail {
            tone: Some(tone),
            ..self
        }
    }
}

/// The disc an icon sits in, and the space around a fact in its table.
const DISC: f32 = 36.0;
const PADDING: f32 = 16.0;

pub fn shape(_painter: &Painter) -> Shape {
    Shape {
        title: say!("home.this_computer"),
        lead: Some(say!("details.lead")),
        width: 780.0,
        left: Vec::new(),
        right: vec![Action::new(
            say!("dialog.close"),
            Kind::Primary,
            Target::CloseDialog,
        )],
    }
}

pub fn draw(painter: &mut Painter, at: Rect) -> f32 {
    let seen = painter.seen;
    let colours = painter.colours;
    let (left, width) = (at.left, size(at).0);
    let mut y = at.top;

    let own = seen
        .standing
        .as_ref()
        .map(|said| said.fingerprint.clone())
        .unwrap_or_default();
    y += callout(
        painter,
        (left, y, width),
        colours.accent,
        |painter, (left, top, width)| {
            let colours = painter.colours;
            let mut y = top;
            let pen = painter.body().in_bold();
            let height = painter.line(pen);
            painter.text(
                &say!("details.fingerprint"),
                pen,
                colours.text,
                Rect::at(left, y, width, height),
            );
            y += height + painter.px(2.0);
            y += painter.paragraph(
                &say!("details.fingerprint_lead"),
                painter.caption(),
                colours.text_soft,
                (left, y, width),
            );
            y += painter.px(design::SPACE_3);
            y += fingerprint(painter, (left, y, width), &own);
            y - top
        },
    );

    for (icon, title, details) in groups(seen) {
        if details.is_empty() {
            continue;
        }
        y += painter.px(design::SPACE_6);
        y += heading(painter, (left, y, width), icon, &title);
        y += painter.px(design::SPACE_3);
        y += table(painter, (left, y, width), &details);
    }
    y - at.top
}

/// A group's heading: its icon in the accent, and its name.
fn heading(
    painter: &mut Painter,
    (left, top, width): (f32, f32, f32),
    icon: &Icon,
    title: &str,
) -> f32 {
    let colours = painter.colours;
    let pen = painter.body().in_bold();
    let height = painter.line(pen);
    let mark = painter.px(18.0);
    painter.icon(
        icon,
        Rect::at(left, top + (height - mark) / 2.0, mark, mark),
        colours.accent,
    );
    let words = left + mark + painter.px(design::SPACE_2);
    painter.text(
        title,
        pen,
        colours.text,
        Rect::at(words, top, left + width - words, height),
    );
    height
}

/// A group's facts in a framed table, two to a row when the dialog has
/// the room, a rule between the rows and between the two of a row.
/// Gives back its height.
fn table(painter: &mut Painter, (left, top, width): (f32, f32, f32), details: &[Detail]) -> f32 {
    let colours = painter.colours;
    let height = painter.measured(|painter| rows(painter, (left, top, width), details));
    let at = Rect::at(left, top, width, height);
    let radius = painter.px(design::RADIUS);
    painter.fill(at, radius, colours.surface_2);
    painter.stroke(at, radius, colours.border);
    rows(painter, (left, top, width), details);
    height
}

fn rows(painter: &mut Painter, (left, top, width): (f32, f32, f32), details: &[Detail]) -> f32 {
    let colours = painter.colours;
    let two = width >= painter.px(520.0);
    let half = width / 2.0;
    let mut y = top;
    let mut rest = details;
    while let Some(first) = rest.first() {
        let pair = two && !first.wide && rest.get(1).is_some_and(|next| !next.wide);
        if y > top {
            painter.rule(left, y, width);
        }
        let row = if pair {
            let height = cell(painter, (left, y, half), first).max(cell(
                painter,
                (left + half, y, half),
                &rest[1],
            ));
            painter.upright(left + half, y, height, colours.border);
            rest = &rest[2..];
            height
        } else {
            rest = &rest[1..];
            cell(painter, (left, y, width), first)
        };
        y += row;
    }
    y - top
}

/// One fact in its place in a table: its icon in a disc, its label, and
/// what it says. Gives back its height.
fn cell(painter: &mut Painter, (left, top, width): (f32, f32, f32), detail: &Detail) -> f32 {
    let colours = painter.colours;
    let padding = painter.px(PADDING);
    let disc = painter.px(DISC);
    let at = Rect::at(left + padding, top + padding, disc, disc);
    painter.fill(at, disc / 2.0, colours.accent.faded(0.12));
    painter.icon(
        detail.icon,
        square(middle(at), painter.px(18.0)),
        colours.accent,
    );

    let words = at.right + painter.px(design::SPACE_3);
    let room = left + width - padding - words;
    let label_pen = painter.caption();
    let label_height = painter.line(label_pen);
    painter.text(
        &detail.label,
        label_pen,
        colours.text_soft,
        Rect::at(words, top + padding, room, label_height),
    );
    let mut y = top + padding + label_height + painter.px(2.0);
    for (rank, (value, below)) in detail.said.iter().enumerate() {
        if rank > 0 {
            y += painter.px(design::SPACE_2);
        }
        let body = painter.body().in_bold();
        let mut x = words;
        if let Some(tone) = detail.tone {
            let dot = painter.px(8.0);
            painter.dot(
                (x + dot / 2.0, y + painter.line(body) / 2.0),
                dot,
                tone_colour(painter, tone),
                false,
            );
            x += dot + painter.px(design::SPACE_2);
        }
        y += painter.paragraph(value, body, colours.text, (x, y, words + room - x));
        if let Some(below) = below {
            y += painter.paragraph(
                below,
                painter.caption(),
                colours.text_soft,
                (words, y, room),
            );
        }
    }
    (y - top + padding).max(disc + padding * 2.0)
}

/// What is known of this computer, in its three groups, what could not
/// be read left out.
fn groups(seen: &Seen) -> [(&'static Icon, String, Vec<Detail>); 3] {
    let (access, access_tone) = words::this_computer(seen.standing.as_ref(), seen.ffmpeg_here);
    let version = match words::versions_differ(seen) {
        Some(service) => Detail::new(
            &icons::CODE,
            say!("details.version"),
            zyr_proto::PRODUCT_VERSION.to_owned(),
            Some(say!(
                "home.version_differs",
                window = zyr_proto::BUILD,
                service = service
            )),
        )
        .toned(Tone::Waiting),
        None => Detail::new(
            &icons::CODE,
            say!("details.version"),
            zyr_proto::PRODUCT_VERSION.to_owned(),
            Some(zyr_proto::BUILD.to_owned()),
        ),
    };
    let zyrdesk = vec![
        Detail::new(&icons::LAPTOP, say!("details.name"), seen.this_name(), None),
        Detail::new(&icons::SHIELD, say!("details.remote_access"), access, None).toned(access_tone),
        version,
    ];

    let mut hardware = Vec::new();
    if let Some(machine) = seen.machine.as_ref() {
        if let (Some(named), Some(windows)) = (words::windows(machine), machine.windows.as_ref()) {
            let build = match windows.revision {
                Some(revision) => format!("{}.{revision}", windows.build),
                None => windows.build.to_string(),
            };
            let detail = match windows.version.as_deref() {
                Some(version) => say!(
                    "details.version_and_build",
                    version = version,
                    build = build
                ),
                None => say!("details.build", build = build),
            };
            hardware.push(Detail::new(
                &icons::WINDOWS,
                say!("details.system"),
                named,
                Some(detail),
            ));
        }
        if let Some(processor) = machine.processor.as_deref() {
            let threads = (machine.processors > 0)
                .then(|| say!("details.threads", count = machine.processors));
            hardware.push(Detail::new(
                &icons::PROCESSOR,
                say!("details.processor"),
                words::processor(processor),
                threads,
            ));
        }
        if let Some(memory) = words::memory(machine) {
            hardware.push(Detail::new(
                &icons::MEMORY,
                say!("details.memory"),
                memory,
                None,
            ));
        }
        let source = match machine.power.source {
            Some(Source::Mains) => Some(say!("details.on_mains")),
            Some(Source::Battery) => Some(say!("details.on_battery")),
            None => None,
        };
        match (source, machine.power.plan.clone()) {
            (Some(source), plan) => hardware.push(Detail::new(
                &icons::POWER,
                say!("details.power"),
                source,
                plan,
            )),
            (None, Some(plan)) => hardware.push(Detail::new(
                &icons::POWER,
                say!("details.power"),
                plan,
                None,
            )),
            (None, None) => {}
        }
        if !machine.cards.is_empty() {
            hardware.push(Detail {
                icon: &icons::GRAPHICS,
                label: say!("details.graphics"),
                said: machine
                    .cards
                    .iter()
                    .map(|card| {
                        let driver = card
                            .driver
                            .as_deref()
                            .map(|driver| say!("details.driver", driver = driver));
                        (card.name.clone(), driver)
                    })
                    .collect(),
                tone: None,
                wide: true,
            });
        }
    }

    let (network, network_tone) = words::connectivity(seen.connectivity);
    let addresses = if seen.addresses.is_empty() {
        say!("details.no_address")
    } else {
        seen.addresses
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    };
    let network = vec![
        Detail::new(&icons::WORLD, say!("details.connection"), network, None).toned(network_tone),
        Detail::new(
            &icons::LOCAL_NETWORK,
            say!("details.addresses"),
            addresses,
            None,
        ),
    ];

    [
        (&icons::LAPTOP, say!("details.on_zyrdesk"), zyrdesk),
        (&icons::PROCESSOR, say!("details.hardware"), hardware),
        (&icons::WORLD, say!("home.network"), network),
    ]
}
