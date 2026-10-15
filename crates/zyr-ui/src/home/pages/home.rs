//! The home page: the banner, this computer, the others, and what can
//! be done from here.
//!
//! Drawn as the plan of the new home screen sets it out
//! (`docs/INTERFACE.md`): everything planned is shown from the first day,
//! and what does not work yet is shown in its place, faded, with a bubble
//! that says it is coming.

use zyr_control::Peer;
use zyr_draw::design::{self, Colour};
use zyr_draw::icons::{self, Icon};
use zyr_draw::{Align, Rect};
use zyr_i18n::say;

use crate::home::pages::computers::{self, seed_of};
use crate::home::pages::{activity, contacts};
use crate::home::seen::Seen;
use crate::home::state::{Page, State, Target, Toggle};
use crate::home::ui::{Button, Kind, Painter, SWITCH_SIZE, middle, size, square};
use crate::home::words::{self, Tone};

/// What is added to the banner when its block of remote access has to go
/// under its words.
const STRIP: f32 = 76.0;
/// The room inside the banner, and the block of remote access.
const BANNER_INSIDE: f32 = 36.0;
const ACCESS: f32 = 340.0;

/// This computer within the banner: the narrowest its row is, and its
/// wallpaper's frame.
const IDENTITY: f32 = 640.0;
const PORTRAIT: (f32, f32) = (272.0, 170.0);

/// The gap between the blocks of the bottom row.
const GAP: f32 = 16.0;

/// The space between two sections, and between a heading and what it
/// heads.
const BETWEEN: f32 = 28.0;
const UNDER_HEADING: f32 = 12.0;

/// The bottom row: the quick actions' width beside the two lists, and the
/// height of the row.
const ACTIONS: f32 = 432.0;
const BOTTOM: f32 = 236.0;
/// The least the bottom row is drawn at: below it, it leaves the page to
/// the computers rather than be squeezed.
const BOTTOM_LEAST: f32 = 150.0;

/// Draws the page within `at`, from its top, in the `room` the window
/// gives it, and gives back the height it takes.
///
/// The home page is not scrolled: it fits the window. The banner keeps
/// its size, the computers take as many rows as fit, and the row at the
/// bottom takes what is left, or leaves when too little is. Only a window
/// too short for the banner and one row of computers lets it run longer.
pub fn draw(painter: &mut Painter, at: Rect, room: f32) -> f32 {
    let seen = painter.seen;
    let between = painter.px(BETWEEN);
    let heading = painter.line(painter.subtitle()) + painter.px(UNDER_HEADING);
    let floor = at.top + room;

    let mut y = at.top;
    y += banner(painter, seen, Rect { top: y, ..at });
    y += between;

    let peers = computers::shown(seen, painter.state);
    let (columns, _, row, gap) = computers::laid(painter, size(at).0, peers.len());
    let rows_in = |height: f32| ((height - heading + gap) / (row + gap)).floor();
    let with_bottom_rows = rows_in(floor - y - between - heading - painter.px(BOTTOM_LEAST));
    let with_bottom = with_bottom_rows >= 1.0;
    let fit = if with_bottom {
        with_bottom_rows
    } else {
        rows_in(floor - y)
    };
    let needed = peers.len().div_ceil(columns).max(1);
    let rows = (fit.max(1.0) as usize).min(needed);
    y += computers(painter, seen, &peers, Rect { top: y, ..at }, rows);

    if with_bottom {
        y += between;
        let height = (floor - y - heading).min(painter.px(BOTTOM));
        y += bottom(painter, Rect { top: y, ..at }, height);
    }
    y - at.top
}

/* ---- The banner -------------------------------------------------------- */

