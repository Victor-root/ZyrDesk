//! The page of contacts: the requests either way, then each contact with
//! the computers of the account, one by one, lent to them or not, and
//! what they may do on each.
//!
//! A contact can do nothing until a computer is lent to them: that is
//! the page's whole shape, a switch per computer and, under it, what that
//! computer lets them do.

use zyr_control::{Contact, Device};
use zyr_draw::icons::{self, Icon};
use zyr_draw::{Rect, design};
use zyr_i18n::say;
use zyr_proto::rights::{Right, Rights};

use crate::home::pages::computers::empty_place;
use crate::home::pages::{Control, Head, line, page_title, section, subheading};
use crate::home::seen::{AccountState, Seen};
use crate::home::state::{Lever, State, Target};
use crate::home::ui::{Button, Kind, Painter, SWITCH_SIZE, TAG_HEIGHT, size};

/// The widest the page goes, like the settings.
const WIDEST: f32 = 860.0;

/// Draws the page within `at`, from its top, and gives back the height it
/// takes.
pub fn draw(painter: &mut Painter, at: Rect) -> f32 {
    let seen = painter.seen;
    let width = size(at).0.min(painter.px(WIDEST));
    let at = Rect {
        right: at.left + width,
        ..at
    };
    let linked = seen
        .account
        .as_ref()
        .filter(|account| account.link.is_some());

    // The way to invite someone, beside the title.
    let invite = say!("home.invite");
    let button_width = painter.button_width(&invite, Some(&icons::USER_PLUS));
    let title_width = if linked.is_some() {
        width - button_width - painter.px(design::SPACE_4)
    } else {
        width
    };
    let mut y = at.top
        + page_title(
            painter,
            Rect::at(at.left, at.top, title_width, 0.0),
            &say!("contacts.title"),
            &say!("contacts.lead"),
        );
    let Some(account) = linked else {
        y += empty_place(
            painter,
            Rect::at(at.left, y, width, painter.px(240.0)),
            (
                &icons::CONTACTS,
                &say!("contacts.no_account"),
                &say!("contacts.no_account_text"),
            ),
            Some((&say!("signin.sign_in"), &icons::LOGIN, Target::SignIn)),
        );
        return y - at.top;
    };
    painter.button(
        Rect::at(
            at.right - button_width,
            at.top,
            button_width,
            painter.px(38.0),
        ),
        Target::Invite,
        Button::new(&invite, Kind::Primary).with_icon(&icons::USER_PLUS),
    );

    let gap = painter.px(design::SPACE_5);
    let asked: Vec<&Contact> = account
        .contacts
        .iter()
        .filter(|contact| !contact.accepted)
        .collect();
    if !asked.is_empty() {
        y += section(
            painter,
            Rect::at(at.left, y, width, 0.0),
            &Head {
                icon: &icons::USER_PLUS,
                title: say!("contacts.requests"),
                lead: None,
            },
            |painter, room| requests(painter, room, &asked),
        ) + gap;
    }

    let accepted: Vec<&Contact> = account
        .contacts
        .iter()
        .filter(|contact| contact.accepted)
        .collect();
    if accepted.is_empty() {
        y += empty_place(
            painter,
            Rect::at(at.left, y, width, painter.px(220.0)),
            (
                &icons::CONTACTS,
                &say!("contacts.none"),
                &say!("contacts.none_text"),
            ),
            None,
        );
        return y - at.top;
    }
    for contact in accepted {
        let presence = if contact.online {
            say!("computer.online")
        } else {
            say!("computer.offline")
        };
        y += section(
            painter,
            Rect::at(at.left, y, width, 0.0),
            &Head {
                icon: &icons::CONTACTS,
                title: contact.username.clone(),
                lead: Some(presence),
            },
            |painter, room| lending(painter, room, account, contact),
        ) + gap;
    }
    y - at.top
}

