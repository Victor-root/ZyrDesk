//! Answering the programs that drive the service.
//!
//! The interface and the command line own nothing: they ask here, and
//! the service does the holding. This desk is open for the whole life of
//! the service, engine or no engine, because most of what is asked of it
//! has nothing to do with the local engine.

// Outside Windows nothing calls this module: the service does not exist
// there. Its logic has nothing platform-specific about it and stays
// compiled and tested everywhere.
#![cfg_attr(not(windows), allow(dead_code))]

use std::io;
use std::sync::{Arc, Mutex};

use tokio::runtime::Handle;
use tokio::task::{JoinHandle, JoinSet};
use zyr_control::pipe::Heard;
use zyr_control::{Answer, Door, Holdup, PROTOCOL, Request, Standing};
use zyr_lan::Found;
use zyr_proto::log::Log;
use zyr_proto::paths;
use zyr_transport::{Fingerprint, authorized};

use crate::known;
use crate::preferences::Remembered;
use crate::supervisor::StopOrder;
use crate::ways::Ways;

/// Whether this computer can be reached right now, and what is in the
/// way when it is not.
///
/// The supervisor opens it once the engine answers and the tunnel is
/// standing, and holds it back whenever something stops that from
/// happening. It is the one thing the desk cannot work out on its own,
/// and the reason matters as much as the fact: an engine that is missing
/// and an engine that is starting look alike from a window, and only one
/// of the two is worth waiting for.
#[derive(Clone)]
pub struct Hosting(Arc<Mutex<Option<Holdup>>>);

impl Hosting {
    /// Not reachable yet, and nothing wrong with that.
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(Some(Holdup::Starting))))
    }

    /// The tunnel stands: this computer can be reached.
    pub fn open(&self) {
        *self.0.lock().expect("état de l'accès distant") = None;
    }

    /// It cannot, for this reason.
    pub fn held_by(&self, holdup: Holdup) {
        *self.0.lock().expect("état de l'accès distant") = Some(holdup);
    }

    fn standing(&self) -> Option<Holdup> {
        *self.0.lock().expect("état de l'accès distant")
    }
}

/// The desk, open. Dropping it closes the channel.
pub struct Desk {
    task: JoinHandle<()>,
}

