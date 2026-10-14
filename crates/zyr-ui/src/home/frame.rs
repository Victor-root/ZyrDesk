//! The frame every page is shown in.
//!
//! The bar on the left, which folds behind a button in a narrow window;
//! the bar at the top of the page; the page itself, which scrolls under
//! that bar; and, over all of it in this order, a computer's menu, the
//! open dialog, a session being opened, what the person has just been
//! told, and the bubble over what the hand is on.

use std::time::{Duration, Instant};

use zyr_draw::design;
use zyr_draw::icons::{self, Icon};
use zyr_draw::{Align, Rect};
use zyr_i18n::say;

use crate::home::state::{CONFIRM_TIME, COPIED_TIME, NEWS_TIME, Page, Scroller, Target};
use crate::home::ui::{Painter, QUICK, STEADY, middle, size, square};
use crate::home::{dialogs, opening, pages, words};

/// The bar on the left, and the width below which it folds away.
pub const SIDEBAR: f32 = 224.0;
const FOLDS_BELOW: f32 = 1120.0;

/// What surrounds the page: on its sides, above the bar at its top, and
/// that bar's height.
pub const SIDE: f32 = 32.0;
const ABOVE: f32 = 20.0;
const BAR: f32 = 40.0;

/// How long the hand rests on something before its bubble opens.
const BUBBLE_WAITS: Duration = Duration::from_millis(450);

/// The pages a bar on the left lists, what each is called, its icon, and
/// whether it exists yet: those that do not are shown, faded, and say so.
const NAVIGATION: [(Option<Page>, &str, &Icon); 6] = [
    (Some(Page::Home), zyr_i18n::key!("nav.home"), &icons::HOME),
    (None, zyr_i18n::key!("nav.computers"), &icons::COMPUTERS),
    (None, zyr_i18n::key!("nav.contacts"), &icons::CONTACTS),
    (None, zyr_i18n::key!("nav.activity"), &icons::ACTIVITY),
    (
        Some(Page::Settings),
        zyr_i18n::key!("nav.settings"),
        &icons::SETTINGS,
    ),
    (
        Some(Page::About),
        zyr_i18n::key!("nav.about"),
        &icons::ABOUT,
    ),
];

/// Draws the whole window, `width` by `height` real pixels.
pub fn draw(painter: &mut Painter, width: f32, height: f32) {
    lapse(painter);
    let narrow = width / painter.scale < FOLDS_BELOW;
    if !narrow {
        painter.state.drawer = false;
    }
    let covered = painter.state.dialog.is_some() || painter.state.opening.is_some();
    let left = if narrow { 0.0 } else { painter.px(SIDEBAR) };

    painter.living(!covered && !painter.state.drawer, |painter| {
        page(painter, Rect::at(left, 0.0, width - left, height), narrow);
        if !narrow {
            sidebar(painter, Rect::at(0.0, 0.0, left, height));
        }
    });
    if narrow {
        drawer(painter, width, height, covered);
    }
    painter.living(!covered, |painter| {
        pages::home::menu(painter, width, height)
    });
    if painter.state.dialog.is_some() && painter.state.opening.is_none() {
        dialogs::draw(painter, width, height);
    }
    if painter.state.opening.is_some() {
        opening::draw(painter, width, height);
    }
    notice(painter, width, height);
    bubble(painter, width, height);
}

/// What changes by itself after a while asks for the frame that shows
/// it: a button that stops asking for its second click, another that
/// stops saying it has copied.
fn lapse(painter: &mut Painter) {
    let state = &mut *painter.state;
    match state.armed.as_ref().map(|(_, since)| *since + CONFIRM_TIME) {
        Some(until) if Instant::now() < until => state.wake(until),
        Some(_) => state.armed = None,
        None => {}
    }
    match state.copied.as_ref().map(|(_, since)| *since + COPIED_TIME) {
        Some(until) if Instant::now() < until => state.wake(until),
        Some(_) => state.copied = None,
        None => {}
    }
}