/// The banner: the greeting, this computer as it is, and the switch of
/// remote access, on ZyrDesk's picture.
fn banner(painter: &mut Painter, seen: &Seen, at: Rect) -> f32 {
    let colours = painter.colours;
    let width = size(at).0;
    let inside = painter.px(BANNER_INSIDE);

    let hello = say!("home.hello", name = greeted(seen));
    let display = painter.display();
    // The words take everything the block of remote access leaves: the
    // banner is the page's widest piece, and what it says is not cut.
    let least = painter.width_of(&hello, display).max(painter.px(IDENTITY));
    let beside_room = width - inside * 3.0 - painter.px(ACCESS);
    let beside = beside_room >= least;
    let words = if beside {
        beside_room
    } else {
        width - inside * 2.0
    };

    let hello_height = painter.line(display);
    let identity_top = painter.px(30.0) + hello_height + painter.px(design::SPACE_5);
    let identity_height = painter.px(PORTRAIT.1);
    let height =
        identity_top + identity_height + inside + if beside { 0.0 } else { painter.px(STRIP) };
    let card = Rect::at(at.left, at.top, width, height);
    let radius = painter.px(design::RADIUS_LARGE);
    // Strong behind the words on the left, thin on the right, where the
    // dunes and the sun or the moon show.
    painter.scenery(
        card,
        radius,
        colours.surface_1,
        ((card.left, card.top), (card.right, card.top)),
        &[(0.0, 1.0), (0.45, 0.85), (0.75, 0.3), (1.0, 0.1)],
    );
    painter.stroke(card, radius, colours.border);

    painter.text(
        &hello,
        display,
        colours.text,
        Rect::at(
            card.left + inside,
            card.top + painter.px(30.0),
            words + painter.px(4.0),
            hello_height,
        ),
    );
    this_computer(
        painter,
        seen,
        Rect::at(
            card.left + inside,
            card.top + identity_top,
            words,
            identity_height,
        ),
    );

    if beside {
        access(
            painter,
            seen,
            card,
            Rect::at(
                card.right - inside - painter.px(ACCESS),
                card.top,
                painter.px(ACCESS),
                height,
            ),
            false,
        );
    } else {
        let strip = painter.px(56.0);
        access(
            painter,
            seen,
            card,
            Rect::at(
                card.left + inside,
                card.bottom - inside - strip + painter.px(8.0),
                width - inside * 2.0,
                strip,
            ),
            true,
        );
    }
    height
}

/// The name the banner greets: the account's when there is one, the
/// person's otherwise.
fn greeted(seen: &Seen) -> String {
    let (name, _) = seen.with_person();
    words::first_name(&name)
}

/// The colour a state's dot is drawn in.
pub fn tone_colour(painter: &Painter, tone: Tone) -> Colour {
    match tone {
        Tone::Good => painter.colours.online,
        Tone::Waiting => painter.colours.warning,
        Tone::Off => painter.colours.offline,
        Tone::Trouble => painter.colours.error,
    }
}

