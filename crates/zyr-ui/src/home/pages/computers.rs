//! The computers: their page, and the way both pages show them, as cards
//! in a grid or rows in a list, narrowed by the search and the filter of
//! the bar at the top; and the menus that open over them.

use zyr_control::Peer;
use zyr_draw::design::{self, Colour};
use zyr_draw::icons::{self, Icon};
use zyr_draw::{Align, Rect};
use zyr_i18n::say;
use zyr_session::presence::{Narrowed, Presence, Road};

use crate::home::pages::page_title;
use crate::home::seen::Seen;
use crate::home::state::{Menu, Offer, Page, State, Target, View};
use crate::home::ui::{Button, Kind, Painter, QUICK, TAG_HEIGHT, middle, size, square};
use crate::home::words;

/// A card's height, the gap between cards, and the narrowest and widest a
/// computer's card is.
const CARD: f32 = 262.0;
const GAP: f32 = 16.0;
const NARROWEST: f32 = 220.0;
const WIDEST: f32 = 300.0;

/// A computer's picture on its card.
const PICTURE: f32 = 104.0;

/// A row of the list, the gap between two, and its picture.
const ROW: f32 = 76.0;
const ROW_GAP: f32 = 8.0;
const THUMBNAIL: (f32, f32) = (96.0, 54.0);
/// What the name and what is known of it take in a row, at most.
const WORDS: f32 = 280.0;

/// Draws the page within `at`, from its top, and gives back the height it
/// takes.
pub fn draw(painter: &mut Painter, at: Rect) -> f32 {
    let seen = painter.seen;
    let label = say!("adding.title");
    let button_width = painter.button_width(&label, Some(&icons::PLUS));
    let button_height = painter.px(38.0);
    painter.button(
        Rect::at(at.right - button_width, at.top, button_width, button_height),
        Target::AddComputer,
        Button::new(&label, Kind::Primary).with_icon(&icons::PLUS),
    );
    let words = Rect {
        right: at.right - button_width - painter.px(design::SPACE_5),
        ..at
    };
    let mut y = at.top
        + page_title(
            painter,
            words,
            &say!("computers.title"),
            &say!("computers.lead"),
        );
    let peers = shown(seen, painter.state);
    y += lay(painter, seen, &peers, Rect { top: y, ..at }, None);
    y - at.top
}

/// The computers the search and the filter keep, in the order they are
/// shown.
pub fn shown<'s>(seen: &'s Seen, state: &State) -> Vec<&'s Peer> {
    seen.peers
        .iter()
        .filter(|peer| zyr_session::presence::kept(peer, state.narrowed, &state.search))
        .collect()
}

/// How the computers go across a page of that width, in the view chosen:
/// how many to a row, how wide each is, how tall a row is and the gap
/// between rows.
pub fn laid(painter: &Painter, width: f32, count: usize) -> (usize, f32, f32, f32) {
    match painter.state.view {
        View::Grid => {
            let gap = painter.px(GAP);
            let fit = ((width + gap) / (painter.px(NARROWEST) + gap))
                .floor()
                .max(1.0) as usize;
            let columns = fit.min(count.max(1));
            let column =
                ((width - gap * (columns as f32 - 1.0)) / columns as f32).min(painter.px(WIDEST));
            (columns, column, painter.px(CARD), gap)
        }
        View::List => (1, width, painter.px(ROW), painter.px(ROW_GAP)),
    }
}