/// The page, under the bar at its top, scrolled.
fn page(painter: &mut Painter, at: Rect, narrow: bool) {
    let colours = painter.colours;
    painter.fill(at, 0.0, colours.background);
    let side = painter.px(SIDE);
    let bar_top = at.top + painter.px(ABOVE);
    let bar = Rect::at(
        at.left + side,
        bar_top,
        size(at).0 - side * 2.0,
        painter.px(BAR),
    );
    let room = Rect {
        top: bar.bottom + painter.px(design::SPACE_4),
        ..at
    };

    let now = painter.scrolled_to(Scroller::Page);
    let inner = Rect {
        left: at.left + side,
        right: at.right - side,
        top: room.top + painter.px(design::SPACE_2) - now,
        bottom: room.bottom,
    };
    let mut content = 0.0;
    painter.clipped(room, |painter| {
        content = match painter.state.page {
            Page::Home => pages::home::draw(painter, inner),
            Page::Settings => pages::settings::draw(painter, inner),
            Page::About => pages::about::draw(painter, inner),
        } + painter.px(design::SPACE_2 + design::SPACE_6);
    });
    painter.scrollbar(Scroller::Page, room, content, 0.0);

    // The bar at the top stays where it is while the page goes under it,
    // and a line says so as soon as something has.
    let veil = Rect {
        bottom: room.top,
        ..at
    };
    painter.fill(veil, 0.0, colours.background);
    if now > 0.0 {
        let shown = (now / painter.px(design::SPACE_4)).min(1.0);
        painter.fill(
            Rect::at(
                at.left,
                room.top - painter.px(1.0),
                size(at).0,
                painter.px(1.0),
            ),
            0.0,
            colours.border.faded(shown),
        );
    }
    top_bar(painter, bar, narrow);
}

/// The bar at the top of the page: the button that unfolds the bar on
/// the left in a narrow window, and on the right the search, the filter
/// and the two views, which are drawn and do not work yet.
fn top_bar(painter: &mut Painter, at: Rect, narrow: bool) {
    let colours = painter.colours;
    let height = size(at).1;
    let radius = painter.px(design::RADIUS_SMALL + 1.0);
    let gap = painter.px(design::SPACE_3);
    let soon = say!("soon.tip");
    if narrow {
        painter.icon_button(
            Rect::at(at.left - painter.px(6.0), at.top, height, height),
            &icons::MENU,
            Target::Drawer,
            say!("nav.open_menu"),
        );
    }

    // From the right: the views, the filter, then the search, which takes
    // what is left, within reason.
    let view = height - painter.px(2.0);
    let list = Rect::at(at.right - view, at.top + painter.px(1.0), view, view);
    let grid = list.shifted(-(view + painter.px(design::SPACE_2)), 0.0);
    painter.fill(grid, radius, colours.surface_1);
    painter.stroke(grid, radius, colours.border);
    let side = painter.px(20.0);
    painter.icon(&icons::GRID, square(middle(grid), side), colours.accent);
    painter.icon(&icons::LIST, square(middle(list), side), colours.text_faint);
    painter.tells(
        Rect {
            left: grid.left,
            ..list
        },
        soon.clone(),
    );
    let divider = |painter: &Painter, x: f32| {
        painter.upright(
            x,
            at.top + painter.px(9.0),
            height - painter.px(18.0),
            colours.border,
        );
    };
    divider(painter, grid.left - gap);

    let pen = painter.body();
    let all = say!("bar.all");
    let filter_width =
        painter.px(16.0 + 10.0 + 16.0 + 6.0) + painter.width_of(&all, pen) + gap * 2.0;
    let filter = Rect::at(
        grid.left - gap * 2.0 - filter_width,
        at.top,
        filter_width,
        height,
    );
    painter.fill(filter, radius, colours.surface_1);
    painter.stroke(filter, radius, colours.border);
    let small = painter.px(17.0);
    let (_, centre) = middle(filter);
    painter.icon(
        &icons::FILTER,
        Rect::at(filter.left + gap, centre - small / 2.0, small, small),
        colours.text_faint,
    );
    painter.text(
        &all,
        pen,
        colours.text_faint,
        Rect::at(
            filter.left + gap + small + painter.px(8.0),
            at.top,
            painter.width_of(&all, pen) + painter.px(2.0),
            height,
        ),
    );
    painter.icon(
        &icons::CHEVRON_DOWN,
        Rect::at(
            filter.right - gap - painter.px(16.0),
            centre - painter.px(8.0),
            painter.px(16.0),
            painter.px(16.0),
        ),
        colours.text_faint,
    );
    painter.tells(filter, soon.clone());
    divider(painter, filter.left - gap);

    let floor = at.left + if narrow { height + gap } else { 0.0 };
    let search_right = filter.left - gap * 2.0;
    let search_width = (search_right - floor).clamp(0.0, painter.px(380.0));
    if search_width < painter.px(160.0) {
        return;
    }
    let search = Rect::at(search_right - search_width, at.top, search_width, height);
    painter.fill(search, radius, colours.surface_1);
    painter.stroke(search, radius, colours.border);
    let glass = painter.px(18.0);
    painter.icon(
        &icons::SEARCH,
        Rect::at(
            search.left + painter.px(14.0),
            centre - glass / 2.0,
            glass,
            glass,
        ),
        colours.text_faint,
    );
    let keys = say!("bar.search_keys");
    let caption = painter.caption();
    let keys_width = painter.width_of(&keys, caption) + painter.px(design::SPACE_3);
    let cap = Rect::at(
        search.right - painter.px(8.0) - keys_width,
        centre - painter.px(11.0),
        keys_width,
        painter.px(22.0),
    );
    let words_left = search.left + painter.px(14.0) + glass + painter.px(10.0);
    let words_width = (cap.left - painter.px(8.0) - words_left).max(0.0);
    painter.text(
        &say!("bar.search"),
        pen.ellipsized(),
        colours.text_faint,
        Rect::at(words_left, at.top, words_width, height),
    );
    painter.keycap(cap, &keys, colours.text_faint);
    painter.tells(search, soon);
}