/// The block of remote access: the switch, where this computer stands in
/// words, and what to do when something is in the way. Laid as a block
/// within `room`, centred in its height, or as a strip across it, on a
/// pane of frosted glass over the banner's scenery, which covers
/// `banner`.
fn access(painter: &mut Painter, seen: &Seen, banner: Rect, room: Rect, strip: bool) {
    let colours = painter.colours;
    let inside = painter.px(design::SPACE_5);
    let (said, tone, remedy) = access_state(seen);
    let title = say!("home.remote_access");
    let title_pen = painter.title();
    let body = painter.pen(design::SUBTITLE);
    let switch = Target::Switch(Toggle::Access);
    let on = toggle_is_on(seen, painter.state, Toggle::Access);
    let enabled = toggle_enabled(seen, painter.state, Toggle::Access);
    let radius = painter.px(design::RADIUS);

    if strip {
        painter.glass(room, radius, banner);
        let (_, centre) = middle(room);
        let title_width = painter.width_of(&title, title_pen);
        painter.text(
            &title,
            title_pen,
            colours.text,
            Rect::at(
                room.left + inside,
                room.top,
                title_width + painter.px(2.0),
                size(room).1,
            ),
        );
        let divider = room.left + inside + title_width + painter.px(design::SPACE_5);
        painter.upright(
            divider,
            room.top + painter.px(14.0),
            size(room).1 - painter.px(28.0),
            colours.border,
        );
        let switch_at = painter.px(SWITCH_SIZE.0);
        let right = room.right - inside - switch_at;
        let mut words_right = right - painter.px(design::SPACE_4);
        if let Some((label, target)) = remedy.as_ref() {
            let button_width = painter.button_width(label, None);
            let height = painter.px(34.0);
            painter.button(
                Rect::at(
                    words_right - button_width,
                    centre - height / 2.0,
                    button_width,
                    height,
                ),
                target.clone(),
                Button::new(label, Kind::Quiet),
            );
            words_right -= button_width + painter.px(design::SPACE_4);
        }
        let dot = painter.px(8.0);
        let words_left = divider + painter.px(design::SPACE_5);
        painter.dot(
            (words_left + dot / 2.0, centre),
            dot,
            tone_colour(painter, tone),
            tone == Tone::Good,
        );
        painter.text(
            &said,
            body.ellipsized(),
            colours.text,
            Rect::at(
                words_left + painter.px(14.0),
                room.top,
                (words_right - words_left - painter.px(14.0)).max(0.0),
                size(room).1,
            ),
        );
        painter.switch(
            (right, centre - painter.px(SWITCH_SIZE.1) / 2.0),
            on,
            enabled,
            switch,
        );
        return;
    }

    let width = size(room).0;
    let words_width = width - inside * 2.0 - painter.px(14.0);
    let said_height = painter.height_of(&said, body, words_width);
    let title_height = painter.px(32.0);
    let button = remedy
        .as_ref()
        .map(|_| painter.px(34.0 + design::SPACE_3))
        .unwrap_or(0.0);
    let height = inside
        + title_height
        + painter.px(design::SPACE_4 + 1.0 + design::SPACE_4)
        + said_height
        + button
        + inside;
    let at = Rect::at(
        room.left,
        room.top + (size(room).1 - height) / 2.0,
        width,
        height,
    );
    painter.glass(at, radius, banner);
    let mut y = at.top + inside;
    painter.text(
        &title,
        title_pen,
        colours.text,
        Rect::at(at.left + inside, y, width, title_height),
    );
    painter.switch(
        (
            at.right - inside - painter.px(SWITCH_SIZE.0),
            y + (title_height - painter.px(SWITCH_SIZE.1)) / 2.0,
        ),
        on,
        enabled,
        switch,
    );
    y += title_height + painter.px(design::SPACE_4);
    painter.rule(at.left + inside, y, width - inside * 2.0);
    y += painter.px(1.0 + design::SPACE_4);
    let dot = painter.px(8.0);
    let line = painter.line(body);
    painter.dot(
        (at.left + inside + dot / 2.0, y + line / 2.0),
        dot,
        tone_colour(painter, tone),
        tone == Tone::Good,
    );
    painter.paragraph(
        &said,
        body,
        colours.text,
        (at.left + inside + painter.px(14.0), y, words_width),
    );
    y += said_height;
    if let Some((label, target)) = remedy {
        y += painter.px(design::SPACE_3);
        let button_width = painter.button_width(&label, None);
        painter.button(
            Rect::at(
                at.left + inside + painter.px(14.0),
                y,
                button_width,
                painter.px(34.0),
            ),
            target,
            Button::new(&label, Kind::Quiet),
        );
    }
}

/// Where this computer stands as the block of remote access says it,
/// and what repairs what is in the way, if anything does.
fn access_state(seen: &Seen) -> (String, Tone, Option<(String, Target)>) {
    if let Some(watcher) = seen.watching.first() {
        let name = seen
            .peer(&watcher.fingerprint)
            .map_or_else(|| watcher.address.clone(), |peer| peer.name.clone());
        return (
            say!("home.controlled_by", name = name),
            Tone::Waiting,
            Some((say!("home.take_back"), Target::SendBackAll)),
        );
    }
    let (_, tone) = words::this_computer(seen.standing.as_ref(), seen.ffmpeg_here);
    let Some(said) = seen.standing.as_ref() else {
        return (say!("home.looking_for_the_service"), Tone::Waiting, None);
    };
    if said.unreachable.is_some() {
        return (
            say!("home.service_stopped_long"),
            Tone::Trouble,
            Some((say!("home.start_the_service"), Target::Remedy)),
        );
    }
    if tone == Tone::Trouble {
        return (
            say!("home.ffmpeg_missing_long"),
            Tone::Trouble,
            Some((say!("home.open_its_folder"), Target::Remedy)),
        );
    }
    let words = match tone {
        Tone::Good => say!("home.ready_long"),
        Tone::Off => say!("home.access_off_long"),
        _ => say!("home.starting"),
    };
    (words, tone, None)
}

/* ---- This computer, and the others --------------------------------------- */