/// These computers, on at most that many rows, and gives back the height
/// they take. When they do not all fit, the last place says how many
/// more there are and leads to their page. A card when there is none,
/// which says how they come, or, when the search or the filter has left
/// none, which says so.
pub fn lay(
    painter: &mut Painter,
    seen: &Seen,
    peers: &[&Peer],
    at: Rect,
    rows: Option<usize>,
) -> f32 {
    let width = size(at).0;
    if peers.is_empty() {
        let empty = Rect::at(at.left, at.top, width, painter.px(CARD));
        return if seen.peers.is_empty() {
            no_computer(painter, empty)
        } else {
            nothing_found(painter, empty)
        };
    }
    let (columns, column, height, gap) = laid(painter, width, peers.len());
    let places = rows.map_or(usize::MAX, |rows| columns * rows.max(1));
    let all = peers.len() <= places;
    let shown = if all { peers.len() } else { places - 1 };
    let place = |rank: usize| {
        Rect::at(
            at.left + (rank % columns) as f32 * (column + gap),
            at.top + (rank / columns) as f32 * (height + gap),
            column,
            height,
        )
    };
    let view = painter.state.view;
    for (rank, peer) in peers.iter().take(shown).enumerate() {
        match view {
            View::Grid => card(painter, seen, peer, place(rank)),
            View::List => row(painter, seen, peer, place(rank)),
        }
    }
    if !all {
        more(painter, place(shown), peers.len() - shown);
    }
    let laid = if all { shown } else { places };
    let rows = laid.div_ceil(columns) as f32;
    rows * height + (rows - 1.0) * gap
}

/// The number a computer's view of the dunes is taken from, read from
/// its fingerprint so that it keeps its view from one day to the next.
pub fn seed_of(fingerprint: &str) -> u32 {
    u32::from_str_radix(fingerprint.get(..8).unwrap_or_default(), 16).unwrap_or(0)
}

/// The place of the computers that did not fit: how many there are, and
/// the way to their page.
fn more(painter: &mut Painter, at: Rect, count: usize) {
    let colours = painter.colours;
    let target = Target::Nav(Page::Computers);
    let hover = painter.hover_of(&target);
    let radius = painter.px(design::RADIUS);
    if hover > 0.0 {
        painter.fill(at, radius, colours.surface_3.faded(hover * 0.6));
    }
    painter.dashed(at, radius, colours.border_strong);
    let title = say!("home.more_computers_count", count = count);
    let pen = painter.body().in_bold();
    let arrow = painter.px(16.0);
    let width = painter.width_of(&title, pen);
    let (centre_x, centre_y) = middle(at);
    let left = centre_x - (width + painter.px(design::SPACE_2) + arrow) / 2.0;
    painter.text(
        &title,
        pen,
        colours.text_soft,
        Rect::at(left, at.top, width + painter.px(2.0), size(at).1),
    );
    painter.icon(
        &icons::ARROW_RIGHT,
        Rect::at(
            left + width + painter.px(design::SPACE_2),
            centre_y - arrow / 2.0,
            arrow,
            arrow,
        ),
        colours.text_soft,
    );
    if painter.focused(&target) {
        painter.focus_ring(at, radius);
    }
    painter.answers(target, at);
}

/// What a computer's card and its row both show, and how.
struct Shown {
    fingerprint: String,
    presence: Presence,
    reachable: bool,
    connect: Target,
    menu: Target,
    menu_open: bool,
    /// How far the hand being on the way to it has come.
    lit: f32,
}

impl Shown {
    fn of(painter: &mut Painter, seen: &Seen, peer: &Peer) -> Self {
        let fingerprint = peer.fingerprint.to_string();
        let presence = zyr_session::presence::presence(peer);
        let busy = seen.busy(painter.state.opening.is_some());
        let reachable = presence.can_be_reached() && !busy;
        let connect = Target::Connect(fingerprint.clone());
        let menu = Target::Menu(Menu::Computer(fingerprint.clone()));
        let menu_open = painter
            .state
            .menu
            .as_ref()
            .is_some_and(|(open, _)| *open == Menu::Computer(fingerprint.clone()));
        let lit = if reachable {
            painter.hover_of(&connect)
        } else {
            0.0
        };
        Shown {
            fingerprint,
            presence,
            reachable,
            connect,
            menu,
            menu_open,
            lit,
        }
    }

