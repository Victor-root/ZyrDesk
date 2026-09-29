//! Answering the programs that drive the service.
//!
//! The interface and the command line own nothing: they ask here, and
//! the service does the holding. This desk is open for the whole life of
//! the service, engine or no engine, because most of what is asked of it
//! has nothing to do with the local engine.

use std::io;
use std::net::SocketAddr;

use tokio::runtime::Handle;
use tokio::task::{JoinHandle, JoinSet};
use zyr_control::pipe::Heard;
use zyr_control::{Answer, Door, PROTOCOL, Reached, Request, Standing};
use zyr_proto::fact::Fact;
use zyr_proto::fingerprint::Fingerprint;
use zyr_proto::log::Log;
use zyr_proto::net::TUNNEL_PORT;
use zyr_proto::paths;
use zyr_proto::sifting::Sifting;
use zyr_transport::authorized;

use crate::account::{self, Attaching};
use crate::known;
use crate::machine::Machine;
use crate::supervisor::{StopOrder, Wiring};
use crate::transfer;
use crate::ways::Knock;

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

/// What this module's lines are filed under.
pub const TAG: &str = "control";

/// Everything an answer is drawn from.
///
/// Built by the supervisor, which owns all of it: the desk holds no
/// state of its own, it reads and reports.
#[derive(Clone)]
pub struct Answering {
    pub fingerprint: Fingerprint,
    pub machine: Machine,
    /// What ends the service. Held here so that quitting the interface
    /// can take the service with it without asking Windows, which would
    /// mean an administrator prompt at every quit.
    pub order: StopOrder,
    /// Whether Windows starts the service on its own, which only the
    /// program that registered it can say or change.
    pub wiring: Wiring,
    pub log: Log,
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
            Err(e) => vec![Answer::Refused(
                Fact::new("service.misunderstood").with("detail", e),
            )],
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
        Request::Peers => ended(
            answering
                .machine
                .on_screen(&answering.log)
                .into_iter()
                .map(Answer::Peer)
                .collect(),
        ),
        Request::Sessions => ended(
            answering
                .machine
                .ways
                .held()
                .into_iter()
                .map(Answer::Session)
                .collect(),
        ),
        Request::Watching => ended(
            answering
                .machine
                .incoming
                .watching()
                .into_iter()
                .map(Answer::Watching)
                .collect(),
        ),
        Request::Devices => ended(
            answering
                .machine
                .account
                .devices()
                .into_iter()
                .map(Answer::Device)
                .collect(),
        ),
        other => vec![one(other, answering).await],
    }
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
fn kept(written: std::io::Result<()>) -> Result<(), Fact> {
    written.map_err(|e| Fact::new("setting.not_saved").with("detail", e))
}

/// Every address that computer has answered on.
///
/// A machine with two cards answers on both, and only trying tells which
/// one is the cable and which is a detour through a virtual adapter. So
/// whatever is being reached for is reached through all of them at once
/// and the fastest to answer wins.
fn every_address_of(peer: Fingerprint, answering: &Answering) -> Vec<std::net::IpAddr> {
    answering
        .machine
        .neighbours
        .peers()
        .into_iter()
        .find(|seen| seen.fingerprint == peer)
        .map(|seen| seen.addresses)
        .unwrap_or_default()
}

/// The addresses of a computer this network announces, and nothing else.
///
/// The very list `where_to_knock` falls back on, asked for on purpose:
/// what this machine has heard that computer answer at, and the address
/// it was named by. No meeting, no relay, and the account not so much as
/// consulted, so a session opened on this stands or falls with this
/// network alone.
///
/// What it does not do is judge the addresses. They come from what this
/// network announced or from what somebody wrote down, and a rule
/// invented here about which of them count as near enough would be a
/// second promise nobody asked for. The promise made is the one that
/// matters: nothing outside this machine is asked where to knock.
///
/// A computer of the account that this network does not announce is
/// named by its road at the server rather than by an address, and there
/// is nothing here to reach it at: saying so plainly is worth more than
/// letting that road fail to resolve.
fn only_on_this_network(host: &str, also: Vec<std::net::IpAddr>) -> Result<Vec<SocketAddr>, Fact> {
    if account::device_of_road(host).is_some() {
        return Err(Fact::new("reach.not_on_this_network"));
    }
    crate::ways::where_to_knock(host, &also)
}

