//! Calling out, when nobody can hear an announcement.
//!
//! Discovery by mDNS asks a whole network at once, on a multicast
//! address. It is the right way to do it and it costs one packet, but it
//! rests on every box and every access point forwarding that multicast,
//! and a great many of them do not: between a wired card and a wireless
//! one in particular, the announcement is quietly dropped and the two
//! computers spend their lives shouting into a wall. Nothing in either
//! machine is wrong, and nothing anywhere says so.
//!
//! So this asks the same question the other way round: one small
//! datagram, sent to the network's broadcast address and, while nobody
//! has answered, to each of its addresses in turn. That is ordinary
//! traffic, the same kind a session is made of, and a network that
//! carries a session carries this.
//!
//! It runs beside mDNS rather than instead of it. Both fill the same
//! list, a computer found twice is one computer, and on a network where
//! the announcements do pass, nothing here ever has anything to do.

use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use zyr_proto::net::TUNNEL_PORT;
use zyr_transport::Fingerprint;

use crate::{Found, Peer};

/// Port ZyrDesk calls out on.
///
/// Its own, beside the tunnel's: the tunnel's socket is held by the
/// transport, and mDNS owns the one it is given by the standard.
pub const PORT: u16 = 47001;

/// Version of what is said here, first word of every line.
///
/// Two computers running different builds of the product have to be able
/// to say so rather than misread each other.
const VERSION: u32 = 1;

/// What every line starts with, so that anything else arriving on this
/// port is dropped without a thought.
const MARK: &str = "zyrdesk";

/// How often this computer calls out.
///
/// Short, because it is also what keeps the list honest: every computer
/// already on it is asked directly at this rhythm, and a green dot that
/// stood for a machine gone half a minute ago would be a lie the whole
/// screen rests on.
const ROUND: Duration = Duration::from_secs(3);

/// How often the whole network is asked one address at a time, while
/// nobody has answered.
///
/// Slower than the broadcast, and it stops on its own the moment a
/// neighbour is found: a network that answers a broadcast never needs
/// this, and one that does not is not worth flooding.
const SWEEP: Duration = Duration::from_secs(30);

/// Most addresses a network may hold before it is only ever broadcast to.
const MOST: u32 = 256;

/// Longest thing anybody may say here.
///
/// A name and a fingerprint and no more. Anything longer is not ours.
const LIMIT: usize = 512;

/// How long the socket waits before letting the loop look at the clock.
const WAKE: Duration = Duration::from_millis(500);

/// What a computer says about itself, whether it is asking or answering.
///
/// The same three things either way, and that is the point: a computer
/// calling out is announcing itself exactly as plainly as one answering,
/// and there is no reason for only one of the two to be heard.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Itself {
    port: u16,
    fingerprint: Fingerprint,
    name: String,
}

impl Itself {
    fn said(&self) -> String {
        format!("{} {} {}", self.port, self.fingerprint, self.name)
    }

    /// Reads it back off what is left of a line.
    fn read<'a>(pieces: &mut impl Iterator<Item = &'a str>) -> Option<Self> {
        let port = pieces.next()?.parse().ok()?;
        // The fingerprint and the name travel together in what is left,
        // the name last since it is the only one that may hold a space.
        let (fingerprint, name) = pieces.next()?.split_once(' ')?;
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        Some(Itself {
            port,
            fingerprint: fingerprint.parse().ok()?,
            name: name.to_string(),
        })
    }
}

/// What is said on this port.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Said {
    /// Who is there, and this is who is asking.
    ///
    /// The asking half is what makes a neighbourhood work both ways. A
    /// computer only ever learns of another by hearing it say where it
    /// is, and until this it only said so when answering: the one that
    /// called learned, the one that was called learned nothing. On any
    /// network where both sides can call out, the two find each other
    /// within a round and nobody notices. On a network where only one
    /// side can, which is what a private tunnel between two machines
    /// looks like when one end holds a single address and no
    /// neighbourhood to sweep, the far computer stayed a stranger for
    /// ever and turned away every session as one.
    ///
    /// Said and not merely implied, because being called tells a
    /// computer an address and nothing else, and an address is not who
    /// somebody is.
    ///
    /// Absent from an older ZyrDesk, which asked without introducing
    /// itself, so it is read as a maybe and never as a promise. Such a
    /// question is still answered, exactly as before.
    Who { asking: Option<Itself> },
    /// This computer is, and this is how to reach it.
    Here(Itself),
    /// This computer is leaving, now.
    ///
    /// Waiting for it to stop answering works and takes ten seconds;
    /// saying so takes one packet and none at all. A machine that
    /// crashes says nothing, which is what the waiting is still there
    /// for.
    Bye { fingerprint: Fingerprint },
}