impl Drop for Desk {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Desk {
    /// Opens the desk on a named channel.
    ///
    /// The fingerprint is handed over rather than read here: this desk
    /// answers questions, it does not own the identity of the computer.
    pub fn open(runtime: &Handle, channel: &str, answering: Answering) -> io::Result<Self> {
        let _guard = runtime.enter();
        let door = Door::open(channel)?;

        Ok(Self {
            task: runtime.spawn(serve(door, answering)),
        })
    }
}

/// Everything an answer is drawn from.
///
/// Built by the supervisor, which owns all of it: the desk holds no
/// state of its own, it reads and reports.
#[derive(Clone)]
pub struct Answering {
    pub fingerprint: Fingerprint,
    pub ways: Ways,
    pub hosting: Hosting,
    pub remembered: Remembered,
    pub neighbours: Found,
    /// What ends the service. Held here so that quitting the interface
    /// can take the service with it without asking Windows, which would
    /// mean an administrator prompt at every quit.
    pub order: StopOrder,
    pub log: Log,
}

/// Whether Windows starts the service on its own.
#[cfg(windows)]
fn at_boot() -> bool {
    crate::service::starts_with_windows().unwrap_or(false)
}

/// Outside Windows there is no service to start.
#[cfg(not(windows))]
fn at_boot() -> bool {
    false
}

#[cfg(windows)]
fn set_at_boot(on: bool) -> Result<(), String> {
    crate::service::start_with_windows(on).map_err(|e| e.to_string())
}

#[cfg(not(windows))]
fn set_at_boot(_on: bool) -> Result<(), String> {
    Err("le service ZyrDesk n'existe que sous Windows".to_string())
}

async fn serve(mut door: Door, answering: Answering) {
    let mut talking = JoinSet::new();
    loop {
        match door.accept().await {
            Ok(conversation) => {
                let answering = answering.clone();
                talking.spawn(converse(conversation, answering));
                while talking.try_join_next().is_some() {}
            }
            // One program failing to be taken in must not shut the desk
            // on all the others.
            Err(e) => answering
                .log
                .write(&format!("control channel refused a program: {e}")),
        }
    }
}

/// One program, for as long as it has something to ask.
async fn converse(mut talking: Heard, answering: Answering) {
    loop {
        let heard = match talking.hear().await {
            Ok(Some(line)) => line,
            // Gone, or saying something impossible to read. Either way
            // there is nothing left to answer.
            Ok(None) => return,
            Err(e) => {
                answering
                    .log
                    .write(&format!("control channel gave up on a program: {e}"));
                return;
            }
        };

        let answers = match Request::parse(&heard) {
            Ok(request) => answer(request, &answering).await,
            Err(e) => vec![Answer::Refused(e.to_string())],
        };

        for answer in answers {
            if talking.say(&answer.to_string()).await.is_err() {
                return;
            }
        }
    }
}

/// What a request is answered with. A list is several answers, ended by
/// `Done`; everything else is one.
async fn answer(request: Request, answering: &Answering) -> Vec<Answer> {
    match request {
        Request::Peers => ended(on_screen(answering).into_iter().map(Answer::Peer).collect()),
        Request::Sessions => ended(
            answering
                .ways
                .held()
                .into_iter()
                .map(Answer::Session)
                .collect(),
        ),
        other => vec![one(other, answering).await],
    }
}

/// The computers the home screen shows.
///
/// Those announcing themselves on the local network, and then those
/// written down by hand that are not announcing anything. A computer on
/// both lists is announced once: what the network says of it is fresher
/// than what was written down months ago, its address most of all.
fn on_screen(answering: &Answering) -> Vec<zyr_control::Peer> {
    let written = match known::read(&paths::known_computers()) {
        Ok(written) => written,
        Err(e) => {
            answering
                .log
                .write(&format!("written-down computers unreadable: {e}"));
            Vec::new()
        }
    };

    let mut shown: Vec<zyr_control::Peer> = answering
        .neighbours
        .peers()
        .into_iter()
        .map(|peer| zyr_control::Peer {
            written: written
                .iter()
                .any(|known| known.fingerprint == peer.fingerprint),
            name: peer.name,
            fingerprint: peer.fingerprint,
            host: peer.address.to_string(),
            port: peer.port,
            seen: true,
        })
        .collect();

    for computer in written {
        if shown
            .iter()
            .any(|peer| peer.fingerprint == computer.fingerprint)
        {
            continue;
        }
        shown.push(zyr_control::Peer {
            name: computer.name,
            fingerprint: computer.fingerprint,
            host: computer.host,
            port: zyr_proto::net::TUNNEL_PORT,
            seen: false,
            written: true,
        });
    }
    shown
}

/// Closes a list with the ending that says it is whole.
///
/// Without it a caller cannot tell an empty list from a service that
/// stopped talking.
fn ended(mut said: Vec<Answer>) -> Vec<Answer> {
    said.push(Answer::Done);
    said
}

/// Turns a choice that could not be written down into a refusal.
///
/// A choice honoured but not kept would come back to haunt whoever made
/// it at the next restart, so it is answered as a failure and not as a
/// success with a footnote.
fn kept(written: std::io::Result<()>) -> Result<(), Answer> {
    written.map_err(|e| {
        Answer::Refused(format!(
            "le choix n'a pas pu être enregistré : {e}\n  \
             Il aurait été oublié au prochain démarrage."
        ))
    })
}

async fn one(request: Request, answering: &Answering) -> Answer {
    match request {
        Request::Standing => {
            let held = answering.hosting.standing();
            Answer::Standing(Standing {
                protocol: PROTOCOL,
                build: zyr_proto::BUILD.to_string(),
                fingerprint: answering.fingerprint,
                hosting: held.is_none(),
                holdup: held.unwrap_or_default(),
                wanted: answering.remembered.remote_access(),
                trusting: answering.remembered.trust_local_network(),
                at_boot: at_boot(),
                serving: answering.remembered.serving(),
                ways: answering.ways.count(),
            })
        }
        Request::Reach { host, peer, media } => {
            // Every address that computer has answered on, so the way is
            // opened through the one that is actually fast rather than
            // the one that happened to be written down.
            let also: Vec<std::net::IpAddr> = answering
                .neighbours
                .peers()
                .into_iter()
                .find(|seen| seen.fingerprint == peer)
                .map(|seen| seen.addresses)
                .unwrap_or_default();
            match answering.ways.open(&host, peer, media, &also).await {
                Ok(reached) => Answer::Reached(reached),
                Err(reason) => Answer::Refused(reason),
            }
        }
        Request::Pair { way, pin } => match answering.ways.hand_over_the_code(way, &pin).await {
            Ok(()) => Answer::Done,
            Err(reason) => Answer::Refused(reason),
        },
        Request::SecureAttention { way } => {
            match answering.ways.ask_for_the_secure_attention(way).await {
                Ok(()) => Answer::Done,
                Err(reason) => Answer::Refused(reason),
            }
        }
        Request::LockScreen { way } => match answering.ways.ask_to_lock(way).await {
            Ok(()) => Answer::Done,
            Err(reason) => Answer::Refused(reason),
        },
        Request::SteadyFar { way, rate } => {
            match answering.ways.ask_to_serve_steady(way, rate).await {
                Ok(()) => Answer::Done,
                Err(reason) => Answer::Refused(reason),
            }
        }
        Request::FarScreen { way, size } => {
            match answering.ways.ask_for_a_screen(way, size).await {
                Ok(size) => Answer::Showing { size },
                Err(reason) => Answer::Refused(reason),
            }
        }
        Request::Hush { way, quiet } => match answering.ways.ask_to_hush(way, quiet).await {
            Ok(()) => Answer::Done,
            Err(reason) => Answer::Refused(reason),
        },
        Request::Hold { way, process } => {
            if answering.ways.hold(way, process) {
                Answer::Done
            } else {
                Answer::Refused(format!("la voie {way} n'existe plus"))
            }
        }
        Request::Release { way } => {
            answering.ways.release(way);
            // A way already closed is the state that was asked for,
            // reached: saying no would make closing twice an error.
            Answer::Done
        }
        Request::SetHosting { on } => match kept(answering.remembered.set_remote_access(on)) {
            Ok(()) => {
                answering.log.write(if on {
                    "remote access turned on"
                } else {
                    "remote access turned off"
                });
                Answer::Done
            }
            Err(refusal) => refusal,
        },
        // The engine reads both of these once, when it starts, so
        // writing them down is only half the job: the supervisor sees
        // them change and starts it again. Said in the answer, since a
        // session in progress goes with it.
        Request::ServeLike { serving } => match kept(answering.remembered.set_serving(serving)) {
            Ok(()) => {
                answering.log.write(&format!(
                    "this computer will serve with a steady rate {} and {} capture",
                    if serving.steady_rate { "on" } else { "off" },
                    serving.capture
                ));
                Answer::Done
            }
            Err(refusal) => refusal,
        },
        Request::SetTrust { on } => match kept(answering.remembered.set_trust_local_network(on)) {
            Ok(()) => {
                answering.log.write(if on {
                    "the local network is trusted again"
                } else {
                    "the local network is no longer trusted"
                });
                Answer::Done
            }
            Err(refusal) => refusal,
        },
        Request::Authorize { peer, host, name } => {
            // Cette empreinte est déjà celle de cet ordinateur : l'écrire
            // n'ouvrirait rien et laisserait croire à un appairage fait.
            if peer == answering.fingerprint {
                return Answer::Refused(
                    "c'est l'empreinte de cet ordinateur.\n  \
                     Celle à saisir se lit dans la fenêtre de l'autre machine."
                        .to_string(),
                );
            }
            if let Err(e) = authorized::add(&paths::authorized_devices(), peer) {
                return Answer::Refused(format!(
                    "cet ordinateur n'a pas pu être écrit dans la liste : {e}"
                ));
            }
            answering
                .log
                .write(&format!("{peer} written down as allowed in"));

            // L'adresse est ce qui le fait rester à l'écran. Sans elle,
            // il faudrait la retaper à chaque session, et c'est
            // exactement ce que ce produit existe pour supprimer.
            let Some(host) = host else {
                return Answer::Done;
            };
            let name = name.filter(|name| !name.trim().is_empty());
            let computer = known::Known {
                fingerprint: peer,
                name: name.unwrap_or_else(|| host.clone()),
                host,
            };
            match known::add(&paths::known_computers(), computer) {
                Ok(()) => {
                    answering.log.write(&format!("{peer} kept on the screen"));
                    Answer::Done
                }
                Err(e) => Answer::Refused(format!(
                    "cet ordinateur est autorisé, mais n'a pas pu être gardé à l'écran : {e}\n  \
                     Il faudra le saisir à nouveau à la prochaine session."
                )),
            }
        }
        Request::Forget { peer } => {
            // Les deux listes, sinon un ordinateur retiré de l'écran
            // continuerait d'entrer, ce que personne ne devinerait.
            //
            // L'autorisation d'abord. Si la seconde écriture échoue,
            // l'ordinateur reste visible sans plus pouvoir entrer, et le
            // refus dit de recommencer ; dans l'autre ordre, il aurait
            // disparu de l'écran en gardant le droit d'entrer, invisible
            // et impossible à deviner.
            if let Err(e) = authorized::remove(&paths::authorized_devices(), peer) {
                return Answer::Refused(format!("cet ordinateur n'a pas pu être oublié : {e}"));
            }
            match known::remove(&paths::known_computers(), peer) {
                Ok(_) => {
                    answering.log.write(&format!("{peer} forgotten"));
                    Answer::Done
                }
                Err(e) => Answer::Refused(format!(
                    "cet ordinateur ne peut plus entrer, mais n'a pas pu être retiré \
                     de l'écran : {e}\n  Réessayez « Oublier »."
                )),
            }
        }
        Request::SetAtBoot { on } => match set_at_boot(on) {
            Ok(()) => {
                answering.log.write(if on {
                    "this computer will be reachable from the moment it powers on"
                } else {
                    "this computer will only be reachable while ZyrDesk is open"
                });
                Answer::Done
            }
            Err(reason) => {
                Answer::Refused(format!("ce réglage n'a pas pu être enregistré : {reason}"))
            }
        },
        Request::Stop => {
            answering.log.write("stop asked for by the interface");
            answering.order.ask_for_a_stop();
            Answer::Done
        }
        Request::Settings => Answer::Settings(answering.remembered.read().preferred),
        Request::Choose { preferred } => {
            match kept(answering.remembered.set_preferred(preferred)) {
                Ok(()) => {
                    answering.log.write("session settings changed");
                    Answer::Done
                }
                Err(refusal) => refusal,
            }
        }
        // Handled above, where several answers can be given.
        Request::Peers | Request::Sessions => Answer::Done,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use zyr_control::{Service, WayId};

    /// The desk, open on a channel of its own, with everything it needs
    /// around it. Anything asking for the network is left out: what is
    /// checked here is that a question travels and comes back answered.
    struct Bench {
        _desk: Desk,
        channel: String,
        hosting: Hosting,
        fingerprint: Fingerprint,
        folder: std::path::PathBuf,
    }

    impl Bench {
        fn set_up(runtime: &Handle, what: &str) -> Self {
            let folder = std::env::temp_dir().join(format!(
                "zyrdeskd-desk-{}-{what}",
                zyr_proto::random::alphanumeric_string(8)
            ));
            let log = Log::open(&folder.join("service.log")).unwrap();
            let channel = format!("zyrdeskd-test-{}-{what}", std::process::id());
            let fingerprint = zyr_transport::Identity::generate().unwrap().fingerprint();
            let hosting = Hosting::new();

            let desk = Desk::open(
                runtime,
                &channel,
                Answering {
                    fingerprint,
                    ways: Ways::new(log.clone()),
                    hosting: hosting.clone(),
                    remembered: Remembered::at(folder.join("preferences.conf")),
                    neighbours: Found::new(),
                    order: StopOrder::new(),
                    log: log.clone(),
                },
            )
            .unwrap();

            Self {
                _desk: desk,
                channel,
                hosting,
                fingerprint,
                folder,
            }
        }

        async fn caller(&self) -> Service {
            Service::join_on(&self.channel).await.unwrap()
        }
    }

    impl Drop for Bench {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.folder);
        }
    }