/// A local attempt whose port stayed silent, said in full.
///
/// The addresses tried were the ones this network announced a moment
/// ago, so the far computer is there and answering; a timeout on every
/// one of them is not a computer that is off, and reading it as one
/// costs an evening. What did not answer is the tunnel's port, and from
/// this end the three things that do that cannot be told apart. So all
/// three are named, and the far machine's own journal settles it on its
/// « Tunnel » line, which is one click away on this very card. Any other
/// refusal is told as it is.
fn silent_here(refused: Fact) -> Fact {
    if refused.code() != "reach.port_silent" {
        return refused;
    }
    Fact::new("reach.port_silent_here")
        .with("host", refused.value("host").unwrap_or_default())
        .with("port", TUNNEL_PORT)
        .with("detail", refused.value("detail").unwrap_or_default())
}

/// Where to knock to reach that computer, what to call it on the way,
/// and the meeting it took to know, when it took one.
///
/// A meeting whenever one can be had, and the addresses only when it
/// cannot. The server presents the two, the far one lets this one in,
/// and what comes back is a junction that probes every address this
/// machine already knew of it, every one the far computer names, and a
/// relay if the server has one. Knocking at an address is the same
/// journey with one road, no way to change it, and no way back when it
/// stops carrying: it is what is left when there is no account, no
/// server to be reached, or nobody ready at the other end.
///
/// `only_here` asks for that second journey on purpose, for a computer
/// this network announces, and it is the one case where the account is
/// not so much as consulted: a session opened this way owes nothing to a
/// line leaving the house, and answers of itself the question a session
/// crossing the Internet cannot, which is whether a silence was ours.
async fn where_to_knock(
    host: &str,
    peer: Fingerprint,
    only_here: bool,
    answering: &Answering,
) -> Result<(String, Knock), Fact> {
    let also = every_address_of(peer, answering);
    if only_here {
        // Written before the line that says where to knock, and only
        // when it is a choice: without it, a session held on this
        // network would read in the journal like a session the server
        // turned down, which is something else entirely.
        answering.log.write(&format!(
            "{host} was asked for on this network alone: no account, no meeting, no relay"
        ));
        return Ok((
            host.to_string(),
            Knock::At(only_on_this_network(host, also)?),
        ));
    }
    let device = account::device_of_road(host)
        .map(str::to_string)
        .or_else(|| answering.machine.account.met_through_the_server(peer));
    let Some(device) = device else {
        let candidates = crate::ways::where_to_knock(host, &also)?;
        return Ok((host.to_string(), Knock::At(candidates)));
    };
    let mut met = answering.machine.account.rendezvous(&device).await?;
    // The fingerprint on the card is what the tunnel will pin, and the
    // server has just named one: two different answers is a computer
    // that changed key, or a server that lies, and neither is knocked on.
    if met.peer != peer {
        answering.machine.account.ended(&met.session);
        return Err(Fact::new("reach.fingerprint_changed")
            .with("name", &met.name)
            .with("told", met.peer)
            .with("expected", peer));
    }
    // What the local network saw of it is worth probing before anything
    // the server passes on.
    met.known = also
        .into_iter()
        .map(|address| SocketAddr::new(address, TUNNEL_PORT))
        .collect();
    let label = met.name.clone();
    Ok((label, Knock::Through(Box::new(met))))
}

/// One question asked of a computer, wherever it is: the meeting it took
/// to reach it is over once the question is answered.
async fn one_question<T>(
    host: &str,
    peer: Fingerprint,
    answering: &Answering,
    ask: impl AsyncFnOnce(&str, Knock) -> Result<T, Fact>,
) -> Result<T, Fact> {
    // A question is asked through the best way available: what is
    // chosen to be held on this network is a session, never a round
    // trip of two words.
    let (label, knock) = where_to_knock(host, peer, false, answering).await?;
    let meeting = knock.session();
    let answered = ask(&label, knock).await;
    if let Some(session) = meeting {
        answering.machine.account.ended(&session);
    }
    answered
}

