//! The tunnel at work, on one side or the other.
//!
//! Both sides do the same job, mirrored. On the client side the engine
//! believes it is reaching the remote computer: it actually finds local
//! ports that pour everything into the encrypted connection. On the host
//! side, what comes out is handed to the engine over loopback as though
//! it came from the network. Neither engine knows a tunnel exists.

use std::io;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinSet;
use zyr_proto::log::Log;
use zyr_proto::net::EnginePorts;
use zyr_transport::{Connection, RecvStream, SendStream};

use crate::aside::{self, Answers};
use crate::channel::{DatagramChannel, StreamChannel};
use crate::pump::{self, Counters, DatagramPorts, Reading};

/// What this crate's lines are filed under.
const TAG: &str = "tunnel";

/// One side of the tunnel, pumps running.
///
/// Everything stops when it is dropped: the pumps have no reason to
/// outlive the session they serve.
pub struct Tunnel {
    tasks: JoinSet<io::Result<()>>,
    counters: Arc<Counters>,
}

impl Tunnel {
    /// Host side: what comes out of the tunnel is handed to the local
    /// engine.
    ///
    /// `answering` is the local engine seen from the tunnel: its ports,
    /// and what it can be asked on ZyrDesk's own channel.
    ///
    /// `log` says what it did while it did it, for a stream that goes
    /// quiet without a word: the ordinary end of a session already says
    /// why elsewhere, and this is for the one that never says anything
    /// at all. `None` where nobody is watching, a benchmark or a test.
    pub async fn host(
        connection: Connection,
        engine: IpAddr,
        answering: Arc<dyn Answers>,
        log: Option<Log>,
    ) -> io::Result<Self> {
        let log = log.map(|log| log.about(TAG));
        let ports = answering.engine();
        let datagrams = Arc::new(DatagramPorts::towards_engine(engine, ports)?);
        let counters = Arc::new(Counters::default());
        let mut tasks = datagram_pumps(&connection, &datagrams, &counters);

        let towards_engine = connection.clone();
        tasks
            .spawn(async move { serve_the_streams(&towards_engine, engine, answering, log).await });

        Ok(Self { tasks, counters })
    }