/// The other computers the search and the filter keep, under their
/// heading, on as many rows as the page has room for, and gives back the
/// height they take.
fn computers(painter: &mut Painter, seen: &Seen, peers: &[&Peer], at: Rect, rows: usize) -> f32 {
    let heading = painter.line(painter.subtitle()) + painter.px(UNDER_HEADING);
    painter.heading(
        (at.left, at.top),
        &say!("home.my_computers"),
        Some(peers.len()),
    );
    see_all(
        painter,
        Rect::at(
            at.left,
            at.top,
            size(at).0,
            heading - painter.px(UNDER_HEADING),
        ),
        Page::Computers,
    );
    heading
        + computers::lay(
            painter,
            seen,
            peers,
            Rect {
                top: at.top + heading,
                ..at
            },
            Some(rows),
        )
}

/// « See all », at the end of a heading's row, which leads to that page.
fn see_all(painter: &mut Painter, row: Rect, page: Page) {
    let colours = painter.colours;
    let target = Target::Nav(page);
    let ink = colours
        .text_faint
        .mixed_with(colours.accent, 1.0 - painter.hover_of(&target));
    let pen = painter.caption().in_bold();
    let label = say!("home.see_all");
    let arrow = painter.px(16.0);
    let label_width = painter.width_of(&label, pen);
    let (_, centre) = middle(row);
    let at = Rect::at(
        row.right - arrow - painter.px(6.0) - label_width,
        row.top,
        label_width + painter.px(6.0) + arrow,
        size(row).1,
    );
    painter.text(
        &label,
        pen,
        ink,
        Rect {
            right: at.left + label_width + painter.px(2.0),
            ..at
        },
    );
    painter.icon(
        &icons::ARROW_RIGHT,
        Rect::at(at.right - arrow, centre - arrow / 2.0, arrow, arrow),
        ink,
    );
    if painter.focused(&target) {
        painter.focus_ring(at, painter.px(design::RADIUS_SMALL));
    }
    painter.answers(target, at);
}