impl Said {
    fn read(line: &str) -> Option<Self> {
        let mut pieces = line.trim().splitn(5, ' ');
        if pieces.next()? != MARK {
            return None;
        }
        if pieces.next()?.parse::<u32>().ok()? != VERSION {
            return None;
        }
        match pieces.next()? {
            "who" => Some(Said::Who {
                asking: Itself::read(&mut pieces),
            }),
            "bye" => Some(Said::Bye {
                fingerprint: pieces.next()?.trim().parse().ok()?,
            }),
            "here" => Some(Said::Here(Itself::read(&mut pieces)?)),
            _ => None,
        }
    }

    fn spoken(&self) -> String {
        match self {
            // An older ZyrDesk reads the words it knows and stops, so the
            // introduction rides along on a question it still understands
            // rather than on a word it would throw away.
            Said::Who { asking: None } => format!("{MARK} {VERSION} who"),
            Said::Who {
                asking: Some(itself),
            } => {
                format!("{MARK} {VERSION} who {}", itself.said())
            }
            Said::Here(itself) => format!("{MARK} {VERSION} here {}", itself.said()),
            Said::Bye { fingerprint } => format!("{MARK} {VERSION} bye {fingerprint}"),
        }
    }
}

/// The calling, for as long as it is held.
pub struct Calling {
    stop: Arc<AtomicBool>,
    /// The same socket the loop listens on, kept to say one last thing.
    saying_goodbye: UdpSocket,
    fingerprint: Fingerprint,
    found: Found,
}

impl Drop for Calling {
    /// Stops answering, and says so.
    ///
    /// The goodbye is the whole difference between a card that goes the
    /// moment somebody quits and one that lingers until the others have
    /// noticed the silence. It is sent to the computers already known,
    /// each at its own address, and to the network at large for whoever
    /// is not on that list yet.
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let goodbye = Said::Bye {
            fingerprint: self.fingerprint,
        }
        .spoken();
        for peer in self.found.peers() {
            let _ = self
                .saying_goodbye
                .send_to(goodbye.as_bytes(), SocketAddr::new(peer.address, PORT));
        }
        for card in zyr_proto::machine::addresses() {
            if let Some(everyone) = card.broadcast {
                let _ = self
                    .saying_goodbye
                    .send_to(goodbye.as_bytes(), SocketAddr::from((everyone, PORT)));
            }
        }
    }
}

/// Starts calling out, and answering those who call.
pub fn start(
    name: String,
    fingerprint: Fingerprint,
    found: Found,
    noticed: Arc<dyn Fn(&str) + Send + Sync>,
) -> std::io::Result<Calling> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, PORT))?;
    socket.set_broadcast(true)?;
    socket.set_read_timeout(Some(WAKE))?;

    let saying_goodbye = socket.try_clone()?;
    let stop = Arc::new(AtomicBool::new(false));
    let watching = stop.clone();
    let listing = found.clone();
    std::thread::spawn(move || {
        let mut me = Calls {
            socket,
            name,
            fingerprint,
            found,
            noticed,
            called: None,
            swept: None,
            listed: Vec::new(),
        };
        while !watching.load(Ordering::Relaxed) {
            me.once();
        }
    });
    Ok(Calling {
        stop,
        saying_goodbye,
        fingerprint,
        found: listing,
    })
}

/// Everything one round of calling needs.
struct Calls {
    socket: UdpSocket,
    name: String,
    fingerprint: Fingerprint,
    found: Found,
    noticed: Arc<dyn Fn(&str) + Send + Sync>,
    /// When this computer last called out, and when it last went through
    /// the network one address at a time.
    called: Option<Instant>,
    swept: Option<Instant>,
    /// Who was on the list at the end of the last round, so that a
    /// computer leaving is written down as plainly as one arriving. A
    /// list that only ever reports arrivals leaves the reader guessing
    /// when something went.
    listed: Vec<(Fingerprint, String)>,
}

impl Calls {
    /// One turn: say what is due, then listen for as long as the socket
    /// is willing to wait.
    fn once(&mut self) {
        let now = Instant::now();
        if self.called.is_none_or(|last| now >= last + ROUND) {
            self.called = Some(now);
            self.call_out(now);
        }
        self.listen();
    }

