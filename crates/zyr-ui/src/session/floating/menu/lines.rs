//! What the card is made of: its lines, the settings some of them carry,
//! and the four readings at its top.

use super::*;

/// A line of the card.
///
/// The card is described first and drawn afterwards: measuring its width
/// needs all of its lines known before a single one is laid down, and the
/// card is as wide as its longest line.
///
/// Every word a line carries is the key of its text: the card is fixed
/// when the program is built, and the words are said where it is drawn,
/// in the person's language.
pub(super) enum Line {
    /// What the session costs: four numbers and a sentence.
    Measures,
    /// A stroke between two groups.
    Separator,
    /// What the menu has just refused to do, and why.
    Refusal,
    /// A line that is clicked.
    Entry(Entry),
    /// A line that carries a choice between two sides.
    Toggle(Toggle),
    /// A line that carries a few values side by side.
    Choice(Choice),
    /// A line that is pushed along a bar.
    Slider(Slider),
    /// A line that opens a list of its own.
    List(List),
}

/// A line that carries a few values with no order between them.
///
/// Buttons and not a bar: the codec is not a scale, it is a few names,
/// one of them an "Automatic" that is not a value but a renunciation,
/// and pushing a slider would promise a more and a less that do not
/// exist.
pub(super) struct Choice {
    pub(super) icon: &'static Icon,
    pub(super) label: &'static str,
    pub(super) setting: Setting,
}

/// A line set by pushing a slider, with the value written above it.
///
/// A scale: bigger, faster, and the right notch on it is found by
/// watching the picture move. The notches come from the product, one per
/// megabit, and the slider goes from zero to the number of values minus
/// one: it pushes ranks and not numbers, like the other list lines.
pub(super) struct Slider {
    pub(super) icon: &'static Icon,
    pub(super) label: &'static str,
    pub(super) setting: Setting,
}

/// A line that opens a list of its own, beside the card.
///
/// A list rather than a bar, for two reasons: its first entries are
/// not numbers but say which of the two computers decides, which no
/// bar can say, and there are fifteen of them below, which makes
/// notches one can no longer aim at.
pub(super) struct List {
    pub(super) icon: &'static Icon,
    pub(super) label: &'static str,
    pub(super) setting: Setting,
}