/// This computer, in the banner: its wallpaper, its name, what it is made
/// of, how its network and ZyrDesk are doing, and the way to everything
/// else about it. Written larger than the rest of the page: it is the
/// one thing the banner shows besides the greeting.
fn this_computer(painter: &mut Painter, seen: &Seen, at: Rect) {
    let colours = painter.colours;
    let portrait = Rect::at(
        at.left,
        at.top,
        painter.px(PORTRAIT.0).min(size(at).0 * 0.4),
        size(at).1,
    );
    crate::home::seen::wallpaper_shown_at(
        painter.app,
        painter.px(PORTRAIT.0).round() as u32,
        painter.px(PORTRAIT.1).round() as u32,
    );
    wallpaper(painter, seen, portrait);

    let left = portrait.right + painter.px(design::SPACE_6);
    let width = (at.right - left).max(0.0);
    let pen = painter.pen(design::SUBTITLE);
    let line = painter.line(pen);
    let icon = painter.px(20.0);

    // What this is, above the name: the banner says the rest of it.
    let eyebrow = painter.body().in_bold();
    let eyebrow_line = painter.line(eyebrow);
    let mut y = at.top;
    painter.icon(
        &icons::LAPTOP,
        Rect::at(left, y + (eyebrow_line - icon) / 2.0, icon, icon),
        colours.text_soft,
    );
    painter.text(
        &say!("home.this_computer"),
        eyebrow.ellipsized(),
        colours.text_soft,
        Rect::at(
            left + icon + painter.px(10.0),
            y,
            width - icon - painter.px(10.0),
            eyebrow_line,
        ),
    );
    y += eyebrow_line + painter.px(design::SPACE_1);

    // The name, the pencil that renames it, and the details beside.
    let name_pen = painter.title().ellipsized();
    let label = say!("home.see_details");
    let button_width = painter.button_width(&label, Some(&icons::CHEVRON));
    let button_height = painter.px(34.0);
    let row_height = painter.line(name_pen).max(button_height);
    let name = seen.this_name();
    let pencil = painter.px(32.0);
    let name_width = painter.width_of(&name, name_pen).min(
        (width - pencil - button_width - painter.px(design::SPACE_2 + design::SPACE_4)).max(0.0),
    );
    painter.text(
        &name,
        name_pen,
        colours.text,
        Rect::at(left, y, name_width + painter.px(2.0), row_height),
    );
    let pencil_at = Rect::at(
        left + name_width + painter.px(design::SPACE_2),
        y + (row_height - pencil) / 2.0,
        pencil,
        pencil,
    );
    painter.button(
        Rect::at(
            pencil_at.right + painter.px(design::SPACE_4),
            y + (row_height - button_height) / 2.0,
            button_width,
            button_height,
        ),
        Target::Details,
        Button::new(&label, Kind::Quiet).leading_on(&icons::CHEVRON),
    );
    if seen.this_device().is_some() {
        painter.icon_button(
            pencil_at,
            &icons::PENCIL,
            Target::RenameThis,
            say!("renaming.rename"),
        );
    } else {
        painter.icon(
            &icons::PENCIL,
            square(middle(pencil_at), icon),
            colours.text_faint,
        );
        painter.tells(pencil_at, say!("home.rename_needs_an_account"));
    }
    y += row_height + painter.px(design::SPACE_3);

    // What it is made of, two by two. The first column takes what its
    // words need, and the second all the rest: a processor's name runs
    // long, and the first column's short.
    let spec_line = painter.px(32.0);
    let specs = specs_of(seen);
    let lead = icon + painter.px(12.0);
    let first = specs
        .iter()
        .step_by(2)
        .map(|(_, said)| lead + painter.width_of(said, pen))
        .fold(0.0_f32, f32::max)
        + painter.px(design::SPACE_6);
    let first = first.min(width / 2.0);
    for (rank, (spec_icon, said)) in specs.iter().enumerate() {
        let (column_left, column_width) = if rank % 2 == 0 {
            (left, first)
        } else {
            (left + first, width - first)
        };
        let row = Rect::at(
            column_left,
            y + spec_line * (rank / 2) as f32,
            column_width,
            spec_line,
        );
        let (_, centre) = middle(row);
        painter.icon(
            spec_icon,
            Rect::at(row.left, centre - icon / 2.0, icon, icon),
            colours.text_soft,
        );
        painter.text(
            said,
            pen.ellipsized(),
            colours.text,
            Rect {
                left: row.left + icon + painter.px(12.0),
                right: row.right - painter.px(design::SPACE_4),
                ..row
            },
        );
    }

    // How its network and ZyrDesk are doing, along the bottom.
    let centre = at.bottom - line / 2.0;
    let (network, network_tone) = words::connectivity(seen.connectivity);
    let (zyrdesk, zyrdesk_tone) = words::zyrdesk(seen);
    let dot = painter.px(10.0);
    let mut x = left;
    let end = at.right;
    for (state_icon, said, tone) in [
        (&icons::WORLD, network, network_tone),
        (&icons::SHIELD, zyrdesk, zyrdesk_tone),
    ] {
        if x + icon >= end {
            break;
        }
        painter.icon(
            state_icon,
            Rect::at(x, centre - icon / 2.0, icon, icon),
            colours.text_soft,
        );
        x += icon + painter.px(10.0);
        painter.dot(
            (x + dot / 2.0, centre),
            dot,
            tone_colour(painter, tone),
            false,
        );
        x += dot + painter.px(8.0);
        let said_width = painter.width_of(&said, pen).min((end - x).max(0.0));
        painter.text(
            &said,
            pen.ellipsized(),
            colours.text,
            Rect::at(x, centre - line / 2.0, said_width + painter.px(2.0), line),
        );
        x += said_width + painter.px(design::SPACE_6);
    }
}

/// This computer's wallpaper in its frame: the picture, the desktop's
/// colour when there is none, or a view of the dunes while it is read.
fn wallpaper(painter: &Painter, seen: &Seen, at: Rect) {
    let radius = painter.px(design::RADIUS_SMALL + 2.0);
    match seen.wallpaper.as_ref() {
        Some(shown) => match shown.picture.as_ref() {
            Some(picture) => painter.picture(picture, at, radius),
            None => {
                let [red, green, blue] = shown.colour;
                painter.fill(
                    at,
                    radius,
                    Colour {
                        red: f32::from(red) / 255.0,
                        green: f32::from(green) / 255.0,
                        blue: f32::from(blue) / 255.0,
                        alpha: 1.0,
                    },
                );
            }
        },
        None => painter.dunes(at, radius, seed_of(&own_fingerprint(seen))),
    }
    painter.stroke(at, radius, painter.colours.text.faded(0.07));
}