    /// Client side: what the local engine sends goes into the tunnel.
    ///
    /// See `host` for what `log` is.
    pub async fn client(
        connection: Connection,
        listen: IpAddr,
        ports: EnginePorts,
        log: Option<Log>,
    ) -> io::Result<Self> {
        let log = log.map(|log| log.about(TAG));
        // The listeners are open before we hand back: the engine may
        // show up the instant the session is announced to it.
        let mut listeners = Vec::new();
        for channel in StreamChannel::ALL {
            let Some(port) = channel.port(ports) else {
                continue;
            };
            let bound = TcpListener::bind(SocketAddr::new(listen, port)).await?;
            listeners.push((channel, bound));
        }

        let datagrams = Arc::new(DatagramPorts::from_engine(listen, ports)?);
        let counters = Arc::new(Counters::default());
        let mut tasks = datagram_pumps(&connection, &datagrams, &counters);

        for (channel, bound) in listeners {
            let towards_tunnel = connection.clone();
            let log = log.clone();
            tasks
                .spawn(async move { carry_the_streams(channel, bound, towards_tunnel, log).await });
        }

        Ok(Self { tasks, counters })
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

    /// Waits for the tunnel to stop, and says why it stopped.
    ///
    /// The pumps run for as long as the connection holds: the first one
    /// to hand back signals the end of the session.
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
}

/// What a pump handing back means, a panic in it included.
fn stopped_because(outcome: Result<io::Result<()>, tokio::task::JoinError>) -> io::Result<()> {
    outcome.map_err(io::Error::other)?
}

/// The UDP pumps, identical on both sides.
fn datagram_pumps(
    connection: &Connection,
    datagrams: &Arc<DatagramPorts>,
    counters: &Arc<Counters>,
) -> JoinSet<io::Result<()>> {
    let mut tasks = JoinSet::new();

    // One reader for the three channels: a connection's datagrams arrive
    // through a single queue.
    let reading_connection = connection.clone();
    let reading_ports = datagrams.clone();
    let reading_counters = counters.clone();
    tasks.spawn(async move {
        pump::distribute_datagrams(&reading_connection, &reading_ports, &reading_counters).await
    });

    for channel in DatagramChannel::ALL {
        let connection = connection.clone();
        let ports = datagrams.clone();
        let counters = counters.clone();
        tasks.spawn(async move {
            pump::collect_datagrams(channel, ports.port(channel), &connection, &counters).await
        });
    }

    tasks
}

/// Hands the engine the reliable streams that arrive from the tunnel.
async fn serve_the_streams(
    connection: &Connection,
    engine: IpAddr,
    answering: Arc<dyn Answers>,
    log: Option<Log>,
) -> io::Result<()> {
    let mut sessions = JoinSet::new();
    loop {
        let (sending, receiving) = connection.accept_stream().await.map_err(io::Error::other)?;

        // One stream's failure stays on that stream: a botched pairing
        // must not take the running session with it.
        let answering = answering.clone();
        let log = log.clone();
        sessions.spawn(async move {
            let _ = hand_to_the_engine(sending, receiving, engine, answering, log).await;
        });
        while sessions.try_join_next().is_some() {}
    }
}

async fn hand_to_the_engine(
    sending: SendStream,
    mut receiving: RecvStream,
    engine: IpAddr,
    answering: Arc<dyn Answers>,
    log: Option<Log>,
) -> io::Result<()> {
    let channel = match pump::read_announcement(&mut receiving).await {
        Ok(channel) => channel,
        Err(e) => {
            if let Some(log) = &log {
                log.write(&format!("a stream from the tunnel never named itself: {e}"));
            }
            return Err(e);
        }
    };
    let Some(port) = channel.port(answering.engine()) else {
        // ZyrDesk's own channel goes to no engine: it is the tunnel
        // talking to the tunnel, and this is where it answers.
        return aside::answer(sending, receiving, answering).await;
    };

    if let Some(log) = &log {
        log.debug(|| {
            format!("{channel:?}: reaching this computer's own engine at {engine}:{port}")
        });
    }
    let local = match TcpStream::connect(SocketAddr::new(engine, port)).await {
        Ok(local) => local,
        Err(e) => {
            if let Some(log) = &log {
                log.write(&format!(
                    "{channel:?}: the engine at {engine}:{port} would not take it: {e}"
                ));
            }
            return Err(e);
        }
    };
    local.set_nodelay(true)?;
    if let Some(log) = &log {
        log.debug(|| format!("{channel:?}: connected, carrying it to and from the engine"));
    }
    let outcome = pump::relay_stream(local, sending, receiving).await;
    how_it_ended(log.as_ref(), channel, &outcome);
    outcome
}

/// Carries into the tunnel the connections the local engine opens.
async fn carry_the_streams(
    channel: StreamChannel,
    listener: TcpListener,
    connection: Connection,
    log: Option<Log>,
) -> io::Result<()> {
    let mut sessions = JoinSet::new();
    loop {
        let (local, _) = listener.accept().await?;
        local.set_nodelay(true)?;

        let connection = connection.clone();
        let log = log.clone();
        sessions.spawn(async move {
            let _ = carry_to_the_tunnel(channel, local, connection, log).await;
        });
        while sessions.try_join_next().is_some() {}
    }
}

async fn carry_to_the_tunnel(
    channel: StreamChannel,
    local: TcpStream,
    connection: Connection,
    log: Option<Log>,
) -> io::Result<()> {
    if let Some(log) = &log {
        log.debug(|| {
            format!("{channel:?}: the local engine reached it, opening the tunnel's own stream")
        });
    }
    let (mut sending, receiving) = match connection.open_stream().await {
        Ok(stream) => stream,
        Err(e) => {
            if let Some(log) = &log {
                log.write(&format!(
                    "{channel:?}: the tunnel would not open a stream: {e}"
                ));
            }
            return Err(io::Error::other(e));
        }
    };
    pump::announce(&mut sending, channel).await?;
    if let Some(log) = &log {
        log.debug(|| {
            format!("{channel:?}: named to the far computer, carrying it to and from the engine")
        });
    }
    let outcome = pump::relay_stream(local, sending, receiving).await;
    how_it_ended(log.as_ref(), channel, &outcome);
    outcome
}

/// Says how a stream ended, and in which voice.
///
/// The two are not the same news at all. A stream that simply ran out is
/// the ordinary end of every one of the twenty a session opens, and
/// twenty lines of it drown whatever else happened; one that was cut is
/// the thing somebody opened the journal to find.
fn how_it_ended(log: Option<&Log>, channel: StreamChannel, outcome: &io::Result<()>) {
    let Some(log) = log else {
        return;
    };
    match outcome {
        Ok(()) => log.debug(|| format!("{channel:?}: both ends are done")),
        Err(e) => log.write(&format!("{channel:?}: stopped: {e}")),
    }
}