    /// Asks the networks this computer is on who else is there.
    fn call_out(&mut self, now: Instant) {
        let question = Said::Who {
            asking: Some(self.itself()),
        }
        .spoken();
        let around = self.found.peers();

        // Those already known are asked to their face, every round. It is
        // what keeps the list honest: a broadcast that a box drops would
        // otherwise leave a computer on the screen until it timed out,
        // and one that stopped answering has to fall off it quickly.
        for peer in &around {
            self.say(&question, SocketAddr::new(peer.address, PORT));
        }
        self.say_who_came_and_went(&around);

        // One address at a time only while nobody has answered: a network
        // that carries a broadcast never needs it, and one that does not
        // is not worth knocking on twice a minute forever.
        let one_by_one = around.is_empty() && self.swept.is_none_or(|last| now >= last + SWEEP);
        if one_by_one {
            self.swept = Some(now);
        }

        for card in zyr_proto::machine::addresses() {
            if let Some(everyone) = card.broadcast {
                self.say(&question, SocketAddr::from((everyone, PORT)));
            }
            if !one_by_one {
                continue;
            }
            for address in card.neighbourhood(MOST) {
                self.say(&question, SocketAddr::from((address, PORT)));
            }
        }
    }

    /// Writes down what changed on the list, and only that.
    fn say_who_came_and_went(&mut self, around: &[Peer]) {
        let now: Vec<(Fingerprint, String)> = around
            .iter()
            .map(|peer| (peer.fingerprint, peer.name.clone()))
            .collect();
        for (fingerprint, name) in &self.listed {
            if !now.iter().any(|(seen, _)| seen == fingerprint) {
                (self.noticed)(&format!("{name} left the local network"));
            }
        }
        self.listed = now;
    }

    /// Takes in whatever arrived, or comes back when nothing did.
    fn listen(&mut self) {
        let mut room = [0u8; LIMIT];
        let (read, from) = match self.socket.recv_from(&mut room) {
            Ok(heard) => heard,
            // A timeout is the ordinary case: it is how the loop gets to
            // look at the clock. Anything else is worth no more here,
            // since the next turn will try again in half a second.
            Err(_) => return,
        };
        let Ok(line) = std::str::from_utf8(&room[..read]) else {
            return;
        };
        let Some(said) = Said::read(line) else {
            return;
        };
        match said {
            Said::Who { asking } => {
                // Written down before the answer goes back out, so that
                // a computer which cannot call out is still known to the
                // one that can, and by the same rule: whoever says where
                // they are on a network this computer trusts is a
                // neighbour, asking or answering.
                if let Some(asking) = asking {
                    self.note(from, asking);
                }
                self.answer(from);
            }
            Said::Here(said) => self.note(from, said),
            Said::Bye { fingerprint } => self.goodbye(fingerprint, from),
        }
    }

    /// Takes a computer off the list, on its own say-so.
    fn goodbye(&mut self, fingerprint: Fingerprint, from: SocketAddr) {
        if let Some(name) = self.found.forget_the_one_at(fingerprint, from.ip()) {
            (self.noticed)(&format!("{name} said goodbye and left the local network"));
            self.listed.retain(|(seen, _)| *seen != fingerprint);
        }
    }

    /// What this computer says about itself, asking or answering.
    fn itself(&self) -> Itself {
        Itself {
            port: TUNNEL_PORT,
            fingerprint: self.fingerprint,
            name: self.name.clone(),
        }
    }

    /// Answers whoever asked, to their face and not to the whole network.
    fn answer(&self, asking: SocketAddr) {
        self.say(&Said::Here(self.itself()).spoken(), asking);
    }

    /// Writes down a computer that said where it is.
    fn note(&self, from: SocketAddr, said: Itself) {
        // This computer hears its own broadcast: it is not a neighbour.
        if said.fingerprint == self.fingerprint {
            return;
        }
        let peer = Peer {
            name: said.name,
            fingerprint: said.fingerprint,
            address: from.ip(),
            // The one this answer came from. A computer with several
            // cards answers on each of them, and every answer adds its
            // own: the list is gathered, never replaced.
            addresses: vec![from.ip()],
            port: said.port,
        };
        let named = format!("{} at {}", peer.name, peer.address);
        if self.found.note(peer, Instant::now()) {
            (self.noticed)(&format!("{named} answered a call on the local network"));
        }
    }

