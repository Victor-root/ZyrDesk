//! The whole tunnel, between a fake engine and a fake player.
//!
//! Neither the real engine nor the real player is needed to check what
//! matters here: that what one of them writes on its local link comes out
//! identical on the other's, on the right channel, in both directions,
//! and that ZyrDesk's own questions are still answered beside them.
//!
//! Everything runs in one process: both ends of the connection on
//! loopback, and both links as the real ones are made, with the test
//! holding the engine's end of one and the player's end of the other.

use std::future::Future;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use zyr_control::link::{self, Access, Channel, Link, LinkListener, LinkReader, LinkWriter};
use zyr_proto::clipboard::{Clip, Stamp};
use zyr_proto::session::{Pointer, WantedScreen};
use zyr_transport::{Bytes, Connection, Identity, MediaProfile, TunnelEndpoint};
use zyr_tunnel::aside::{self, Given, Wanted};
use zyr_tunnel::{Answers, ServiceEnd, StreamChannel, Tunnel, service_channel};

/// Past this, nothing is getting through.
const PATIENCE: Duration = Duration::from_secs(10);

async fn before_the_end<T>(work: impl Future<Output = T>) -> T {
    tokio::time::timeout(PATIENCE, work)
        .await
        .expect("the tunnel let nothing through")
}

/// What the session opening says it will be served.
const SERVED: MediaProfile = MediaProfile {
    bits_per_second: 35_000_000,
    frames_per_second: 120,
};

/// What the service says to the engine before anything else.
const SETUP: &[u8] = b"setup: 1161";

/// What someone had copied on the far machine before the session
/// opened.
const HOST_CLIPBOARD: &str = "l'adresse du serveur : 10.0.0.4";

/// Two screens switched on at the far machine, the main one first, named
/// the way the engine names them.
const HOST_SCREENS: &str = "MONITOR\\GSM5B7F\\0003 main 2560x1440 ROG PG279Q\n\\\\.\\DISPLAY2 other \
                            1920x1080 Dell U2412M";

/// The shape of the far pointer: something other than the arrow,
/// otherwise the round trip would prove nothing, an arrow also being
/// what a word nobody recognises gives back.
const HOST_POINTER: Pointer = Pointer::Text;

/// What the far machine answers when the session asks it to keep its
/// screen as it is.
const HOST_SCREEN: (u32, u32) = (1366, 768);

/// The computer being watched, as its own channel answers for it:
/// everything asked of it written down instead of done.
#[derive(Default)]
struct FarComputer {
    /// Times Ctrl+Alt+Suppr was asked for. Counted rather than done:
    /// nothing here has a Windows to press it on.
    attended: AtomicU32,
    hushed: AtomicBool,
    locked: AtomicBool,
    /// The screen its virtual one was last asked to be, `None` standing
    /// for the ask to put it back to sleep.
    screen: Mutex<Option<WantedScreen>>,
    emptied: AtomicBool,
    /// Which of its screens it was last asked to be served from, `None`
    /// standing for its main one.
    filming: Mutex<Option<String>>,
    clipboard: Mutex<Option<Clip>>,
    /// The files its own clipboard named, as their bytes.
    has: Mutex<Vec<Vec<u8>>>,
    /// What it wants next of what the far computer named.
    wants: Mutex<Option<Wanted>>,
    /// The pieces it was handed, in the order they came.
    taken: Mutex<Vec<Given>>,
}

impl Answers for FarComputer {
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

    fn screen_for_a_session(
        &self,
        wanted: Option<WantedScreen>,
    ) -> Result<Option<(u32, u32)>, String> {
        *self.screen.lock().unwrap() = wanted;
        // What a real machine answers when nobody wants its virtual
        // screen: the size of its own.
        Ok(wanted
            .map(|screen| (screen.wide, screen.high))
            .or(Some(HOST_SCREEN)))
    }