fn own_fingerprint(seen: &Seen) -> String {
    seen.standing
        .as_ref()
        .map(|said| said.fingerprint.clone())
        .unwrap_or_default()
}

/// What this computer is made of, line by line, what could not be read
/// left out.
fn specs_of(seen: &Seen) -> Vec<(&'static Icon, String)> {
    let Some(machine) = seen.machine.as_ref() else {
        return Vec::new();
    };
    let mut specs = Vec::new();
    if let Some(windows) = words::windows(machine) {
        specs.push((&icons::WINDOWS, windows));
    }
    if let Some(processor) = machine.processor.as_deref() {
        specs.push((&icons::PROCESSOR, words::processor(processor)));
    }
    if let Some(memory) = words::memory(machine) {
        specs.push((&icons::MEMORY, memory));
    }
    if let Some(card) = machine.main_card() {
        specs.push((&icons::GRAPHICS, card.name.clone()));
    }
    specs
}

/* ---- The bottom of the page ----------------------------------------------- */

/// The contacts, the recent sessions and the quick actions, on one row of
/// the height the page leaves them.
fn bottom(painter: &mut Painter, at: Rect, height: f32) -> f32 {
    let width = size(at).0;
    let gap = painter.px(GAP);
    let heading = painter.line(painter.subtitle()) + painter.px(UNDER_HEADING);
    // On a narrow page only the quick actions stay, in a row: the lists
    // beside them have pages of their own, and the page does not grow for
    // them.
    if width < painter.px(1100.0) {
        return actions(painter, at, heading, true, height);
    }
    let list = (width - painter.px(ACTIONS) - gap * 2.0) / 2.0;
    let contacts_at = Rect::at(at.left, at.top, list, heading + height);
    let sessions_at = Rect::at(at.left + list + gap, at.top, list, heading + height);
    let actions_at = Rect::at(
        sessions_at.right + gap,
        at.top,
        painter.px(ACTIONS),
        heading + height,
    );
    contacts(painter, contacts_at, heading);
    sessions(painter, sessions_at, heading);
    actions(painter, actions_at, heading, false, height);
    heading + height
}

/// The contacts, as many as the card holds, what each is to this account,
/// and the way to invite one more.
fn contacts(painter: &mut Painter, at: Rect, heading: f32) {
    let seen = painter.seen;
    painter.heading((at.left, at.top), &say!("home.contacts"), None);
    see_all(
        painter,
        Rect::at(
            at.left,
            at.top,
            size(at).0,
            heading - painter.px(UNDER_HEADING),
        ),
        Page::Contacts,
    );
    let card = Rect {
        top: at.top + heading,
        ..at
    };
    painter.card(card);
    let inside = painter.px(design::SPACE_5);
    let button = painter.px(38.0);
    let invite = Rect::at(
        card.left + inside,
        card.bottom - inside - button,
        size(card).0 - inside * 2.0,
        button,
    );
    painter.button(
        invite,
        Target::Invite,
        Button::new(&say!("home.invite"), Kind::Quiet).with_icon(&icons::USER_PLUS),
    );
    let room = Rect {
        bottom: invite.top,
        ..card
    };
    let Some(account) = seen
        .account
        .as_ref()
        .filter(|account| account.link.is_some())
    else {
        quiet_word(painter, room, &say!("contacts.no_account"));
        return;
    };
    if account.contacts.is_empty() {
        quiet_word(painter, room, &say!("contacts.none"));
        return;
    }
    let top = painter.px(design::SPACE_2);
    let row = painter.px(56.0);
    let fit = ((size(room).1 - top) / row).floor().max(0.0) as usize;
    for (rank, contact) in account.contacts.iter().take(fit).enumerate() {
        let line = Rect::at(
            card.left,
            card.top + top + row * rank as f32,
            size(card).0,
            row,
        );
        if rank > 0 {
            painter.rule(line.left + inside, line.top, size(line).0 - inside * 2.0);
        }
        let lent = account
            .lent
            .iter()
            .filter(|lent| lent.with == contact.username)
            .count();
        contact_line(
            painter,
            line,
            &contact.username,
            &contacts::standing_of(contact, lent),
        );
    }
}

