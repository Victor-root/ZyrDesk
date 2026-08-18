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
use zyr_proto::net::EnginePorts;
use zyr_transport::{Connection, RecvStream, SendStream};

use crate::aside::{self, Answers};
use crate::channel::{DatagramChannel, StreamChannel};
use crate::pump::{self, Counters, DatagramPorts, Reading};

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
    pub async fn host(
        connection: Connection,
        engine: IpAddr,
        answering: Arc<dyn Answers>,
    ) -> io::Result<Self> {
        let ports = answering.engine();
        let datagrams = Arc::new(DatagramPorts::towards_engine(engine, ports)?);
        let counters = Arc::new(Counters::default());
        let mut tasks = datagram_pumps(&connection, &datagrams, &counters);

        let towards_engine = connection.clone();
        tasks.spawn(async move { serve_the_streams(&towards_engine, engine, answering).await });

        Ok(Self { tasks, counters })
    }

    /// Client side: what the local engine sends goes into the tunnel.
    pub async fn client(
        connection: Connection,
        listen: IpAddr,
        ports: EnginePorts,
    ) -> io::Result<Self> {
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
            tasks.spawn(async move { carry_the_streams(channel, bound, towards_tunnel).await });
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
            Some(outcome) => outcome.map_err(io::Error::other)?,
            None => Ok(()),
        }
    }
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
) -> io::Result<()> {
    let mut sessions = JoinSet::new();
    loop {
        let (sending, receiving) = connection.accept_stream().await.map_err(io::Error::other)?;

        // One stream's failure stays on that stream: a botched pairing
        // must not take the running session with it.
        let answering = answering.clone();
        sessions.spawn(async move {
            let _ = hand_to_the_engine(sending, receiving, engine, answering).await;
        });
        while sessions.try_join_next().is_some() {}
    }
}

async fn hand_to_the_engine(
    sending: SendStream,
    mut receiving: RecvStream,
    engine: IpAddr,
    answering: Arc<dyn Answers>,
) -> io::Result<()> {
    let channel = pump::read_announcement(&mut receiving).await?;
    let Some(port) = channel.port(answering.engine()) else {
        // ZyrDesk's own channel goes to no engine: it is the tunnel
        // talking to the tunnel, and this is where it answers.
        return aside::answer(sending, receiving, answering).await;
    };

    let local = TcpStream::connect(SocketAddr::new(engine, port)).await?;
    local.set_nodelay(true)?;
    pump::relay_stream(local, sending, receiving).await
}

/// Carries into the tunnel the connections the local engine opens.
async fn carry_the_streams(
    channel: StreamChannel,
    listener: TcpListener,
    connection: Connection,
) -> io::Result<()> {
    let mut sessions = JoinSet::new();
    loop {
        let (local, _) = listener.accept().await?;
        local.set_nodelay(true)?;

        let connection = connection.clone();
        sessions.spawn(async move {
            let _ = carry_to_the_tunnel(channel, local, connection).await;
        });
        while sessions.try_join_next().is_some() {}
    }
}

async fn carry_to_the_tunnel(
    channel: StreamChannel,
    local: TcpStream,
    connection: Connection,
) -> io::Result<()> {
    let (mut sending, receiving) = connection.open_stream().await.map_err(io::Error::other)?;
    pump::announce(&mut sending, channel).await?;
    pump::relay_stream(local, sending, receiving).await
}