    /// The colour of its presence dot, and whether the dot is live.
    fn dot(&self, painter: &Painter) -> (Colour, bool) {
        let colours = painter.colours;
        match self.presence {
            Presence::Ready => (colours.online, true),
            Presence::Closed { .. } => (colours.warning, false),
            Presence::Gone | Presence::Unknown => (colours.offline, false),
        }
    }

    /// Its picture: a view of the dunes, with its presence in the corner.
    fn picture(&self, painter: &mut Painter, at: Rect, radius: f32) {
        let colours = painter.colours;
        painter.dunes(at, radius, seed_of(&self.fingerprint));
        painter.stroke(at, radius, colours.text.faded(0.07));
        let tip = say!("computer.wallpaper_soon");
        if self.reachable {
            painter.answers_with_tip(self.connect.clone(), at, tip);
        } else {
            painter.tells(at, tip);
        }
        let (ink, live) = self.dot(painter);
        let disc = painter.px(18.0);
        let disc_at = Rect::at(
            at.left + painter.px(6.0),
            at.top + painter.px(6.0),
            disc,
            disc,
        );
        painter.fill(disc_at, disc / 2.0, colours.surface_1);
        painter.dot(middle(disc_at), painter.px(8.0), ink, live);
    }

    /// The button of its menu in `at`, shown as strongly as `shown`, and
    /// where its menu opens from while it is open.
    fn menu_button(&self, painter: &mut Painter, at: Rect, shown: f32) {
        let colours = painter.colours;
        if shown > 0.0 {
            painter.fill(at, painter.px(7.0), colours.surface_1.faded(0.92 * shown));
            painter.icon(
                &icons::MORE,
                square(middle(at), painter.px(16.0)),
                colours.text_soft.faded(shown),
            );
        }
        painter.answers_with_tip(self.menu.clone(), at, say!("computer.more"));
        if self.menu_open {
            painter.state.menu = Some((Menu::Computer(self.fingerprint.clone()), at));
        }
    }

    /// The way to it at the bottom: « Connect », or « Wake up » for one
    /// that is gone, which does not work yet.
    fn way_to(&self, painter: &mut Painter, at: Rect) {
        let colours = painter.colours;
        if self.presence != Presence::Gone {
            let label = say!("computer.connect");
            painter.button(
                at,
                self.connect.clone(),
                Button::new(&label, Kind::Action)
                    .with_icon(&icons::COMPUTERS)
                    .enabled(self.reachable)
                    .lit(self.lit),
            );
            return;
        }
        let radius = painter.px(design::RADIUS_SMALL);
        painter.fill(at, radius, colours.surface_1);
        painter.stroke(at, radius, colours.border);
        let label = say!("computer.wake");
        let pen = painter.body().in_bold();
        let label_width = painter.width_of(&label, pen);
        let icon = painter.px(18.0);
        let (centre_x, centre_y) = middle(at);
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
                at.top,
                label_width + painter.px(2.0),
                size(at).1,
            ),
        );
        painter.tells(at, say!("soon.tip"));
    }
}

