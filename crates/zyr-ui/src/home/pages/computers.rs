//! The computers: their page, and the way both pages show them, as cards
//! in a grid or rows in a list, narrowed by the search and the filter of
//! the bar at the top; and the filter's menu.

use zyr_control::Peer;
use zyr_draw::design::{self, Colour};
use zyr_draw::icons::{self, Icon};
use zyr_draw::{Align, Rect};
use zyr_i18n::say;
use zyr_session::presence::{Narrowed, Presence, Road};

use crate::home::pages::page_title;
use crate::home::seen::Seen;
use crate::home::state::{Offer, Page, State, Target, View};
use crate::home::ui::{Button, Kind, Painter, QUICK, STEADY, TAG_HEIGHT, middle, size, square};
use crate::home::words;

/// What a card takes under its picture, the gap between cards, and the
/// narrowest and widest a computer's card is.
const BELOW_THE_PICTURE: f32 = 158.0;
const GAP: f32 = 16.0;
const NARROWEST: f32 = 220.0;
const WIDEST: f32 = 300.0;

/// The margin inside a card, around its picture and under its buttons.
const INSIDE: f32 = 14.0;

/// The buttons at the foot of a card or at the end of a row.
const BUTTON: f32 = 38.0;

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

/// The computers the search and the filter keep, in the order the person
/// put them in.
pub fn shown<'s>(seen: &'s Seen, state: &State) -> Vec<&'s Peer> {
    zyr_session::presence::arranged(
        seen.peers
            .iter()
            .filter(|peer| zyr_session::presence::kept(peer, state.narrowed, &state.search)),
        &state.order,
    )
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
            (columns, column, card_height(painter, column), gap)
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
        let empty = Rect::at(
            at.left,
            at.top,
            width,
            card_height(painter, painter.px(WIDEST)),
        );
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
    let draw = |painter: &mut Painter, peer: &Peer, at: Rect| match view {
        View::Grid => card(painter, seen, peer, at),
        View::List => row(painter, seen, peer, at),
    };

    // The card a hand is moving goes where the hand would drop it, and
    // the others make room for it.
    let mut laid: Vec<&Peer> = peers[..shown].to_vec();
    let held = held(painter, &laid, place(0));
    if let Some((from, corner)) = held {
        let moving = laid.remove(from);
        let (centre_x, centre_y) = (corner.0 + column / 2.0, corner.1 + height / 2.0);
        let across = ((centre_x - at.left) / (column + gap))
            .floor()
            .clamp(0.0, columns as f32 - 1.0) as usize;
        let down = ((centre_y - at.top) / (height + gap)).floor().max(0.0) as usize;
        laid.insert((down * columns + across).min(laid.len()), moving);
        if let Some(dragging) = painter.state.dragging.as_mut() {
            dragging.placed = laid
                .iter()
                .map(|peer| peer.fingerprint.to_string())
                .collect();
        }
    }
    let moving = held.map(|(from, _)| peers[from].fingerprint);
    for (rank, peer) in laid.iter().enumerate() {
        if Some(peer.fingerprint) == moving {
            continue;
        }
        // Each card slides to its place rather than jumping, so that the
        // hand sees which ones made room.
        let goal = place(rank);
        let target = Target::Move(peer.fingerprint.to_string());
        let left = painter.ease(&target, 5, goal.left - at.left, STEADY);
        let top = painter.ease(&target, 6, goal.top - at.top, STEADY);
        draw(
            painter,
            peer,
            Rect::at(at.left + left, at.top + top, column, height),
        );
    }
    if !all {
        more(painter, place(shown), peers.len() - shown);
    }
    if let Some((from, corner)) = held {
        let at = Rect::at(corner.0, corner.1, column, height);
        painter.shadow(at, painter.px(design::RADIUS), painter.colours.shadow_2);
        painter.living(false, |painter| draw(painter, peers[from], at));
    }
    let laid = if all { shown } else { places };
    let rows = laid.div_ceil(columns) as f32;
    rows * height + (rows - 1.0) * gap
}

/// The height of a computer's picture on a card that wide: the shape of
/// a screen, 16 by 9, so that its wallpaper shows whole.
fn picture_height(painter: &Painter, width: f32) -> f32 {
    (width - painter.px(INSIDE) * 2.0) * 9.0 / 16.0
}

