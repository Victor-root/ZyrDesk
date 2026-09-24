//! The whole tunnel, between two fake engines.
//!
//! The real engines are not needed to check what matters here: that a
//! byte dropped in on one side comes out identical on the other, on the
//! right port, in both directions, without either end having to know a
//! tunnel exists.
//!
//! Both sides run in the same process, on two distinct loopback
//! addresses: the host engine on 127.0.0.1, the client-side listeners on
//! the address dedicated to the device. That is exactly the addressing
//! scheme planned for real.

use std::future::Future;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use zyr_proto::clipboard::{Clip, Stamp};
use zyr_proto::net::{EnginePorts, device_loopback_addr};
use zyr_proto::session::WantedScreen;
use zyr_transport::{Identity, MediaProfile, TunnelEndpoint};
use zyr_tunnel::aside::{Given, Wanted};
use zyr_tunnel::{Answers, StreamChannel, Tunnel, aside};

/// Past this, nothing is getting through.
const PATIENCE: Duration = Duration::from_secs(10);

/// Where the host engine listens, as on a real machine.
const ENGINE: IpAddr = IpAddr::V4(std::net::Ipv4Addr::LOCALHOST);

async fn before_the_end<T>(work: impl Future<Output = T>) -> T {
    tokio::time::timeout(PATIENCE, work)
        .await
        .expect("the tunnel let nothing through")
}

/// Code the fake engine refuses, standing in for an engine that has
/// nobody waiting on one.
const REFUSED_PIN: &str = "9999";

/// The host engine as the tunnel sees it: its ports, and a pairing code
/// written down instead of handed to anything.
struct FakeEngine {
    ports: EnginePorts,
    handed: Arc<std::sync::Mutex<Vec<(String, String)>>>,
    /// Times Ctrl+Alt+Suppr was asked for. Counted rather than done:
    /// nothing here has a Windows to press it on.
    attended: Arc<AtomicU32>,
    /// Whether the far computer was asked to go quiet. Written down for
    /// the same reason: nothing here has speakers to silence.
    hushed: Arc<AtomicBool>,
    /// Whether it was asked to lock itself, for the same reason again.
    locked: Arc<AtomicBool>,
    /// The rate it was last asked to serve a still screen at.
    steady: Arc<AtomicBool>,
    /// The screen its virtual one was last asked to be, `None` standing
    /// for the ask to put it back to sleep.
    screen: Arc<std::sync::Mutex<Option<WantedScreen>>>,
    /// Whether its journal was emptied. Written down rather than done:
    /// nothing here has four files to cut.
    emptied: Arc<AtomicBool>,
    /// Which of its screens it was last asked to be served from, `None`
    /// standing for its main one, which is what it films to begin with.
    filming: Arc<std::sync::Mutex<Option<String>>>,
    /// What the session opening on it said it would be served.
    opening: Arc<std::sync::Mutex<Option<MediaProfile>>>,
    /// What is on its clipboard, which a session may both read and
    /// replace.
    clipboard: Arc<std::sync::Mutex<Option<Clip>>>,
    /// The files its own clipboard named, as their bytes, so a piece of
    /// one can be handed over.
    has: Arc<std::sync::Mutex<Vec<Vec<u8>>>>,
    /// What it wants next of what the far computer named, which is a
    /// paste under way over there.
    wants: Arc<std::sync::Mutex<Option<Wanted>>>,
    /// The pieces it was handed, in the order they came.
    taken: Arc<std::sync::Mutex<Vec<Given>>>,
}

impl Answers for FakeEngine {
    fn engine(&self) -> EnginePorts {
        self.ports
    }

    fn a_session_is_opening(&self, serving: MediaProfile) {
        *self.opening.lock().unwrap() = Some(serving);
    }

    fn hand_over_the_code(&self, pin: &str, name: &str) -> Result<(), String> {
        if pin == REFUSED_PIN {
            return Err("le moteur n'attend aucun code".to_string());
        }
        self.handed
            .lock()
            .unwrap()
            .push((pin.to_string(), name.to_string()));
        Ok(())
    }