    /// Says one thing to one place, and lets a refusal pass.
    ///
    /// A card that has just been unplugged, a broadcast a firewall will
    /// not let out: neither is worth a line, and both would fill the
    /// journal a hundred times a minute if they were.
    fn say(&self, what: &str, to: SocketAddr) {
        let _ = self.socket.send_to(what.as_bytes(), to);
    }
}

/// Where this computer would send its calls, for the journal.
///
/// Written down once at the start: a machine whose only card has no
/// broadcast address, or sits on a network too wide to go through, will
/// never find anybody that way, and nothing else would say so.
pub fn where_calls_go() -> Vec<String> {
    zyr_proto::machine::addresses()
        .into_iter()
        .map(|card| match card.broadcast {
            Some(everyone) => format!(
                "calling on {} through {everyone}, {} addresses to try one by one",
                card.interface,
                card.neighbourhood(MOST).len()
            ),
            None => format!(
                "calling on {} with no broadcast address, {} addresses to try one by one",
                card.interface,
                card.neighbourhood(MOST).len()
            ),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::net::IpAddr;

    fn fingerprint() -> Fingerprint {
        "0829cc7ecb9e9ba53cd36e6f342268ddf3c8ef05a49d1d7944ac6332c89cf237"
            .parse()
            .unwrap()
    }

    /// One computer, on a port the system hands out, so two of them can
    /// talk in one test without fighting over the real one.
    fn computer(name: &str, seed: u8, found: Found) -> Calls {
        let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).expect("un port libre");
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        Calls {
            socket,
            name: name.to_string(),
            fingerprint: format!("{seed:02x}").repeat(32).parse().unwrap(),
            found,
            noticed: Arc::new(|_: &str| {}),
            called: None,
            swept: None,
            listed: Vec::new(),
        }
    }

    #[test]
    fn one_computer_calls_and_the_other_answers() {
        // Le tout : une question part, l'autre machine répond à celle qui
        // a demandé et à elle seule, et elle se retrouve dans la liste
        // avec son nom et le port de son tunnel.
        let list = Found::new();
        let asking = computer("PC-PORTABLE", 1, list.clone());
        let mut answering = computer("PC de Victor", 2, Found::new());

        let door = answering.socket.local_addr().unwrap();
        asking.say(
            &Said::Who {
                asking: Some(asking.itself()),
            }
            .spoken(),
            door,
        );
        answering.listen();

        let mut asking = asking;
        asking.listen();

        let found = list.peers();
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].name, "PC de Victor");
        assert_eq!(found[0].port, TUNNEL_PORT);
        assert_eq!(found[0].address, IpAddr::V4(Ipv4Addr::LOCALHOST));
    }

    #[test]
    fn a_computer_that_is_called_learns_who_called_it() {
        // Le défaut qui a coûté un ordinateur entier. Une machine ne
        // connaissait que celles qui lui avaient répondu, donc seule
        // celle qui appelle apprenait quelque chose. Sur un tunnel privé
        // entre deux machines, où un seul des deux bouts a un voisinage
        // à balayer, l'autre restait un inconnu pour toujours et
        // refusait chaque session comme telle.
        let heard = Found::new();
        let asking = computer("PC-PORTABLE", 1, Found::new());
        let mut answering = computer("PC de Victor", 2, heard.clone());

        let door = answering.socket.local_addr().unwrap();
        asking.say(
            &Said::Who {
                asking: Some(asking.itself()),
            }
            .spoken(),
            door,
        );
        answering.listen();

        let found = heard.peers();
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].name, "PC-PORTABLE");
        assert_eq!(found[0].port, TUNNEL_PORT);
    }

    #[test]
    fn a_question_from_an_older_zyrdesk_is_still_answered() {
        // Le mot « who » tout seul est ce que disaient les versions
        // d'avant. Il ne présente personne, donc il n'apprend rien, mais
        // il doit toujours recevoir sa réponse : sinon une mise à jour
        // d'un seul côté couperait la découverte au lieu de l'améliorer.
        let heard = Found::new();
        let list = Found::new();
        let asking = computer("PC-PORTABLE", 1, list.clone());
        let mut answering = computer("PC de Victor", 2, heard.clone());

        let door = answering.socket.local_addr().unwrap();
        asking.say(&Said::Who { asking: None }.spoken(), door);
        answering.listen();
        assert!(heard.peers().is_empty(), "{:?}", heard.peers());

        let mut asking = asking;
        asking.listen();
        assert_eq!(list.peers().len(), 1, "{:?}", list.peers());
    }

    #[test]
    fn a_computer_never_finds_itself() {
        // Une machine reçoit sa propre diffusion : sans ce garde-fou elle
        // s'inscrirait dans la liste de ses propres voisins, et l'écran
        // d'accueil montrerait l'ordinateur sur lequel il est ouvert.
        let list = Found::new();
        let mut me = computer("PC-BUREAU", 1, list.clone());
        let door = me.socket.local_addr().unwrap();

        me.say(
            &Said::Who {
                asking: Some(me.itself()),
            }
            .spoken(),
            door,
        );
        // La question, puis la réponse qu'elle s'est faite à elle-même.
        me.listen();
        me.listen();

        assert!(list.peers().is_empty(), "{:?}", list.peers());
    }

    #[test]
    fn a_computer_that_says_goodbye_is_off_the_list_at_once() {
        // Sans cela, une machine qu'on vient de quitter reste affichée en
        // vert le temps que les autres remarquent son silence, et on leur
        // propose de s'y connecter.
        let list = Found::new();
        let mut asking = computer("PC-PORTABLE", 1, list.clone());
        let mut answering = computer("PC de Victor", 2, Found::new());

        let door = answering.socket.local_addr().unwrap();
        asking.say(
            &Said::Who {
                asking: Some(asking.itself()),
            }
            .spoken(),
            door,
        );
        answering.listen();
        asking.listen();
        assert_eq!(list.peers().len(), 1);

        answering.say(
            &Said::Bye {
                fingerprint: answering.fingerprint,
            }
            .spoken(),
            asking.socket.local_addr().unwrap(),
        );
        asking.listen();
        assert!(list.peers().is_empty(), "{:?}", list.peers());
    }

    #[test]
    fn anything_arriving_that_is_not_ours_leaves_no_trace() {
        // Ce port est ouvert sur le réseau : n'importe quoi peut y
        // arriver, et rien de tout cela ne doit devenir un ordinateur.
        let list = Found::new();
        let mut listening = computer("PC-BUREAU", 1, list.clone());
        let door = listening.socket.local_addr().unwrap();
        let outside = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();

        for bruit in [b"bonjour".as_slice(), b"zyrdesk 9 who", &[0xff, 0xfe, 0x00]] {
            outside.send_to(bruit, door).unwrap();
            listening.listen();
        }
        assert!(list.peers().is_empty());
    }

    #[test]
    fn everything_said_here_survives_the_round_trip() {
        let card = Itself {
            port: TUNNEL_PORT,
            fingerprint: fingerprint(),
            // Un nom d'ordinateur porte des espaces, et il est écrit
            // en dernier pour cela.
            name: "PC de Victor".to_string(),
        };
        for said in [
            Said::Who { asking: None },
            Said::Who {
                asking: Some(card.clone()),
            },
            Said::Here(card),
        ] {
            let line = said.spoken();
            assert_eq!(Said::read(&line), Some(said), "sur « {line} »");
            assert!(line.len() < LIMIT, "« {line} » est trop long");
        }
    }

    #[test]
    fn anything_that_is_not_ours_is_dropped() {
        // Ce port peut recevoir n'importe quoi : rien de ce qui n'a pas
        // été dit par ce produit ne doit être lu comme un ordinateur.
        for line in [
            "",
            "bonjour",
            "zyrdesk",
            "zyrdesk 1",
            "zyrdesk 1 bonjour",
            "autrechose 1 who",
            // Une version que nous ne parlons pas : on se tait plutôt que
            // de deviner.
            "zyrdesk 2 who",
            "zyrdesk 1 here",
            "zyrdesk 1 here 47000",
            "zyrdesk 1 here 47000 pas-une-empreinte PC",
            "zyrdesk 1 here pas-un-port 0829cc7ecb9e9ba53cd36e6f342268ddf3c8ef05a49d1d7944ac6332c89cf237 PC",
        ] {
            assert_eq!(Said::read(line), None, "« {line} » aurait dû être ignoré");
        }
    }

    #[test]
    fn a_computer_without_a_name_is_not_a_computer() {
        // Une carte sans titre ne se reconnaît pas : mieux vaut ignorer
        // l'annonce que d'afficher une ligne vide sur laquelle cliquer.
        let line = format!("{MARK} {VERSION} here {TUNNEL_PORT} {} ", fingerprint());
        assert_eq!(Said::read(&line), None);
    }
}