/// The bar on the left: the mark and the name, the pages, and at the
/// bottom the person and whether everything is up to date.
fn sidebar(painter: &mut Painter, at: Rect) {
    let colours = painter.colours;
    painter.fill(at, 0.0, colours.surface_2);
    painter.upright(
        at.right - painter.px(1.0),
        at.top,
        size(at).1,
        colours.border,
    );

    let edge = painter.px(design::SPACE_5);
    let mark = painter.px(36.0);
    painter.mark(Rect::at(at.left + edge, at.top + edge, mark, mark));
    let title = painter.title();
    painter.text(
        "ZyrDesk",
        title,
        colours.text,
        Rect::at(
            at.left + edge + mark + painter.px(design::SPACE_3),
            at.top + edge,
            size(at).0,
            mark,
        ),
    );

    let inset = painter.px(design::SPACE_3);
    let mut y = at.top + edge + mark + painter.px(design::SPACE_6);
    for (rank, (page, label, icon)) in NAVIGATION.iter().enumerate() {
        if rank == 4 {
            y += painter.px(design::SPACE_2);
            painter.rule(at.left + edge, y, size(at).0 - edge * 2.0);
            y += painter.px(design::SPACE_3);
        }
        let item = Rect::at(
            at.left + inset,
            y,
            size(at).0 - inset * 2.0,
            painter.px(44.0),
        );
        navigation_item(painter, item, *page, &zyr_i18n::text(label), icon);
        y += painter.px(44.0 + design::SPACE_1);
    }

    // At the bottom, from the floor up.
    let floor = at.bottom - edge;
    let up_to_date = say!("nav.up_to_date");
    let caption = painter.caption();
    let line = painter.line(caption);
    let dot = painter.px(8.0);
    let status = Rect::at(at.left + edge, floor - line, size(at).0 - edge * 2.0, line);
    painter.dot(
        (status.left + dot / 2.0, middle(status).1),
        dot,
        colours.text_faint,
        false,
    );
    painter.text(
        &up_to_date,
        caption,
        colours.text_faint,
        Rect {
            left: status.left + dot + painter.px(design::SPACE_2),
            ..status
        },
    );
    painter.tells(status, say!("soon.tip"));
    let person = Rect::at(
        at.left + inset,
        status.top - painter.px(design::SPACE_4 + 52.0),
        size(at).0 - inset * 2.0,
        painter.px(52.0),
    );
    painter.rule(
        at.left + edge,
        person.top - painter.px(design::SPACE_3),
        size(at).0 - edge * 2.0,
    );
    person_row(painter, person);
}