/// The height of a card that wide.
fn card_height(painter: &Painter, width: f32) -> f32 {
    picture_height(painter, width) + painter.px(BELOW_THE_PICTURE)
}

/// The card a hand is moving, if it is one of `laid`: its rank there, and
/// where its corner follows the hand to. `first` is the first place,
/// which says how large a card is.
fn held(painter: &Painter, laid: &[&Peer], first: Rect) -> Option<(usize, (f32, f32))> {
    let dragging = painter.state.dragging.as_ref()?;
    let pointer = painter.state.pointer?;
    let from = laid
        .iter()
        .position(|peer| peer.fingerprint.to_string() == dragging.fingerprint)?;
    let handle = handle_in(painter, painter.state.view, first);
    Some((
        from,
        (
            pointer.0 - dragging.grab.0 - (handle.left - first.left),
            pointer.1 - dragging.grab.1 - (handle.top - first.top),
        ),
    ))
}

/// Where a computer's handle is, within its card or its row.
fn handle_in(painter: &Painter, view: View, at: Rect) -> Rect {
    match view {
        View::Grid => {
            let corner = painter.px(28.0);
            Rect::at(
                at.right - painter.px(INSIDE + 6.0) - corner,
                at.top + painter.px(INSIDE + 6.0),
                corner,
                corner,
            )
        }
        View::List => Rect::at(
            at.left + painter.px(6.0),
            middle(at).1 - painter.px(14.0),
            painter.px(24.0),
            painter.px(28.0),
        ),
    }
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
    /// Whether it can be woken from here, its card being known.
    wakeable: bool,
    reachable: bool,
    connect: Target,
    /// What its card offers besides opening a session.
    offers: Vec<Offer>,
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
        let lit = if reachable {
            painter.hover_of(&connect)
        } else {
            0.0
        };
        Shown {
            wakeable: peer.wakeable,
            fingerprint,
            presence,
            reachable,
            connect,
            offers: offers(seen, peer, painter.state.opening.is_some()),
            lit,
        }
    }

    /// The colour of its presence dot.
    fn dot(&self, painter: &Painter) -> Colour {
        let colours = painter.colours;
        match self.presence {
            Presence::Ready => colours.online,
            Presence::Closed { .. } => colours.warning,
            Presence::Gone | Presence::Unknown => colours.offline,
        }
    }

    /// Its picture: its wallpaper, as its last session brought it, or a
    /// view of the dunes until one has; its presence in the corner.
    fn picture(&self, painter: &mut Painter, at: Rect, radius: f32) {
        let colours = painter.colours;
        let seen = painter.seen;
        let far = seen.far_wallpapers.get(&self.fingerprint);
        match far {
            Some(far) => painter.picture(&far.picture, at, radius),
            None => painter.dunes(at, radius, seed_of(&self.fingerprint)),
        }
        painter.stroke(at, radius, colours.text.faded(0.07));
        let tip = far
            .is_none()
            .then(|| say!("computer.wallpaper_after_a_session"));
        match (self.reachable, tip) {
            (true, Some(tip)) => painter.answers_with_tip(self.connect.clone(), at, tip),
            (true, None) => painter.answers(self.connect.clone(), at),
            (false, Some(tip)) => painter.tells(at, tip),
            (false, None) => {}
        }
        // One dot in a thin ring of the card's surface, which keeps it
        // apart from any wallpaper, and no halo: on a picture, a ring of
        // light around it reads as one more layer.
        let ink = self.dot(painter);
        let ring = painter.px(14.0);
        let ring_at = Rect::at(
            at.left + painter.px(8.0),
            at.top + painter.px(8.0),
            ring,
            ring,
        );
        painter.fill(ring_at, ring / 2.0, colours.surface_1);
        painter.dot(middle(ring_at), painter.px(9.0), ink, false);
    }

    /// The handle a hand moves it by, in `at`, on a backing over a
    /// picture.
    fn handle(&self, painter: &mut Painter, at: Rect, backed: bool) {
        let colours = painter.colours;
        let target = Target::Move(self.fingerprint.clone());
        let hover = painter.hover_of(&target);
        let radius = painter.px(7.0);
        if backed {
            painter.fill(at, radius, colours.surface_1.faded(0.92));
        } else if hover > 0.0 {
            painter.fill(at, radius, colours.surface_3.faded(hover * 0.8));
        }
        painter.icon(
            &icons::MOVE,
            square(middle(at), painter.px(16.0)),
            colours.text.mixed_with(colours.text_faint, hover),
        );
        painter.answers_with_tip(target, at, say!("computer.move"));
    }

    /// What it offers besides opening a session, as buttons that show
    /// their icon, laid leftwards from `right` on the line from `top`.
    /// Gives back where they leave off.
    fn offers(&self, painter: &mut Painter, (right, top): (f32, f32)) -> f32 {
        let side = painter.px(BUTTON);
        let gap = painter.px(design::SPACE_2);
        let mut left = right;
        for offer in self.offers.iter().rev() {
            left -= side;
            let target = Target::Offer(self.fingerprint.clone(), *offer);
            let armed = painter.state.is_armed(&target);
            let (tip, icon) = offered(*offer);
            let tip = if armed { say!("dialog.confirm") } else { tip };
            let kind = if armed { Kind::Warning } else { Kind::Quiet };
            painter.button(
                Rect::at(left, top, side, side),
                target,
                Button::new("", kind).with_icon(icon).told(&tip),
            );
            left -= gap;
        }
        left
    }

    /// The way to it: « Connect », or « Wake up » for one that is gone,
    /// faded while its card is not known. Only its icon where its word
    /// does not fit.
    fn way_to(&self, painter: &mut Painter, at: Rect) {
        let gone = self.presence == Presence::Gone;
        let (label, icon, target, kind, enabled) = if gone {
            (
                say!("computer.wake"),
                &icons::POWER,
                Target::Wake(self.fingerprint.clone()),
                Kind::Quiet,
                self.wakeable,
            )
        } else {
            (
                say!("computer.connect"),
                &icons::COMPUTERS,
                self.connect.clone(),
                Kind::Action,
                self.reachable,
            )
        };
        let fits = size(at).0 >= painter.button_width(&label, Some(icon));
        let tip = if gone && !self.wakeable {
            Some(say!("computer.wake_unknown"))
        } else {
            (!fits).then(|| label.clone())
        };
        let mut button = Button::new(if fits { &label } else { "" }, kind)
            .with_icon(icon)
            .enabled(enabled)
            .lit(self.lit);
        if let Some(tip) = &tip {
            button = button.told(tip);
        }
        painter.button(at, target, button);
    }
}

