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
use zyr_session::presence::{Presence, Road};

use crate::home::seen::Seen;
use crate::home::state::{Offer, Page, State, Target, Toggle};
use crate::home::ui::{Button, Kind, Painter, QUICK, SWITCH_SIZE, middle, size, square};
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

/// A card's height, the gap between cards, and the narrowest and widest a
/// computer's card is.
const CARD: f32 = 262.0;
const GAP: f32 = 16.0;
const NARROWEST: f32 = 220.0;
const WIDEST: f32 = 300.0;

/// A computer's picture on its card.
const PICTURE: f32 = 104.0;

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
    let gap = painter.px(GAP);
    let heading = painter.line(painter.subtitle()) + painter.px(UNDER_HEADING);
    let floor = at.top + room;

    let mut y = at.top;
    y += banner(painter, seen, Rect { top: y, ..at });
    y += between;

    let card_row = painter.px(CARD) + gap;
    let rows_in = |height: f32| ((height - heading + gap) / card_row).floor();
    let with_bottom_rows = rows_in(floor - y - between - heading - painter.px(BOTTOM_LEAST));
    let with_bottom = with_bottom_rows >= 1.0;
    let fit = if with_bottom {
        with_bottom_rows
    } else {
        rows_in(floor - y)
    };
    let (columns, _) = grid(painter, size(at).0, seen.peers.len());
    let needed = seen.peers.len().div_ceil(columns).max(1);
    let rows = (fit.max(1.0) as usize).min(needed);
    y += computers(painter, seen, Rect { top: y, ..at }, rows);

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
/// within `room`, centred in its height, or as a strip across it.
fn access(painter: &mut Painter, seen: &Seen, room: Rect, strip: bool) {
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
        painter.shadow(room, radius, colours.shadow_2);
        painter.fill(room, radius, colours.surface_1.faded(0.95));
        painter.stroke(room, radius, colours.border);
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
    painter.shadow(at, radius, colours.shadow_2);
    painter.fill(at, radius, colours.surface_1.faded(0.95));
    painter.stroke(at, radius, colours.border);
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

/// The other computers, under their heading, on as many rows as the page
/// has room for, and gives back the height they take.
fn computers(painter: &mut Painter, seen: &Seen, at: Rect, rows: usize) -> f32 {
    let heading = painter.line(painter.subtitle()) + painter.px(UNDER_HEADING);
    painter.heading(
        (at.left, at.top),
        &say!("home.my_computers"),
        Some(seen.peers.len()),
    );
    see_all(
        painter,
        Rect::at(
            at.left,
            at.top,
            size(at).0,
            heading - painter.px(UNDER_HEADING),
        ),
    );
    heading
        + their_cards(
            painter,
            seen,
            Rect {
                top: at.top + heading,
                ..at
            },
            rows,
        )
}

/// « See all », which leads to the page of computers once there is one.
fn see_all(painter: &mut Painter, row: Rect) {
    let colours = painter.colours;
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
        colours.text_faint,
        Rect {
            right: at.left + label_width + painter.px(2.0),
            ..at
        },
    );
    painter.icon(
        &icons::ARROW_RIGHT,
        Rect::at(at.right - arrow, centre - arrow / 2.0, arrow, arrow),
        colours.text_faint,
    );
    painter.tells(at, say!("soon.tip"));
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

    // What it is made of, two by two.
    let spec_line = painter.px(32.0);
    let column = width / 2.0;
    for (rank, (spec_icon, said)) in specs_of(seen).iter().enumerate() {
        let row = Rect::at(
            left + column * (rank % 2) as f32,
            y + spec_line * (rank / 2) as f32,
            column,
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
/// colour when there is none, or ZyrDesk's own picture while it is read.
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
        None => painter.art(at, radius, seed_of(&own_fingerprint(seen))),
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

/// The number a computer's picture is drawn from, taken from its
/// fingerprint so that it keeps its mountains from one day to the next.
pub fn seed_of(fingerprint: &str) -> u32 {
    u32::from_str_radix(fingerprint.get(..8).unwrap_or_default(), 16).unwrap_or(0)
}

/// How many cards go across a row of that width, for that many
/// computers, and how wide each is.
fn grid(painter: &Painter, width: f32, count: usize) -> (usize, f32) {
    let gap = painter.px(GAP);
    let fit = ((width + gap) / (painter.px(NARROWEST) + gap))
        .floor()
        .max(1.0) as usize;
    let columns = fit.min(count.max(1));
    let column = ((width - gap * (columns as f32 - 1.0)) / columns as f32).min(painter.px(WIDEST));
    (columns, column)
}

/// The other computers' cards, in a grid of at most that many rows, and
/// gives back its height. A card when there is none, which says how they
/// come; and when they do not all fit, the last place says how many more
/// there are, the first ones being those that can be reached.
fn their_cards(painter: &mut Painter, seen: &Seen, at: Rect, rows: usize) -> f32 {
    let width = size(at).0;
    let gap = painter.px(GAP);
    if seen.peers.is_empty() {
        return no_computer(painter, Rect::at(at.left, at.top, width, painter.px(CARD)));
    }
    let (columns, column) = grid(painter, width, seen.peers.len());
    let places = columns * rows.max(1);
    let all = seen.peers.len() <= places;
    let shown = if all { seen.peers.len() } else { places - 1 };
    let height = painter.px(CARD);
    let place = |rank: usize| {
        Rect::at(
            at.left + (rank % columns) as f32 * (column + gap),
            at.top + (rank / columns) as f32 * (height + gap),
            column,
            height,
        )
    };
    for (rank, peer) in seen.peers.iter().take(shown).enumerate() {
        computer_card(painter, seen, peer, place(rank));
    }
    if !all {
        more_computers(painter, place(shown), seen.peers.len() - shown);
    }
    let laid = if all { shown } else { places };
    let rows = laid.div_ceil(columns) as f32;
    rows * height + (rows - 1.0) * gap
}

/// The place of the computers that did not fit: how many there are, until
/// the page of computers shows them all.
fn more_computers(painter: &mut Painter, at: Rect, count: usize) {
    let colours = painter.colours;
    let radius = painter.px(design::RADIUS);
    painter.dashed(at, radius, colours.border_strong);
    let (_, centre) = middle(at);
    let title = painter.title().aligned(Align::Centre);
    let caption = painter.body().aligned(Align::Centre);
    let (title_line, caption_line) = (painter.line(title), painter.line(caption));
    let top = centre - (title_line + caption_line) / 2.0;
    painter.text(
        &format!("+{count}"),
        title,
        colours.text,
        Rect::at(at.left, top, size(at).0, title_line),
    );
    painter.text(
        &say!("home.more_computers"),
        caption,
        colours.text_soft,
        Rect::at(at.left, top + title_line, size(at).0, caption_line),
    );
    painter.tells(at, say!("soon.tip"));
}

/// One computer's card: its picture, its presence, its name, the road to
/// it and how long it takes, and the way to it.
fn computer_card(painter: &mut Painter, seen: &Seen, peer: &Peer, at: Rect) {
    let colours = painter.colours;
    let fingerprint = peer.fingerprint.to_string();
    let presence = zyr_session::presence::presence(peer);
    let busy = seen.busy(painter.state.opening.is_some());
    let reachable = presence.can_be_reached() && !busy;
    let connect = Target::Connect(fingerprint.clone());
    let menu = Target::CardMenu(fingerprint.clone());
    let menu_open = painter
        .state
        .menu
        .as_ref()
        .is_some_and(|(open, _)| *open == fingerprint);
    let lit = if reachable {
        painter.hover_of(&connect)
    } else {
        0.0
    };
    let near = painter
        .hover_of(&menu)
        .max(lit)
        .max(f32::from(u8::from(menu_open)));
    painter.card_lit(at, lit);
    if reachable {
        painter.answers(connect.clone(), at);
    }

    let inside = painter.px(14.0);
    let picture = Rect::at(
        at.left + inside,
        at.top + inside,
        size(at).0 - inside * 2.0,
        painter.px(PICTURE),
    );
    let radius = painter.px(design::RADIUS_SMALL + 1.0);
    painter.art(picture, radius, seed_of(&fingerprint));
    painter.stroke(picture, radius, colours.text.faded(0.07));
    let tip = say!("computer.wallpaper_soon");
    if reachable {
        painter.answers_with_tip(connect.clone(), picture, tip);
    } else {
        painter.tells(picture, tip);
    }

    // On the picture: the presence, and in its corner the handle that
    // will move the card, and the computer's menu.
    let (dot_colour, live) = match presence {
        Presence::Ready => (colours.online, true),
        Presence::Closed { .. } => (colours.warning, false),
        Presence::Gone | Presence::Unknown => (colours.offline, false),
    };
    let disc = painter.px(18.0);
    let disc_at = Rect::at(
        picture.left + painter.px(8.0),
        picture.top + painter.px(8.0),
        disc,
        disc,
    );
    painter.fill(disc_at, disc / 2.0, colours.surface_1);
    painter.dot(middle(disc_at), painter.px(8.0), dot_colour, live);
    let corner = painter.px(28.0);
    let handle = Rect::at(
        picture.right - painter.px(6.0) - corner,
        picture.top + painter.px(6.0),
        corner,
        corner,
    );
    let small = painter.px(16.0);
    painter.fill(handle, painter.px(7.0), colours.surface_1.faded(0.92));
    painter.icon(
        &icons::MOVE,
        square(middle(handle), small),
        colours.text_faint,
    );
    painter.tells(handle, say!("computer.move_soon"));
    let more = handle.shifted(-(corner + painter.px(6.0)), 0.0);
    if near > 0.0 || painter.focused(&menu) {
        painter.fill(
            more,
            painter.px(7.0),
            colours.surface_1.faded(0.92 * near.max(0.6)),
        );
        painter.icon(
            &icons::MORE,
            square(middle(more), small),
            colours.text_soft.faded(near.max(0.6)),
        );
    }
    painter.answers_with_tip(menu, more, say!("computer.more"));
    if menu_open {
        painter.state.menu = Some((fingerprint.clone(), more));
    }

    // Under the picture: the name, what is known of it, its tags.
    let left = at.left + inside;
    let width = size(at).0 - inside * 2.0;
    let mut y = picture.bottom + painter.px(design::SPACE_3);
    let name_pen = painter.body().in_bold().ellipsized();
    let name_height = painter.line(name_pen);
    painter.text(
        &peer.name,
        name_pen,
        colours.text,
        Rect::at(left, y, width, name_height),
    );
    y += name_height + painter.px(2.0);
    let caption = painter.caption();
    let caption_height = painter.line(caption);
    painter.text(
        &words::below_the_name(peer, seen.device_of(peer)),
        caption.ellipsized(),
        colours.text_soft,
        Rect::at(left, y, width, caption_height),
    );
    y += caption_height + painter.px(design::SPACE_2);
    tags(painter, seen, peer, &presence, (left, y, left + width));

    // And at the bottom, the way to it.
    let button = Rect::at(
        left,
        at.bottom - inside - painter.px(38.0),
        width,
        painter.px(38.0),
    );
    match presence {
        Presence::Gone => {
            painter.fill(button, painter.px(design::RADIUS_SMALL), colours.surface_1);
            painter.stroke(button, painter.px(design::RADIUS_SMALL), colours.border);
            let label = say!("computer.wake");
            let pen = painter.body().in_bold();
            let label_width = painter.width_of(&label, pen);
            let icon = painter.px(18.0);
            let (centre_x, centre_y) = middle(button);
            let start = centre_x - (label_width + icon + painter.px(design::SPACE_2)) / 2.0;
            painter.icon(
                &icons::POWER,
                Rect::at(start, centre_y - icon / 2.0, icon, icon),
                colours.text_faint,
            );
            painter.text(
                &label,
                pen,
                colours.text_faint,
                Rect::at(
                    start + icon + painter.px(design::SPACE_2),
                    button.top,
                    label_width + painter.px(2.0),
                    size(button).1,
                ),
            );
            painter.tells(button, say!("soon.tip"));
        }
        _ => {
            let label = say!("computer.connect");
            painter.button(
                button,
                connect,
                Button::new(&label, Kind::Action)
                    .with_icon(&icons::COMPUTERS)
                    .enabled(reachable)
                    .lit(lit),
            );
        }
    }
}

/// A card's tags: the road to the computer, how long it takes, and what
/// is going on with it right now, as many as fit before `right`.
///
/// During a session, the road and the delay are the session's own, as
/// the service measures them: through the relay or straight there, and
/// the bubble over the road says where the packets go.
fn tags(
    painter: &mut Painter,
    seen: &Seen,
    peer: &Peer,
    presence: &Presence,
    (left, top, right): (f32, f32, f32),
) {
    let colours = painter.colours;
    let mut row = Row {
        x: left,
        top,
        right,
    };
    let fingerprint = peer.fingerprint.to_string();
    let session = seen
        .sessions
        .iter()
        .find(|session| session.fingerprint == fingerprint);
    let (road, icon, tint) = match (session, zyr_session::presence::road(peer)) {
        (Some(session), _) if session.relayed => {
            (say!("computer.road_relay"), &icons::SERVER, colours.warning)
        }
        (_, Road::Local) => (
            say!("computer.road_local"),
            &icons::LOCAL_NETWORK,
            colours.road_local,
        ),
        (_, Road::Internet) => (
            say!("computer.road_internet"),
            &icons::WORLD,
            colours.road_internet,
        ),
        (_, Road::Written) => (
            say!("computer.road_written"),
            &icons::PENCIL,
            colours.text_soft,
        ),
    };
    let laid = row.lay(painter, &road, Some(icon), tint);
    if let Some(session) = session {
        if let Some(laid) = laid {
            let via = if session.relayed {
                say!("computer.via_relay", via = session.via)
            } else {
                say!("computer.via_direct", via = session.via)
            };
            painter.tells(laid, via);
        }
        let during = say!(
            "computer.in_session",
            duration = words::duration(session.since)
        );
        row.lay(painter, &during, None, colours.online);
        row.latency(painter, session.round_trip_ms);
        return;
    }
    if seen
        .watching
        .iter()
        .any(|watcher| watcher.fingerprint == fingerprint)
    {
        row.lay(
            painter,
            &say!("computer.controls_this_one"),
            None,
            colours.warning,
        );
        return;
    }
    // Only when it was measured and still holds: a computer that is gone
    // has no delay, and a number left from before would say it is there.
    if let Some(round_trip) = peer
        .round_trip_ms
        .filter(|_| !matches!(presence, Presence::Gone))
    {
        row.latency(painter, u64::from(round_trip));
    }
}

/// A row of tags, laid from the left as long as they fit: once one does
/// not, nothing after it shows, so that a narrow card keeps what matters
/// first rather than spilling over its edge.
struct Row {
    x: f32,
    top: f32,
    right: f32,
}

impl Row {
    fn lay(
        &mut self,
        painter: &Painter,
        label: &str,
        icon: Option<&Icon>,
        tint: Colour,
    ) -> Option<Rect> {
        if self.x + painter.tag_width(label, icon) > self.right {
            self.x = self.right;
            return None;
        }
        let laid = painter.tag((self.x, self.top), label, icon, tint);
        self.x = laid.right + painter.px(design::SPACE_2);
        Some(laid)
    }

    /// How long a question takes to come back from the computer.
    fn latency(&mut self, painter: &Painter, round_trip: u64) {
        self.lay(
            painter,
            &say!("computer.round_trip", ms = round_trip.max(1)),
            Some(&icons::ROUND_TRIP),
            painter.colours.online,
        );
    }
}

/// The card shown while no computer is known, which says how they come.
fn no_computer(painter: &mut Painter, at: Rect) -> f32 {
    let colours = painter.colours;
    let radius = painter.px(design::RADIUS);
    painter.dashed(at, radius, colours.border_strong);
    let inside = painter.px(design::SPACE_6);
    let width = (size(at).0 - inside * 2.0).min(painter.px(440.0));
    let title = say!("home.no_computer");
    let text = say!("home.no_computer_text");
    let button_label = say!("adding.title");
    let title_pen = painter.subtitle().aligned(Align::Centre);
    let pen = painter.caption().aligned(Align::Centre);
    let icon = painter.px(36.0);
    let text_height = painter.height_of(&text, pen, width);
    let content = icon
        + painter.px(design::SPACE_3)
        + painter.line(title_pen)
        + painter.px(design::SPACE_2)
        + text_height
        + painter.px(design::SPACE_4)
        + painter.px(38.0);
    let height = (content + inside * 2.0).max(size(at).1);
    let at = Rect {
        bottom: at.top + height,
        ..at
    };
    let (centre, _) = middle(at);
    let mut y = at.top + (height - content) / 2.0;
    painter.icon(
        &icons::COMPUTERS,
        Rect::at(centre - icon / 2.0, y, icon, icon),
        colours.text_faint,
    );
    y += icon + painter.px(design::SPACE_3);
    let title_height = painter.line(title_pen);
    painter.text(
        &title,
        title_pen,
        colours.text,
        Rect::at(at.left, y, size(at).0, title_height),
    );
    y += title_height + painter.px(design::SPACE_2);
    painter.paragraph(
        &text,
        pen,
        colours.text_soft,
        (centre - width / 2.0, y, width),
    );
    y += text_height + painter.px(design::SPACE_4);
    let button_width = painter.button_width(&button_label, Some(&icons::PLUS));
    painter.button(
        Rect::at(
            centre - button_width / 2.0,
            y,
            button_width,
            painter.px(38.0),
        ),
        Target::AddComputer,
        Button::new(&button_label, Kind::Primary).with_icon(&icons::PLUS),
    );
    height
}

/* ---- The bottom of the page ----------------------------------------------- */

/// The contacts, the recent sessions and the quick actions, on one row of
/// the height the page leaves them.
fn bottom(painter: &mut Painter, at: Rect, height: f32) -> f32 {
    let width = size(at).0;
    let gap = painter.px(GAP);
    let heading = painter.line(painter.subtitle()) + painter.px(UNDER_HEADING);
    // On a narrow page only the quick actions stay, in a row: the lists
    // beside them are still to come, and the page does not grow for them.
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

/// The contacts and what they may do, which is drawn and does not work
/// yet: sharing between accounts is still to come.
fn contacts(painter: &mut Painter, at: Rect, heading: f32) {
    let colours = painter.colours;
    painter.heading((at.left, at.top), &say!("home.contacts"), None);
    let card = Rect {
        top: at.top + heading,
        ..at
    };
    painter.card(card);
    let inside = painter.px(design::SPACE_5);
    let row = painter.px(52.0);
    let invite_top = card.bottom - inside - painter.px(38.0);
    for rank in 0..3 {
        let top = card.top + inside + row * rank as f32;
        if top + row > invite_top {
            break;
        }
        let disc = painter.px(36.0);
        painter.fill(
            Rect::at(card.left + inside, top, disc, disc),
            disc / 2.0,
            colours.surface_3,
        );
        let words = card.left + inside + disc + painter.px(design::SPACE_3);
        painter.stand_in(Rect::at(
            words,
            top + painter.px(6.0),
            painter.px(120.0),
            painter.px(10.0),
        ));
        painter.stand_in(Rect::at(
            words,
            top + painter.px(22.0),
            painter.px(88.0),
            painter.px(8.0),
        ));
        painter.fill(
            Rect::at(
                card.right - inside - painter.px(56.0),
                top + painter.px(8.0),
                painter.px(56.0),
                painter.px(20.0),
            ),
            painter.px(6.0),
            colours.surface_3,
        );
    }
    let invite = Rect::at(
        card.left + inside,
        card.bottom - inside - painter.px(38.0),
        size(card).0 - inside * 2.0,
        painter.px(38.0),
    );
    let radius = painter.px(design::RADIUS_SMALL);
    painter.fill(invite, radius, colours.accent_tint.faded(0.6));
    painter.stroke(invite, radius, colours.border);
    soon_label(painter, invite, &icons::USER_PLUS, &say!("home.invite"));
    painter.tells(card, say!("home.contacts_soon"));
}

/// The sessions that took place, which will come once they are kept.
fn sessions(painter: &mut Painter, at: Rect, heading: f32) {
    let colours = painter.colours;
    painter.heading((at.left, at.top), &say!("home.recent_sessions"), None);
    let card = Rect {
        top: at.top + heading,
        ..at
    };
    painter.card(card);
    let inside = painter.px(design::SPACE_5);
    let row = painter.px(52.0);
    let icon = painter.px(20.0);
    for rank in 0..4 {
        let top = card.top + painter.px(design::SPACE_4) + row * rank as f32;
        if top + row > card.bottom - painter.px(design::SPACE_2) {
            break;
        }
        painter.icon(
            &icons::COMPUTERS,
            Rect::at(card.left + inside, top + painter.px(8.0), icon, icon),
            colours.border_strong,
        );
        let words = card.left + inside + icon + painter.px(design::SPACE_4);
        painter.stand_in(Rect::at(
            words,
            top + painter.px(6.0),
            painter.px(110.0),
            painter.px(10.0),
        ));
        painter.stand_in(Rect::at(
            words,
            top + painter.px(22.0),
            painter.px(150.0),
            painter.px(8.0),
        ));
        painter.icon(
            &icons::MORE,
            Rect::at(
                card.right - inside - icon,
                top + painter.px(8.0),
                icon,
                icon,
            ),
            colours.border_strong,
        );
        if rank < 3 && top + row * 2.0 <= card.bottom - painter.px(design::SPACE_2) {
            painter.rule(
                card.left + inside,
                top + painter.px(46.0),
                size(card).0 - inside * 2.0,
            );
        }
    }
    painter.tells(card, say!("home.sessions_soon"));
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
    let tiles: [(&Icon, String, &Icon, Option<Target>); 4] = [
        (
            &icons::COMPUTERS,
            say!("adding.title"),
            &icons::PLUS,
            Some(Target::AddComputer),
        ),
        (&icons::USER_PLUS, say!("home.invite"), &icons::PLUS, None),
        (
            &icons::REPORT,
            say!("home.action_activity"),
            &icons::CHEVRON,
            None,
        ),
        (
            &icons::SETTINGS,
            say!("home.action_settings"),
            &icons::CHEVRON,
            Some(Target::Nav(Page::Settings)),
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
    target: Option<Target>,
) {
    let colours = painter.colours;
    let live = target.is_some();
    let hover = target
        .as_ref()
        .map_or(0.0, |target| painter.hover_of(target));
    painter.card_lit(at, hover);
    let inside = painter.px(18.0);
    let ink = if live {
        colours.text
    } else {
        colours.text_faint
    };
    let big = painter.px(26.0);
    painter.icon(
        icon,
        Rect::at(at.left + inside, at.top + inside, big, big),
        ink,
    );
    let pen = painter.body().in_bold();
    let words_width = size(at).0 - inside * 2.0;
    let words_height = painter.height_of(label, pen, words_width);
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
    let round = painter.px(32.0);
    let button = Rect::at(
        at.right - inside - round,
        at.top + painter.px(16.0),
        round,
        round,
    );
    painter.fill(
        button,
        round / 2.0,
        colours.surface_2.mixed_with(colours.surface_3, 1.0 - hover),
    );
    painter.stroke(button, round / 2.0, colours.border);
    let small = painter.px(16.0);
    painter.icon(
        go,
        square(middle(button), small),
        if live {
            colours.text_soft
        } else {
            colours.text_faint
        },
    );
    match target {
        Some(target) => {
            if painter.focused(&target) {
                painter.focus_ring(at, painter.px(design::RADIUS));
            }
            painter.answers(target, at);
        }
        None => painter.tells(at, say!("soon.tip")),
    }
}

/// A word and its icon, faded, centred in a frame that does not work yet.
fn soon_label(painter: &Painter, at: Rect, icon: &Icon, label: &str) {
    let colours = painter.colours;
    let pen = painter.body().in_bold();
    let words = painter.width_of(label, pen);
    let side = painter.px(18.0);
    let (centre_x, centre_y) = middle(at);
    let start = centre_x - (words + side + painter.px(design::SPACE_2)) / 2.0;
    painter.icon(
        icon,
        Rect::at(start, centre_y - side / 2.0, side, side),
        colours.text_faint,
    );
    painter.text(
        label,
        pen,
        colours.text_faint,
        Rect::at(
            start + side + painter.px(design::SPACE_2),
            at.top,
            words + painter.px(2.0),
            size(at).1,
        ),
    );
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

/* ---- A computer's menu ------------------------------------------------------ */

/// What a computer's menu offers, in the order it shows them.
pub fn offers(seen: &Seen, peer: &Peer, opening: bool) -> Vec<Offer> {
    let fingerprint = peer.fingerprint.to_string();
    let mut offers = Vec::new();
    if peer.seen && !seen.busy(opening) {
        offers.push(Offer::HereOnly);
    }
    offers.push(Offer::Journal);
    if seen
        .watching
        .iter()
        .any(|watcher| watcher.fingerprint == fingerprint)
    {
        offers.push(Offer::SendBack);
    }
    if peer.written {
        offers.push(Offer::Forget);
    }
    offers
}

/// The menu of the computer whose button was clicked, under that button.
pub fn menu(painter: &mut Painter, width: f32, height: f32) {
    let Some((fingerprint, anchor)) = painter.state.menu.clone() else {
        return;
    };
    let seen = painter.seen;
    let Some(peer) = seen.peer(&fingerprint) else {
        painter.state.menu = None;
        return;
    };
    let colours = painter.colours;
    let offers = offers(seen, peer, painter.state.opening.is_some());
    let shown = painter.arriving(&format!("menu {fingerprint}"), QUICK);
    let row = painter.px(38.0);
    let inside = painter.px(design::SPACE_1 + 2.0);
    let pen = painter.body();
    let icon = painter.px(18.0);
    let labels: Vec<(Offer, String, &Icon)> = offers
        .into_iter()
        .map(|offer| match offer {
            Offer::HereOnly => (offer, say!("computer.here_only"), &icons::LOCAL_NETWORK),
            Offer::Journal => (offer, say!("computer.journal"), &icons::JOURNAL),
            Offer::SendBack => (offer, say!("computer.send_back"), &icons::DISCONNECT),
            Offer::Forget => (offer, say!("computer.forget"), &icons::CROSS),
        })
        .collect();
    let wide = labels
        .iter()
        .map(|(_, label, _)| painter.width_of(label, pen))
        .fold(0.0, f32::max)
        + icon
        + painter.px(design::SPACE_3 * 2.0 + design::SPACE_4);
    let tall = row * labels.len() as f32 + inside * 2.0;
    let left = (anchor.right - wide).clamp(
        painter.px(design::SPACE_2),
        width - wide - painter.px(design::SPACE_2),
    );
    let below = anchor.bottom + painter.px(design::SPACE_1);
    let top = if below + tall <= height - painter.px(design::SPACE_2) {
        below
    } else {
        anchor.top - painter.px(design::SPACE_1) - tall
    };
    let at = Rect::at(left, top - painter.px(6.0) * (1.0 - shown), wide, tall);
    let radius = painter.px(design::RADIUS);
    painter.shadow(at, radius, colours.shadow_2);
    painter.fill(at, radius, colours.surface_1);
    painter.stroke(at, radius, colours.border_strong);
    painter.answers(Target::CardMenu(fingerprint.clone()), at);
    for (rank, (offer, label, glyph)) in labels.iter().enumerate() {
        let line = Rect::at(
            at.left + inside,
            at.top + inside + row * rank as f32,
            wide - inside * 2.0,
            row,
        );
        let target = Target::Offer(fingerprint.clone(), *offer);
        let hover = painter.hover_of(&target);
        if hover > 0.0 {
            painter.fill(
                line,
                painter.px(design::RADIUS_SMALL),
                colours.surface_3.faded(hover),
            );
        }
        let (_, centre) = middle(line);
        let ink = if *offer == Offer::Forget {
            colours.warning
        } else {
            colours.text
        };
        painter.icon(
            glyph,
            Rect::at(
                line.left + painter.px(design::SPACE_3),
                centre - icon / 2.0,
                icon,
                icon,
            ),
            ink,
        );
        painter.text(
            label,
            pen,
            ink,
            Rect {
                left: line.left + painter.px(design::SPACE_3) + icon + painter.px(design::SPACE_3),
                ..line
            },
        );
        if painter.focused(&target) {
            painter.focus_ring(line, painter.px(design::RADIUS_SMALL));
        }
        painter.answers(target, line);
    }
}
