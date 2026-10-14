//! « See the details »: everything about this computer, its fingerprint
//! first.
//!
//! The fingerprint lives here and in « Add a computer », not on the home
//! page: it is only ever needed to write this computer down on another
//! one (`docs/INTERFACE.md`).

use zyr_draw::Rect;
use zyr_draw::design;
use zyr_i18n::say;
use zyr_system::Source;

use super::{Action, Shape, callout, fact, fingerprint};
use crate::home::seen::Seen;
use crate::home::state::Target;
use crate::home::ui::{Kind, Painter, size};
use crate::home::words;

pub fn shape(_painter: &Painter) -> Shape {
    Shape {
        title: say!("home.this_computer"),
        lead: Some(say!("details.lead")),
        width: 660.0,
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
    y += painter.px(design::SPACE_5);

    let facts = facts_of(seen);
    let gap = painter.px(design::SPACE_5);
    let columns = if width >= painter.px(480.0) { 2 } else { 1 };
    let column = (width - gap * (columns as f32 - 1.0)) / columns as f32;
    for row in facts.chunks(columns) {
        let mut tallest: f32 = 0.0;
        for (rank, (label, value)) in row.iter().enumerate() {
            let x = left + (column + gap) * rank as f32;
            tallest = tallest.max(fact(painter, (x, y, column), label, value));
        }
        y += tallest + painter.px(design::SPACE_4);
    }
    y - painter.px(design::SPACE_4) - at.top
}

/// What is known of this computer, label and value, what could not be
/// read left out.
fn facts_of(seen: &Seen) -> Vec<(String, String)> {
    let mut facts = vec![
        (say!("details.name"), seen.this_name()),
        (
            say!("details.remote_access"),
            words::this_computer(seen.standing.as_ref(), seen.ffmpeg_here).0,
        ),
    ];
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
            facts.push((say!("details.system"), format!("{named}\n{detail}")));
        }
        if let Some(processor) = machine.processor.as_deref() {
            let mut said = words::processor(processor);
            if machine.processors > 0 {
                said = format!(
                    "{said}\n{}",
                    say!("details.threads", count = machine.processors)
                );
            }
            facts.push((say!("details.processor"), said));
        }
        if let Some(memory) = words::memory(machine) {
            facts.push((say!("details.memory"), memory));
        }
        if !machine.cards.is_empty() {
            let cards = machine
                .cards
                .iter()
                .map(|card| match card.driver.as_deref() {
                    Some(driver) => {
                        say!("details.card_and_driver", card = card.name, driver = driver)
                    }
                    None => card.name.clone(),
                })
                .collect::<Vec<_>>()
                .join("\n");
            facts.push((say!("details.graphics"), cards));
        }
        let source = match machine.power.source {
            Some(Source::Mains) => Some(say!("details.on_mains")),
            Some(Source::Battery) => Some(say!("details.on_battery")),
            None => None,
        };
        let power = source
            .into_iter()
            .chain(machine.power.plan.clone())
            .collect::<Vec<_>>()
            .join(" · ");
        if !power.is_empty() {
            facts.push((say!("details.power"), power));
        }
    }
    facts.push((
        say!("home.network"),
        words::connectivity(seen.connectivity).0,
    ));
    facts.push((
        say!("details.addresses"),
        if seen.addresses.is_empty() {
            say!("details.no_address")
        } else {
            seen.addresses
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        },
    ));
    let version = match words::versions_differ(seen) {
        Some(service) => say!(
            "home.version_differs",
            window = seen.version,
            service = service
        ),
        None => seen.version.clone(),
    };
    facts.push((say!("details.version"), version));
    facts
}
