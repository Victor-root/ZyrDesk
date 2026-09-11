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

    /// Comme une machine dont le moteur ne peut pas être prié : il lit la
    /// cadence à son démarrage, donc en changer le fait repartir.
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
        // Ce qu'une vraie machine répondrait quand personne ne veut de
        // son écran virtuel : la taille de son écran à elle.
        Ok(wanted
            .map(|screen| (screen.wide, screen.high))
            .or(Some(HOST_SCREEN)))
    }

    fn journal(&self, sift: &str) -> Result<String, String> {
        if self.emptied.load(Ordering::Relaxed) {
            return Ok(String::new());
        }
        // Le tri se fait là où le journal se rassemble : ce qu'on rend
        // ici dit sous quel tri on l'a rendu, ce qui suffit à voir qu'il
        // a bien traversé.
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

    /// Ce que ferait une vraie machine : elle filme son écran principal
    /// et il faut redémarrer son moteur pour en filmer un autre.
    fn film_this_screen(&self, id: Option<String>) -> Result<zyr_tunnel::Settled, String> {
        let mut filming = self.filming.lock().unwrap();
        if *filming == id {
            return Ok(zyr_tunnel::Settled::Already);
        }
        *filming = id;
        Ok(zyr_tunnel::Settled::StartingOver)
    }

    /// Ce que fait une vraie machine : elle prend ce qui vient, et ne
    /// rend ce qu'elle a que si ce n'est pas déjà ce que l'autre dit
    /// tenir.
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

    /// Ce que fait une vraie machine : elle rend le morceau demandé de
    /// ce que son presse-papiers nomme, prend celui qu'on lui donne, et
    /// dit ce qu'elle veut ensuite.
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

/// Ce qu'une machine à carte Intel sait faire : pas d'AV1. C'est le cas
/// pour lequel cette question existe.
const HOST_CODECS: &str = "H.264 HEVC";

/// Deux écrans allumés sur la machine d'en face, le principal d'abord :
/// c'est le cas qui a valu la question.
const HOST_SCREENS: &str = "{aaa} main 2560x1440 ROG PG279Q\n{bbb} other 1920x1080 Dell U2412M";

/// Ce que quelqu'un avait copié sur la machine d'en face avant que la
/// session ne s'ouvre.
const HOST_CLIPBOARD: &str = "l'adresse du serveur : 10.0.0.4";

/// La forme du curseur d'en face : autre chose que la flèche, sans quoi
/// le tour ne prouverait rien, une flèche étant aussi ce que rend un mot
/// que personne ne reconnaît.
const HOST_POINTER: zyr_proto::session::Pointer = zyr_proto::session::Pointer::Text;

/// Ce que la machine d'en face répond quand la session lui demande de
/// garder son écran tel quel.
const HOST_SCREEN: (u32, u32) = (1366, 768);

/// Un journal de la taille de ceux que le produit écrit vraiment.
///
/// Bien au-delà de ce que ce canal acceptait avant lui : c'est le premier
/// message qui pèse une page et non une ligne, et c'est ce que ce test
/// existe pour vérifier.
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
async fn l_ordinateur_regarde_apprend_ce_qu_on_lui_demande_de_servir() {
    // Le défaut que ceci répare : la machine regardée ouvre son tunnel
    // au démarrage de son service, bien avant qu'une session existe, et
    // tenait donc une fenêtre calculée sur un débit nominal quel que
    // soit le débit réellement demandé. Le premier mot d'une session le
    // lui dit désormais.
    let bench = Bench::bring_up(42750, 6).await;
    assert_eq!(
        *bench.opening.lock().unwrap(),
        Some(MediaProfile::default())
    );
}

#[tokio::test]
async fn the_pairing_code_travels_through_the_tunnel() {
    // C'est ce qui remplace un code affiché sur un écran et tapé sur
    // l'autre. Le tunnel a déjà reconnu les deux ordinateurs à leur
    // empreinte avant de s'ouvrir : le code ne prouve rien de plus, et
    // personne n'a plus à se lever.
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
async fn ctrl_alt_suppr_travels_on_the_product_s_own_channel() {
    // Windows garde cette combinaison pour lui aux deux bouts : celui qui
    // regarde ne la voit jamais, et celui qui est regardé ne peut pas la
    // recevoir d'un moteur. Elle traverse donc entre les deux moitiés de
    // ZyrDesk, et aucun moteur n'en sait rien.
    let bench = Bench::bring_up(42500, 7).await;

    before_the_end(aside::ask_for_the_secure_attention(&bench.connection))
        .await
        .unwrap();

    assert_eq!(bench.attended.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn couper_le_son_de_l_hote_se_demande_depuis_le_client() {
    // C'est celui qui prend la main qui sait si la pièce d'en face doit
    // se taire, et il n'est pas dedans pour aller le dire. La demande
    // traverse donc entre les deux moitiés de ZyrDesk, comme le reste de
    // ce qui n'appartient à aucun moteur.
    let bench = Bench::bring_up(42950, 10).await;

    before_the_end(aside::ask_to_hush(&bench.connection, true))
        .await
        .unwrap();
    assert!(bench.hushed.load(Ordering::Relaxed));

    // Et dans l'autre sens, parce qu'une session peut finir sans que la
    // machine d'en face s'en aperçoive autrement.
    before_the_end(aside::ask_to_hush(&bench.connection, false))
        .await
        .unwrap();
    assert!(!bench.hushed.load(Ordering::Relaxed));
}

#[tokio::test]
async fn verrouiller_l_ordinateur_distant_passe_par_le_canal_du_produit() {
    // Windows+L ne voyage pas : Windows la traite là où aucun programme
    // ne la voit, aux deux bouts d'une session, et c'est exactement ce
    // qui fait qu'un écran de verrouillage vaut quelque chose. La demande
    // prend donc le chemin de Ctrl+Alt+Suppr, et le service d'en face
    // lève l'écran depuis le seul endroit d'où son Windows l'accepte.
    let bench = Bench::bring_up(42750, 11).await;

    before_the_end(aside::ask_to_lock(&bench.connection))
        .await
        .unwrap();
    assert!(bench.locked.load(Ordering::Relaxed));
}

#[tokio::test]
async fn la_cadence_de_l_ecran_immobile_se_demande_depuis_le_client() {
    // Ce que ça coûte est payé là-bas, mais la seule personne capable de
    // dire si l'image est fluide est celle qui la regarde, et elle n'est
    // pas devant la machine qu'il faudrait aller régler.
    let bench = Bench::bring_up(42760, 13).await;

    // Et un changement lui coûte un redémarrage de son moteur, qu'elle
    // dit plutôt que de laisser l'autre bout le découvrir sur un tunnel
    // cassé : c'est la même réponse que pour l'écran à filmer.
    assert_eq!(
        before_the_end(aside::ask_to_serve_steady(&bench.connection, true))
            .await
            .unwrap(),
        zyr_tunnel::Settled::StartingOver
    );
    assert!(bench.steady.load(Ordering::Relaxed));

    // Redemandée telle quelle, elle ne coûte rien du tout, ce qui est le
    // cas ordinaire : toute session la demande.
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
    // Sinon l'ordinateur qui se connecte attendrait sur un moteur qui
    // n'attend rien, sans rien à montrer.
    let bench = Bench::bring_up(42900, 9).await;

    let refusal = before_the_end(aside::ask_to_pair(&bench.connection, REFUSED_PIN, "PC"))
        .await
        .unwrap_err();
    assert!(
        refusal.to_string().contains("n'attend aucun code"),
        "{refusal}"
    );

    // Et la voie tient toujours : un appairage raté n'emporte pas la
    // session avec lui.
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
async fn l_ecran_virtuel_se_demande_a_l_ouverture_et_se_rend_a_la_fin() {
    // L'écran virtuel dort entre les sessions, ce qui est tout l'intérêt :
    // une machine que personne ne regarde a les écrans que son
    // propriétaire a branchés et pas un de plus. Il faut donc le demander,
    // et le rendre.
    let bench = Bench::bring_up(42770, 15).await;

    let asked = WantedScreen {
        wide: 3840,
        high: 2160,
        scale: 150,
    };
    let showing = before_the_end(aside::ask_for_a_screen(&bench.connection, Some(asked)))
        .await
        .unwrap();
    // L'agrandissement voyage avec la taille : un écran à la bonne taille
    // mais pas au bon agrandissement, c'est le bureau de quelqu'un
    // d'autre à la bonne résolution.
    assert_eq!(*bench.screen.lock().unwrap(), Some(asked));
    assert_eq!(showing, Some((3840, 2160)));

    // Et ce qui est demandé voyage à chaque fois : c'est au réveil que le
    // pilote lit les tailles qu'on lui a écrites, il n'y a pas de
    // deuxième chance.
    let asked = WantedScreen {
        wide: 2560,
        high: 1440,
        scale: 125,
    };
    before_the_end(aside::ask_for_a_screen(&bench.connection, Some(asked)))
        .await
        .unwrap();
    assert_eq!(*bench.screen.lock().unwrap(), Some(asked));

    // Et sans rien de demandé, la machine d'en face répond la sienne :
    // c'est ce qui rend « garder la résolution de l'hôte » possible,
    // puisque rien de ce côté-ci ne peut deviner ce qui est branché
    // là-bas.
    let showing = before_the_end(aside::ask_for_a_screen(&bench.connection, None))
        .await
        .unwrap();
    assert_eq!(*bench.screen.lock().unwrap(), None);
    assert_eq!(showing, Some(HOST_SCREEN));
}

#[tokio::test]
async fn le_journal_de_la_machine_d_en_face_arrive_entier() {
    // Lire le journal de l'ordinateur distant sans marcher jusqu'à lui,
    // c'est la panne diagnostiquée sur les deux journaux à la fois. Ce
    // qui arrive doit donc être la page entière, lignes comprises : une
    // page tronquée en silence se lit comme une page complète.
    let bench = Bench::bring_up(42780, 16).await;

    let page = before_the_end(aside::ask_for_the_journal(&bench.connection, ""))
        .await
        .unwrap();
    assert_eq!(page, host_journal());
    // Et elle pèse bien plus qu'une question : c'est tout l'intérêt de
    // deux plafonds séparés sur ce canal.
    assert!(page.len() > 20_000, "{} octets", page.len());

    // L'autre moitié, et elle vient du même besoin : on vide les deux
    // journaux, on refait ce qui ne marche pas, on lit les deux. Vider
    // seulement celui qu'on a sous la main laisse la marche jusqu'à
    // l'autre machine exactement là où elle était.
    before_the_end(aside::ask_to_empty_the_journal(&bench.connection))
        .await
        .unwrap();
    assert!(bench.emptied.load(Ordering::Relaxed));

    let page = before_the_end(aside::ask_for_the_journal(&bench.connection, ""))
        .await
        .unwrap();
    assert!(page.is_empty(), "{page}");

    // Et le tri traverse avec la question : il se fait là-bas, avant que
    // la page ne soit coupée, seul ordre où un tri vaut quelque chose.
    bench.emptied.store(false, Ordering::Relaxed);
    let trie = before_the_end(aside::ask_for_the_journal(
        &bench.connection,
        "tag:clipboard",
    ))
    .await
    .unwrap();
    assert_eq!(trie, "trié par « tag:clipboard »");

    // Et ce que cette machine sait encoder, qui décide de ce que le menu
    // d'en face a le droit d'offrir. Elle est la seule à le savoir :
    // c'est elle qui encode.
    let named = before_the_end(aside::ask_what_it_can_encode(&bench.connection))
        .await
        .unwrap();
    assert_eq!(named, HOST_CODECS);

    // Et les écrans de cette machine, avec celui qu'on veut regarder. Le
    // cas de Victor : deux écrans allumés en face, et aucun moyen jusque-là
    // de demander le second.
    let listed = before_the_end(aside::ask_what_screens_it_has(&bench.connection))
        .await
        .unwrap();
    assert_eq!(listed, HOST_SCREENS);
    let read = zyr_proto::session::far_screens_read(&listed);
    assert_eq!(read.len(), 2);
    assert!(read[0].main);

    // Et la forme que son curseur a en ce moment, qui est ce que le
    // curseur dessiné ici va prendre. Un aller-retour de plus, sur un
    // canal déjà ouvert, pour un mot : c'est demandé plusieurs fois par
    // seconde tant qu'une main bouge.
    assert_eq!(
        before_the_end(aside::ask_for_the_pointer(&bench.connection))
            .await
            .unwrap(),
        HOST_POINTER
    );

    // L'écran principal est celui qu'elle filme déjà : toute session le
    // demande, et presque aucune ne change quoi que ce soit.
    assert_eq!(
        before_the_end(aside::ask_to_film_this_screen(&bench.connection, None))
            .await
            .unwrap(),
        zyr_tunnel::Settled::Already
    );
    // L'autre lui coûte un redémarrage de son moteur, et elle le dit
    // plutôt que de laisser l'autre bout le découvrir sur un tunnel cassé.
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
async fn ce_que_la_machine_d_en_face_atteint_arrive_entier() {
    // Le pendant du journal, sur le même canal et pour la même raison :
    // lu depuis ici plutôt qu'en marchant jusqu'à l'autre machine, et
    // entier, une mesure par seconde comprise.
    let bench = Bench::bring_up(42790, 17).await;

    let page = before_the_end(aside::ask_for_the_reach_log(&bench.connection))
        .await
        .unwrap();
    assert_eq!(page, host_reach_log());
    assert!(page.len() > 10_000, "{} octets", page.len());
}

#[tokio::test]
async fn le_presse_papiers_traverse_le_tunnel_dans_les_deux_sens() {
    // Un presse-papiers partagé n'a pas de sens dans un seul sens : ce
    // qu'on copie là-bas doit se coller ici, et ce qu'on copie ici doit
    // se coller là-bas. Un seul message fait les deux.
    let bench = Bench::bring_up(42840, 20).await;

    // Ce que quelqu'un avait copié en face, remis parce que celui qui
    // demande ne tient rien.
    let venu = before_the_end(aside::ask_about_the_clipboard(
        &bench.connection,
        None,
        None,
    ))
    .await
    .unwrap()
    .expect("ce qui était copié en face");
    assert_eq!(venu.said(), Some(HOST_CLIPBOARD));

    // Et redemandé en disant qu'on le tient déjà : rien ne revient.
    // C'est ce qui fait tenir la fonction, une question étant posée
    // plusieurs fois par seconde pendant toute une session.
    let encore = before_the_end(aside::ask_about_the_clipboard(
        &bench.connection,
        None,
        Some(venu.stamp()),
    ))
    .await
    .unwrap();
    assert_eq!(encore, None);

    // L'autre sens : une image copiée ici, plus lourde qu'une ligne,
    // part par la question elle-même. C'est le seul message de ce canal
    // qui pèse une page en partant, et c'est ce que la lecture en deux
    // temps existe pour laisser passer.
    let image = Clip::picture(vec![0x89; 300_000]);
    let rien = before_the_end(aside::ask_about_the_clipboard(
        &bench.connection,
        Some(image.clone()),
        Some(venu.stamp()),
    ))
    .await
    .unwrap();
    assert_eq!(rien, None, "ce qu'on vient de donner ne doit pas revenir");
    assert_eq!(bench.clipboard.lock().unwrap().as_ref(), Some(&image));

    // Et ce qui est copié en face après coup revient, image comprise :
    // les deux sens portent la même chose.
    let la_bas = Clip::picture(vec![0x50; 200_000]);
    *bench.clipboard.lock().unwrap() = Some(la_bas.clone());
    let recu = before_the_end(aside::ask_about_the_clipboard(
        &bench.connection,
        None,
        Some(image.stamp()),
    ))
    .await
    .unwrap();
    assert_eq!(recu, Some(la_bas));
}

#[tokio::test]
async fn une_question_trop_longue_qui_n_est_pas_le_presse_papiers_est_refusee() {
    // Le plafond d'une page n'est levé que pour la question qui le
    // nomme : sans ça, n'importe quel verbe inconnu pourrait faire
    // retenir des mégaoctets à un ordinateur qui n'a encore rien
    // compris de ce qu'on lui dit.
    let bench = Bench::bring_up(42850, 21).await;

    let (mut sending, mut receiving) = bench.connection.open_stream().await.unwrap();
    zyr_tunnel::pump::announce(&mut sending, StreamChannel::ZyrDesk)
        .await
        .unwrap();
    let trop = format!("{} pair 1234 {}", aside::VERSION, "n".repeat(8192));
    sending.write_all(trop.as_bytes()).await.unwrap();
    sending.shutdown().await.unwrap();

    // Le canal se ferme sans rien répondre, ce qui est exactement ce
    // qu'on veut : rien n'a été retenu, rien n'a été fait.
    let heard = before_the_end(receiving.read_to_end(64 * 1024)).await;
    assert!(
        heard.as_ref().map(Vec::is_empty).unwrap_or(true),
        "{heard:?}"
    );
    assert!(bench.handed.lock().unwrap().is_empty());
}

#[tokio::test]
async fn les_morceaux_d_un_fichier_traversent_dans_les_deux_sens() {
    // Ce qu'un presse-papiers porte d'un fichier est son nom ; les
    // octets suivent, un morceau à la fois, et dans le sens où on les
    // veut. Un seul message porte les deux, comme pour le presse-papiers
    // lui-même, et pour la même raison : seul celui qui a ouvert la voie
    // peut demander quoi que ce soit.
    let bench = Bench::bring_up(42860, 22).await;

    // Le sens où l'on tire : la machine d'en face a copié, celle-ci
    // colle, donc elle demande.
    let fichier: Vec<u8> = (0..200_000u32).map(|at| (at % 251) as u8).collect();
    *bench.has.lock().unwrap() = vec![b"court".to_vec(), fichier.clone()];

    let mut rassemble = Vec::new();
    let mut depuis = 0u64;
    while (depuis as usize) < fichier.len() {
        let asked = Wanted {
            rank: 1,
            from: depuis,
            how_many: aside::A_PIECE as u32,
        };
        let (given, _) =
            before_the_end(aside::ask_for_pieces(&bench.connection, Some(asked), None))
                .await
                .unwrap();
        let given = given.expect("le morceau demandé");
        assert_eq!(given.rank, 1);
        assert_eq!(given.from, depuis);
        assert!(!given.bytes.is_empty(), "un morceau vide ne finit jamais");
        depuis += given.bytes.len() as u64;
        rassemble.extend_from_slice(&given.bytes);
    }
    assert_eq!(
        rassemble, fichier,
        "le fichier remonté n'est pas le fichier"
    );

    // Et le sens où l'on pousse : c'est cette machine-ci qui a copié, et
    // celle d'en face qui colle, donc elle dit ce qu'elle veut et on le
    // lui donne. La réponse à un morceau donné dit le morceau suivant,
    // ce qui fait un aller-retour par morceau et pas deux.
    let voulu = Wanted {
        rank: 0,
        from: 4096,
        how_many: 1024,
    };
    *bench.wants.lock().unwrap() = Some(voulu);
    let (_, wanted) = before_the_end(aside::ask_for_pieces(&bench.connection, None, None))
        .await
        .unwrap();
    assert_eq!(wanted, Some(voulu));

    let donne = Given {
        rank: 0,
        from: 4096,
        bytes: vec![0x2a; 1024],
    };
    *bench.wants.lock().unwrap() = None;
    let (_, wanted) = before_the_end(aside::ask_for_pieces(
        &bench.connection,
        None,
        Some(donne.clone()),
    ))
    .await
    .unwrap();
    assert_eq!(wanted, None, "rien voulu de plus veut dire que c'est fini");
    assert_eq!(*bench.taken.lock().unwrap(), vec![donne]);
}