    fn secure_attention(&self) -> Result<(), String> {
        self.attended.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    fn hush_the_speakers(&self, quiet: bool) -> Result<(), String> {
        self.hushed.store(quiet, Ordering::Relaxed);
        Ok(())
    }

    fn lock_the_screen(&self) -> Result<(), String> {
        self.locked.store(true, Ordering::Relaxed);
        Ok(())
    }

    /// Like a machine whose engine cannot be asked: it reads the frame
    /// rate when it starts, so changing it makes it start over.
    fn serve_steady(&self, rate: bool) -> Result<zyr_tunnel::Settled, String> {
        if self.steady.swap(rate, Ordering::Relaxed) == rate {
            return Ok(zyr_tunnel::Settled::Already);
        }
        Ok(zyr_tunnel::Settled::StartingOver)
    }

    fn serve_at(&self, _kbps: u32) -> Result<(), String> {
        Err("ce moteur-là ne se règle pas en marche".to_string())
    }

    fn draw_the_pointer(&self, _drawn: bool) -> Result<(), String> {
        Err("ce moteur-là ne se règle pas en marche".to_string())
    }

    fn screen_for_a_session(
        &self,
        wanted: Option<WantedScreen>,
    ) -> Result<Option<(u32, u32)>, String> {
        *self.screen.lock().unwrap() = wanted;
        // What a real machine would answer when nobody wants its
        // virtual screen: the size of its own screen.
        Ok(wanted
            .map(|screen| (screen.wide, screen.high))
            .or(Some(HOST_SCREEN)))
    }

    fn journal(&self, sift: &str) -> Result<String, String> {
        if self.emptied.load(Ordering::Relaxed) {
            return Ok(String::new());
        }
        // The sift is done where the journal is gathered: what is given
        // back here says which sift it was given back under, which is
        // enough to see that the sift did get across.
        if !sift.is_empty() {
            return Ok(format!("trié par « {sift} »"));
        }
        Ok(host_journal())
    }

    fn reach_log(&self) -> Result<String, String> {
        Ok(host_reach_log())
    }

    fn empty_the_journal(&self) -> Result<(), String> {
        self.emptied.store(true, Ordering::Relaxed);
        Ok(())
    }

    fn codecs(&self) -> Result<String, String> {
        Ok(HOST_CODECS.to_string())
    }

    fn screens(&self) -> Result<String, String> {
        Ok(HOST_SCREENS.to_string())
    }

    fn pointer(&self) -> Result<zyr_proto::session::Pointer, String> {
        Ok(HOST_POINTER)
    }

    /// What a real machine would do: it films its main screen, and its
    /// engine has to be restarted to film another one.
    fn film_this_screen(&self, id: Option<String>) -> Result<zyr_tunnel::Settled, String> {
        let mut filming = self.filming.lock().unwrap();
        if *filming == id {
            return Ok(zyr_tunnel::Settled::Already);
        }
        *filming = id;
        Ok(zyr_tunnel::Settled::StartingOver)
    }

    /// What a real machine does: it takes what comes, and only gives
    /// back what it has if that is not already what the other side
    /// says it holds.
    fn clipboard(
        &self,
        pushing: Option<Clip>,
        seen: Option<Stamp>,
    ) -> Result<Option<Clip>, String> {
        let mut held = self.clipboard.lock().unwrap();
        if let Some(coming) = pushing {
            *held = Some(coming);
            return Ok(None);
        }
        match held.as_ref() {
            Some(clip) if Some(clip.stamp()) != seen => Ok(Some(clip.clone())),
            _ => Ok(None),
        }
    }

    /// What a real machine does: it hands over the asked-for piece of
    /// what its clipboard names, takes the one it is given, and says
    /// what it wants next.
    fn pieces(
        &self,
        asking: Option<Wanted>,
        giving: Option<Given>,
    ) -> Result<(Option<Given>, Option<Wanted>), String> {
        if let Some(coming) = giving {
            self.taken.lock().unwrap().push(coming);
        }
        let given = asking.and_then(|asked| {
            let has = self.has.lock().unwrap();
            let file = has.get(asked.rank as usize)?;
            let from = (asked.from as usize).min(file.len());
            let upto = (from + asked.how_many as usize).min(file.len());
            Some(Given {
                rank: asked.rank,
                from: asked.from,
                bytes: file[from..upto].to_vec(),
            })
        });
        Ok((given, *self.wants.lock().unwrap()))
    }
}

/// What a machine with an Intel graphics card can do: no AV1. That is
/// the case this question exists for.
const HOST_CODECS: &str = "H.264 HEVC";

/// Two screens switched on at the far machine, the main one first: that
/// is the case that called for the question.
const HOST_SCREENS: &str = "{aaa} main 2560x1440 ROG PG279Q\n{bbb} other 1920x1080 Dell U2412M";

/// What someone had copied on the far machine before the session
/// opened.
const HOST_CLIPBOARD: &str = "l'adresse du serveur : 10.0.0.4";

/// The shape of the far pointer: something other than the arrow,
/// otherwise the round trip would prove nothing, an arrow also being
/// what a word nobody recognises gives back.
const HOST_POINTER: zyr_proto::session::Pointer = zyr_proto::session::Pointer::Text;

/// What the far machine answers when the session asks it to keep its
/// screen as it is.
const HOST_SCREEN: (u32, u32) = (1366, 768);

/// A journal the size of the ones the product really writes.
///
/// Well beyond what this channel accepted before it: it is the first
/// message that weighs a page and not a line, and that is what this test
/// exists to check.
fn host_journal() -> String {
    let mut page = String::from("ZyrDesk 0.1.0\nOrdinateur       : PC du SAV");
    page.push_str("\n\n--- Le service (service.log) ---");
    for line in 0..480 {
        page.push_str(&format!(
            "\n2026-08-30T12:00:{:02}Z  ligne {line} du journal de la machine d'en face",
            line % 60
        ));
    }
    page
}

fn host_reach_log() -> String {
    let mut page = String::from("2026-08-30 12:00:00 8.8.8.8:53 answered in 8 ms");
    for line in 1..480 {
        page.push_str(&format!(
            "\n2026-08-30 12:00:{:02} 8.8.8.8:53 answered in 8 ms",
            line % 60
        ));
    }
    page
}

/// The tunnel brought up on both sides, kept alive for the test.
///
/// Everything is dropped together at the end: the pumps stop with it.
struct Bench {
    _endpoints: (TunnelEndpoint, TunnelEndpoint),
    _host: Tunnel,
    client: Tunnel,
    /// Address the client engine believes the host to be at.
    client_side: IpAddr,
    ports: EnginePorts,
    /// The way, still open, to speak to the far ZyrDesk rather than to
    /// its engine.
    connection: zyr_transport::Connection,
    /// What the host engine was handed.
    handed: Arc<std::sync::Mutex<Vec<(String, String)>>>,
    /// Times the far ZyrDesk was asked to press Ctrl+Alt+Suppr.
    attended: Arc<AtomicU32>,
    /// Whether the far ZyrDesk was asked to silence its speakers.
    hushed: Arc<AtomicBool>,
    /// Whether it was asked to put its lock screen up.
    locked: Arc<AtomicBool>,
    /// The rate it was asked to serve a still screen at.
    steady: Arc<AtomicBool>,
    /// The screen its virtual one was last asked to be.
    screen: Arc<std::sync::Mutex<Option<WantedScreen>>>,
    /// What the session opening on it said it would be served.
    opening: Arc<std::sync::Mutex<Option<MediaProfile>>>,
    /// Whether its journal was emptied.
    emptied: Arc<AtomicBool>,
    /// Which of its screens it was last asked to be served from.
    filming: Arc<std::sync::Mutex<Option<String>>>,
    /// What is on the far computer's clipboard.
    clipboard: Arc<std::sync::Mutex<Option<Clip>>>,
    /// The bytes of the files its clipboard names.
    has: Arc<std::sync::Mutex<Vec<Vec<u8>>>>,
    /// What it wants next of what this computer named.
    wants: Arc<std::sync::Mutex<Option<Wanted>>>,
    /// The pieces it was handed.
    taken: Arc<std::sync::Mutex<Vec<Given>>>,
}

impl Bench {
    async fn bring_up(base: u16, device: u16) -> Self {
        let ports = EnginePorts::new(base).unwrap();
        let client_side = IpAddr::V4(device_loopback_addr(device).unwrap());

        let host_identity = Identity::generate().unwrap();
        let client_identity = Identity::generate().unwrap();
        let profile = MediaProfile::default();
        let ephemeral = SocketAddr::new(ENGINE, 0);

        let host_endpoint = TunnelEndpoint::host(
            &host_identity,
            client_identity.fingerprint(),
            profile,
            ephemeral,
        )
        .unwrap();
        let meeting_point = host_endpoint.local_address().unwrap();
        let client_endpoint = TunnelEndpoint::client(
            &client_identity,
            host_identity.fingerprint(),
            profile,
            ephemeral,
        )
        .unwrap();

        let (host_side, client_connection) = tokio::join!(
            host_endpoint.accept(),
            client_endpoint.connect(meeting_point)
        );

        let handed = Arc::new(std::sync::Mutex::new(Vec::new()));
        let attended = Arc::new(AtomicU32::new(0));
        let hushed = Arc::new(AtomicBool::new(false));
        let locked = Arc::new(AtomicBool::new(false));
        let steady = Arc::new(AtomicBool::new(false));
        let screen: Arc<std::sync::Mutex<Option<WantedScreen>>> =
            Arc::new(std::sync::Mutex::new(None));
        let emptied = Arc::new(AtomicBool::new(false));
        let filming: Arc<std::sync::Mutex<Option<String>>> = Arc::new(std::sync::Mutex::new(None));
        let opening: Arc<std::sync::Mutex<Option<MediaProfile>>> =
            Arc::new(std::sync::Mutex::new(None));
        let clipboard: Arc<std::sync::Mutex<Option<Clip>>> =
            Arc::new(std::sync::Mutex::new(Some(Clip::text(HOST_CLIPBOARD))));
        let has: Arc<std::sync::Mutex<Vec<Vec<u8>>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
        let wants: Arc<std::sync::Mutex<Option<Wanted>>> = Arc::new(std::sync::Mutex::new(None));
        let taken: Arc<std::sync::Mutex<Vec<Given>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
        let host = Tunnel::host(
            host_side.unwrap(),
            ENGINE,
            Arc::new(FakeEngine {
                ports,
                handed: handed.clone(),
                attended: attended.clone(),
                hushed: hushed.clone(),
                locked: locked.clone(),
                steady: steady.clone(),
                screen: screen.clone(),
                emptied: emptied.clone(),
                filming: filming.clone(),
                opening: opening.clone(),
                clipboard: clipboard.clone(),
                has: has.clone(),
                wants: wants.clone(),
                taken: taken.clone(),
            }),
            None,
        )
        .await
        .unwrap();

        // The real sequence, not a shortcut: the client learns the
        // host's engine ports before opening the local ones that stand
        // in for them. Nothing here is allowed to know them in advance.
        let client_connection = client_connection.unwrap();
        let engine = aside::ask_the_ports(&client_connection, profile)
            .await
            .unwrap();
        let client = Tunnel::client(client_connection.clone(), client_side, engine, None)
            .await
            .unwrap();

        Self {
            _endpoints: (host_endpoint, client_endpoint),
            _host: host,
            client,
            client_side,
            ports: engine,
            connection: client_connection,
            handed,
            attended,
            hushed,
            locked,
            steady,
            screen,
            emptied,
            filming,
            opening,
            clipboard,
            has,
            wants,
            taken,
        }
    }