/// One computer's card: its picture, its presence, its name, the road to
/// it and how long it takes, and the way to it.
fn card(painter: &mut Painter, seen: &Seen, peer: &Peer, at: Rect) {
    let colours = painter.colours;
    let shown = Shown::of(painter, seen, peer);
    let near = painter
        .hover_of(&shown.menu)
        .max(shown.lit)
        .max(f32::from(u8::from(shown.menu_open)));
    painter.card_lit(at, shown.lit);
    if shown.reachable {
        painter.answers(shown.connect.clone(), at);
    }

    let inside = painter.px(14.0);
    let picture = Rect::at(
        at.left + inside,
        at.top + inside,
        size(at).0 - inside * 2.0,
        painter.px(PICTURE),
    );
    let radius = painter.px(design::RADIUS_SMALL + 1.0);
    shown.picture(painter, picture, radius);

    // In the picture's corner, the handle that will move the card, and
    // the computer's menu.
    let corner = painter.px(28.0);
    let handle = Rect::at(
        picture.right - painter.px(6.0) - corner,
        picture.top + painter.px(6.0),
        corner,
        corner,
    );
    painter.fill(handle, painter.px(7.0), colours.surface_1.faded(0.92));
    painter.icon(
        &icons::MOVE,
        square(middle(handle), painter.px(16.0)),
        colours.text_faint,
    );
    painter.tells(handle, say!("computer.move_soon"));
    let more = handle.shifted(-(corner + painter.px(6.0)), 0.0);
    let strength = if near > 0.0 || painter.focused(&shown.menu) {
        near.max(0.6)
    } else {
        0.0
    };
    shown.menu_button(painter, more, strength);

    // Under the picture: the name, what is known of it, its tags.
    let left = at.left + inside;
    let width = size(at).0 - inside * 2.0;
    let mut y = picture.bottom + painter.px(design::SPACE_3);
    y += name_and_more(painter, seen, peer, (left, y, width)) + painter.px(design::SPACE_2);
    tags(
        painter,
        seen,
        peer,
        &shown.presence,
        (left, y, left + width),
    );

    let height = painter.px(38.0);
    shown.way_to(
        painter,
        Rect::at(left, at.bottom - inside - height, width, height),
    );
}

/// One computer's row in the list: its picture, its name and what is
/// known of it, its tags, its menu and the way to it, across.
fn row(painter: &mut Painter, seen: &Seen, peer: &Peer, at: Rect) {
    let shown = Shown::of(painter, seen, peer);
    painter.card_lit(at, shown.lit);
    if shown.reachable {
        painter.answers(shown.connect.clone(), at);
    }
    let inside = painter.px(11.0);
    let (_, centre) = middle(at);
    let thumbnail = Rect::at(
        at.left + inside,
        centre - painter.px(THUMBNAIL.1) / 2.0,
        painter.px(THUMBNAIL.0),
        painter.px(THUMBNAIL.1),
    );
    let radius = painter.px(design::RADIUS_SMALL);
    shown.picture(painter, thumbnail, radius);

    // From the right: the way to it, then its menu.
    let label = say!("computer.connect");
    let button_width = painter
        .button_width(&label, Some(&icons::COMPUTERS))
        .max(painter.px(150.0));
    let button_height = painter.px(38.0);
    let edge = painter.px(design::SPACE_4);
    let button = Rect::at(
        at.right - edge - button_width,
        centre - button_height / 2.0,
        button_width,
        button_height,
    );
    shown.way_to(painter, button);
    let corner = painter.px(32.0);
    let more = Rect::at(
        button.left - painter.px(design::SPACE_2) - corner,
        centre - corner / 2.0,
        corner,
        corner,
    );
    shown.menu_button(painter, more, 1.0);

    let left = thumbnail.right + painter.px(design::SPACE_4);
    let room = (more.left - painter.px(design::SPACE_4) - left).max(0.0);
    let words_width = room.min(painter.px(WORDS));
    let taken =
        painter.measured(|painter| name_and_more(painter, seen, peer, (left, 0.0, words_width)));
    name_and_more(
        painter,
        seen,
        peer,
        (left, centre - taken / 2.0, words_width),
    );
    let tags_left = left + words_width + painter.px(design::SPACE_4);
    if tags_left < more.left {
        tags(
            painter,
            seen,
            peer,
            &shown.presence,
            (
                tags_left,
                centre - painter.px(TAG_HEIGHT) / 2.0,
                more.left - painter.px(design::SPACE_4),
            ),
        );
    }
}

/// A computer's name, and under it what is known of it. Gives back the
/// height they take.
fn name_and_more(
    painter: &mut Painter,
    seen: &Seen,
    peer: &Peer,
    (left, top, width): (f32, f32, f32),
) -> f32 {
    let colours = painter.colours;
    let name_pen = painter.body().in_bold().ellipsized();
    let name_height = painter.line(name_pen);
    painter.text(
        &peer.name,
        name_pen,
        colours.text,
        Rect::at(left, top, width, name_height),
    );
    let below = top + name_height + painter.px(2.0);
    let caption = painter.caption();
    let caption_height = painter.line(caption);
    painter.text(
        &words::below_the_name(peer, seen.device_of(peer)),
        caption.ellipsized(),
        colours.text_soft,
        Rect::at(left, below, width, caption_height),
    );
    below + caption_height - top
}

