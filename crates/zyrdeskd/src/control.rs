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
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::runtime::Handle;
use tokio::task::{JoinHandle, JoinSet};
use zyr_control::pipe::Heard;
use zyr_control::{Answer, Door, PROTOCOL, Request, Standing};
use zyr_transport::Fingerprint;

use zyr_lan::Found;

use crate::log::Log;
use crate::ways::Ways;

/// Whether this computer can be reached right now.
///
/// The supervisor raises it once the engine answers and the tunnel is
/// open, and lowers it whenever the engine is being started over. It is
/// the one thing the desk cannot work out on its own.
#[derive(Clone, Default)]
pub struct Hosting(Arc<AtomicBool>);

impl Hosting {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&self, open: bool) {
        self.0.store(open, Ordering::Relaxed);
    }

    fn open(&self) -> bool {
        self.0.load(Ordering::Relaxed)
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
    pub fn open(
        runtime: &Handle,
        channel: &str,
        fingerprint: Fingerprint,
        ways: Ways,
        hosting: Hosting,
        neighbours: Found,
        log: &Log,
    ) -> io::Result<Self> {
        let _guard = runtime.enter();
        let door = Door::open(channel)?;

        Ok(Self {
            task: runtime.spawn(serve(
                door,
                Answering {
                    fingerprint,
                    ways,
                    hosting,
                    neighbours,
                    log: log.clone(),
                },
            )),
        })
    }
}

/// Everything an answer is drawn from.
#[derive(Clone)]
struct Answering {
    fingerprint: Fingerprint,
    ways: Ways,
    hosting: Hosting,
    neighbours: Found,
    log: Log,
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
        Request::Peers => {
            let mut said: Vec<Answer> = answering
                .neighbours
                .peers()
                .into_iter()
                .map(|peer| {
                    Answer::Peer(zyr_control::Peer {
                        name: peer.name,
                        fingerprint: peer.fingerprint,
                        address: peer.address,
                        port: peer.port,
                    })
                })
                .collect();
            said.push(Answer::Done);
            said
        }
        other => vec![one(other, answering).await],
    }
}

async fn one(request: Request, answering: &Answering) -> Answer {
    match request {
        Request::Standing => Answer::Standing(Standing {
            protocol: PROTOCOL,
            fingerprint: answering.fingerprint,
            hosting: answering.hosting.open(),
            ways: answering.ways.count(),
        }),
        Request::Reach { host, peer, media } => {
            match answering.ways.open(&host, peer, media).await {
                Ok(reached) => Answer::Reached(reached),
                Err(reason) => Answer::Refused(reason),
            }
        }
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
        // Handled above, where several answers can be given.
        Request::Peers => Answer::Done,
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
                fingerprint,
                Ways::new(log.clone()),
                hosting.clone(),
                Found::new(),
                &log,
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

            bench.hosting.set(true);
            let answer = caller.ask(&Request::Standing).await.unwrap();
            let Answer::Standing(standing) = answer else {
                panic!("attendu un état, reçu {answer}");
            };
            assert!(standing.hosting);
        });
    }

    #[test]
    fn an_empty_neighbourhood_is_an_empty_list_and_not_a_refusal() {
        // A list answer is several messages ended by « done ». With
        // nothing to list, only the ending is said, and the caller has
        // to come back with an empty list rather than hang or fail.
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let bench = Bench::set_up(runtime.handle(), "peers");

        runtime.block_on(async {
            let mut caller = bench.caller().await;
            let found = caller.ask_for_a_list(&Request::Peers).await.unwrap();
            assert!(found.is_empty(), "{found:?}");

            // And the channel is still usable afterwards: the ending was
            // consumed, not left in the way of the next question.
            let answer = caller.ask(&Request::Standing).await.unwrap();
            assert!(matches!(answer, Answer::Standing(_)), "{answer}");
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