    /// Address of an engine port, as the client engine sees it.
    fn as_the_client_sees(&self, port: u16) -> SocketAddr {
        SocketAddr::new(self.client_side, port)
    }
}

/// Fake host engine that echoes back whatever is written to it, in TCP.
async fn tcp_engine(port: u16) {
    let listener = TcpListener::bind(SocketAddr::new(ENGINE, port))
        .await
        .unwrap();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let (mut reading, mut writing) = stream.split();
                let _ = tokio::io::copy(&mut reading, &mut writing).await;
                let _ = writing.shutdown().await;
            });
        }
    });
}

/// Fake host engine that answers in UDP, naming the port it was reached
/// on: that is how we check no channel crosses another.
async fn udp_engine(port: u16) {
    let socket = UdpSocket::bind(SocketAddr::new(ENGINE, port))
        .await
        .unwrap();
    tokio::spawn(async move {
        let mut buffer = [0u8; 2048];
        while let Ok((read, source)) = socket.recv_from(&mut buffer).await {
            let answer = format!("{port}:{}", String::from_utf8_lossy(&buffer[..read]));
            let _ = socket.send_to(answer.as_bytes(), source).await;
        }
    });
}

#[tokio::test]
async fn the_client_learns_the_host_engine_ports_from_the_host() {
    // The base port is picked by the host when its engine starts. A
    // client that guessed it would open its stand-in ports on the wrong
    // numbers, and the session would go nowhere with nothing to explain
    // it.
    let bench = Bench::bring_up(42700, 5).await;
    assert_eq!(bench.ports.base(), 42700);
}