/// One attempt at reaching a computer for a session: a fresh meeting
/// through the account, if the road runs through it, and the way opened
/// on the back of it. What the meeting was for is over the moment this
/// answers, one way or the other: followed by the account on success,
/// ended on it on failure.
async fn one_reach(
    host: &str,
    peer: Fingerprint,
    media: zyr_transport::MediaProfile,
    only_here: bool,
    answering: &Answering,
) -> Result<Reached, Fact> {
    let (label, knock) = where_to_knock(host, peer, only_here, answering).await?;
    let meeting = knock.session();
    match answering
        .machine
        .ways
        .open(&label, peer, media, knock)
        .await
    {
        Ok(reached) => {
            if let Some(session) = meeting {
                answering.machine.account.follow(reached.way, session);
            }
            Ok(reached)
        }
        Err(reason) => {
            if let Some(session) = meeting {
                answering.machine.account.ended(&session);
            }
            Err(reason)
        }
    }
}

/// Writes down what a far computer just handed over about what it
/// reaches, replacing whatever an earlier such fetch left there.
///
/// Landed next to this computer's own file of the same name, in the
/// same folder every journal already points to: the whole point is
/// sparing a copy and a paste of something that would otherwise cost a
/// walk to the other machine. Best-effort, like the fetch it follows:
/// a page fetched but not written down is still a page fetched.
fn save_the_far_reach_log(text: &str, answering: &Answering) {
    let path = paths::reach_distant_log();
    if let Err(e) = std::fs::write(&path, text) {
        answering.log.write(&format!(
            "what a far computer reaches could not be written to {}: {e}",
            path.display()
        ));
    }
}