/// The requests waiting, either way, one to a line.
fn requests(painter: &mut Painter, room: Rect, asked: &[&Contact]) -> f32 {
    let (left, width) = (room.left, size(room).0);
    let mut y = room.top;
    for contact in asked {
        let (caption, buttons) = if contact.asked_by_me {
            (
                say!("contacts.waiting"),
                vec![(
                    say!("contacts.withdraw"),
                    Kind::Quiet,
                    Target::RemoveContact(contact.id.clone()),
                    None,
                )],
            )
        } else {
            (
                say!("contacts.asks_you"),
                vec![
                    (
                        say!("contacts.accept"),
                        Kind::Primary,
                        Target::Answer(contact.id.clone(), true),
                        Some(&icons::CHECK),
                    ),
                    (
                        say!("contacts.decline"),
                        Kind::Quiet,
                        Target::Answer(contact.id.clone(), false),
                        None,
                    ),
                ],
            )
        };
        y += line(
            painter,
            (left, y, width),
            Some(&icons::CONTACTS),
            &contact.username,
            &caption,
            Control::Buttons(buttons),
        );
    }
    y - room.top
}

/// The computers of the account, each lent to this contact or not, then
/// the way to end the contact.
fn lending(painter: &mut Painter, room: Rect, account: &AccountState, contact: &Contact) -> f32 {
    let (left, width) = (room.left, size(room).0);
    let mut y = room.top;
    y += subheading(painter, (left, y, width), &say!("contacts.computers"));
    if account.devices.is_empty() {
        y += painter.paragraph(
            &say!("settings.devices_later"),
            painter.caption(),
            painter.colours.text_soft,
            (left, y, width),
        ) + painter.px(design::SPACE_3);
    }
    for device in &account.devices {
        y += computer(painter, (left, y, width), device, contact);
    }
    let remove = Target::RemoveContact(contact.id.clone());
    let armed = painter.state.is_armed(&remove);
    y += line(
        painter,
        (left, y, width),
        None,
        &say!("contacts.end"),
        &say!("contacts.end_caption", name = contact.username),
        Control::Buttons(vec![(
            if armed {
                say!("dialog.confirm")
            } else {
                say!("contacts.remove")
            },
            if armed { Kind::Warning } else { Kind::Ghost },
            remove,
            None,
        )]),
    );
    y - room.top
}

/// One computer of the account and what this contact may do on it: the
/// switch that lends it, and once lent, a row of what it lets them do.
fn computer(
    painter: &mut Painter,
    (left, top, width): (f32, f32, f32),
    device: &Device,
    contact: &Contact,
) -> f32 {
    let lent = lent_now(painter.seen, painter.state, &device.id, &contact.username);
    let name = if device.this {
        say!("settings.this_device", name = device.name)
    } else {
        device.name.clone()
    };
    let caption = match lent {
        None => say!("contacts.not_lent"),
        Some(rights) if rights.unattended => {
            say!("contacts.lent_unattended", name = contact.username)
        }
        Some(_) => say!("contacts.lent", name = contact.username),
    };
    let lever = |lever| Target::Lend(device.id.clone(), contact.username.clone(), lever);
    let mut height = line(
        painter,
        (left, top, width),
        Some(if device.this {
            &icons::COMPUTERS
        } else {
            &icons::LAPTOP
        }),
        &name,
        &caption,
        Control::Flip(lent.is_some(), lever(Lever::Lent)),
    );
    if let Some(rights) = lent {
        height += rights_row(
            painter,
            (left, top + height, width),
            rights,
            &contact.username,
            &lever,
        );
    }
    height
}

/// One thing a lent computer lets a contact do, as its row shows it: where
/// its switch stands, if the product carries it yet, and what its bubble
/// says.
struct Allowed {
    icon: &'static Icon,
    label: String,
    switch: Option<(bool, Right)>,
    tip: Option<String>,
}