/// A line that carries a choice rather than an action.
///
/// Both words are there and the one in place is lit: the old line
/// announced what the click would do and never where things stood, and
/// the two modes cannot be told apart by eye on a still desktop. It has
/// to be seen without reading.
///
/// The line itself is not clicked, only its two sides: no hand under the
/// pointer and no lit background over the rest, which would promise a
/// click that does nothing.
pub(super) struct Toggle {
    pub(super) icon: &'static Icon,
    pub(super) label: &'static str,
    /// The two sides, in the order they are read. The second is the one
    /// that stands for "yes".
    pub(super) sides: [&'static str; 2],
    /// What the session is asked for to switch sides.
    pub(super) act: Act,
    /// Where things stand: true for the right-hand side.
    state: &'static AtomicBool,
}

/// An entry of the menu: an icon, a word, what is written to its right,
/// and what it asks for.
pub(super) struct Entry {
    pub(super) icon: &'static Icon,
    pub(super) label: &'static str,
    pub(super) trailing: Trailing,
    pub(super) does: Does,
    /// Written in the colour of things that cannot be undone. Only one
    /// line of the menu is, and it is the one that cuts the session off.
    pub(super) destructive: bool,
}

/// What is written to the right of a line, by the key of its words when
/// there are any.
pub(super) enum Trailing {
    /// What the line does, spelled out, or nothing.
    Text(Option<&'static str>),
    /// The combination in place for it, or these words here as long as
    /// nobody has given it one.
    Key(Doing, Option<&'static str>),
}

/// What a line asks for when it is clicked.
#[derive(Clone, Copy)]
pub(super) enum Does {
    /// What the session can do, in its own language.
    Session(Act),
    /// Put the button away until the shortcut calls it back.
    PutAway,
}

/// One of the settings the session carries.
///
/// Named here as the product names it on both sides: that is the word
/// that travels to the service, and having a second one for display
/// would be two names for one setting.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Setting {
    Size,
    Screen,
    Bitrate,
    Codec,
    Steady,
}

/// What the card holds, in order.
pub(super) const LINES: [Line; 21] = [
    Line::Measures,
    // Just under the readings, so at the top of what is read: what has
    // just been refused is read before whatever was going to be clicked
    // next. It takes no room at all while there is nothing to say.
    Line::Refusal,
    Line::Separator,
    Line::Entry(Entry {
        icon: &icons::FULL_SCREEN,
        label: key!("menu.fullscreen"),
        trailing: Trailing::Key(Doing::Fullscreen, None),
        does: Does::Session(Act::Fullscreen),
        destructive: false,
    }),
    Line::Entry(Entry {
        icon: &icons::STATISTICS,
        label: key!("menu.figures"),
        trailing: Trailing::Text(None),
        does: Does::Session(Act::Stats),
        destructive: false,
    }),
    Line::Toggle(Toggle {
        icon: &icons::LINK,
        label: key!("menu.badges"),
        sides: [key!("menu.badges_when_needed"), key!("menu.badges_held")],
        act: Act::Badges,
        state: &HELD,
    }),
    Line::Toggle(Toggle {
        icon: &icons::MOUSE,
        label: key!("menu.mouse"),
        sides: [key!("menu.mouse_desktop"), key!("menu.mouse_game")],
        act: Act::MouseMode,
        state: &IN_GAME,
    }),
    Line::Toggle(Toggle {
        icon: &icons::SOUND,
        label: key!("menu.sound"),
        sides: [key!("menu.sound_on"), key!("menu.sound_muted")],
        act: Act::Sound,
        state: &MUTED,
    }),
    Line::Toggle(Toggle {
        icon: &icons::KEYBOARD,
        label: key!("menu.keyboard"),
        sides: [
            key!("menu.keyboard_shared"),
            key!("menu.keyboard_immersive"),
        ],
        act: Act::SystemKeys,
        state: &IMMERSIVE,
    }),
    Line::Toggle(Toggle {
        icon: &icons::CLIPBOARD,
        label: key!("menu.clipboard"),
        sides: [
            key!("menu.clipboard_each_its_own"),
            key!("menu.clipboard_shared"),
        ],
        act: Act::Clipboard,
        state: &SHARED,
    }),
    Line::Entry(Entry {
        icon: &icons::CAD,
        label: key!("menu.ctrl_alt_del"),
        trailing: Trailing::Text(Some(key!("menu.on_the_far_computer"))),
        does: Does::Session(Act::SecureAttention),
        destructive: false,
    }),
    Line::Entry(Entry {
        icon: &icons::LOCK,
        label: key!("menu.lock"),
        trailing: Trailing::Text(Some(key!("menu.the_far_computer"))),
        does: Does::Session(Act::LockScreen),
        destructive: false,
    }),
    Line::Separator,
    Line::List(List {
        icon: &icons::RESOLUTION,
        label: key!("menu.resolution"),
        setting: Setting::Size,
    }),
    Line::List(List {
        icon: &icons::HOST_SCREEN,
        label: key!("menu.host_screen"),
        setting: Setting::Screen,
    }),
    Line::Slider(Slider {
        icon: &icons::BITRATE,
        label: key!("menu.bitrate"),
        setting: Setting::Bitrate,
    }),
    Line::Choice(Choice {
        icon: &icons::CODEC,
        label: key!("menu.codec"),
        setting: Setting::Codec,
    }),
    Line::Choice(Choice {
        icon: &icons::FAR_SCREEN,
        label: key!("menu.far_screen"),
        setting: Setting::Steady,
    }),
    Line::Separator,
    Line::Entry(Entry {
        icon: &icons::HIDE,
        label: key!("menu.hide_the_button"),
        trailing: Trailing::Key(Doing::Menu, Some(key!("menu.until_the_end"))),
        does: Does::PutAway,
        destructive: false,
    }),
    Line::Entry(Entry {
        icon: &icons::QUIT,
        label: key!("menu.end_the_session"),
        trailing: Trailing::Key(Doing::End, Some(key!("menu.hands_the_desktop_back"))),
        does: Does::Session(Act::End),
        destructive: true,
    }),
];

/// One of the bar's four figures: what it costs, how it reads, and
/// where it is taken from in what the player measures.
pub(super) struct Reading {
    /// The key of its word.
    pub(super) label: &'static str,
    unit: &'static str,
    /// How many decimals: the network reads in whole milliseconds, the
    /// rest to the hundredth.
    decimals: usize,
    read: fn(&Measures) -> Option<f64>,
}

/// The four readings, in the order they are read: what a frame costs
/// here, what it cost over there, what lies between the two, and what the
/// wire really carries.
///
/// The same words and the same units as the banner of figures, because
/// they are the same readings: two bars that do not say the same thing
/// about the same engine would be two engines.
pub(super) const READINGS: [Reading; 4] = [
    Reading {
        label: key!("figures.decode"),
        unit: "ms",
        decimals: 2,
        read: |said| said.decode_ms,
    },
    Reading {
        label: key!("figures.host"),
        unit: "ms",
        decimals: 2,
        read: |said| said.host_ms,
    },
    Reading {
        label: key!("figures.network"),
        unit: "ms",
        decimals: 0,
        read: |said| said.network_ms,
    },
    Reading {
        label: key!("figures.bitrate"),
        unit: "Mb/s",
        decimals: 2,
        read: |said| said.bitrate_mbps,
    },
];

/// How long a missing reading keeps what it was saying.
///
/// One of these four is sometimes missing for one second and back the
/// next: what the far computer measures does not travel with every
/// frame, and a second can go by without any frame carrying it. Wiped at
/// once, the reading flickers between a number and a dash, and a
/// flickering number is harder to read than a number one second late,
/// which is what is being read anyway, since these four are averages
/// over the second just gone.
///
/// Three seconds and no more: beyond that it is no longer a reading that
/// skipped but a measurement that no longer exists, and the dash then
/// tells the truth.
const KEEP_FOR: std::time::Duration = std::time::Duration::from_secs(3);

/// What the readings bar shows: four numbers already written out and the
/// stream's sentence.
///
/// Written out where they are read rather than kept as numbers: the
/// formatting then happens once a second and not once a frame, and the
/// drawing thread only has to lay down text.
pub(super) struct ReadingsBar {
    pub(super) figures: [String; 4],
    /// When each one was really read, and not copied over from the
    /// reading before. Left out of every comparison: these instants move
    /// at every turn without anything reading differently.
    read_at: [Option<Instant>; 4],
    pub(super) stream: String,
}

impl ReadingsBar {
    pub(super) const fn empty() -> Self {
        ReadingsBar {
            figures: [String::new(), String::new(), String::new(), String::new()],
            read_at: [None; 4],
            stream: String::new(),
        }
    }

