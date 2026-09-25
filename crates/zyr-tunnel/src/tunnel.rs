//! The tunnel at work, on one side or the other.
//!
//! Both sides do the same job, mirrored. On each, one engine talks to
//! the service through a local link, and the tunnel carries what that
//! link says to the other computer and back: the host engine on one
//! side, the player on the other. Neither of them opens a socket, and
//! neither knows a tunnel exists.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::oneshot;
use tokio::task::JoinSet;
use zyr_control::link::{Link, LinkListener};
use zyr_proto::log::Log;
use zyr_transport::{Connection, RecvStream, SendStream};

use crate::aside::{self, Answers};
use crate::channel::StreamChannel;
use crate::flow::{self, Flow};
use crate::pump::{self, Counters, Reading};
use crate::queue::DatagramQueue;
use crate::service::ServiceSide;

/// What this crate's lines are filed under.
const TAG: &str = "tunnel";

/// One side of the tunnel, pumps running.
///
/// Everything stops when it is dropped: the pumps have no reason to
/// outlive the session they serve. [`Tunnel::close`] stops them and waits
/// until they have let go of the link, for whoever needs the other end
/// to see it closed before going on.
pub struct Tunnel {
    tasks: JoinSet<io::Result<()>>,
    counters: Arc<Counters>,
    somebody_there: Arc<AtomicBool>,
}

/// Whether somebody is at the other end of a tunnel's link, readable
/// apart from the tunnel, by whoever speaks to that somebody only while
/// they are there.
#[derive(Debug, Clone)]
pub struct Presence(Arc<AtomicBool>);