/// What a lent computer lets the contact do, as switches side by side,
/// wrapping when the page is narrow. The gamepad is said, and waits for
/// the product to carry one.
fn rights_row(
    painter: &mut Painter,
    (left, top, width): (f32, f32, f32),
    rights: Rights,
    name: &str,
    lever: &dyn Fn(Lever) -> Target,
) -> f32 {
    let colours = painter.colours;
    let indent = painter.px(36.0 + design::SPACE_4);
    let switch = (painter.px(SWITCH_SIZE.0), painter.px(SWITCH_SIZE.1));
    let pen = painter.caption();
    let gap = painter.px(design::SPACE_2);
    let apart = painter.px(design::SPACE_6);
    let row = switch.1.max(painter.line(pen));
    let items = [
        Allowed {
            icon: &icons::KEYBOARD,
            label: say!("contacts.keyboard"),
            switch: Some((rights.keyboard, Right::Keyboard)),
            tip: None,
        },
        Allowed {
            icon: &icons::MOUSE,
            label: say!("contacts.mouse"),
            switch: Some((rights.mouse, Right::Mouse)),
            tip: None,
        },
        Allowed {
            icon: &icons::GAMEPAD,
            label: say!("contacts.gamepad"),
            switch: None,
            tip: None,
        },
        Allowed {
            icon: &icons::SHIELD,
            label: say!("contacts.unattended"),
            switch: Some((rights.unattended, Right::Unattended)),
            tip: Some(say!("contacts.unattended_tip", name = name)),
        },
    ];
    let soon = say!("soon.tip");
    let soon_width = painter.tag_width(&soon, None);
    let mut x = left + indent;
    let mut y = top;
    let side = painter.px(16.0);
    for Allowed {
        icon,
        label,
        switch: state,
        tip,
    } in items
    {
        let words = painter.width_of(&label, pen);
        let control = if state.is_some() {
            switch.0
        } else {
            soon_width
        };
        let item = side + gap + words + gap + control;
        if x + item > left + width && x > left + indent {
            x = left + indent;
            y += row + painter.px(design::SPACE_3);
        }
        let live = state.is_some();
        let ink = if live {
            colours.text_soft
        } else {
            colours.text_faint
        };
        let centre = y + row / 2.0;
        painter.icon(icon, Rect::at(x, centre - side / 2.0, side, side), ink);
        painter.text(
            &label,
            pen,
            ink,
            Rect::at(x + side + gap, y, words + gap, row),
        );
        let control_left = x + side + gap + words + gap;
        let at = match state {
            Some((on, right)) => painter.switch(
                (control_left, centre - switch.1 / 2.0),
                on,
                true,
                lever(Lever::Right(right)),
            ),
            None => painter.tag(
                (control_left, centre - painter.px(TAG_HEIGHT) / 2.0),
                &soon,
                None,
                colours.text_faint,
            ),
        };
        if let Some(tip) = tip {
            painter.tells(Rect { left: x, ..at }, tip);
        }
        x += item + apart;
    }
    y + row + painter.px(design::SPACE_4) - top
}

/// What a computer lends this contact right now: what the hand last
/// asked, until the service has said it, and what the service said
/// otherwise. Nothing when it is not lent.
pub fn lent_now(seen: &Seen, state: &State, device: &str, with: &str) -> Option<Rights> {
    if let Some((_, _, asked)) = state
        .lending
        .iter()
        .find(|(lent, to, _)| lent == device && to == with)
    {
        return *asked;
    }
    seen.account.as_ref().and_then(|account| {
        account
            .lent
            .iter()
            .find(|lent| lent.device == device && lent.with == with)
            .map(|lent| lent.rights)
    })
}

/// The requests waiting for this account's answer.
pub fn asking_me(seen: &Seen) -> usize {
    seen.account.as_ref().map_or(0, |account| {
        account
            .contacts
            .iter()
            .filter(|contact| !contact.accepted && !contact.asked_by_me)
            .count()
    })
}

/// What a contact is to this account, said under their name.
pub fn standing_of(contact: &Contact, lent: usize) -> String {
    if !contact.accepted {
        return if contact.asked_by_me {
            say!("contacts.waiting")
        } else {
            say!("contacts.asks_you")
        };
    }
    let presence = if contact.online {
        say!("computer.online")
    } else {
        say!("computer.offline")
    };
    format!("{presence} · {}", say!("contacts.lent_count", count = lent))
}