/// One page of the bar on the left.
fn navigation_item(painter: &mut Painter, at: Rect, page: Option<Page>, label: &str, icon: &Icon) {
    let colours = painter.colours;
    let radius = painter.px(10.0);
    let side = painter.px(20.0);
    let (_, centre) = middle(at);
    let icon_at = Rect::at(
        at.left + painter.px(design::SPACE_4),
        centre - side / 2.0,
        side,
        side,
    );
    let words = Rect {
        left: icon_at.right + painter.px(design::SPACE_3),
        ..at
    };
    let Some(page) = page else {
        painter.icon(icon, icon_at, colours.text_faint);
        painter.text(label, painter.body(), colours.text_faint, words);
        painter.tells(at, say!("soon.tip"));
        return;
    };
    let target = Target::Nav(page);
    let current = painter.state.page == page;
    let hover = painter.hover_of(&target);
    let lit = painter.ease(&target, 1, if current { 1.0 } else { 0.0 }, STEADY);
    if lit > 0.0 {
        painter.fill(at, radius, colours.accent_tint.faded(lit));
        let bar = painter.px(3.0);
        painter.fill(
            Rect::at(at.left, centre - painter.px(10.0), bar, painter.px(20.0)),
            bar / 2.0,
            colours.accent.faded(lit),
        );
    }
    if hover > 0.0 && !current {
        painter.fill(at, radius, colours.surface_3.faded(hover * 0.8));
    }
    let ink = colours.accent.mixed_with(colours.text, lit);
    painter.icon(
        icon,
        icon_at,
        colours.accent.mixed_with(colours.text_soft, lit),
    );
    painter.text(
        label,
        if current {
            painter.body().in_bold()
        } else {
            painter.body()
        },
        ink,
        words,
    );
    if painter.focused(&target) {
        painter.focus_ring(at, radius);
    }
    painter.answers(target, at);
}

/// The person at this computer: with an account, its name and its server;
/// without one, the name Windows knows them by.
fn person_row(painter: &mut Painter, at: Rect) {
    let colours = painter.colours;
    let (name, server) = painter.seen.with_person();
    let target = Target::Person;
    let hover = painter.hover_of(&target);
    let radius = painter.px(10.0);
    if hover > 0.0 {
        painter.fill(at, radius, colours.surface_3.faded(hover * 0.8));
    }
    let disc = painter.px(36.0);
    let (_, centre) = middle(at);
    let avatar = Rect::at(
        at.left + painter.px(design::SPACE_2),
        centre - disc / 2.0,
        disc,
        disc,
    );
    painter.avatar(avatar, &words::initials(&name));
    let chevron = painter.px(16.0);
    let words_left = avatar.right + painter.px(design::SPACE_3);
    let words_width = (at.right - chevron - painter.px(design::SPACE_3) - words_left).max(0.0);
    let body = painter.body().in_bold().ellipsized();
    let caption = painter.caption().ellipsized();
    let (name_height, server_height) = (painter.line(body), painter.line(caption));
    match server {
        Some(server) => {
            let top = centre - (name_height + server_height) / 2.0;
            painter.text(
                &name,
                body,
                colours.text,
                Rect::at(words_left, top, words_width, name_height),
            );
            painter.text(
                &server,
                caption,
                colours.text_soft,
                Rect::at(words_left, top + name_height, words_width, server_height),
            );
        }
        None => painter.text(
            &name,
            body,
            colours.text,
            Rect::at(words_left, at.top, words_width, size(at).1),
        ),
    }
    painter.icon(
        &icons::CHEVRON,
        Rect::at(
            at.right - chevron - painter.px(design::SPACE_2),
            centre - chevron / 2.0,
            chevron,
            chevron,
        ),
        colours.text_faint,
    );
    if painter.focused(&target) {
        painter.focus_ring(at, radius);
    }
    painter.answers_with_tip(target, at, say!("nav.account_tip"));
}

/// The bar on the left, unfolded over a narrow window, and the veil
/// behind it that folds it back.
fn drawer(painter: &mut Painter, width: f32, height: f32, covered: bool) {
    let open = painter.state.drawer;
    let shown = painter.ease(&Target::Drawer, 1, if open { 1.0 } else { 0.0 }, STEADY);
    if shown <= 0.0 {
        return;
    }
    painter.living(open && !covered, |painter| {
        let whole = Rect::at(0.0, 0.0, width, height);
        painter.fill(
            whole,
            0.0,
            zyr_draw::design::Colour::BLACK.faded(VEIL * shown),
        );
        painter.answers(Target::Fold, whole);
        let wide = painter.px(SIDEBAR);
        let at = Rect::at(-wide * (1.0 - shown), 0.0, wide, height);
        painter.shadow(at, 0.0, painter.colours.shadow_2);
        sidebar(painter, at);
    });
}

/// How much black a veil lays over what it covers.
pub const VEIL: f32 = 0.45;