    /// What a read of the player shows, with the previous one at hand.
    ///
    /// The previous one because a missing reading keeps what it was
    /// saying for a while rather than being wiped; see `KEEP_FOR`.
    pub(super) fn of(readings: &Measures, before: &ReadingsBar, now: Instant) -> Self {
        let mut figures: [String; 4] = std::array::from_fn(|_| String::new());
        let mut read_at = [None; 4];
        for (rank, reading) in READINGS.iter().enumerate() {
            if let Some(number) = (reading.read)(readings) {
                figures[rank] =
                    crate::session::statistics::written(number, reading.decimals, reading.unit);
                read_at[rank] = Some(now);
                continue;
            }
            match before.read_at[rank] {
                Some(when) if now.duration_since(when) < KEEP_FOR => {
                    figures[rank].clone_from(&before.figures[rank]);
                    read_at[rank] = Some(when);
                }
                _ => figures[rank] = crate::session::statistics::NOTHING.to_string(),
            }
        }
        ReadingsBar {
            figures,
            read_at,
            stream: crate::session::statistics::stream(readings),
        }
    }

    /// Whether what is read has changed, leaving the
    /// instants aside.
    pub(super) fn reads_differently(&self, other: &ReadingsBar) -> bool {
        self.figures != other.figures || self.stream != other.stream
    }
}

impl Setting {
    /// The name it travels under, on both sides.
    pub(super) fn name(self) -> &'static str {
        match self {
            Setting::Size => "asked",
            Setting::Screen => "screen",
            Setting::Bitrate => "bitrate",
            Setting::Codec => "codec",
            Setting::Steady => "steady",
        }
    }