/// What an offer says of itself, and its icon.
fn offered(offer: Offer) -> (String, &'static Icon) {
    match offer {
        Offer::HereOnly => (say!("computer.here_only"), &icons::LOCAL_NETWORK),
        Offer::Journal => (say!("computer.journal"), &icons::JOURNAL),
        Offer::SendBack => (say!("computer.send_back"), &icons::DISCONNECT),
        Offer::Forget => (say!("computer.forget"), &icons::CROSS),
    }
}

/// One computer's card: its picture, its presence, its name, the road to
/// it and how long it takes, the way to it and what else it offers.
fn card(painter: &mut Painter, seen: &Seen, peer: &Peer, at: Rect) {
    let shown = Shown::of(painter, seen, peer);
    painter.card_lit(at, shown.lit);
    if shown.reachable {
        painter.answers(shown.connect.clone(), at);
    }

    let inside = painter.px(INSIDE);
    let width = size(at).0 - inside * 2.0;
    let picture = Rect::at(
        at.left + inside,
        at.top + inside,
        width,
        picture_height(painter, size(at).0),
    );
    let radius = painter.px(design::RADIUS_SMALL + 1.0);
    shown.picture(painter, picture, radius);

    // In the picture's corner, while the computers are being rearranged,
    // the handle that moves the card.
    if painter.state.arranging {
        shown.handle(painter, handle_in(painter, View::Grid, at), true);
    }

    // Under the picture: the name, what is known of it, its tags.
    let left = at.left + inside;
    let mut y = picture.bottom + painter.px(design::SPACE_3);
    y += name_and_more(painter, seen, peer, (left, y, width)) + painter.px(design::SPACE_2);
    tags(
        painter,
        seen,
        peer,
        &shown.presence,
        (left, y, left + width),
    );

    // At the foot: the way to it, and beside it what else it offers.
    let height = painter.px(BUTTON);
    let top = at.bottom - inside - height;
    let right = shown.offers(painter, (left + width, top));
    shown.way_to(painter, Rect::at(left, top, right - left, height));
}

