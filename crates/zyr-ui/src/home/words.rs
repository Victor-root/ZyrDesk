//! What the home window says, put into the person's words.
//!
//! The service, the account and Windows say facts and numbers; this is
//! where they become sentences, in the language the person reads Windows
//! in. Nothing here decides anything: what a computer's state is comes
//! from below, and only the words are chosen here.

use zyr_control::{Device, Peer};
use zyr_i18n::say;
use zyr_session::presence::{Presence, Road};
use zyr_system::{Connectivity, Machine};

use crate::home::seen::Seen;
use crate::shell::desk::Standing;

const MINUTE: u64 = 60;
const HOUR: u64 = 3600;
const DAY: u64 = 86_400;

/// How long something has lasted, in words: « 42 min », « 1 h 12 ».
pub fn duration(seconds: u64) -> String {
    if seconds < MINUTE {
        return say!("time.under_a_minute");
    }
    let minutes = (seconds % HOUR) / MINUTE;
    if seconds < HOUR {
        return say!("time.minutes", minutes = minutes);
    }
    let hours = seconds / HOUR;
    if minutes == 0 {
        say!("time.hours", hours = hours)
    } else {
        say!(
            "time.hours_minutes",
            hours = hours,
            minutes = format!("{minutes:02}")
        )
    }
}

/// How long ago something was, in the largest unit worth saying: « 2 h »,
/// « 3 days ».
pub fn ago(seconds: u64) -> String {
    if seconds < HOUR {
        duration(seconds)
    } else if seconds < DAY {
        say!("time.hours", hours = seconds / HOUR)
    } else {
        say!("time.days", days = seconds / DAY)
    }
}

/// The day something took place, as the clock of this computer reads it:
/// « Today », « Yesterday », or its date.
pub fn day_of(unix: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    let day = |unix: u64| zyr_system::local_time(unix).map(|at| (at.year, at.month, at.day));
    let Some((year, month, date)) = day(unix) else {
        return String::new();
    };
    if day(now) == Some((year, month, date)) {
        say!("time.today")
    } else if day(now.saturating_sub(DAY)) == Some((year, month, date)) {
        say!("time.yesterday")
    } else {
        say!(
            "time.date",
            day = format!("{date:02}"),
            month = format!("{month:02}"),
            year = year
        )
    }
}

/// The hour something took place at, on this computer's clock: « 14:32 ».
pub fn hour_of(unix: u64) -> String {
    zyr_system::local_time(unix)
        .map(|at| format!("{:02}:{:02}", at.hour, at.minute))
        .unwrap_or_default()
}

/// When something took place: « Today at 14:32 ».
pub fn when(unix: u64) -> String {
    say!("time.at", day = day_of(unix), hour = hour_of(unix))
}

/// The name a person is greeted by: the first of their names, written
/// with a capital, for « Hello, Camille » rather than « Hello, camille ».
pub fn first_name(name: &str) -> String {
    let first = name.split_whitespace().next().unwrap_or_default();
    let mut letters = first.chars();
    match letters.next() {
        Some(initial) => initial.to_uppercase().chain(letters).collect(),
        None => String::new(),
    }
}

/// A person's initials, two at most, for the disc that stands for them.
pub fn initials(name: &str) -> String {
    let mut words = name
        .split(|letter: char| {
            letter.is_whitespace() || letter == '.' || letter == '_' || letter == '-'
        })
        .filter(|word| !word.is_empty());
    let first = words.next().and_then(|word| word.chars().next());
    let last = words.next_back().and_then(|word| word.chars().next());
    first
        .into_iter()
        .chain(last)
        .flat_map(char::to_uppercase)
        .collect()
}

/// A processor's name as its owner knows it: the marks written out as
/// signs, and the speed some names carry at their end left out, since
/// the processor already says it.
pub fn processor(name: &str) -> String {
    let named = name
        .replace("(R)", "®")
        .replace("(r)", "®")
        .replace("(TM)", "™")
        .replace("(tm)", "™");
    let named = match named.find(" CPU @") {
        Some(at) => named[..at].to_string(),
        None => named,
    };
    named.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Windows, with its edition in the person's words: « Windows 11
/// Professional ».
pub fn windows(machine: &Machine) -> Option<String> {
    let windows = machine.windows.as_ref()?;
    let edition = windows.edition.as_deref().map(|edition| match edition {
        "Core" | "CoreSingleLanguage" | "CoreCountrySpecific" => say!("edition.home"),
        "Professional" => say!("edition.pro"),
        "ProfessionalWorkstation" => say!("edition.workstation"),
        "ProfessionalEducation" | "Education" => say!("edition.education"),
        "Enterprise" | "EnterpriseS" => say!("edition.enterprise"),
        other => other.to_string(),
    });
    Some(match edition {
        Some(edition) => format!("{} {edition}", windows.name()),
        None => windows.name().to_string(),
    })
}

/// How much memory a computer has, as it was bought: « 32 GB of RAM ».
pub fn memory(machine: &Machine) -> Option<String> {
    let bytes = machine.installed.or(machine.memory)?;
    let gigabytes = (bytes as f64 / (1024.0 * 1024.0 * 1024.0)).round().max(1.0) as u64;
    Some(say!("home.memory", gigabytes = gigabytes))
}

/// Where this computer stands, in a few words, and whether that is good
/// news: what its card and the banner say.
pub fn this_computer(standing: Option<&Standing>, ffmpeg_here: Option<bool>) -> (String, Tone) {
    let Some(said) = standing else {
        return (say!("home.looking_for_the_service"), Tone::Waiting);
    };
    if said.unreachable.is_some() {
        return (say!("home.service_stopped"), Tone::Trouble);
    }
    if ffmpeg_here == Some(false) || (said.wanted && said.holdup == "engineMissing") {
        return (say!("home.ffmpeg_missing"), Tone::Trouble);
    }
    if !said.wanted {
        return (say!("home.access_off"), Tone::Off);
    }
    if said.hosting {
        return (say!("home.ready"), Tone::Good);
    }
    (say!("home.starting"), Tone::Waiting)
}

/// How a state reads: the colour of its dot, and never its only sign,
/// since the words beside it say it too.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Good,
    Waiting,
    Off,
    Trouble,
}