/// One contact across `at`: their initials, their name, and what they
/// are to this account. The whole line leads to the page of contacts.
fn contact_line(painter: &mut Painter, at: Rect, name: &str, standing: &str) {
    let colours = painter.colours;
    let inside = painter.px(design::SPACE_5);
    let (_, centre) = middle(at);
    let disc = painter.px(34.0);
    painter.avatar(
        Rect::at(at.left + inside, centre - disc / 2.0, disc, disc),
        &words::initials(name),
    );
    let left = at.left + inside + disc + painter.px(design::SPACE_3);
    let width = (at.right - inside - left).max(0.0);
    let name_pen = painter.body().in_bold().ellipsized();
    let caption = painter.caption().ellipsized();
    let (name_height, caption_height) = (painter.line(name_pen), painter.line(caption));
    let top = centre - (name_height + caption_height) / 2.0;
    painter.text(
        name,
        name_pen,
        colours.text,
        Rect::at(left, top, width, name_height),
    );
    painter.text(
        standing,
        caption,
        colours.text_soft,
        Rect::at(left, top + name_height, width, caption_height),
    );
    painter.answers(Target::Nav(Page::Contacts), at);
}

/// A few words in the middle of a card that has nothing to list.
fn quiet_word(painter: &Painter, at: Rect, said: &str) {
    let pen = painter.body().aligned(Align::Centre);
    let width = size(at).0 - painter.px(design::SPACE_5) * 2.0;
    let height = painter.height_of(said, pen, width);
    painter.paragraph(
        said,
        pen,
        painter.colours.text_faint,
        (
            at.left + painter.px(design::SPACE_5),
            middle(at).1 - height / 2.0,
            width,
        ),
    );
}

/// The sessions that took place lately, as many as the card holds, and
/// the way to all of them.
fn sessions(painter: &mut Painter, at: Rect, heading: f32) {
    let seen = painter.seen;
    painter.heading((at.left, at.top), &say!("home.recent_sessions"), None);
    see_all(
        painter,
        Rect::at(
            at.left,
            at.top,
            size(at).0,
            heading - painter.px(UNDER_HEADING),
        ),
        Page::Activity,
    );
    let card = Rect {
        top: at.top + heading,
        ..at
    };
    painter.card(card);
    if seen.history.is_empty() {
        quiet_word(painter, card, &say!("activity.none"));
        return;
    }
    let inside = painter.px(design::SPACE_2);
    let line = painter.px(activity::LINE);
    let fit = ((size(card).1 - inside * 2.0) / line).floor().max(1.0) as usize;
    for (rank, past) in seen.history.iter().take(fit).enumerate() {
        let at = Rect::at(
            card.left,
            card.top + inside + line * rank as f32,
            size(card).0,
            line,
        );
        if rank > 0 {
            let edge = painter.px(design::SPACE_5);
            painter.rule(at.left + edge, at.top, size(at).0 - edge * 2.0);
        }
        activity::past_line(painter, seen, past, at, true);
    }
}

/// The quick actions, two by two, or four in a row when they have the
/// page's width. Gives back the height they take.
fn actions(painter: &mut Painter, at: Rect, heading: f32, in_a_row: bool, height: f32) -> f32 {
    painter.heading((at.left, at.top), &say!("home.quick_actions"), None);
    let gap = painter.px(GAP);
    let width = size(at).0;
    let columns = if in_a_row { 4 } else { 2 };
    let tile_width = (width - gap * (columns as f32 - 1.0)) / columns as f32;
    let tile_height = if in_a_row {
        ((painter.px(BOTTOM) - gap) / 2.0).min(height)
    } else {
        (height - gap) / 2.0
    };
    let tiles: [(&Icon, String, &Icon, Target); 4] = [
        (
            &icons::COMPUTERS,
            say!("adding.title"),
            &icons::PLUS,
            Target::AddComputer,
        ),
        (
            &icons::USER_PLUS,
            say!("home.invite"),
            &icons::PLUS,
            Target::Invite,
        ),
        (
            &icons::REPORT,
            say!("home.action_activity"),
            &icons::CHEVRON,
            Target::Nav(Page::Activity),
        ),
        (
            &icons::SETTINGS,
            say!("home.action_settings"),
            &icons::CHEVRON,
            Target::Nav(Page::Settings),
        ),
    ];
    for (rank, (icon, label, go, target)) in tiles.into_iter().enumerate() {
        let tile = Rect::at(
            at.left + (rank % columns) as f32 * (tile_width + gap),
            at.top + heading + (rank / columns) as f32 * (tile_height + gap),
            tile_width,
            tile_height,
        );
        quick_action(painter, tile, icon, &label, go, target);
    }
    let rows = 4usize.div_ceil(columns) as f32;
    heading + rows * tile_height + (rows - 1.0) * gap
}

