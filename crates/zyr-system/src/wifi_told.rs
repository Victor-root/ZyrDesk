//! What a Wi-Fi card went through and how its connection stood, worded
//! for the journal.
//!
//! What Windows says about a card is a number: a notification code, a
//! signal quality, a standard. This puts each in the words the journal
//! keeps, so that the wording is tried on any system. Asking Windows, and
//! listening to it, is in `wifi.rs`.

use std::fmt::Write as _;

/// What a card's connection went through, as Windows tells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Moved {
    Connecting,
    CouldNotConnect,
    Disconnecting,
    Disconnected,
    RoamingStarted,
    RoamingEnded,
    LinkDegraded,
    LinkImproved,
    RadioChanged,
    /// The card looked around at the other networks.
    Scanned,
    ScanFailed,
    NetworksListed,
}

impl Moved {
    #[cfg(test)]
    const ALL: [Moved; 12] = [
        Moved::Connecting,
        Moved::CouldNotConnect,
        Moved::Disconnecting,
        Moved::Disconnected,
        Moved::RoamingStarted,
        Moved::RoamingEnded,
        Moved::LinkDegraded,
        Moved::LinkImproved,
        Moved::RadioChanged,
        Moved::Scanned,
        Moved::ScanFailed,
        Moved::NetworksListed,
    ];

    /// The card and what happened to it, with the reason Windows gave
    /// when it gave one.
    pub(crate) fn told(self, card: &str, reason: Option<u32>) -> String {
        let what = match self {
            Moved::Connecting => "is connecting to a network",
            Moved::CouldNotConnect => "could not connect to a network",
            Moved::Disconnecting => "is disconnecting from its network",
            Moved::Disconnected => "disconnected from its network",
            Moved::RoamingStarted => "started moving to another access point",
            Moved::RoamingEnded => "finished moving to another access point",
            Moved::LinkDegraded => "reports its link degraded",
            Moved::LinkImproved => "reports its link improved",
            Moved::RadioChanged => "had its radio switched on or off",
            Moved::Scanned => "finished looking around at the other networks",
            Moved::ScanFailed => "failed to look around at the other networks",
            Moved::NetworksListed => "refreshed the list of networks around",
        };
        let mut told = format!("the Wi-Fi card {card} {what}");
        if let Some(reason) = reason.filter(|reason| *reason != 0) {
            let _ = write!(told, " (reason code 0x{reason:08X})");
        }
        told
    }

    /// Whether it is an event of the session, or only the card looking
    /// around, which is for whoever hunts.
    pub(crate) fn is_an_event(self) -> bool {
        !matches!(
            self,
            Moved::Scanned | Moved::ScanFailed | Moved::NetworksListed
        )
    }
}

/// How a card's connection stood at one moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Link {
    /// What Windows makes of the signal, out of a hundred.
    pub(crate) signal: u32,
    /// The strength received, in dBm, when the driver gives it.
    pub(crate) strength: Option<i32>,
    pub(crate) channel: Option<u32>,
    /// The standard the connection speaks, as Windows numbers them.
    pub(crate) standard: i32,
    /// The rates negotiated, in kilobits a second.
    pub(crate) receiving: u32,
    pub(crate) sending: u32,
    /// Whether the access point is another than the one before.
    pub(crate) moved: bool,
}

impl Link {
    pub(crate) fn told(&self, card: &str) -> String {
        let mut told = format!("the Wi-Fi card {card}: signal {} %", self.signal);
        if let Some(dbm) = self.strength {
            let _ = write!(told, ", {dbm} dBm");
        }
        if let Some(channel) = self.channel {
            let _ = write!(told, ", channel {channel}");
        }
        if let Some(standard) = standard_name(self.standard) {
            let _ = write!(told, ", {standard}");
        }
        let _ = write!(
            told,
            ", receiving at {} Mb/s and sending at {} Mb/s",
            self.receiving / 1000,
            self.sending / 1000
        );
        if self.moved {
            told.push_str(", on another access point than a moment ago");
        }
        told
    }
}

/// The 802.11 standard Windows numbers `standard` for.
fn standard_name(standard: i32) -> Option<&'static str> {
    match standard {
        4 => Some("802.11a"),
        5 => Some("802.11b"),
        6 => Some("802.11g"),
        7 => Some("802.11n"),
        8 => Some("802.11ac"),
        9 => Some("802.11ad"),
        10 => Some("802.11ax"),
        11 => Some("802.11be"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_a_card_went_through_is_worded_and_the_reason_given_is_kept() {
        let told = Moved::Disconnected.told("Example Wi-Fi 6", Some(0x38002));
        assert_eq!(
            told,
            "the Wi-Fi card Example Wi-Fi 6 disconnected from its network (reason code \
             0x00038002)"
        );
        // A reason of nought is Windows saying there is none.
        assert_eq!(
            Moved::RoamingStarted.told("Example Wi-Fi 6", Some(0)),
            "the Wi-Fi card Example Wi-Fi 6 started moving to another access point"
        );
        let all: Vec<String> = Moved::ALL
            .iter()
            .map(|moved| moved.told("x", None))
            .collect();
        for (at, one) in all.iter().enumerate() {
            assert!(!all[..at].contains(one), "{one} said twice");
        }
    }

    #[test]
    fn only_the_card_looking_around_is_left_to_whoever_hunts() {
        let looking: Vec<Moved> = Moved::ALL
            .into_iter()
            .filter(|moved| !moved.is_an_event())
            .collect();
        assert_eq!(
            looking,
            [Moved::Scanned, Moved::ScanFailed, Moved::NetworksListed]
        );
    }

    #[test]
    fn a_connection_is_told_with_what_the_driver_gives() {
        let link = Link {
            signal: 87,
            strength: Some(-52),
            channel: Some(36),
            standard: 8,
            receiving: 866_000,
            sending: 780_000,
            moved: false,
        };
        assert_eq!(
            link.told("Example Wi-Fi 6"),
            "the Wi-Fi card Example Wi-Fi 6: signal 87 %, -52 dBm, channel 36, 802.11ac, \
             receiving at 866 Mb/s and sending at 780 Mb/s"
        );
        // A driver that gives no strength or channel, an unknown standard,
        // and an access point that changed.
        let bare = Link {
            strength: None,
            channel: None,
            standard: 0,
            moved: true,
            ..link
        };
        assert_eq!(
            bare.told("Example Wi-Fi 6"),
            "the Wi-Fi card Example Wi-Fi 6: signal 87 %, receiving at 866 Mb/s and sending at \
             780 Mb/s, on another access point than a moment ago"
        );
    }

    #[test]
    fn the_standards_windows_numbers_are_named() {
        assert_eq!(standard_name(7), Some("802.11n"));
        assert_eq!(standard_name(10), Some("802.11ax"));
        assert_eq!(standard_name(0), None);
    }
}