/// A computer's tags: the road to it, how long it takes, and what is
/// going on with it right now, as many as fit before `right`.
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
    let mut row = Tags {
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
struct Tags {
    x: f32,
    top: f32,
    right: f32,
}

impl Tags {
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

/// A dashed place saying something in the middle of where the computers
/// go: an icon, a title, a few words and a button. Gives back its
/// height, at least that of `at`.
fn empty_place(
    painter: &mut Painter,
    at: Rect,
    (icon, title, text): (&Icon, &str, &str),
    (label, button_icon, target): (&str, &Icon, Target),
) -> f32 {
    let colours = painter.colours;
    let radius = painter.px(design::RADIUS);
    let inside = painter.px(design::SPACE_6);
    let width = (size(at).0 - inside * 2.0).min(painter.px(440.0));
    let title_pen = painter.subtitle().aligned(Align::Centre);
    let pen = painter.caption().aligned(Align::Centre);
    let side = painter.px(36.0);
    let text_height = painter.height_of(text, pen, width);
    let content = side
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
    painter.dashed(at, radius, colours.border_strong);
    let (centre, _) = middle(at);
    let mut y = at.top + (height - content) / 2.0;
    painter.icon(
        icon,
        Rect::at(centre - side / 2.0, y, side, side),
        colours.text_faint,
    );
    y += side + painter.px(design::SPACE_3);
    let title_height = painter.line(title_pen);
    painter.text(
        title,
        title_pen,
        colours.text,
        Rect::at(at.left, y, size(at).0, title_height),
    );
    y += title_height + painter.px(design::SPACE_2);
    painter.paragraph(
        text,
        pen,
        colours.text_soft,
        (centre - width / 2.0, y, width),
    );
    y += text_height + painter.px(design::SPACE_4);
    let button_width = painter.button_width(label, Some(button_icon));
    painter.button(
        Rect::at(
            centre - button_width / 2.0,
            y,
            button_width,
            painter.px(38.0),
        ),
        target,
        Button::new(label, Kind::Primary).with_icon(button_icon),
    );
    height
}

/// The place shown while no computer is known, which says how they come.
fn no_computer(painter: &mut Painter, at: Rect) -> f32 {
    empty_place(
        painter,
        at,
        (
            &icons::COMPUTERS,
            &say!("home.no_computer"),
            &say!("home.no_computer_text"),
        ),
        (&say!("adding.title"), &icons::PLUS, Target::AddComputer),
    )
}

/// The place shown when the search and the filter leave no computer.
fn nothing_found(painter: &mut Painter, at: Rect) -> f32 {
    empty_place(
        painter,
        at,
        (
            &icons::SEARCH,
            &say!("computers.nothing_found"),
            &say!("computers.nothing_found_text"),
        ),
        (
            &say!("computers.show_all"),
            &icons::COMPUTERS,
            Target::ShowAll,
        ),
    )
}

/* ---- The menus ---------------------------------------------------------- */

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

/// What the filter is called, and its icon.
pub fn narrowed_to(narrowed: Narrowed) -> (String, &'static Icon) {
    match narrowed {
        Narrowed::All => (say!("filter.all"), &icons::COMPUTERS),
        Narrowed::Online => (say!("filter.online"), &icons::LINK),
        Narrowed::Offline => (say!("filter.offline"), &icons::POWER),
        Narrowed::Local => (say!("filter.local"), &icons::LOCAL_NETWORK),
        Narrowed::Internet => (say!("filter.internet"), &icons::WORLD),
    }
}

/// One line of a menu: what it does, its words, its icon, its ink, and
/// whether it is the one chosen.
struct Item {
    target: Target,
    label: String,
    icon: &'static Icon,
    ink: Colour,
    chosen: bool,
}

/// The menu open, under the button that opened it.
pub fn menu(painter: &mut Painter, width: f32, height: f32) {
    let Some((open, anchor)) = painter.state.menu.clone() else {
        return;
    };
    let colours = painter.colours;
    let items: Vec<Item> = match &open {
        Menu::Computer(fingerprint) => {
            let seen = painter.seen;
            let Some(peer) = seen.peer(fingerprint) else {
                painter.state.menu = None;
                return;
            };
            offers(seen, peer, painter.state.opening.is_some())
                .into_iter()
                .map(|offer| {
                    let (label, icon) = match offer {
                        Offer::HereOnly => (say!("computer.here_only"), &icons::LOCAL_NETWORK),
                        Offer::Journal => (say!("computer.journal"), &icons::JOURNAL),
                        Offer::SendBack => (say!("computer.send_back"), &icons::DISCONNECT),
                        Offer::Forget => (say!("computer.forget"), &icons::CROSS),
                    };
                    Item {
                        target: Target::Offer(fingerprint.clone(), offer),
                        label,
                        icon,
                        ink: if offer == Offer::Forget {
                            colours.warning
                        } else {
                            colours.text
                        },
                        chosen: false,
                    }
                })
                .collect()
        }
        Menu::Filter => Narrowed::ALL
            .into_iter()
            .map(|narrowed| {
                let (label, icon) = narrowed_to(narrowed);
                Item {
                    target: Target::Narrow(narrowed),
                    label,
                    icon,
                    ink: colours.text,
                    chosen: painter.state.narrowed == narrowed,
                }
            })
            .collect(),
    };
    let shown = painter.arriving(&format!("menu {open:?}"), QUICK);
    let row = painter.px(38.0);
    let inside = painter.px(design::SPACE_1 + 2.0);
    let pen = painter.body();
    let icon = painter.px(18.0);
    let check = if items.iter().any(|item| item.chosen) {
        icon + painter.px(design::SPACE_3)
    } else {
        0.0
    };
    let wide = items
        .iter()
        .map(|item| painter.width_of(&item.label, pen))
        .fold(0.0, f32::max)
        + icon
        + check
        + painter.px(design::SPACE_3 * 2.0 + design::SPACE_4);
    let tall = row * items.len() as f32 + inside * 2.0;
    let left = match open {
        Menu::Filter => anchor.left,
        Menu::Computer(_) => anchor.right - wide,
    }
    .clamp(
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
    painter.answers(Target::Menu(open.clone()), at);
    for (rank, item) in items.into_iter().enumerate() {
        let line = Rect::at(
            at.left + inside,
            at.top + inside + row * rank as f32,
            wide - inside * 2.0,
            row,
        );
        let hover = painter.hover_of(&item.target);
        if hover > 0.0 {
            painter.fill(
                line,
                painter.px(design::RADIUS_SMALL),
                colours.surface_3.faded(hover),
            );
        }
        let (_, centre) = middle(line);
        let ink = if item.chosen {
            colours.accent
        } else {
            item.ink
        };
        painter.icon(
            item.icon,
            Rect::at(
                line.left + painter.px(design::SPACE_3),
                centre - icon / 2.0,
                icon,
                icon,
            ),
            ink,
        );
        painter.text(
            &item.label,
            if item.chosen { pen.in_bold() } else { pen },
            ink,
            Rect {
                left: line.left + painter.px(design::SPACE_3) + icon + painter.px(design::SPACE_3),
                ..line
            },
        );
        if item.chosen {
            painter.icon(
                &icons::CHECK,
                Rect::at(
                    line.right - painter.px(design::SPACE_3) - icon,
                    centre - icon / 2.0,
                    icon,
                    icon,
                ),
                colours.accent,
            );
        }
        if painter.focused(&item.target) {
            painter.focus_ring(line, painter.px(design::RADIUS_SMALL));
        }
        painter.answers(item.target, line);
    }
}