    /// The values on offer, in the product's order.
    pub(super) fn values(self, menu: &SessionMenu) -> Vec<String> {
        match self {
            Setting::Size => menu.sizes.iter().map(|size| size.value.clone()).collect(),
            Setting::Screen => menu
                .screens
                .iter()
                .map(|screen| screen.id.clone())
                .collect(),
            Setting::Bitrate => menu.rates.iter().map(u32::to_string).collect(),
            Setting::Codec => menu.codecs.clone(),
            // Two words and not a list: it is a switch, and its two
            // sides are named in the window like the ones next to it.
            Setting::Steady => vec!["off".to_string(), "on".to_string()],
        }
    }

    /// What is written for this value, where it is chosen.
    pub(super) fn label(self, menu: &SessionMenu, value: &str) -> String {
        match self {
            Setting::Size => match value {
                "client" => zyr_i18n::say!("menu.client_resolution"),
                "host" => zyr_i18n::say!("menu.host_resolution"),
                _ => menu
                    .sizes
                    .iter()
                    .find(|size| size.value == value)
                    .map_or_else(|| value.to_string(), in_pixels),
            },
            Setting::Screen => menu
                .screens
                .iter()
                .find(|screen| screen.id == value)
                .map_or_else(
                    || value.to_string(),
                    |screen| {
                        if screen.main {
                            zyr_i18n::say!("menu.main_screen", name = screen.name)
                        } else {
                            screen.name.clone()
                        }
                    },
                ),
            Setting::Bitrate => format!(
                "{} Mb/s",
                (value.parse::<f64>().unwrap_or(0.0) / 1000.0).round()
            ),
            Setting::Codec => {
                if value == "auto" {
                    zyr_i18n::say!("menu.codec_auto")
                } else {
                    value.to_string()
                }
            }
            Setting::Steady => {
                if value == "on" {
                    zyr_i18n::say!("menu.far_screen_smooth")
                } else {
                    zyr_i18n::say!("menu.far_screen_thrifty")
                }
            }
        }
    }

    /// What is written to the right of the menu line, when the value
    /// in place is not already read there.
    pub(super) fn summary(self, menu: &SessionMenu) -> String {
        let current = self.current(menu);
        match self {
            // What the choice really comes to here: "client" does not say
            // whether 4K or 1080p is being asked for, and that is exactly
            // what one wants to know before opening the session.
            Setting::Size => {
                if current == "host" {
                    return zyr_i18n::say!("menu.host_resolution_short");
                }
                let pixels = menu
                    .sizes
                    .iter()
                    .find(|size| size.value == current)
                    .map_or_else(|| current.clone(), in_pixels);
                if current == "client" {
                    zyr_i18n::say!("menu.client_resolution_short", pixels = pixels)
                } else {
                    pixels
                }
            }
            // The name alone: "(main)" would take the name's room there
            // without teaching anything, since the list already says it.
            Setting::Screen => menu
                .screens
                .iter()
                .find(|screen| screen.id == current)
                .map_or_else(String::new, |screen| screen.name.clone()),
            _ => self.label(menu, &current),
        }
    }