fn quick_action(
    painter: &mut Painter,
    at: Rect,
    icon: &Icon,
    label: &str,
    go: &Icon,
    target: Target,
) {
    let colours = painter.colours;
    let hover = painter.hover_of(&target);
    painter.card_lit(at, hover);
    let inside = painter.px(18.0);
    let ink = colours.text;
    let big = painter.px(26.0);
    let round = painter.px(32.0);
    let pen = painter.body().in_bold();
    let apart = painter.px(design::SPACE_3);
    let words_width = size(at).0 - inside * 2.0;
    let words_height = painter.height_of(label, pen, words_width);
    // The icon above the words when the tile is tall enough for both,
    // beside them when the page leaves it short.
    let stacked = size(at).1 >= inside * 2.0 + big + apart + words_height;
    let (_, centre) = middle(at);
    let button = if stacked {
        painter.icon(
            icon,
            Rect::at(at.left + inside, at.top + inside, big, big),
            ink,
        );
        painter.paragraph(
            label,
            pen,
            ink,
            (
                at.left + inside,
                at.bottom - inside - words_height,
                words_width,
            ),
        );
        Rect::at(
            at.right - inside - round,
            at.top + painter.px(16.0),
            round,
            round,
        )
    } else {
        painter.icon(
            icon,
            Rect::at(at.left + inside, centre - big / 2.0, big, big),
            ink,
        );
        let words_left = at.left + inside + big + apart;
        let words_width = at.right - inside - round - apart - words_left;
        let words_height = painter.height_of(label, pen, words_width);
        painter.paragraph(
            label,
            pen,
            ink,
            (words_left, centre - words_height / 2.0, words_width),
        );
        Rect::at(
            at.right - inside - round,
            centre - round / 2.0,
            round,
            round,
        )
    };
    painter.fill(
        button,
        round / 2.0,
        colours.surface_2.mixed_with(colours.surface_3, 1.0 - hover),
    );
    painter.stroke(button, round / 2.0, colours.border);
    let small = painter.px(16.0);
    painter.icon(go, square(middle(button), small), colours.text_soft);
    if painter.focused(&target) {
        painter.focus_ring(at, painter.px(design::RADIUS));
    }
    painter.answers(target, at);
}

/* ---- What the page asks of the switches ----------------------------------- */

/// Where a switch stands: where a hand has just put it, until the service
/// says it has done it, and what the service says otherwise.
pub fn toggle_is_on(seen: &Seen, state: &State, toggle: Toggle) -> bool {
    if let Some((_, wanted)) = state.pushed.iter().find(|(pushed, _)| *pushed == toggle) {
        return *wanted;
    }
    let standing = seen.standing.as_ref();
    let settings = seen.settings.as_ref();
    match toggle {
        Toggle::Access => standing.is_some_and(|said| said.wanted),
        Toggle::Trust => standing.is_some_and(|said| said.trusting),
        Toggle::AtBoot => standing.is_some_and(|said| said.at_boot),
        Toggle::Marking => standing.is_some_and(|said| said.ecn),
        Toggle::FixedPort => standing.is_some_and(|said| said.fixed_port),
        Toggle::FarSpeakers => settings.is_some_and(|said| said.mute_far_speakers),
        Toggle::Statistics => settings.is_some_and(|said| said.stats_overlay),
    }
}

/// Whether a switch can be pushed: not while the service has not yet
/// answered the last push, and not at all while the service is silent,
/// which is a failure and not a choice, so the switch stays where it was
/// rather than suggesting a decision nobody made.
pub fn toggle_enabled(seen: &Seen, state: &State, toggle: Toggle) -> bool {
    if state.pushed.iter().any(|(pushed, _)| *pushed == toggle) {
        return false;
    }
    match toggle {
        Toggle::FarSpeakers | Toggle::Statistics => seen.settings.is_some(),
        _ => seen
            .standing
            .as_ref()
            .is_some_and(|said| said.unreachable.is_none()),
    }
}