/// How far this computer's network reaches, in words.
pub fn connectivity(said: Option<Connectivity>) -> (String, Tone) {
    match said {
        Some(Connectivity::Internet) => (say!("home.connected"), Tone::Good),
        Some(Connectivity::LocalOnly) => (say!("home.local_only"), Tone::Waiting),
        Some(Connectivity::Offline) => (say!("computer.offline"), Tone::Trouble),
        None => (say!("home.network_unknown"), Tone::Off),
    }
}

/// What ZyrDesk itself is like on this computer: all is well, or the one
/// thing that is not.
pub fn zyrdesk(seen: &Seen) -> (String, Tone) {
    let (said, tone) = this_computer(seen.standing.as_ref(), seen.ffmpeg_here);
    match tone {
        Tone::Trouble => (said, tone),
        _ if versions_differ(seen).is_some() => (say!("home.versions_differ"), Tone::Waiting),
        Tone::Waiting => (said, tone),
        Tone::Good | Tone::Off => (say!("home.all_is_well"), Tone::Good),
    }
}

/// The build the service runs, when it is not this window's.
///
/// The two are built and installed together, and the day they differ,
/// that is the fault, whatever else it looks like.
pub fn versions_differ(seen: &Seen) -> Option<&str> {
    let service = seen.standing.as_ref()?.service_build.as_str();
    (!service.is_empty() && !seen.version.is_empty() && !seen.version.contains(service))
        .then_some(service)
}

/// What is written under a computer's name on its card.
pub fn below_the_name(peer: &Peer, device: Option<&Device>) -> String {
    let presence = zyr_session::presence::presence(peer);
    let state = match (&presence, zyr_session::presence::road(peer)) {
        (Presence::Gone, _) => match device.and_then(|device| device.last_seen) {
            Some(seen) => say!(
                "computer.offline_since",
                since = ago(zyr_broker::now().saturating_sub(seen))
            ),
            None => say!("computer.offline"),
        },
        (Presence::Closed { why }, _) => zyr_i18n::fact(why),
        (_, Road::Internet) => say!("computer.online"),
        _ => peer.host.clone(),
    };
    let origin = match &peer.account {
        Some(account) => match &account.shared_by {
            Some(owner) => say!("computer.shared_by", owner = owner),
            None => say!("computer.of_the_account"),
        },
        None if peer.written => say!("computer.written"),
        None => String::new(),
    };
    if origin.is_empty() {
        state
    } else {
        format!("{state} · {origin}")
    }
}

/// A device of the account, where it stands, in words.
pub fn device_presence(device: &Device) -> String {
    if device.online {
        return say!(
            "settings.device_online",
            access = zyr_i18n::fact(&device.access.fact())
        );
    }
    match device.last_seen {
        Some(seen) => say!(
            "settings.device_seen",
            since = ago(zyr_broker::now().saturating_sub(seen))
        ),
        None => say!("computer.offline"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_processor_is_named_as_its_owner_knows_it() {
        assert_eq!(
            processor("12th Gen Intel(R) Core(TM) i7-12700H"),
            "12th Gen Intel® Core™ i7-12700H"
        );
        assert_eq!(
            processor("Intel(R) Core(TM) i7-8700K CPU @ 3.70GHz"),
            "Intel® Core™ i7-8700K"
        );
        assert_eq!(
            processor("AMD Ryzen 7 5800X 8-Core Processor   "),
            "AMD Ryzen 7 5800X 8-Core Processor"
        );
    }

    #[test]
    fn a_person_is_greeted_by_the_first_of_their_names() {
        assert_eq!(first_name("camille durand"), "Camille");
        assert_eq!(first_name("  Élodie Martin "), "Élodie");
        assert_eq!(first_name(""), "");
    }

    #[test]
    fn initials_are_the_first_and_the_last_names_letters() {
        assert_eq!(initials("Camille Durand"), "CD");
        assert_eq!(initials("camille"), "C");
        assert_eq!(initials("jean-pierre.martin"), "JM");
        assert_eq!(initials("Anne Marie Dupont"), "AD");
        assert_eq!(initials(""), "");
    }
}