    /// What is written in the list's right-hand column.
    pub(super) fn aside(self, menu: &SessionMenu, value: &str) -> String {
        match self {
            // The size's ratio, said the way screens are sold: two
            // numbers compare badly, and 21:9 next to 16:9 says at once
            // what is going to be cut. Nothing for the first two: what
            // they come to depends on the screen one is facing.
            Setting::Size if value != "client" && value != "host" => menu
                .sizes
                .iter()
                .find(|size| size.value == value)
                .filter(|size| size.width > 0)
                .map_or_else(String::new, |size| ratio(size.width, size.height)),
            // The screen's size, as the ratio is for the resolution: two
            // screens are told apart by that first, and a model name
            // says nothing to anyone who did not buy it.
            Setting::Screen => menu
                .screens
                .iter()
                .find(|screen| screen.id == value)
                .map_or_else(String::new, |screen| {
                    format!("{}x{}", screen.wide, screen.high)
                }),
            _ => String::new(),
        }
    }

    /// Where things stand.
    pub(super) fn current(self, menu: &SessionMenu) -> String {
        match self {
            Setting::Size => menu.now.asked.clone(),
            Setting::Screen => menu.now.screen.clone(),
            Setting::Bitrate => menu.now.bitrate_kbps.to_string(),
            Setting::Codec => menu.now.codec.clone(),
            Setting::Steady => if menu.now.steady { "on" } else { "off" }.to_string(),
        }
    }

    /// What the far machine said it cannot do.
    ///
    /// Nothing at all means it has said nothing, never that it can do
    /// nothing: outside a session, or while its engine is starting, the
    /// question has no answer, and a question with no answer must leave
    /// the menu exactly as it was.
    pub(super) fn out_of_reach(self, menu: &SessionMenu, value: &str) -> bool {
        self == Setting::Codec && menu.beyond_it.iter().flatten().any(|other| other == value)
    }
}

/// A size, in pixels.
fn in_pixels(size: &Offered) -> String {
    format!("{}x{}", size.width, size.height)
}

/// A size's ratio, reduced as it reads on a screen's spec sheet.
///
/// Worked out rather than written next to each number: a second table
/// would drift from the first the day a size is added. The two ratios
/// nobody writes in their reduced form are said the way everybody says
/// them.
fn ratio(width: u32, top: u32) -> String {
    fn gcd(a: u32, b: u32) -> u32 {
        if b == 0 { a } else { gcd(b, a % b) }
    }

    let divisor = gcd(width, top).max(1);
    match (width / divisor, top / divisor) {
        (8, 5) => "16:10".to_string(),
        (683, 384) => "16:9".to_string(),
        (x, y) => format!("{x}:{y}"),
    }
}

impl Trailing {
    /// What is written, once the shortcuts are known.
    pub(super) fn text(&self) -> String {
        let words = |label: &Option<&str>| label.map(zyr_i18n::text).unwrap_or_default();
        match self {
            Trailing::Text(label) => words(label),
            Trailing::Key(doing, otherwise) => KEYS
                .lock()
                .expect("menu's shortcuts")
                .iter()
                .find(|(other, _)| other == doing)
                .and_then(|(_, said)| said.clone())
                .unwrap_or_else(|| words(otherwise)),
        }
    }
}

impl Line {
    /// The height this line takes, in real pixels.
    pub(super) fn height(&self, scale: f32) -> f32 {
        match self {
            Line::Measures => readings_height(scale),
            Line::Separator => (design::SPACE_2 * 2.0 + layout::HAIRLINE) * scale,
            // Measured when drawing, where there is what it takes to
            // measure wrapped text, and read back here as the card's
            // width is.
            Line::Refusal => load(&REFUSAL_HEIGHT),
            Line::Slider(_) => slider_height(scale),
            _ => layout::LINE * scale,
        }
    }