async fn one(request: Request, answering: &Answering) -> Answer {
    match request {
        Request::Standing => {
            let held = answering.machine.hosting.standing();
            Answer::Standing(Standing {
                protocol: PROTOCOL,
                build: zyr_proto::BUILD.to_string(),
                fingerprint: answering.fingerprint,
                hosting: held.is_none(),
                holdup: held.unwrap_or_default(),
                wanted: answering.machine.remembered.remote_access(),
                trusting: answering.machine.remembered.trust_local_network(),
                ecn: answering.machine.remembered.read().ecn,
                fixed_port: answering.machine.remembered.read().fixed_port,
                at_boot: (answering.wiring.starts_with_windows)(),
                ways: answering.machine.ways.count(),
            })
        }
        Request::Reach {
            host,
            peer,
            bitrate_kbps,
            fps,
            only_here,
        } => {
            let media = zyr_transport::MediaProfile {
                bits_per_second: u64::from(bitrate_kbps) * 1000,
                frames_per_second: fps,
            };
            // Asked once more before giving up: a road that missed this
            // second's worth of probes is common enough, and the account
            // a meeting goes through costs nothing next to the person who
            // would otherwise have to ask again themselves for the very
            // same computer.
            if let Ok(reached) = one_reach(&host, peer, media, only_here, answering).await {
                return Answer::Reached(reached);
            }
            answering.log.write(&format!(
                "{host}: first attempt did not reach it, trying once more"
            ));
            match one_reach(&host, peer, media, only_here, answering).await {
                Ok(reached) => Answer::Reached(reached),
                Err(refused) => Answer::Refused(if only_here {
                    silent_here(refused)
                } else {
                    refused
                }),
            }
        }
        Request::SecureAttention { way } => {
            match answering
                .machine
                .ways
                .ask_for_the_secure_attention(way)
                .await
            {
                Ok(()) => Answer::Done,
                Err(reason) => Answer::Refused(reason),
            }
        }
        Request::LockScreen { way } => match answering.machine.ways.ask_to_lock(way).await {
            Ok(()) => Answer::Done,
            Err(reason) => Answer::Refused(reason),
        },
        Request::FarScreen { way, wanted } => {
            match answering.machine.ways.ask_for_a_screen(way, wanted).await {
                Ok(size) => Answer::Showing { size },
                Err(reason) => Answer::Refused(reason),
            }
        }
        Request::Hush { way, quiet } => {
            match answering.machine.ways.ask_to_hush(way, quiet).await {
                Ok(()) => Answer::Done,
                Err(reason) => Answer::Refused(reason),
            }
        }
        Request::FarPointer { way } => {
            match answering
                .machine
                .ways
                .ask_what_shape_its_pointer_has(way)
                .await
            {
                Ok(shape) => Answer::Pointer(shape),
                Err(reason) => Answer::Refused(reason),
            }
        }
        Request::FarScreens { way } => {
            match answering.machine.ways.ask_what_screens_it_has(way).await {
                Ok(listed) => Answer::Screens(listed),
                Err(reason) => Answer::Refused(reason),
            }
        }
        Request::FilmFarScreen { way, id } => {
            match answering
                .machine
                .ways
                .ask_to_film_this_screen(way, id)
                .await
            {
                Ok(()) => Answer::Done,
                Err(reason) => Answer::Refused(reason),
            }
        }
        Request::FilesComing => Answer::Coming(transfer::how_far()),
        Request::Hold { way, process } => {
            if answering.machine.ways.hold(way, process) {
                Answer::Done
            } else {
                Answer::Refused(Fact::new("way.gone").with("way", way))
            }
        }
        Request::Release { way } => {
            answering.machine.ways.release(way);
            // A way already closed is the state that was asked for,
            // reached: saying no would make closing twice an error.
            Answer::Done
        }
        Request::SetHosting { on } => {
            match kept(answering.machine.remembered.set_remote_access(on)) {
                Ok(()) => {
                    answering.log.write(if on {
                        "remote access turned on"
                    } else {
                        "remote access turned off"
                    });
                    Answer::Done
                }
                Err(reason) => Answer::Refused(reason),
            }
        }
        Request::SetTrust { on } => {
            match kept(answering.machine.remembered.set_trust_local_network(on)) {
                Ok(()) => {
                    answering.log.write(if on {
                        "the local network is trusted again"
                    } else {
                        "the local network is no longer trusted"
                    });
                    Answer::Done
                }
                Err(reason) => Answer::Refused(reason),
            }
        }
        Request::SetEcn { on } => match kept(answering.machine.remembered.set_ecn(on)) {
            Ok(()) => {
                answering.log.write(if on {
                    "the tunnel's packets carry their congestion mark again"
                } else {
                    "the tunnel's packets leave without their congestion mark"
                });
                Answer::Done
            }
            Err(reason) => Answer::Refused(reason),
        },
        Request::SetFixedPort { on } => {
            match kept(answering.machine.remembered.set_fixed_port(on)) {
                Ok(()) => {
                    answering.log.write(if on {
                        "the door listens on the product's own port again"
                    } else {
                        "the door listens on a port the system picks"
                    });
                    Answer::Done
                }
                Err(reason) => Answer::Refused(reason),
            }
        }
        Request::Authorize { peer, host, name } => {
            // This fingerprint is already this computer's own: writing it
            // down would open nothing and would suggest a pairing had
            // been made.
            if peer == answering.fingerprint {
                return Answer::Refused(Fact::new("peer.own_fingerprint"));
            }
            if let Err(e) = authorized::add(&paths::authorized_devices(), peer) {
                return Answer::Refused(Fact::new("peer.not_written").with("detail", e));
            }
            answering
                .log
                .write(&format!("{peer} written down as allowed in"));

            // The address is what keeps it on the screen. Without it,
            // it would have to be typed again for every session, and
            // that is exactly what this product exists to do away with.
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
                Err(e) => Answer::Refused(Fact::new("peer.not_kept").with("detail", e)),
            }
        }
        Request::Forget { peer } => {
            // Both lists, otherwise a computer taken off the screen
            // would still get in, which nobody would guess.
            //
            // The authorisation first. If the second write fails, the
            // computer stays visible without being able to get in any
            // more, and the refusal says to try again; in the other
            // order, it would have vanished from the screen while
            // keeping the right to get in, invisible and impossible to
            // guess.
            if let Err(e) = authorized::remove(&paths::authorized_devices(), peer) {
                return Answer::Refused(Fact::new("peer.not_forgotten").with("detail", e));
            }
            match known::remove(&paths::known_computers(), peer) {
                Ok(_) => {
                    answering.log.write(&format!("{peer} forgotten"));
                    Answer::Done
                }
                Err(e) => Answer::Refused(Fact::new("peer.not_removed").with("detail", e)),
            }
        }
        Request::Kick { peer } => {
            if answering.machine.incoming.kick(peer) {
                answering.log.write(&format!("{peer} disconnected"));
                Answer::Done
            } else {
                Answer::Refused(Fact::new("peer.not_connected"))
            }
        }
        Request::SetAtBoot { on } => match (answering.wiring.start_with_windows)(on) {
            Ok(()) => {
                answering.log.write(if on {
                    "this computer will be reachable from the moment it powers on"
                } else {
                    "this computer will only be reachable while ZyrDesk is open"
                });
                Answer::Done
            }
            Err(reason) => {
                Answer::Refused(Fact::new("setting.at_boot_not_saved").with("detail", reason))
            }
        },
        Request::Stop => {
            answering.log.write("stop asked for by the interface");
            answering.order.ask_for_a_stop();
            Answer::Done
        }
        Request::Journal { sift } => Answer::Journal(answering.machine.journal(
            answering.fingerprint,
            &answering.log,
            &Sifting::of(&sift),
        )),
        Request::FarJournal { host, peer, sift } => {
            let ways = &answering.machine.ways;
            match one_question(&host, peer, answering, async |label, knock| {
                ways.ask_a_computer_for_its_journal(label, peer, knock, &sift)
                    .await
            })
            .await
            {
                Ok((text, reach)) => {
                    if let Some(reach) = reach {
                        save_the_far_reach_log(&reach, answering);
                    }
                    Answer::Journal(text)
                }
                Err(reason) => Answer::Refused(reason),
            }
        }
        Request::ClearFarJournal { host, peer } => {
            let ways = &answering.machine.ways;
            match one_question(&host, peer, answering, async |label, knock| {
                ways.ask_a_computer_to_empty_its_journal(label, peer, knock)
                    .await
            })
            .await
            {
                Ok(()) => Answer::Done,
                Err(reason) => Answer::Refused(reason),
            }
        }
        Request::Settings => Answer::Settings(answering.machine.remembered.read().preferred),
        Request::Choose { preferred } => {
            match kept(answering.machine.remembered.set_preferred(preferred)) {
                Ok(()) => {
                    answering.log.write("session settings changed");
                    Answer::Done
                }
                Err(reason) => Answer::Refused(reason),
            }
        }
        Request::Account => Answer::Account(answering.machine.account.standing()),
        Request::Attach(attach) => match answering.machine.account.attach(attach).await {
            Ok(()) => Answer::Done,
            Err(Attaching::Unpinned(presented)) => Answer::Unpinned { presented },
            Err(Attaching::Refused(reason)) => Answer::Refused(reason),
        },
        Request::Detach => match answering.machine.account.detach().await {
            Ok(()) => Answer::Done,
            Err(reason) => Answer::Refused(reason),
        },
        Request::RenameDevice { device, name } => {
            match answering.machine.account.rename(&device, &name).await {
                Ok(()) => Answer::Done,
                Err(reason) => Answer::Refused(reason),
            }
        }
        Request::RevokeDevice { device } => match answering.machine.account.revoke(&device).await {
            Ok(()) => Answer::Done,
            Err(reason) => Answer::Refused(reason),
        },
        // Handled above, where several answers can be given.
        Request::Peers | Request::Sessions | Request::Watching | Request::Devices => Answer::Done,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use zyr_control::{Holdup, Service, WayId};

    use crate::machine::Hosting;

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
            let remembered = crate::preferences::Remembered::at(folder.join("preferences.conf"));
            let machine = Machine {
                hosting: Hosting::new(),
                ways: crate::ways::Ways::new(log.clone(), remembered.clone()),
                incoming: crate::incoming::Incoming::default(),
                remembered,
                neighbours: zyr_lan::Found::new(),
                account: crate::account::Account::at(folder.join("account.conf"), log.clone()),
                door: crate::machine::Door::default(),
            };

            let desk = Desk::open(
                runtime,
                &channel,
                Answering {
                    fingerprint,
                    machine: machine.clone(),
                    order: StopOrder::new(),
                    wiring: Wiring {
                        engine_missing_from: |_| Vec::new(),
                        starts_with_windows: || false,
                        start_with_windows: |_| Err("there is no Windows here to ask".to_string()),
                    },
                    log: log.about(TAG),
                },
            )
            .unwrap();

            Self {
                _desk: desk,
                channel,
                hosting: machine.hosting,
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
                panic!("expected a state, got {answer}");
            };
            assert_eq!(standing.protocol, PROTOCOL);
            assert_eq!(standing.fingerprint, bench.fingerprint);
            assert_eq!(standing.ways, 0);
            // No door is open here, so this computer is not reachable
            // and must not claim otherwise.
            assert!(!standing.hosting);
            assert_eq!(standing.holdup, Holdup::Starting);

            // And the holdup travels: without it, an engine that cannot
            // run reads like a door that is opening, forever.
            bench.hosting.held_by(Holdup::EngineMissing);
            let Ok(Answer::Standing(standing)) = caller.ask(&Request::Standing).await else {
                panic!("expected a state");
            };
            assert_eq!(standing.holdup, Holdup::EngineMissing);

            bench.hosting.open();
            let answer = caller.ask(&Request::Standing).await.unwrap();
            let Answer::Standing(standing) = answer else {
                panic!("expected a state, got {answer}");
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
            for request in [Request::Peers, Request::Sessions, Request::Watching] {
                let found = caller.ask_for_a_list(&request).await.unwrap();
                assert!(found.is_empty(), "on « {request} »: {found:?}");
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

            // What was asked for and what the door has reached are two
            // different things: only the first moves here.
            let answer = caller
                .ask(&Request::SetHosting { on: false })
                .await
                .unwrap();
            assert!(matches!(answer, Answer::Done), "{answer}");

            let Ok(Answer::Standing(standing)) = caller.ask(&Request::Standing).await else {
                panic!("expected a state");
            };
            assert!(!standing.wanted);

            let answer = caller.ask(&Request::SetHosting { on: true }).await.unwrap();
            assert!(matches!(answer, Answer::Done), "{answer}");
            let Ok(Answer::Standing(standing)) = caller.ask(&Request::Standing).await else {
                panic!("expected a state");
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
                panic!("expected settings");
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
                panic!("expected settings");
            };
            assert_eq!(after, wanted);

            // And remote access, which shares the same file, was not
            // carried off along the way.
            let Ok(Answer::Standing(standing)) = caller.ask(&Request::Standing).await else {
                panic!("expected a state");
            };
            assert!(standing.wanted);
        });
    }

    #[test]
    fn without_a_link_the_account_is_none_and_asks_nothing_of_anyone() {
        // Standalone mode, down to the byte: no link, no server, and the
        // questions about the account are answered without a network.
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let bench = Bench::set_up(runtime.handle(), "account");

        runtime.block_on(async {
            let mut caller = bench.caller().await;
            let answer = caller.ask(&Request::Account).await.unwrap();
            assert!(matches!(answer, Answer::Account(None)), "{answer}");

            let devices = caller.ask_for_a_list(&Request::Devices).await.unwrap();
            assert!(devices.is_empty(), "{devices:?}");

            let answer = caller.ask(&Request::Detach).await.unwrap();
            assert!(matches!(answer, Answer::Refused(_)), "{answer}");

            // An account road with no account is refused before a
            // single address is tried.
            let answer = caller
                .ask(&Request::Reach {
                    host: "account:d2".to_string(),
                    peer: bench.fingerprint,
                    bitrate_kbps: 20_000,
                    fps: 60,
                    only_here: false,
                })
                .await
                .unwrap();
            let Answer::Refused(reason) = answer else {
                panic!("expected a refusal, got {answer}");
            };
            assert_eq!(reason.code(), "account.not_attached", "{reason}");

            // And the same road asked for locally is refused for what
            // it is: a road at the server is not an address from here,
            // and letting it fail to resolve would say something else
            // entirely.
            let answer = caller
                .ask(&Request::Reach {
                    host: "account:d2".to_string(),
                    peer: bench.fingerprint,
                    bitrate_kbps: 20_000,
                    fps: 60,
                    only_here: true,
                })
                .await
                .unwrap();
            let Answer::Refused(reason) = answer else {
                panic!("expected a refusal, got {answer}");
            };
            assert_eq!(reason.code(), "reach.not_on_this_network", "{reason}");
        });
    }

    #[test]
    fn a_silence_on_this_network_names_what_makes_it_rather_than_the_network() {
        let silent = Fact::new("reach.port_silent")
            .with("host", "192.168.1.20")
            .with("port", TUNNEL_PORT)
            .with("detail", "timed out");
        let said = silent_here(silent);
        assert_eq!(said.code(), "reach.port_silent_here");
        // The original reason stays whole: it is what says which
        // addresses were tried.
        assert_eq!(said.value("host"), Some("192.168.1.20"));
        assert_eq!(said.value("detail"), Some("timed out"));
        // Anything else is told as it is: a computer this network does
        // not announce is not one whose port stayed silent.
        let elsewhere = Fact::new("reach.not_on_this_network");
        assert_eq!(silent_here(elsewhere.clone()), elsewhere);
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
            assert!(matches!(answer, Answer::Refused(_)), "got {answer}");
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
                assert!(matches!(answer, Answer::Done), "got {answer}");
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
            assert!(answer.starts_with("no "), "got « {answer} »");

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