/// One computer's row in the list: its picture, its name and what is
/// known of it, its tags, what it offers and the way to it, across.
fn row(painter: &mut Painter, seen: &Seen, peer: &Peer, at: Rect) {
    let shown = Shown::of(painter, seen, peer);
    painter.card_lit(at, shown.lit);
    if shown.reachable {
        painter.answers(shown.connect.clone(), at);
    }
    let (_, centre) = middle(at);
    let thumbnail_left = if painter.state.arranging {
        let handle = handle_in(painter, View::List, at);
        shown.handle(painter, handle, false);
        handle.right + painter.px(4.0)
    } else {
        at.left + painter.px(11.0)
    };
    let thumbnail = Rect::at(
        thumbnail_left,
        centre - painter.px(THUMBNAIL.1) / 2.0,
        painter.px(THUMBNAIL.0),
        painter.px(THUMBNAIL.1),
    );
    let radius = painter.px(design::RADIUS_SMALL);
    shown.picture(painter, thumbnail, radius);

    // From the right: the way to it, then what else it offers.
    let label = say!("computer.connect");
    let button_width = painter
        .button_width(&label, Some(&icons::COMPUTERS))
        .max(painter.px(150.0));
    let button_height = painter.px(BUTTON);
    let edge = painter.px(design::SPACE_4);
    let button = Rect::at(
        at.right - edge - button_width,
        centre - button_height / 2.0,
        button_width,
        button_height,
    );
    shown.way_to(painter, button);
    let end = shown.offers(
        painter,
        (button.left - painter.px(design::SPACE_2), button.top),
    );

    let left = thumbnail.right + painter.px(design::SPACE_4);
    let room = (end - painter.px(design::SPACE_4) - left).max(0.0);
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
    if tags_left < end {
        tags(
            painter,
            seen,
            peer,
            &shown.presence,
            (
                tags_left,
                centre - painter.px(TAG_HEIGHT) / 2.0,
                end - painter.px(design::SPACE_4),
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

/// A dashed place saying something where a list has nothing to show: an
/// icon, a title, a few words and what to do, if anything. Gives back its
/// height, at least that of `at`.
pub fn empty_place(
    painter: &mut Painter,
    at: Rect,
    (icon, title, text): (&Icon, &str, &str),
    action: Option<(&str, &Icon, Target)>,
) -> f32 {
    let colours = painter.colours;
    let radius = painter.px(design::RADIUS);
    let inside = painter.px(design::SPACE_6);
    let width = (size(at).0 - inside * 2.0).min(painter.px(440.0));
    let title_pen = painter.subtitle().aligned(Align::Centre);
    let pen = painter.caption().aligned(Align::Centre);
    let side = painter.px(36.0);
    let text_height = painter.height_of(text, pen, width);
    let button = action
        .as_ref()
        .map_or(0.0, |_| painter.px(design::SPACE_4 + 38.0));
    let content = side
        + painter.px(design::SPACE_3)
        + painter.line(title_pen)
        + painter.px(design::SPACE_2)
        + text_height
        + button;
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
    if let Some((label, button_icon, target)) = action {
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
    }
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
        Some((&say!("adding.title"), &icons::PLUS, Target::AddComputer)),
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
        Some((
            &say!("computers.show_all"),
            &icons::COMPUTERS,
            Target::ShowAll,
        )),
    )
}

/* ---- The offers and the filter's menu --------------------------------------- */

/// What a computer's card offers besides opening a session, in the order
/// it shows them.
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

/// One line of a menu: what it does, its words, its icon, and whether
/// it is the one chosen.
struct Item {
    target: Target,
    label: String,
    icon: &'static Icon,
    chosen: bool,
}

/// The menu open, under the button that opened it.
pub fn menu(painter: &mut Painter, width: f32, height: f32) {
    let Some((open, anchor)) = painter.state.menu.clone() else {
        return;
    };
    let colours = painter.colours;
    let items: Vec<Item> = Narrowed::ALL
        .into_iter()
        .map(|narrowed| {
            let (label, icon) = narrowed_to(narrowed);
            Item {
                target: Target::Narrow(narrowed),
                label,
                icon,
                chosen: painter.state.narrowed == narrowed,
            }
        })
        .collect();
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
    let left = anchor.left.clamp(
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
            colours.text
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