    /// Whether this line has a reason to be there right now.
    ///
    /// A far machine with only one screen, or whose engine has not yet
    /// said which ones, leaves nothing to choose: the line goes away
    /// rather than open an empty list.
    pub(super) fn is_visible(&self, menu: Option<&SessionMenu>) -> bool {
        // With no refusal to say, the line is not there at all: it must
        // cost nothing the nine hundred and ninety-nine times when all
        // goes well.
        if matches!(self, Line::Refusal) {
            return refusal_to_say().is_some();
        }
        let Some(menu) = menu else {
            // With no answer, the card shrinks to what does not depend
            // on the session: a short card is better than a card of
            // empty lines.
            return !matches!(self, Line::Choice(_) | Line::Slider(_) | Line::List(_));
        };
        match self {
            Line::List(list) => !list.setting.values(menu).is_empty(),
            _ => true,
        }
    }
}

impl Toggle {
    /// What is written on its two sides.
    pub(super) fn words(&self) -> Vec<String> {
        self.sides
            .iter()
            .map(|label| zyr_i18n::text(label))
            .collect()
    }

    /// Which of the two is in place.
    pub(super) fn current_side(&self) -> usize {
        usize::from(self.state.load(Ordering::Relaxed))
    }
}

/// What is written on the sides of a choice line.
///
/// Apart from the line so that it can be asked for with the settings
/// already in hand: asking for them again at that moment would take
/// again a lock that is already held.
pub(super) fn words_of(menu: &SessionMenu, setting: Setting) -> Vec<String> {
    setting
        .values(menu)
        .iter()
        .map(|value| setting.label(menu, value))
        .collect()
}

impl Choice {
    /// What is written on its sides, as the session offers them.
    pub(super) fn words(&self) -> Option<Vec<String>> {
        let session_menu = SESSION_MENU.lock().expect("menu's settings");
        Some(words_of(session_menu.as_ref()?, self.setting))
    }

    /// Which one is in place, and the ones the far machine cannot do.
    pub(super) fn current(&self) -> Option<(usize, Vec<bool>)> {
        let session_menu = SESSION_MENU.lock().expect("menu's settings");
        let menu = session_menu.as_ref()?;
        let values = self.setting.values(menu);
        let current = self.setting.current(menu);
        Some((
            values.iter().position(|value| *value == current)?,
            values
                .iter()
                .map(|value| self.setting.out_of_reach(menu, value))
                .collect(),
        ))
    }
}

impl Slider {
    /// The notch it is at: the one a hand is holding, otherwise the one
    /// that is written.
    pub(super) fn notch(&self) -> Option<(usize, usize)> {
        let session_menu = SESSION_MENU.lock().expect("menu's settings");
        let menu = session_menu.as_ref()?;
        let values = self.setting.values(menu);
        if values.is_empty() {
            return None;
        }
        let current = self.setting.current(menu);
        let written = values
            .iter()
            .position(|value| *value == current)
            .unwrap_or(0);
        let pushed = *PUSHED.lock().expect("menu's slider");
        Some((
            pushed.unwrap_or(written).min(values.len() - 1),
            values.len(),
        ))
    }

    /// What is written to the right of its word: what it is worth at
    /// the notch it is at, including while a hand is pushing it.
    pub(super) fn value(&self) -> String {
        let session_menu = SESSION_MENU.lock().expect("menu's settings");
        let Some(menu) = session_menu.as_ref() else {
            return String::new();
        };
        let values = self.setting.values(menu);
        match *PUSHED.lock().expect("menu's slider") {
            Some(notch) if notch < values.len() => self.setting.label(menu, &values[notch]),
            _ => self.setting.summary(menu),
        }
    }
}