    fn journal(&self, sift: &str) -> Result<String, String> {
        if self.emptied.load(Ordering::Relaxed) {
            return Ok(String::new());
        }
        // The sift is done where the journal is gathered: what is given
        // back says which sift it was given back under, which is enough
        // to see that the sift did get across.
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

    fn pointer(&self) -> Result<Pointer, String> {
        Ok(HOST_POINTER)
    }

    fn screens(&self) -> Result<String, String> {
        Ok(HOST_SCREENS.to_string())
    }

    fn film_this_screen(&self, id: Option<String>) -> Result<(), String> {
        *self.filming.lock().unwrap() = id;
        Ok(())
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

/// A journal the size of the ones the product really writes: a page and
/// not a line.
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

/// Both ends of a connection over loopback, and the endpoints under
/// them, which have to outlive it.
async fn connected() -> (Connection, Connection, (TunnelEndpoint, TunnelEndpoint)) {
    let host_identity = Identity::generate().unwrap();
    let client_identity = Identity::generate().unwrap();
    let profile = MediaProfile::default();
    let loopback = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 0);
    let host_endpoint = TunnelEndpoint::host(
        &host_identity,
        client_identity.fingerprint(),
        profile,
        loopback,
    )
    .unwrap();
    let meeting_point = host_endpoint.local_address().unwrap();
    let client_endpoint = TunnelEndpoint::client(
        &client_identity,
        host_identity.fingerprint(),
        profile,
        loopback,
    )
    .unwrap();
    let (host_side, client_side) = tokio::join!(
        host_endpoint.accept(),
        client_endpoint.connect(meeting_point)
    );
    (
        host_side.unwrap(),
        client_side.unwrap(),
        (host_endpoint, client_endpoint),
    )
}

/// One end of a link, as a test drives it.
struct End {
    reads: LinkReader,
    writes: LinkWriter,
}

impl End {
    fn of(link: Link) -> Self {
        let (reads, writes) = link.split();
        Self { reads, writes }
    }

    async fn say(&mut self, channel: Channel, payload: &[u8]) {
        self.writes.send(channel, payload).await.unwrap();
    }

    async fn next(&mut self) -> (Channel, Bytes) {
        before_the_end(self.reads.next())
            .await
            .unwrap()
            .expect("the link closed")
    }

    /// Reads the control stream until `wanted` bytes of it have come,
    /// however the tunnel cut them up on the way.
    async fn control(&mut self, wanted: usize) -> Vec<u8> {
        let mut heard = Vec::new();
        while heard.len() < wanted {
            let (channel, payload) = self.next().await;
            assert_eq!(channel, Channel::Control, "{payload:?}");
            heard.extend_from_slice(&payload);
        }
        heard
    }

    /// Waits for the link to close, dropping whatever comes before.
    async fn closed(&mut self) {
        before_the_end(async { while let Ok(Some(_)) = self.reads.next().await {} }).await;
    }
}

/// A session, open: the tunnel on both sides, the engine's end of the
/// host's link and the player's end of the client's.
struct Bench {
    _endpoints: (TunnelEndpoint, TunnelEndpoint),
    /// The way, to ask the far ZyrDesk things beside the session.
    connection: Connection,
    host: Tunnel,
    client: Tunnel,
    engine: End,
    player: End,
    /// The service's half on the host, which hears the engine.
    host_service: ServiceEnd,
    /// The way's half on the client, which speaks to the player.
    client_service: ServiceEnd,
    /// What the opening asked to be served.
    served: MediaProfile,
    far: Arc<FarComputer>,
}

impl Bench {
    /// The real sequence, not a shortcut: the question that opens the
    /// session, the engine brought up behind it and the service saying
    /// its first word to it, the answer, then the player.
    async fn bring_up() -> Self {
        let (host_connection, client_connection, endpoints) = connected().await;
        let far = Arc::new(FarComputer::default());
        *far.clipboard.lock().unwrap() = Some(Clip::text(HOST_CLIPBOARD));
        let answering: Arc<dyn Answers> = far.clone();

        let hosting = async {
            let opening = aside::until_a_session_opens(&host_connection, answering.clone(), None)
                .await
                .unwrap();
            let listener = LinkListener::create(Access::SystemOnly).unwrap();
            let name = listener.name().to_string();
            let (engine, accepted) = tokio::join!(link::connect(&name), listener.accept());
            let (side, service) = service_channel();
            service.to_link.send(SETUP.to_vec()).await.unwrap();
            let served = opening.serving();
            opening.opened().await.unwrap();
            let host = Tunnel::host(host_connection, answering, accepted.unwrap(), side, None);
            (host, End::of(engine.unwrap()), service, served)
        };
        let ((host, engine, host_service, served), opened) = before_the_end(async {
            tokio::join!(hosting, aside::ask_to_open(&client_connection, SERVED))
        })
        .await;
        opened.unwrap();

        let listener = LinkListener::create(Access::SystemAndInteractive).unwrap();
        let name = listener.name().to_string();
        let (side, client_service) = service_channel();
        let client = Tunnel::client(client_connection.clone(), listener, side, None);
        assert!(!client.connected());
        let player = End::of(before_the_end(link::connect(&name)).await.unwrap());

        let mut bench = Self {
            _endpoints: endpoints,
            connection: client_connection,
            host,
            client,
            engine,
            player,
            host_service,
            client_service,
            served,
            far,
        };
        // What the service said before the tunnel was up is the first
        // thing the engine hears.
        let (channel, said) = bench.engine.next().await;
        assert_eq!((channel, &said[..]), (Channel::Service, SETUP));
        bench
    }

    /// Waits for the engine's stream to stand, which the player's first
    /// word on it opens.
    async fn the_player_speaks_first(&mut self) {
        self.player.say(Channel::Control, b"hello").await;
        assert_eq!(self.engine.control(5).await, b"hello");
    }
}

#[tokio::test]
async fn the_watched_computer_learns_what_it_is_asked_to_serve() {
    // The first word of a session says what it will be served, and the
    // watched computer sizes its tunnel on it.
    let bench = Bench::bring_up().await;
    assert_eq!(bench.served, SERVED);
    assert!(bench.host.connected());
    // The player is in as soon as the tunnel has taken its link, which
    // is what spares a way the patience given to a player never coming.
    before_the_end(async {
        while !bench.client.connected() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await;
}

#[tokio::test]
async fn the_control_stream_crosses_both_ways_whole_and_in_order() {
    let mut bench = Bench::bring_up().await;
    bench.the_player_speaks_first().await;

    // Many small messages, as the engine's control stream really is: a
    // key press, a ping, an answer. Whatever the tunnel cuts them into,
    // they arrive whole and in order.
    let mut sent = Vec::new();
    for turn in 0..500u32 {
        let message = format!("key {turn};");
        bench.player.say(Channel::Control, message.as_bytes()).await;
        sent.extend_from_slice(message.as_bytes());
    }
    assert_eq!(bench.engine.control(sent.len()).await, sent);

    bench.engine.say(Channel::Control, b"welcome").await;
    assert_eq!(bench.player.control(7).await, b"welcome");
}

#[tokio::test]
async fn pictures_and_sound_arrive_on_their_own_channels_both_ways() {
    let mut bench = Bench::bring_up().await;

    // The engine's pictures and sound, to the player. The channel is
    // what the player reads first: video landing in the sound would be
    // noise.
    for turn in 0..20u8 {
        let channel = if turn % 3 == 0 {
            Channel::Audio
        } else {
            Channel::Video
        };
        bench.engine.say(channel, &[turn; 1100]).await;
        let (arrived_on, arrived) = bench.player.next().await;
        assert_eq!(arrived_on, channel);
        assert_eq!(&arrived[..], &[turn; 1100][..]);
    }

    // And the other way, which nothing uses today and the bench does.
    bench.player.say(Channel::Video, b"echo").await;
    let (arrived_on, arrived) = bench.engine.next().await;
    assert_eq!((arrived_on, &arrived[..]), (Channel::Video, &b"echo"[..]));

    let host = bench.host.reading();
    let client = bench.client.reading();
    assert_eq!(host.to_tunnel, 20);
    assert_eq!(client.to_link, 20);
    assert_eq!(client.to_tunnel, 1);
    assert_eq!(host.to_link, 1);
    assert_eq!(client.crowded_here, 0);
    assert_eq!(host.too_large, 0);
}

#[tokio::test]
async fn a_player_that_falls_behind_loses_the_oldest_pictures_and_holds_nothing_up() {
    let mut bench = Bench::bring_up().await;
    bench.the_player_speaks_first().await;

    // A burst far larger than what waits for the player, and the player
    // reading none of it: each picture numbered, so what arrives says
    // what was kept.
    const BURST: u32 = 8_000;
    for number in 0..BURST {
        let mut picture = number.to_le_bytes().to_vec();
        picture.resize(1100, 0);
        bench.engine.say(Channel::Video, &picture).await;
        if number % 50 == 0 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }
    // The tunnel went on reading while the player did not: the oldest
    // were thrown away on this side, and counted.
    before_the_end(async {
        while bench.client.reading().crowded_here == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    // And what the engine says after the burst still gets through.
    bench.engine.say(Channel::Control, b"still here").await;

    let mut last = None;
    let mut heard = Vec::new();
    while last != Some(BURST - 1) || heard.len() < 10 {
        let (channel, payload) = bench.player.next().await;
        match channel {
            Channel::Video => {
                let number = u32::from_le_bytes(payload[..4].try_into().unwrap());
                // Oldest first, and never one twice: a gap is what was
                // thrown away, and nothing ever comes out of order.
                if let Some(before) = last {
                    assert!(number > before, "{number} after {before}");
                }
                last = Some(number);
            }
            Channel::Control => heard.extend_from_slice(&payload),
            other => panic!("unexpected {other:?}"),
        }
    }
    assert_eq!(heard, b"still here");
    let reading = bench.client.reading();
    assert!(reading.crowded_here > 0, "{reading:?}");
    assert!(reading.to_link < u64::from(BURST), "{reading:?}");
}

#[tokio::test]
async fn the_service_hears_the_engine_and_speaks_to_it_and_the_way_to_the_player() {
    let mut bench = Bench::bring_up().await;

    bench.engine.say(Channel::Service, b"ready").await;
    let said = before_the_end(bench.host_service.from_link.recv())
        .await
        .unwrap();
    assert_eq!(&said[..], b"ready");

    bench
        .host_service
        .to_link
        .send(b"film: main".to_vec())
        .await
        .unwrap();
    let (channel, said) = bench.engine.next().await;
    assert_eq!((channel, &said[..]), (Channel::Service, &b"film: main"[..]));

    // On the other side, the way tells the player how the tunnel stands,
    // and hears what the player has to say to it.
    bench
        .client_service
        .to_link
        .send(b"tunnel: 12 ms".to_vec())
        .await
        .unwrap();
    let (channel, said) = bench.player.next().await;
    assert_eq!(
        (channel, &said[..]),
        (Channel::Service, &b"tunnel: 12 ms"[..])
    );
    bench.player.say(Channel::Service, b"bye").await;
    let said = before_the_end(bench.client_service.from_link.recv())
        .await
        .unwrap();
    assert_eq!(&said[..], b"bye");

    // Service words never cross the tunnel: they are each side's own.
    assert_eq!(bench.host.reading().to_link, 0);
    assert_eq!(bench.client.reading().to_link, 0);
}

#[tokio::test]
async fn the_player_leaving_ends_the_tunnel_on_both_sides() {
    let mut bench = Bench::bring_up().await;
    bench.the_player_speaks_first().await;

    drop(bench.player);
    before_the_end(bench.client.wait()).await.unwrap();
    // The engine's stream ends with the client's tunnel, and the host's
    // ends with it: the engine sees its link close, which is what makes
    // it let go of whatever key it held.
    before_the_end(bench.host.wait()).await.unwrap();
    bench.host.close().await;
    bench.engine.closed().await;
}

#[tokio::test]
async fn the_engine_leaving_ends_the_tunnel_on_both_sides() {
    let mut bench = Bench::bring_up().await;
    bench.the_player_speaks_first().await;

    drop(bench.engine);
    before_the_end(bench.host.wait()).await.unwrap();
    bench.host.close().await;
    before_the_end(bench.client.wait()).await.unwrap();
    bench.client.close().await;
    bench.player.closed().await;
}

#[tokio::test]
async fn a_second_opening_is_refused_and_the_session_goes_on() {
    let mut bench = Bench::bring_up().await;
    let refusal = before_the_end(aside::ask_to_open(&bench.connection, SERVED))
        .await
        .unwrap_err();
    assert!(refusal.to_string().contains("déjà ouverte"), "{refusal}");

    bench.the_player_speaks_first().await;
    bench.engine.say(Channel::Video, b"picture").await;
    let (channel, _) = bench.player.next().await;
    assert_eq!(channel, Channel::Video);
}

#[tokio::test]
async fn a_second_engine_stream_is_refused_and_counted() {
    let mut bench = Bench::bring_up().await;
    bench.the_player_speaks_first().await;

    let (mut sending, mut receiving) = bench.connection.open_stream().await.unwrap();
    zyr_tunnel::pump::announce(&mut sending, StreamChannel::Engine)
        .await
        .unwrap();
    sending.write_all(b"intruder").await.unwrap();
    // Nothing comes back, and nothing of it reaches the engine.
    let heard = before_the_end(receiving.read_to_end(64)).await;
    assert!(heard.map(|it| it.is_empty()).unwrap_or(true));
    before_the_end(async {
        while bench.host.reading().refused == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;

    bench.player.say(Channel::Control, b"still mine").await;
    assert_eq!(bench.engine.control(10).await, b"still mine");
}

#[tokio::test]
async fn a_computer_is_answered_before_any_session_opens() {
    // Most connections never open a session: reading a far computer's
    // journal is a connection, two questions, and gone.
    let (host_connection, client_connection, _endpoints) = connected().await;
    let far: Arc<dyn Answers> = Arc::new(FarComputer::default());
    let waiting =
        tokio::spawn(
            async move { aside::until_a_session_opens(&host_connection, far, None).await },
        );

    let page = before_the_end(aside::ask_for_the_journal(&client_connection, ""))
        .await
        .unwrap();
    assert_eq!(page, host_journal());
    let page = before_the_end(aside::ask_for_the_reach_log(&client_connection))
        .await
        .unwrap();
    assert_eq!(page, host_reach_log());

    // And no session was ever opened.
    assert!(!waiting.is_finished());
    waiting.abort();
}

#[tokio::test]
async fn ctrl_alt_del_the_speakers_and_the_lock_travel_on_the_product_s_own_channel() {
    // Windows keeps these for itself at both ends: they cross between
    // the two halves of ZyrDesk, and no engine knows anything about them.
    let bench = Bench::bring_up().await;

    before_the_end(aside::ask_for_the_secure_attention(&bench.connection))
        .await
        .unwrap();
    assert_eq!(bench.far.attended.load(Ordering::Relaxed), 1);

    before_the_end(aside::ask_to_hush(&bench.connection, true))
        .await
        .unwrap();
    assert!(bench.far.hushed.load(Ordering::Relaxed));
    before_the_end(aside::ask_to_hush(&bench.connection, false))
        .await
        .unwrap();
    assert!(!bench.far.hushed.load(Ordering::Relaxed));

    before_the_end(aside::ask_to_lock(&bench.connection))
        .await
        .unwrap();
    assert!(bench.far.locked.load(Ordering::Relaxed));
}

#[tokio::test]
async fn the_virtual_screen_is_asked_for_at_the_opening_and_given_back_at_the_end() {
    let bench = Bench::bring_up().await;

    let asked = WantedScreen {
        wide: 3840,
        high: 2160,
        scale: 150,
    };
    let showing = before_the_end(aside::ask_for_a_screen(&bench.connection, Some(asked)))
        .await
        .unwrap();
    // The magnification travels with the size.
    assert_eq!(*bench.far.screen.lock().unwrap(), Some(asked));
    assert_eq!(showing, Some((3840, 2160)));

    // With nothing asked for, the far machine answers with its own size:
    // nothing on this side can guess what is plugged in over there.
    let showing = before_the_end(aside::ask_for_a_screen(&bench.connection, None))
        .await
        .unwrap();
    assert_eq!(*bench.far.screen.lock().unwrap(), None);
    assert_eq!(showing, Some(HOST_SCREEN));
}

#[tokio::test]
async fn the_screens_the_pointer_and_the_screen_to_film_are_asked_while_the_picture_runs() {
    let bench = Bench::bring_up().await;

    let listed = before_the_end(aside::ask_what_screens_it_has(&bench.connection))
        .await
        .unwrap();
    assert_eq!(listed, HOST_SCREENS);
    let read = zyr_proto::session::far_screens_read(&listed);
    assert_eq!(read.len(), 2);
    assert!(read[0].main);

    assert_eq!(
        before_the_end(aside::ask_for_the_pointer(&bench.connection))
            .await
            .unwrap(),
        HOST_POINTER
    );

    // The other screen, then the main one again: its engine changes
    // screen where it stands, and the answer is simply that it does.
    before_the_end(aside::ask_to_film_this_screen(
        &bench.connection,
        Some(read[1].id.clone()),
    ))
    .await
    .unwrap();
    assert_eq!(*bench.far.filming.lock().unwrap(), Some(read[1].id.clone()));
    before_the_end(aside::ask_to_film_this_screen(&bench.connection, None))
        .await
        .unwrap();
    assert_eq!(*bench.far.filming.lock().unwrap(), None);
}

#[tokio::test]
async fn the_far_machine_s_journal_arrives_whole_and_empties() {
    let bench = Bench::bring_up().await;

    let page = before_the_end(aside::ask_for_the_journal(&bench.connection, ""))
        .await
        .unwrap();
    assert_eq!(page, host_journal());
    assert!(page.len() > 20_000, "{} octets", page.len());

    before_the_end(aside::ask_to_empty_the_journal(&bench.connection))
        .await
        .unwrap();
    assert!(bench.far.emptied.load(Ordering::Relaxed));
    let page = before_the_end(aside::ask_for_the_journal(&bench.connection, ""))
        .await
        .unwrap();
    assert!(page.is_empty(), "{page}");

    // And the sift crosses with the question.
    bench.far.emptied.store(false, Ordering::Relaxed);
    let sifted = before_the_end(aside::ask_for_the_journal(
        &bench.connection,
        "tag:clipboard",
    ))
    .await
    .unwrap();
    assert_eq!(sifted, "trié par « tag:clipboard »");
}

#[tokio::test]
async fn the_clipboard_crosses_the_tunnel_both_ways() {
    let bench = Bench::bring_up().await;

    let arrived = before_the_end(aside::ask_about_the_clipboard(
        &bench.connection,
        None,
        None,
    ))
    .await
    .unwrap()
    .expect("ce qui était copié en face");
    assert_eq!(arrived.said(), Some(HOST_CLIPBOARD));

    // Asked again while saying it is already held: nothing comes back.
    let again = before_the_end(aside::ask_about_the_clipboard(
        &bench.connection,
        None,
        Some(arrived.stamp()),
    ))
    .await
    .unwrap();
    assert_eq!(again, None);

    // The other way: a picture copied here, a page and not a line.
    let image = Clip::picture(vec![0x89; 300_000]);
    let nothing = before_the_end(aside::ask_about_the_clipboard(
        &bench.connection,
        Some(image.clone()),
        Some(arrived.stamp()),
    ))
    .await
    .unwrap();
    assert_eq!(nothing, None);
    assert_eq!(bench.far.clipboard.lock().unwrap().as_ref(), Some(&image));
}

#[tokio::test]
async fn a_question_too_long_that_is_not_the_clipboard_is_refused() {
    // The one-page ceiling is only lifted for the questions that name
    // it: without that, any unknown verb could make a computer hold on
    // to megabytes while it has still understood nothing.
    let bench = Bench::bring_up().await;

    let (mut sending, mut receiving) = bench.connection.open_stream().await.unwrap();
    zyr_tunnel::pump::announce(&mut sending, StreamChannel::ZyrDesk)
        .await
        .unwrap();
    let too_long = format!("{} journal {}", aside::VERSION, "n".repeat(8192));
    sending.write_all(too_long.as_bytes()).await.unwrap();
    sending.finish().unwrap();

    let heard = before_the_end(receiving.read_to_end(64 * 1024)).await;
    assert!(
        heard.as_ref().map(Vec::is_empty).unwrap_or(true),
        "{heard:?}"
    );
}

#[tokio::test]
async fn the_pieces_of_a_file_cross_both_ways() {
    let bench = Bench::bring_up().await;

    // The pulling way: the far machine copied, this one pastes.
    let file: Vec<u8> = (0..200_000u32).map(|at| (at % 251) as u8).collect();
    *bench.far.has.lock().unwrap() = vec![b"court".to_vec(), file.clone()];
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
        assert_eq!(given.from, offset);
        assert!(!given.bytes.is_empty(), "un morceau vide ne finit jamais");
        offset += given.bytes.len() as u64;
        gathered.extend_from_slice(&given.bytes);
    }
    assert_eq!(gathered, file);

    // The pushing way: the far machine pastes and says what it wants.
    let asked_for = Wanted {
        rank: 0,
        from: 4096,
        how_many: 1024,
    };
    *bench.far.wants.lock().unwrap() = Some(asked_for);
    let (_, wanted) = before_the_end(aside::ask_for_pieces(&bench.connection, None, None))
        .await
        .unwrap();
    assert_eq!(wanted, Some(asked_for));
    let piece = Given {
        rank: 0,
        from: 4096,
        bytes: vec![0x2a; 1024],
    };
    *bench.far.wants.lock().unwrap() = None;
    let (_, wanted) = before_the_end(aside::ask_for_pieces(
        &bench.connection,
        None,
        Some(piece.clone()),
    ))
    .await
    .unwrap();
    assert_eq!(wanted, None);
    assert_eq!(*bench.far.taken.lock().unwrap(), vec![piece]);
}