/// What the person has just been told, at the bottom of the window: good
/// news goes away by itself, a trouble stays until it is closed.
fn notice(painter: &mut Painter, width: f32, height: f32) {
    let Some((text, trouble, since)) = painter
        .state
        .notice
        .as_ref()
        .map(|notice| (notice.text.clone(), notice.trouble, notice.since))
    else {
        return;
    };
    if !trouble && since.elapsed() >= NEWS_TIME {
        painter.state.notice = None;
        return;
    }
    if !trouble {
        painter.state.wake(since + NEWS_TIME);
    }
    let colours = painter.colours;
    let shown = painter.arriving(&format!("notice {since:?}"), STEADY);
    let inside = painter.px(design::SPACE_4);
    let icon = painter.px(20.0);
    let close = painter.px(32.0);
    let wide = (width - painter.px(design::SPACE_6) * 2.0).min(painter.px(560.0));
    let words_width = wide
        - inside * 2.0
        - icon
        - painter.px(design::SPACE_3)
        - if trouble {
            close + painter.px(design::SPACE_2)
        } else {
            0.0
        };
    let pen = painter.body();
    let words_height = painter.height_of(&text, pen, words_width);
    let tall = (words_height + painter.px(design::SPACE_3) * 2.0).max(painter.px(52.0));
    let at = Rect::at(
        (width - wide) / 2.0,
        height - painter.px(design::SPACE_5) - tall + painter.px(12.0) * (1.0 - shown),
        wide,
        tall,
    );
    let radius = painter.px(design::RADIUS);
    painter.shadow(at, radius, colours.shadow_2);
    painter.fill(at, radius, colours.surface_1);
    painter.stroke(at, radius, colours.border_strong);
    let tone = if trouble {
        colours.error
    } else {
        colours.online
    };
    let (_, centre) = middle(at);
    painter.icon(
        if trouble {
            &icons::TROUBLE
        } else {
            &icons::DONE
        },
        Rect::at(at.left + inside, centre - icon / 2.0, icon, icon),
        tone,
    );
    let words_left = at.left + inside + icon + painter.px(design::SPACE_3);
    painter.text(
        &text,
        pen,
        colours.text,
        Rect::at(
            words_left,
            centre - words_height / 2.0,
            words_width,
            words_height,
        ),
    );
    if trouble {
        painter.icon_button(
            Rect::at(
                at.right - painter.px(design::SPACE_2) - close,
                centre - close / 2.0,
                close,
                close,
            ),
            &icons::CROSS,
            Target::DismissNotice,
            say!("dialog.close"),
        );
    }
}

/// The bubble over what the hand is on, once it has rested there.
fn bubble(painter: &mut Painter, width: f32, height: f32) {
    let Some((tip, under)) = painter
        .state
        .hover
        .and_then(|at| painter.state.hits.get(at))
        .and_then(|hit| hit.tip.clone().map(|tip| (tip, hit.at)))
    else {
        return;
    };
    if painter.state.pressed.is_some() {
        return;
    }
    let due = painter.state.hover_since + BUBBLE_WAITS;
    if Instant::now() < due {
        painter.state.wake(due);
        return;
    }
    let colours = painter.colours;
    let shown = painter.arriving(
        &format!("bubble {tip} {:?}", painter.state.hover_since),
        QUICK,
    );
    let pen = painter.caption().aligned(Align::Centre);
    let inside = painter.px(design::SPACE_3);
    let most = painter.px(280.0);
    let words = painter.width_of(&tip, pen).min(most);
    let words_height = painter.height_of(&tip, pen, words + painter.px(1.0));
    let wide = words + inside * 2.0;
    let tall = words_height + painter.px(design::SPACE_2) * 2.0;
    let gap = painter.px(design::SPACE_2);
    let (centre_x, _) = middle(under);
    let above = under.top - gap - tall;
    let top = if above >= painter.px(design::SPACE_2) {
        above
    } else {
        under.bottom + gap
    };
    let left = (centre_x - wide / 2.0).clamp(
        painter.px(design::SPACE_2),
        (width - wide - painter.px(design::SPACE_2)).max(0.0),
    );
    let at = Rect::at(left, top.min(height - tall), wide, tall);
    let radius = painter.px(design::RADIUS_SMALL);
    painter.shadow(at, radius, colours.shadow_2);
    painter.fill(at, radius, colours.text.faded(0.92 * shown));
    painter.text(
        &tip,
        pen,
        colours.background.faded(shown),
        at.grown(-painter.px(1.0)),
    );
}