#[tokio::test]
async fn the_watched_computer_learns_what_it_is_asked_to_serve() {
    // The fault this repairs: the watched machine opens its tunnel when
    // its service starts, long before a session exists, and so held a
    // window worked out for a nominal bitrate whatever the bitrate
    // really asked for. The first word of a session now tells it.
    let bench = Bench::bring_up(42750, 6).await;
    assert_eq!(
        *bench.opening.lock().unwrap(),
        Some(MediaProfile::default())
    );
}

#[tokio::test]
async fn the_pairing_code_travels_through_the_tunnel() {
    // This is what replaces a code shown on one screen and typed on
    // the other. The tunnel has already recognised both computers by
    // their fingerprint before opening: the code proves nothing more,
    // and nobody has to get up any more.
    let bench = Bench::bring_up(42850, 8).await;

    before_the_end(aside::ask_to_pair(
        &bench.connection,
        "0429",
        "PC de Victor",
    ))
    .await
    .unwrap();

    let handed = bench.handed.lock().unwrap().clone();
    assert_eq!(
        handed,
        vec![("0429".to_string(), "PC de Victor".to_string())]
    );
}

#[tokio::test]
async fn ctrl_alt_del_travels_on_the_product_s_own_channel() {
    // Windows keeps this combination for itself at both ends: the one
    // watching never sees it, and the one being watched cannot receive it
    // from an engine. So it crosses between the two halves of ZyrDesk,
    // and no engine knows anything about it.
    let bench = Bench::bring_up(42500, 7).await;

    before_the_end(aside::ask_for_the_secure_attention(&bench.connection))
        .await
        .unwrap();

    assert_eq!(bench.attended.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn muting_the_host_is_asked_from_the_client() {
    // It is whoever takes control who knows whether the room over there
    // should go quiet, and they are not in it to go and say so. So the
    // request crosses between the two halves of ZyrDesk, like the rest
    // of what belongs to no engine.
    let bench = Bench::bring_up(42950, 10).await;

    before_the_end(aside::ask_to_hush(&bench.connection, true))
        .await
        .unwrap();
    assert!(bench.hushed.load(Ordering::Relaxed));

    // And the other way, because a session can end without the far
    // machine noticing it any other way.
    before_the_end(aside::ask_to_hush(&bench.connection, false))
        .await
        .unwrap();
    assert!(!bench.hushed.load(Ordering::Relaxed));
}

#[tokio::test]
async fn locking_the_far_computer_goes_through_the_product_s_own_channel() {
    // Windows+L does not travel: Windows handles it where no program sees
    // it, at both ends of a session, and that is exactly what makes a
    // lock screen worth something. So the request takes the path of
    // Ctrl+Alt+Suppr, and the far service puts the screen up from the
    // only place its Windows accepts it from.
    let bench = Bench::bring_up(42750, 11).await;

    before_the_end(aside::ask_to_lock(&bench.connection))
        .await
        .unwrap();
    assert!(bench.locked.load(Ordering::Relaxed));
}

#[tokio::test]
async fn the_still_screen_rate_is_asked_from_the_client() {
    // What it costs is paid over there, but the only person able to say
    // whether the picture is smooth is the one watching it, and they are
    // not in front of the machine that would need adjusting.
    let bench = Bench::bring_up(42760, 13).await;

    // And a change costs it a restart of its engine, which it says
    // rather than letting the other end find out on a broken tunnel: it
    // is the same answer as for the screen to film.
    assert_eq!(
        before_the_end(aside::ask_to_serve_steady(&bench.connection, true))
            .await
            .unwrap(),
        zyr_tunnel::Settled::StartingOver
    );
    assert!(bench.steady.load(Ordering::Relaxed));

    // Asked for again as it is, it costs nothing at all, which is the
    // ordinary case: every session asks for it.
    assert_eq!(
        before_the_end(aside::ask_to_serve_steady(&bench.connection, true))
            .await
            .unwrap(),
        zyr_tunnel::Settled::Already
    );

    assert_eq!(
        before_the_end(aside::ask_to_serve_steady(&bench.connection, false))
            .await
            .unwrap(),
        zyr_tunnel::Settled::StartingOver
    );
    assert!(!bench.steady.load(Ordering::Relaxed));
}

#[tokio::test]
async fn an_engine_that_refuses_the_code_says_so_rather_than_going_quiet() {
    // Otherwise the computer connecting would wait on an engine that
    // is waiting for nothing, with nothing to show.
    let bench = Bench::bring_up(42900, 9).await;

    let refusal = before_the_end(aside::ask_to_pair(&bench.connection, REFUSED_PIN, "PC"))
        .await
        .unwrap_err();
    assert!(
        refusal.to_string().contains("n'attend aucun code"),
        "{refusal}"
    );

    // And the way still holds: a failed pairing does not take the
    // session down with it.
    let ports = before_the_end(aside::ask_the_ports(
        &bench.connection,
        MediaProfile::default(),
    ))
    .await
    .unwrap();
    assert_eq!(ports.base(), 42900);
}

#[tokio::test]
async fn a_reliable_stream_crosses_the_tunnel_both_ways() {
    let bench = Bench::bring_up(42100, 0).await;
    tcp_engine(bench.ports.http()).await;

    let mut stream = before_the_end(TcpStream::connect(
        bench.as_the_client_sees(bench.ports.http()),
    ))
    .await
    .unwrap();
    stream.write_all(b"pairing").await.unwrap();
    stream.shutdown().await.unwrap();

    let mut received = Vec::new();
    before_the_end(stream.read_to_end(&mut received))
        .await
        .unwrap();
    assert_eq!(received, b"pairing");
}

#[tokio::test]
async fn a_datagram_crosses_the_tunnel_both_ways() {
    let bench = Bench::bring_up(42200, 1).await;
    udp_engine(bench.ports.video()).await;

    let client = UdpSocket::bind(SocketAddr::new(bench.client_side, 0))
        .await
        .unwrap();
    client
        .send_to(b"ping", bench.as_the_client_sees(bench.ports.video()))
        .await
        .unwrap();

    let mut received = [0u8; 64];
    let (read, _) = before_the_end(client.recv_from(&mut received))
        .await
        .unwrap();
    assert_eq!(
        &received[..read],
        format!("{}:ping", bench.ports.video()).as_bytes()
    );
}

#[tokio::test]
async fn each_channel_lands_on_its_own_engine_port() {
    let bench = Bench::bring_up(42300, 2).await;
    for port in bench.ports.udp_ports() {
        udp_engine(port).await;
    }

    // The three channels share one datagram queue: if the header were
    // misread, the video would land in the audio.
    for port in bench.ports.udp_ports() {
        let client = UdpSocket::bind(SocketAddr::new(bench.client_side, 0))
            .await
            .unwrap();
        client
            .send_to(b"ping", bench.as_the_client_sees(port))
            .await
            .unwrap();

        let mut received = [0u8; 64];
        let (read, _) = before_the_end(client.recv_from(&mut received))
            .await
            .unwrap();
        assert_eq!(&received[..read], format!("{port}:ping").as_bytes());
    }
}

#[tokio::test]
async fn the_engine_web_interface_stays_out_of_the_tunnel() {
    let bench = Bench::bring_up(42400, 3).await;
    tcp_engine(bench.ports.web_ui()).await;

    // It does run on the host side, but nothing listens for it on the
    // client side: it is reachable only from the machine hosting it.
    assert!(
        TcpStream::connect(bench.as_the_client_sees(bench.ports.web_ui()))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn packets_sent_before_the_engine_listens_do_not_end_the_session() {
    // The engine opens its media ports only once the negotiation is
    // over, so everything the tunnel relays until then lands nowhere.
    // That must cost those packets and nothing else: ending the pump
    // there would break the negotiation still under way on the reliable
    // streams, and the session would fail with no visible cause.
    let bench = Bench::bring_up(42800, 6).await;
    let client = UdpSocket::bind(SocketAddr::new(bench.client_side, 0))
        .await
        .unwrap();

    for _ in 0..20 {
        client
            .send_to(b"early", bench.as_the_client_sees(bench.ports.video()))
            .await
            .unwrap();
    }
    tokio::time::sleep(Duration::from_millis(200)).await;

    // The engine shows up late, and the session carries on.
    udp_engine(bench.ports.video()).await;
    client
        .send_to(b"ping", bench.as_the_client_sees(bench.ports.video()))
        .await
        .unwrap();

    let mut received = [0u8; 64];
    let (read, _) = before_the_end(client.recv_from(&mut received))
        .await
        .unwrap();
    assert_eq!(
        &received[..read],
        format!("{}:ping", bench.ports.video()).as_bytes()
    );
}

#[tokio::test]
async fn the_counters_follow_what_travels() {
    let bench = Bench::bring_up(42600, 4).await;
    udp_engine(bench.ports.audio()).await;
    assert_eq!(bench.client.reading(), zyr_tunnel::Reading::default());

    let client = UdpSocket::bind(SocketAddr::new(bench.client_side, 0))
        .await
        .unwrap();
    client
        .send_to(b"ping", bench.as_the_client_sees(bench.ports.audio()))
        .await
        .unwrap();
    let mut received = [0u8; 64];
    before_the_end(client.recv_from(&mut received))
        .await
        .unwrap();

    let reading = bench.client.reading();
    assert_eq!(reading.to_tunnel, 1);
    assert_eq!(reading.to_engine, 1);
    assert_eq!(reading.too_large, 0);
    assert_eq!(reading.unreadable, 0);
}

#[tokio::test]
async fn the_virtual_screen_is_asked_for_at_the_opening_and_given_back_at_the_end() {
    // The virtual screen sleeps between sessions, which is the whole
    // point: a machine nobody is watching has the screens its owner
    // plugged in and not one more. So it has to be asked for, and given
    // back.
    let bench = Bench::bring_up(42770, 15).await;

    let asked = WantedScreen {
        wide: 3840,
        high: 2160,
        scale: 150,
    };
    let showing = before_the_end(aside::ask_for_a_screen(&bench.connection, Some(asked)))
        .await
        .unwrap();
    // The magnification travels with the size: a screen at the right size
    // but not at the right magnification is someone else's desktop at the
    // right resolution.
    assert_eq!(*bench.screen.lock().unwrap(), Some(asked));
    assert_eq!(showing, Some((3840, 2160)));

    // And what is asked for travels every time: it is on waking that the
    // driver reads the sizes written for it, there is no second chance.
    let asked = WantedScreen {
        wide: 2560,
        high: 1440,
        scale: 125,
    };
    before_the_end(aside::ask_for_a_screen(&bench.connection, Some(asked)))
        .await
        .unwrap();
    assert_eq!(*bench.screen.lock().unwrap(), Some(asked));

    // And with nothing asked for, the far machine answers with its
    // own: that is what makes "keep the host's resolution" possible,
    // since nothing on this side can guess what is plugged in over
    // there.
    let showing = before_the_end(aside::ask_for_a_screen(&bench.connection, None))
        .await
        .unwrap();
    assert_eq!(*bench.screen.lock().unwrap(), None);
    assert_eq!(showing, Some(HOST_SCREEN));
}

#[tokio::test]
async fn the_far_machine_s_journal_arrives_whole() {
    // Reading the remote computer's journal without walking over to it
    // means the fault is diagnosed on both journals at once. So what
    // arrives must be the whole page, lines included: a page cut short
    // in silence reads like a complete page.
    let bench = Bench::bring_up(42780, 16).await;

    let page = before_the_end(aside::ask_for_the_journal(&bench.connection, ""))
        .await
        .unwrap();
    assert_eq!(page, host_journal());
    // And it weighs far more than a question: that is the whole point
    // of two separate ceilings on this channel.
    assert!(page.len() > 20_000, "{} octets", page.len());

    // The other half, and it comes from the same need: empty both
    // journals, do again what does not work, read both. Emptying only
    // the one at hand leaves the walk to the other machine exactly
    // where it was.
    before_the_end(aside::ask_to_empty_the_journal(&bench.connection))
        .await
        .unwrap();
    assert!(bench.emptied.load(Ordering::Relaxed));

    let page = before_the_end(aside::ask_for_the_journal(&bench.connection, ""))
        .await
        .unwrap();
    assert!(page.is_empty(), "{page}");

    // And the sift crosses with the question: it is done over there,
    // before the page is cut, the only order in which a sift is worth
    // anything.
    bench.emptied.store(false, Ordering::Relaxed);
    let sifted = before_the_end(aside::ask_for_the_journal(
        &bench.connection,
        "tag:clipboard",
    ))
    .await
    .unwrap();
    assert_eq!(sifted, "trié par « tag:clipboard »");

    // And what this machine can encode, which decides what the menu over
    // there is allowed to offer. It is the only one that knows: it is
    // the one that encodes.
    let named = before_the_end(aside::ask_what_it_can_encode(&bench.connection))
        .await
        .unwrap();
    assert_eq!(named, HOST_CODECS);

    // And this machine's screens, with the one to watch. Victor's case: two
    // screens switched on over there, and until then no way to ask for the
    // second one.
    let listed = before_the_end(aside::ask_what_screens_it_has(&bench.connection))
        .await
        .unwrap();
    assert_eq!(listed, HOST_SCREENS);
    let read = zyr_proto::session::far_screens_read(&listed);
    assert_eq!(read.len(), 2);
    assert!(read[0].main);

    // And the shape its pointer has right now, which is what the
    // pointer drawn here is about to take. One more round trip, on a
    // channel already open, for one word: it is asked several times a
    // second while a hand is moving.
    assert_eq!(
        before_the_end(aside::ask_for_the_pointer(&bench.connection))
            .await
            .unwrap(),
        HOST_POINTER
    );

    // The main screen is the one it already films: every session asks
    // for it, and almost none changes anything.
    assert_eq!(
        before_the_end(aside::ask_to_film_this_screen(&bench.connection, None))
            .await
            .unwrap(),
        zyr_tunnel::Settled::Already
    );
    // The other one costs it a restart of its engine, and it says so
    // rather than letting the other end find out on a broken tunnel.
    assert_eq!(
        before_the_end(aside::ask_to_film_this_screen(
            &bench.connection,
            Some(read[1].id.clone())
        ))
        .await
        .unwrap(),
        zyr_tunnel::Settled::StartingOver
    );
    assert_eq!(*bench.filming.lock().unwrap(), Some(read[1].id.clone()));
}

#[tokio::test]
async fn what_the_far_machine_reaches_arrives_whole() {
    // The counterpart of the journal, on the same channel and for the
    // same reason: read from here rather than by walking over to the
    // other machine, and whole, with its one measurement per second.
    let bench = Bench::bring_up(42790, 17).await;

    let page = before_the_end(aside::ask_for_the_reach_log(&bench.connection))
        .await
        .unwrap();
    assert_eq!(page, host_reach_log());
    assert!(page.len() > 10_000, "{} octets", page.len());
}

#[tokio::test]
async fn the_clipboard_crosses_the_tunnel_both_ways() {
    // A shared clipboard makes no sense one way only: what is copied
    // over there must paste here, and what is copied here must paste
    // over there. A single message does both.
    let bench = Bench::bring_up(42840, 20).await;

    // What someone had copied over there, handed over because the one
    // asking holds nothing.
    let arrived = before_the_end(aside::ask_about_the_clipboard(
        &bench.connection,
        None,
        None,
    ))
    .await
    .unwrap()
    .expect("ce qui était copié en face");
    assert_eq!(arrived.said(), Some(HOST_CLIPBOARD));

    // And asked again while saying it is already held: nothing
    // comes back. That is what keeps the feature standing, with a
    // question asked several times a second for a whole session.
    let again = before_the_end(aside::ask_about_the_clipboard(
        &bench.connection,
        None,
        Some(arrived.stamp()),
    ))
    .await
    .unwrap();
    assert_eq!(again, None);

    // The other way: a picture copied here, heavier than a line, leaves
    // with the question itself. It is the only message on this channel
    // that weighs a page on the way out, and it is what the two-step
    // reading exists to let through.
    let image = Clip::picture(vec![0x89; 300_000]);
    let nothing = before_the_end(aside::ask_about_the_clipboard(
        &bench.connection,
        Some(image.clone()),
        Some(arrived.stamp()),
    ))
    .await
    .unwrap();
    assert_eq!(
        nothing, None,
        "ce qu'on vient de donner ne doit pas revenir"
    );
    assert_eq!(bench.clipboard.lock().unwrap().as_ref(), Some(&image));

    // And what is copied over there afterwards comes back, picture
    // included: both ways carry the same thing.
    let over_there = Clip::picture(vec![0x50; 200_000]);
    *bench.clipboard.lock().unwrap() = Some(over_there.clone());
    let received = before_the_end(aside::ask_about_the_clipboard(
        &bench.connection,
        None,
        Some(image.stamp()),
    ))
    .await
    .unwrap();
    assert_eq!(received, Some(over_there));
}

#[tokio::test]
async fn a_question_too_long_that_is_not_the_clipboard_is_refused() {
    // The one-page ceiling is only lifted for the question that
    // names it: without that, any unknown verb could make a
    // computer hold on to megabytes while it has still understood
    // nothing of what it is being told.
    let bench = Bench::bring_up(42850, 21).await;

    let (mut sending, mut receiving) = bench.connection.open_stream().await.unwrap();
    zyr_tunnel::pump::announce(&mut sending, StreamChannel::ZyrDesk)
        .await
        .unwrap();
    let too_long = format!("{} pair 1234 {}", aside::VERSION, "n".repeat(8192));
    sending.write_all(too_long.as_bytes()).await.unwrap();
    sending.shutdown().await.unwrap();

    // The channel closes without answering anything, which is
    // exactly what is wanted: nothing was held on to, nothing was
    // done.
    let heard = before_the_end(receiving.read_to_end(64 * 1024)).await;
    assert!(
        heard.as_ref().map(Vec::is_empty).unwrap_or(true),
        "{heard:?}"
    );
    assert!(bench.handed.lock().unwrap().is_empty());
}

#[tokio::test]
async fn the_pieces_of_a_file_cross_both_ways() {
    // What a clipboard carries of a file is its name; the bytes follow,
    // one piece at a time, and in the direction they are wanted. A
    // single message carries both, as for the clipboard itself, and for
    // the same reason: only the one who opened the way can ask for
    // anything.
    let bench = Bench::bring_up(42860, 22).await;

    // The pulling way: the far machine copied, this one pastes, so
    // it asks.
    let file: Vec<u8> = (0..200_000u32).map(|at| (at % 251) as u8).collect();
    *bench.has.lock().unwrap() = vec![b"court".to_vec(), file.clone()];

    let mut gathered = Vec::new();
    let mut offset = 0u64;
    while (offset as usize) < file.len() {
        let asked = Wanted {
            rank: 1,
            from: offset,
            how_many: aside::A_PIECE as u32,
        };
        let (given, _) =
            before_the_end(aside::ask_for_pieces(&bench.connection, Some(asked), None))
                .await
                .unwrap();
        let given = given.expect("le morceau demandé");
        assert_eq!(given.rank, 1);
        assert_eq!(given.from, offset);
        assert!(!given.bytes.is_empty(), "un morceau vide ne finit jamais");
        offset += given.bytes.len() as u64;
        gathered.extend_from_slice(&given.bytes);
    }
    assert_eq!(gathered, file, "le fichier remonté n'est pas le fichier");

    // And the pushing way: it is this machine that copied, and the far
    // one that pastes, so it says what it wants and is given it. The
    // answer to a piece given names the next piece, which makes one
    // round trip per piece and not two.
    let asked_for = Wanted {
        rank: 0,
        from: 4096,
        how_many: 1024,
    };
    *bench.wants.lock().unwrap() = Some(asked_for);
    let (_, wanted) = before_the_end(aside::ask_for_pieces(&bench.connection, None, None))
        .await
        .unwrap();
    assert_eq!(wanted, Some(asked_for));

    let piece = Given {
        rank: 0,
        from: 4096,
        bytes: vec![0x2a; 1024],
    };
    *bench.wants.lock().unwrap() = None;
    let (_, wanted) = before_the_end(aside::ask_for_pieces(
        &bench.connection,
        None,
        Some(piece.clone()),
    ))
    .await
    .unwrap();
    assert_eq!(wanted, None, "rien voulu de plus veut dire que c'est fini");
    assert_eq!(*bench.taken.lock().unwrap(), vec![piece]);
}