    #[test]
    fn the_desk_says_who_this_computer_is_and_what_it_is_doing() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let bench = Bench::set_up(runtime.handle(), "standing");

        runtime.block_on(async {
            let mut caller = bench.caller().await;

            let answer = caller.ask(&Request::Standing).await.unwrap();
            let Answer::Standing(standing) = answer else {
                panic!("attendu un état, reçu {answer}");
            };
            assert_eq!(standing.protocol, PROTOCOL);
            assert_eq!(standing.fingerprint, bench.fingerprint);
            assert_eq!(standing.ways, 0);
            // No engine has answered here, so this computer is not
            // reachable and must not claim otherwise.
            assert!(!standing.hosting);
            assert_eq!(standing.holdup, Holdup::Starting);

            // Et l'empêchement voyage : sans lui, un moteur absent se
            // lit comme un moteur qui démarre, indéfiniment.
            bench.hosting.held_by(Holdup::EngineMissing);
            let Ok(Answer::Standing(standing)) = caller.ask(&Request::Standing).await else {
                panic!("attendu un état");
            };
            assert_eq!(standing.holdup, Holdup::EngineMissing);

            bench.hosting.open();
            let answer = caller.ask(&Request::Standing).await.unwrap();
            let Answer::Standing(standing) = answer else {
                panic!("attendu un état, reçu {answer}");
            };
            assert!(standing.hosting);
        });
    }

    #[test]
    fn nothing_to_list_is_an_empty_list_and_not_a_refusal() {
        // A list answer is several messages ended by « done ». With
        // nothing to list, only the ending is said, and the caller has
        // to come back with an empty list rather than hang or fail.
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let bench = Bench::set_up(runtime.handle(), "lists");

        runtime.block_on(async {
            let mut caller = bench.caller().await;
            for request in [Request::Peers, Request::Sessions] {
                let found = caller.ask_for_a_list(&request).await.unwrap();
                assert!(found.is_empty(), "sur « {request} » : {found:?}");
            }

            // And the channel is still usable afterwards: the ending was
            // consumed, not left in the way of the next question.
            let answer = caller.ask(&Request::Standing).await.unwrap();
            assert!(matches!(answer, Answer::Standing(_)), "{answer}");
        });
    }

    #[test]
    fn turning_remote_access_off_is_answered_and_remembered() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let bench = Bench::set_up(runtime.handle(), "hosting");

        runtime.block_on(async {
            let mut caller = bench.caller().await;

            // What was asked for and what the engine has reached are two
            // different things: only the first moves here.
            let answer = caller
                .ask(&Request::SetHosting { on: false })
                .await
                .unwrap();
            assert!(matches!(answer, Answer::Done), "{answer}");

            let Ok(Answer::Standing(standing)) = caller.ask(&Request::Standing).await else {
                panic!("attendu un état");
            };
            assert!(!standing.wanted);

            let answer = caller.ask(&Request::SetHosting { on: true }).await.unwrap();
            assert!(matches!(answer, Answer::Done), "{answer}");
            let Ok(Answer::Standing(standing)) = caller.ask(&Request::Standing).await else {
                panic!("attendu un état");
            };
            assert!(standing.wanted);
        });
    }

    #[test]
    fn what_a_session_looks_like_is_chosen_once_and_answered_after() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let bench = Bench::set_up(runtime.handle(), "settings");

        runtime.block_on(async {
            use zyr_proto::session::{Asked, Preferred};

            let mut caller = bench.caller().await;

            let Ok(Answer::Settings(before)) = caller.ask(&Request::Settings).await else {
                panic!("attendu des réglages");
            };
            assert_eq!(before, Preferred::default());

            let wanted = Preferred {
                asked: Asked::Fixed(2560, 1440),
                stats_overlay: true,
                ..before
            };
            let answer = caller
                .ask(&Request::Choose { preferred: wanted })
                .await
                .unwrap();
            assert!(matches!(answer, Answer::Done), "{answer}");

            let Ok(Answer::Settings(after)) = caller.ask(&Request::Settings).await else {
                panic!("attendu des réglages");
            };
            assert_eq!(after, wanted);

            // Et l'accès distant, qui partage le même fichier, n'a pas
            // été emporté au passage.
            let Ok(Answer::Standing(standing)) = caller.ask(&Request::Standing).await else {
                panic!("attendu un état");
            };
            assert!(standing.wanted);
        });
    }

    #[test]
    fn a_way_that_does_not_exist_cannot_be_held() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let bench = Bench::set_up(runtime.handle(), "hold");

        runtime.block_on(async {
            let mut caller = bench.caller().await;
            let answer = caller
                .ask(&Request::Hold {
                    way: WayId(404),
                    process: 1234,
                })
                .await
                .unwrap();
            // Saying yes here would leave the caller believing its
            // session is watched when nothing watches it.
            assert!(matches!(answer, Answer::Refused(_)), "reçu {answer}");
        });
    }

    #[test]
    fn closing_a_way_twice_is_not_an_error() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let bench = Bench::set_up(runtime.handle(), "release");

        runtime.block_on(async {
            let mut caller = bench.caller().await;
            for _ in 0..2 {
                let answer = caller
                    .ask(&Request::Release { way: WayId(7) })
                    .await
                    .unwrap();
                assert!(matches!(answer, Answer::Done), "reçu {answer}");
            }
        });
    }

    #[test]
    fn a_program_the_desk_cannot_understand_is_told_so_and_kept_on() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let bench = Bench::set_up(runtime.handle(), "gibberish");

        runtime.block_on(async {
            let mut caller = bench.caller().await;
            // Reaching the desk through the channel itself, since a
            // nonsense request cannot be built from the request type.
            let mut raw = zyr_control::pipe::call(&bench.channel).await.unwrap();
            raw.say("teleport way=1").await.unwrap();
            let answer = raw.hear().await.unwrap().unwrap();
            assert!(answer.starts_with("no "), "reçu « {answer} »");

            // And the desk is still there for everyone else.
            assert!(matches!(
                caller.ask(&Request::Standing).await.unwrap(),
                Answer::Standing(_)
            ));
        });
    }

    #[test]
    fn several_programs_are_served_at_the_same_time() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let bench = Bench::set_up(runtime.handle(), "several");

        runtime.block_on(async {
            // The interface and the command line are both expected to be
            // open at once, and neither may wait on the other.
            let mut first = bench.caller().await;
            let mut second = bench.caller().await;
            for caller in [&mut first, &mut second] {
                assert!(matches!(
                    caller.ask(&Request::Standing).await.unwrap(),
                    Answer::Standing(_)
                ));
            }
        });
    }
}