impl Presence {
    pub fn here(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// The engine's stream, handed from whoever accepts it to whoever
/// carries it. Taken once: a second one is refused.
type EngineStream = Arc<Mutex<Option<oneshot::Sender<(SendStream, RecvStream)>>>>;

impl Tunnel {
    /// Host side: the engine of this session is on the other end of
    /// `engine`, and the player on the far computer opens the engine's
    /// stream towards it.
    ///
    /// `answering` answers ZyrDesk's own channel for this session. The
    /// session is already open: it was opened by the question
    /// [`aside::until_a_session_opens`] handed back, and a second opening
    /// is refused.
    ///
    /// `log` says what it did while it did it, for a stream that goes
    /// quiet without a word. `None` where nobody is watching, a benchmark
    /// or a test.
    ///
    /// Called within a tokio runtime, which the pumps are spawned on.
    pub fn host(
        connection: Connection,
        answering: Arc<dyn Answers>,
        engine: Link,
        service: ServiceSide,
        log: Option<Log>,
    ) -> Self {
        let log = log.map(|log| log.about(TAG));
        let counters = Arc::new(Counters::default());
        let flow = Arc::new(Flow::default());
        let somebody_there = Arc::new(AtomicBool::new(true));
        let datagrams = Arc::new(DatagramQueue::default());
        let (handing, handed) = oneshot::channel();
        let engine_stream: EngineStream = Arc::new(Mutex::new(Some(handing)));

        let mut tasks = JoinSet::new();
        watch_the_flow(&mut tasks, &connection, &flow, log.as_ref());
        tasks.spawn(out_of_the_tunnel(
            connection.clone(),
            datagrams.clone(),
            somebody_there.clone(),
            counters.clone(),
            flow.clone(),
        ));
        {
            let connection = connection.clone();
            let counters = counters.clone();
            tasks.spawn(async move {
                serve_the_streams(&connection, answering, engine_stream, &counters, log).await
            });
        }
        {
            let counters = counters.clone();
            tasks.spawn(async move {
                let accepted = async {
                    handed
                        .await
                        .map_err(|_| io::Error::other("le flux du moteur n'est jamais arrivé"))
                };
                pump::between(
                    engine,
                    accepted,
                    &connection,
                    &datagrams,
                    service,
                    &counters,
                    &flow,
                )
                .await
            });
        }

        Self {
            tasks,
            counters,
            somebody_there,
        }
    }

    /// Client side: the player connects to `player`, and its session
    /// runs through the engine's stream this side opens once it has.
    ///
    /// Handed back at once: the player is started with the link's name
    /// and connects a moment later. How long it may take is the caller's
    /// to decide, reading [`Tunnel::connected`]; the tunnel waits for as
    /// long as it lives.
    ///
    /// Called within a tokio runtime, which the pumps are spawned on.
    pub fn client(
        connection: Connection,
        player: LinkListener,
        service: ServiceSide,
        log: Option<Log>,
    ) -> Self {
        let log = log.map(|log| log.about(TAG));
        let counters = Arc::new(Counters::default());
        let flow = Arc::new(Flow::default());
        let somebody_there = Arc::new(AtomicBool::new(false));
        let datagrams = Arc::new(DatagramQueue::default());

        let mut tasks = JoinSet::new();
        watch_the_flow(&mut tasks, &connection, &flow, log.as_ref());
        tasks.spawn(out_of_the_tunnel(
            connection.clone(),
            datagrams.clone(),
            somebody_there.clone(),
            counters.clone(),
            flow.clone(),
        ));
        {
            let counters = counters.clone();
            let somebody_there = somebody_there.clone();
            tasks.spawn(async move {
                let link = player.accept().await?;
                somebody_there.store(true, Ordering::Relaxed);
                if let Some(log) = &log {
                    log.write(&match link.peer_process() {
                        Some(process) => format!("the player connected, process {process}"),
                        None => "the player connected".to_string(),
                    });
                }
                let opened = async {
                    let (mut sending, receiving) =
                        connection.open_stream().await.map_err(io::Error::other)?;
                    pump::announce(&mut sending, StreamChannel::Engine).await?;
                    Ok((sending, receiving))
                };
                pump::between(
                    link,
                    opened,
                    &connection,
                    &datagrams,
                    service,
                    &counters,
                    &flow,
                )
                .await
            });
        }

        Self {
            tasks,
            counters,
            somebody_there,
        }
    }

    pub fn reading(&self) -> Reading {
        self.counters.reading()
    }

    /// Shared counters, readable while the tunnel runs.
    ///
    /// Useful to watch the traffic without holding the tunnel still, for
    /// instance to learn when it starts carrying anything.
    pub fn counters(&self) -> Arc<Counters> {
        self.counters.clone()
    }

    /// Whether somebody is at the other end of the link: always on the
    /// host, whose engine was connected before its tunnel started, and
    /// once the player has connected on the client.
    pub fn connected(&self) -> bool {
        self.somebody_there.load(Ordering::Relaxed)
    }

    /// The same, for as long as it is wanted.
    pub fn presence(&self) -> Presence {
        Presence(self.somebody_there.clone())
    }

    /// Waits for the tunnel to stop, and says why it stopped.
    ///
    /// The pumps run for as long as the link and the connection hold:
    /// the first one to hand back signals the end of the session.
    pub async fn wait(&mut self) -> io::Result<()> {
        match self.tasks.join_next().await {
            Some(outcome) => stopped_because(outcome),
            None => Ok(()),
        }
    }

    /// Why the tunnel stopped, when it already has, without waiting for
    /// it to.
    ///
    /// For the side that holds tunnels rather than waiting on them: a
    /// pump that dies there ends the session all the same, and until
    /// this existed it did so without a word.
    pub fn stopped(&mut self) -> Option<io::Result<()>> {
        self.tasks.try_join_next().map(stopped_because)
    }

    /// Stops every pump and waits until they are gone, the link with
    /// them: once this returns, the other end of the link has been let
    /// go of.
    pub async fn close(mut self) {
        self.tasks.shutdown().await;
    }
}

/// What a pump handing back means, a panic in it included.
fn stopped_because(outcome: Result<io::Result<()>, tokio::task::JoinError>) -> io::Result<()> {
    outcome.map_err(io::Error::other)?
}

async fn out_of_the_tunnel(
    connection: Connection,
    datagrams: Arc<DatagramQueue>,
    somebody_there: Arc<AtomicBool>,
    counters: Arc<Counters>,
    flow: Arc<Flow>,
) -> io::Result<()> {
    pump::out_of_the_tunnel(&connection, &datagrams, &somebody_there, &counters, &flow).await
}

/// Writes what went through this side every second, where somebody reads
/// the journal. Never ends by itself: the tunnel stopping stops it.
fn watch_the_flow(
    tasks: &mut JoinSet<io::Result<()>>,
    connection: &Connection,
    flow: &Arc<Flow>,
    log: Option<&Log>,
) {
    if let Some(log) = log {
        let (connection, flow, log) = (connection.clone(), flow.clone(), log.clone());
        tasks.spawn(async move {
            flow::watch(connection, flow, log).await;
            Ok(())
        });
    }
}

/// Takes the reliable streams the far computer opens: its questions, and
/// the engine's stream, once.
async fn serve_the_streams(
    connection: &Connection,
    answering: Arc<dyn Answers>,
    engine_stream: EngineStream,
    counters: &Arc<Counters>,
    log: Option<Log>,
) -> io::Result<()> {
    let mut streams = JoinSet::new();
    loop {
        let (sending, receiving) = connection.accept_stream().await.map_err(io::Error::other)?;
        // One stream's failure stays on that stream: a question that
        // goes wrong must not take the running session with it.
        streams.spawn(one_stream(
            sending,
            receiving,
            answering.clone(),
            engine_stream.clone(),
            counters.clone(),
            log.clone(),
        ));
        while streams.try_join_next().is_some() {}
    }
}

async fn one_stream(
    sending: SendStream,
    mut receiving: RecvStream,
    answering: Arc<dyn Answers>,
    engine_stream: EngineStream,
    counters: Arc<Counters>,
    log: Option<Log>,
) {
    match pump::read_announcement(&mut receiving).await {
        Ok(StreamChannel::ZyrDesk) => {
            if let (Err(e), Some(log)) = (aside::answer(sending, receiving, answering).await, &log)
            {
                log.debug(|| format!("a question went unanswered: {e}"));
            }
        }
        Ok(StreamChannel::Engine) => {
            let handing = engine_stream.lock().expect("flux du moteur").take();
            match handing {
                Some(handing) => {
                    if let Some(log) = &log {
                        log.debug(|| "the player opened the engine's stream".to_string());
                    }
                    // Refused only once the pumps are gone, which is the
                    // session ending anyway.
                    let _ = handing.send((sending, receiving));
                }
                None => {
                    counters.refused();
                    if let Some(log) = &log {
                        log.write("a second engine stream was refused: a session carries one");
                    }
                }
            }
        }
        Err(e) => {
            counters.refused();
            if let Some(log) = &log {
                log.write(&format!("a stream from the tunnel never named itself: {e}"));
            }
        }
    }
}
