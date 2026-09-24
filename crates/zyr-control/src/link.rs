//! The local link: framed channels between an engine and the service.
//!
//! An engine never opens a socket. Everything it says to the rest of
//! the world, and everything it hears, goes through one link to the
//! service on the same computer: the control stream, the video, the
//! sound, and what the service itself has to tell it. Each piece
//! travels as a frame naming its channel, so a single pipe carries all
//! four without mixing them up.
//!
//! On the pipe, a frame is its length (four bytes, little-endian,
//! counting the channel and the payload), its channel (one byte), then
//! the payload.
//!
//! On Windows the link is a named pipe carrying an access list, under a
//! name nobody can guess and nobody can hold before it: only the first
//! instance of a name may claim it. Elsewhere it is a socket file in a
//! directory of its own, which is what the tests run on.

use std::io;

use bytes::{Bytes, BytesMut};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use zyr_proto::random;

/// What a frame carries, named by the byte after its length.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Channel {
    /// The control stream between the player and the engine.
    Control = 0,
    /// Video datagrams, from the host engine to the player.
    Video = 1,
    /// Messages between the service and the engine.
    Service = 2,
    /// Sound datagrams, from the host engine to the player.
    Audio = 3,
}

impl Channel {
    fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0 => Some(Channel::Control),
            1 => Some(Channel::Video),
            2 => Some(Channel::Service),
            3 => Some(Channel::Audio),
            _ => None,
        }
    }
}

/// Who, besides the system account, may open a link.
///
/// Decided when the link is made: Windows writes it into the pipe and
/// checks it on every opening. A socket file has no such list: off
/// Windows the link is reachable by its own account alone, whatever is
/// asked here, which suits the tests, both ends running as one account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Access {
    /// The system account alone: the service and the engine it starts.
    SystemOnly,
    /// The system account, and one account named by its SID, which may
    /// read and write.
    SystemAnd { user_sid: String },
    /// The system account, and whoever is logged in at the machine,
    /// who may read and write: exactly who may already drive sessions
    /// through the service's control channel.
    SystemAndInteractive,
}

/// Full control for the system account, and nothing inherited from the
/// folder of pipes: whatever else a link allows comes after this.
const SYSTEM_ALONE: &str = "D:P(A;;GA;;;SY)";

impl Access {
    /// The access list in its text form, as Windows reads it.
    ///
    /// Written on every system, so a malformed one is caught by the
    /// tests too; only Windows has somewhere to put it. The SID goes
    /// into the text as it is, which is why anything but a SID is
    /// refused: `S-1-5-18)(A;;GA;;;WD` would hand the link to everyone.
    fn access_list(&self) -> io::Result<String> {
        match self {
            Access::SystemOnly => Ok(SYSTEM_ALONE.to_string()),
            Access::SystemAnd { user_sid } if is_a_sid(user_sid) => {
                Ok(format!("{SYSTEM_ALONE}(A;;GRGW;;;{user_sid})"))
            }
            Access::SystemAnd { user_sid } => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("identifiant de compte invalide : « {user_sid} »"),
            )),
            Access::SystemAndInteractive => Ok(format!("{SYSTEM_ALONE}(A;;GRGW;;;IU)")),
        }
    }
}

/// Whether the text is a SID in its written form: `S-1-` followed by
/// numbers separated by dashes.
fn is_a_sid(text: &str) -> bool {
    text.strip_prefix("S-1-").is_some_and(|numbers| {
        numbers
            .split('-')
            .all(|number| !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()))
    })
}

/// Longest frame the link carries, channel byte included.
///
/// A video datagram weighs about a kilobyte and a service message far
/// less: the ceiling keeps a broken or hostile other end from making
/// this one set aside whatever it announces.
const LONGEST_FRAME: usize = 1024 * 1024;

/// Longest payload a frame carries.
pub const LONGEST_PAYLOAD: usize = LONGEST_FRAME - 1;

/// Bytes of the length at the head of a frame.
const LENGTH: usize = 4;

/// Bytes ahead of a payload: its length, then its channel.
const HEAD: usize = LENGTH + 1;

/// Room made for one read when no frame in progress needs more: enough
/// for dozens of video datagrams, so a burst costs few reads.
const READ_AHEAD: usize = 64 * 1024;

/// Characters drawn for a link's name.
///
/// Drawn from the system generator among 62 characters, or 36 once
/// Windows has folded the case of a pipe name: over 160 bits, far out
/// of reach of anyone trying names until one answers.
const NAME_DRAWN: usize = 32;

/// A link waiting for the other process, for one connection only.
pub struct LinkListener {
    name: String,
    waiting: mechanism::Waiting,
}

impl LinkListener {
    /// Makes a link under a name nobody can guess, nor already hold.
    pub fn create(access: Access) -> io::Result<Self> {
        Self::claim(&random::alphanumeric_string(NAME_DRAWN), &access)
    }

    /// Makes a link under the name built from `drawn`, failing when
    /// that name is already taken rather than sharing it.
    fn claim(drawn: &str, access: &Access) -> io::Result<Self> {
        let (name, waiting) = mechanism::claim(drawn, &access.access_list()?)?;
        Ok(Self { name, waiting })
    }

    /// What the other process connects to.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Waits for the other process. Once it is connected, nobody else
    /// can reach the link by its name.
    pub async fn accept(self) -> io::Result<Link> {
        mechanism::accept(self.waiting).await
    }
}

/// Connects to a link made by another process, under the name it gave.
pub async fn connect(name: &str) -> io::Result<Link> {
    mechanism::connect(name).await
}

/// A link, connected.
pub struct Link {
    reader: LinkReader,
    writer: LinkWriter,
    peer: Option<u32>,
}

impl Link {
    fn over<S>(stream: S, peer: Option<u32>) -> Self
    where
        S: AsyncRead + AsyncWrite + Send + 'static,
    {
        let (read, write) = tokio::io::split(stream);
        Self {
            reader: LinkReader {
                stream: Box::new(read),
                received: BytesMut::new(),
            },
            writer: LinkWriter {
                stream: Box::new(write),
                frame: Vec::new(),
            },
            peer,
        }
    }

    /// The process at the other end, when the system says which.
    pub fn peer_process(&self) -> Option<u32> {
        self.peer
    }

    /// The two directions, to be driven apart. The link closes once
    /// both are dropped.
    pub fn split(self) -> (LinkReader, LinkWriter) {
        (self.reader, self.writer)
    }
}

/// What arrives on a link, frame by frame.
pub struct LinkReader {
    stream: Box<dyn AsyncRead + Send + Unpin>,
    /// What came off the pipe and has not been handed out yet.
    received: BytesMut,
}

impl LinkReader {
    /// Waits for the next frame. `None` once the other side has closed
    /// the link between two frames.
    ///
    /// Safe to abandon while waiting, in a `select!` for instance: what
    /// has arrived stays here, and the next call carries on from it.
    pub async fn next(&mut self) -> io::Result<Option<(Channel, Bytes)>> {
        loop {
            if let Some(frame) = next_frame(&mut self.received)? {
                return Ok(Some(frame));
            }
            if self.stream.read_buf(&mut self.received).await? == 0 {
                if self.received.is_empty() {
                    return Ok(None);
                }
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "le lien s'est fermé au milieu d'une trame",
                ));
            }
        }
    }
}

/// Cuts the next frame off the front of what was received, when it is
/// all there.
///
/// Otherwise makes room for what is still missing, so the next read
/// can bring it in one go. A head that cannot be right is refused as
/// soon as it is there, without waiting for a payload that may never
/// come.
fn next_frame(received: &mut BytesMut) -> io::Result<Option<(Channel, Bytes)>> {
    let Some(&[a, b, c, d, byte]) = received.first_chunk::<HEAD>() else {
        received.reserve(READ_AHEAD);
        return Ok(None);
    };
    let length = u32::from_le_bytes([a, b, c, d]) as usize;
    if length == 0 || length > LONGEST_FRAME {
        return Err(unreadable(format!(
            "trame de {length} octets annoncée sur le lien"
        )));
    }
    let channel = Channel::from_byte(byte)
        .ok_or_else(|| unreadable(format!("canal inconnu sur le lien : {byte}")))?;
    let whole = LENGTH + length;
    if received.len() < whole {
        received.reserve((whole - received.len()).max(READ_AHEAD));
        return Ok(None);
    }
    Ok(Some((
        channel,
        received.split_to(whole).split_off(HEAD).freeze(),
    )))
}

/// A frame this side cannot read: the two ends are out of step, or
/// something else than a link is talking.
fn unreadable(what: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what)
}

/// What leaves on a link, frame by frame.
pub struct LinkWriter {
    stream: Box<dyn AsyncWrite + Send + Unpin>,
    /// The frame being sent, head and payload side by side so they
    /// leave in a single write, kept from one frame to the next so a
    /// frame costs no allocation.
    frame: Vec<u8>,
}

impl LinkWriter {
    /// Sends one frame, and flushes it.
    ///
    /// Not to be abandoned halfway: half a frame left on the link
    /// would put the other side out of step for good.
    pub async fn send(&mut self, channel: Channel, payload: &[u8]) -> io::Result<()> {
        if payload.len() > LONGEST_PAYLOAD {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "trame de {} octets, trop longue pour le lien",
                    payload.len()
                ),
            ));
        }
        let length = (1 + payload.len()) as u32;
        self.frame.clear();
        self.frame.extend_from_slice(&length.to_le_bytes());
        self.frame.push(channel as u8);
        self.frame.extend_from_slice(payload);
        self.stream.write_all(&self.frame).await?;
        self.stream.flush().await
    }
}

#[cfg(windows)]
mod mechanism {
    use std::io;
    use std::os::windows::io::AsRawHandle;

    use tokio::net::windows::named_pipe::{NamedPipeServer, PipeMode, ServerOptions};
    use windows_sys::Win32::System::Pipes::{
        GetNamedPipeClientProcessId, GetNamedPipeServerProcessId,
    };

    use super::Link;
    use crate::windows_pipe;

    pub type Waiting = NamedPipeServer;

    pub fn claim(drawn: &str, access_list: &str) -> io::Result<(String, Waiting)> {
        let name = format!(r"\\.\pipe\ZyrDesk-link-{drawn}");
        let pipe = windows_pipe::create(
            ServerOptions::new()
                // Only the first instance claims a name: whoever sat on
                // it earlier makes this fail rather than answer in the
                // link's place.
                .first_pipe_instance(true)
                // And no second instance may join it later: a link
                // takes one connection, ever.
                .max_instances(1)
                .reject_remote_clients(true)
                .pipe_mode(PipeMode::Byte),
            &name,
            access_list,
        )?;
        Ok((name, pipe))
    }

    pub async fn accept(pipe: Waiting) -> io::Result<Link> {
        pipe.connect().await?;
        let mut peer = 0;
        // SAFETY: the handle is the pipe's own, open for the whole
        // call, and the output is ours.
        let known = unsafe { GetNamedPipeClientProcessId(pipe.as_raw_handle(), &mut peer) } != 0;
        Ok(Link::over(pipe, known.then_some(peer)))
    }

    pub async fn connect(name: &str) -> io::Result<Link> {
        let pipe = windows_pipe::open(name).await?;
        let mut peer = 0;
        // SAFETY: as above, on the calling end.
        let known = unsafe { GetNamedPipeServerProcessId(pipe.as_raw_handle(), &mut peer) } != 0;
        Ok(Link::over(pipe, known.then_some(peer)))
    }
}

#[cfg(not(windows))]
mod mechanism {
    use std::fs::{DirBuilder, Permissions};
    use std::io;
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    use std::path::PathBuf;

    use tokio::net::{UnixListener, UnixStream};

    use super::Link;

    /// The link's own directory, removed with its socket once the link
    /// no longer needs a name.
    struct Directory(PathBuf);

    impl Drop for Directory {
        fn drop(&mut self) {
            // Otherwise every link ever made would leave a directory
            // behind in the temporary folder.
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    pub struct Waiting {
        listener: UnixListener,
        _directory: Directory,
    }

    /// A socket file has no access list: its directory, which only
    /// this account may enter, keeps everyone else away from it.
    pub fn claim(drawn: &str, _access_list: &str) -> io::Result<(String, Waiting)> {
        let path = std::env::temp_dir().join(format!("zyrdesk-link-{drawn}"));
        // Made now or not at all: a directory already there belongs to
        // another link, or to someone waiting for one.
        DirBuilder::new().mode(0o700).create(&path)?;
        let directory = Directory(path);
        let socket = directory.0.join("link");
        let listener = UnixListener::bind(&socket)?;
        std::fs::set_permissions(&socket, Permissions::from_mode(0o600))?;
        let name = socket.into_os_string().into_string().map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "dossier temporaire au nom illisible",
            )
        })?;
        Ok((
            name,
            Waiting {
                listener,
                _directory: directory,
            },
        ))
    }

    pub async fn accept(waiting: Waiting) -> io::Result<Link> {
        let (stream, _) = waiting.listener.accept().await?;
        Ok(over(stream))
    }

    pub async fn connect(name: &str) -> io::Result<Link> {
        Ok(over(UnixStream::connect(name).await?))
    }

    fn over(stream: UnixStream) -> Link {
        let peer = stream
            .peer_cred()
            .ok()
            .and_then(|credentials| credentials.pid())
            .and_then(|pid| u32::try_from(pid).ok());
        Link::over(stream, peer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tokio::io::DuplexStream;

    const EVERY_CHANNEL: [Channel; 4] = [
        Channel::Control,
        Channel::Video,
        Channel::Service,
        Channel::Audio,
    ];

    /// Access every test link is made with: the one that also lets a
    /// test run by the person at a Windows machine reach its own link.
    const IN_TESTS: Access = Access::SystemAndInteractive;

    /// Both ends of a fresh link: the one that accepted, then the one
    /// that connected.
    async fn both_ends() -> (Link, Link) {
        let listener = LinkListener::create(IN_TESTS).unwrap();
        let name = listener.name().to_string();
        let accepting = tokio::spawn(listener.accept());
        let connected = connect(&name).await.unwrap();
        (accepting.await.unwrap().unwrap(), connected)
    }

    /// A link over memory, and the raw other end, to feed it bytes no
    /// link would ever write.
    fn fed_by_hand() -> (LinkReader, DuplexStream) {
        let (ours, theirs) = tokio::io::duplex(READ_AHEAD);
        let (reader, _) = Link::over(ours, None).split();
        (reader, theirs)
    }

    fn head(length: u32, channel: u8) -> Vec<u8> {
        let mut head = length.to_le_bytes().to_vec();
        head.push(channel);
        head
    }

    fn kind_of(error: io::Result<Option<(Channel, Bytes)>>) -> io::ErrorKind {
        error.expect_err("une erreur était attendue").kind()
    }

    /// A payload that tells where it came from, so a frame delivered
    /// out of order, or mixed with another, cannot pass for it.
    fn payload(turn: usize, size: usize) -> Vec<u8> {
        (0..size).map(|i| (turn * 31 + i) as u8).collect()
    }

    #[tokio::test]
    async fn every_channel_makes_the_round_trip_both_ways() {
        let (accepted, connected) = both_ends().await;
        let (mut accepted_reads, mut accepted_writes) = accepted.split();
        let (mut connected_reads, mut connected_writes) = connected.split();
        for channel in EVERY_CHANNEL {
            let there = format!("{channel:?} to the one that accepted");
            connected_writes
                .send(channel, there.as_bytes())
                .await
                .unwrap();
            let (arrived_on, arrived) = accepted_reads.next().await.unwrap().unwrap();
            assert_eq!((arrived_on, &arrived[..]), (channel, there.as_bytes()));

            let back = format!("{channel:?} to the one that connected");
            accepted_writes
                .send(channel, back.as_bytes())
                .await
                .unwrap();
            let (arrived_on, arrived) = connected_reads.next().await.unwrap().unwrap();
            assert_eq!((arrived_on, &arrived[..]), (channel, back.as_bytes()));
        }
    }

    #[tokio::test]
    async fn an_empty_payload_is_a_frame_like_any_other() {
        let (accepted, connected) = both_ends().await;
        let (_, mut writes) = connected.split();
        let (mut reads, _) = accepted.split();
        writes.send(Channel::Service, &[]).await.unwrap();
        let (channel, arrived) = reads.next().await.unwrap().unwrap();
        assert_eq!(channel, Channel::Service);
        assert!(arrived.is_empty());
    }

    #[tokio::test]
    async fn many_frames_arrive_whole_and_in_order_both_ways_at_once() {
        const FRAMES: usize = 5_000;
        let (accepted, connected) = both_ends().await;

        async fn talk(link: Link) {
            let (mut reads, mut writes) = link.split();
            let sending = tokio::spawn(async move {
                for turn in 0..FRAMES {
                    let channel = EVERY_CHANNEL[turn % EVERY_CHANNEL.len()];
                    writes
                        .send(channel, &payload(turn, turn % 2_000))
                        .await
                        .unwrap();
                }
            });
            for turn in 0..FRAMES {
                let (channel, arrived) = reads.next().await.unwrap().unwrap();
                assert_eq!(channel, EVERY_CHANNEL[turn % EVERY_CHANNEL.len()]);
                assert_eq!(arrived, payload(turn, turn % 2_000), "trame {turn}");
            }
            sending.await.unwrap();
        }

        let one = tokio::spawn(talk(accepted));
        let other = tokio::spawn(talk(connected));
        one.await.unwrap();
        other.await.unwrap();
    }

    #[tokio::test]
    async fn the_largest_frame_goes_through() {
        let (accepted, connected) = both_ends().await;
        let (_, mut writes) = connected.split();
        let (mut reads, _) = accepted.split();
        let largest = payload(7, LONGEST_PAYLOAD);
        let sent = largest.clone();
        let sending =
            tokio::spawn(async move { writes.send(Channel::Video, &sent).await.unwrap() });
        let (channel, arrived) = reads.next().await.unwrap().unwrap();
        assert_eq!(channel, Channel::Video);
        assert_eq!(arrived, largest);
        sending.await.unwrap();
    }

    #[tokio::test]
    async fn a_payload_over_the_ceiling_is_refused_and_nothing_leaves() {
        let (accepted, connected) = both_ends().await;
        let (_, mut writes) = connected.split();
        let (mut reads, _) = accepted.split();
        let refused = writes
            .send(Channel::Video, &vec![0; LONGEST_PAYLOAD + 1])
            .await
            .expect_err("une trame trop longue doit être refusée");
        assert_eq!(refused.kind(), io::ErrorKind::InvalidInput);

        // Nothing of it reached the link: the next frame is the first
        // the other side sees.
        writes.send(Channel::Control, b"next").await.unwrap();
        let (channel, arrived) = reads.next().await.unwrap().unwrap();
        assert_eq!((channel, &arrived[..]), (Channel::Control, &b"next"[..]));
    }

    #[tokio::test]
    async fn a_frame_announced_over_the_ceiling_is_refused_without_waiting_for_it() {
        let (mut reads, mut feeding) = fed_by_hand();
        // The head alone is written: the refusal must not wait for a
        // megabyte that is never coming.
        feeding
            .write_all(&head(LONGEST_FRAME as u32 + 1, Channel::Video as u8))
            .await
            .unwrap();
        assert_eq!(kind_of(reads.next().await), io::ErrorKind::InvalidData);
    }

    #[tokio::test]
    async fn the_other_side_leaving_reads_as_the_end() {
        let (accepted, connected) = both_ends().await;
        let (mut reads, _writes) = accepted.split();
        let (_, mut writes) = connected.split();
        writes.send(Channel::Control, b"goodbye").await.unwrap();
        drop(writes);

        // What was sent before leaving still arrives, then the end.
        let (_, arrived) = reads.next().await.unwrap().unwrap();
        assert_eq!(&arrived[..], b"goodbye");
        assert!(reads.next().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn a_link_left_halfway_through_a_frame_is_an_error_not_an_end() {
        let (mut reads, mut feeding) = fed_by_hand();
        feeding
            .write_all(&head(10, Channel::Audio as u8))
            .await
            .unwrap();
        feeding.write_all(b"abc").await.unwrap();
        drop(feeding);
        assert_eq!(kind_of(reads.next().await), io::ErrorKind::UnexpectedEof);
    }

    #[tokio::test]
    async fn garbage_is_refused() {
        let garbage: [&[u8]; 4] = [
            // Something that is not a link at all.
            b"GET / HTTP/1.1\r\n\r\n",
            // A length that leaves no room for the channel.
            &head(0, Channel::Control as u8),
            // Channels no link knows.
            &head(3, 4),
            &head(3, 0xff),
        ];
        for bytes in garbage {
            let (mut reads, mut feeding) = fed_by_hand();
            feeding.write_all(bytes).await.unwrap();
            assert_eq!(
                kind_of(reads.next().await),
                io::ErrorKind::InvalidData,
                "{bytes:?}"
            );
        }
    }

    #[tokio::test]
    async fn a_read_abandoned_halfway_loses_nothing() {
        let (mut reads, mut feeding) = fed_by_hand();
        let mut frame = head(1 + 5, Channel::Control as u8);
        frame.extend_from_slice(b"whole");
        let (first, rest) = frame.split_at(3);

        feeding.write_all(first).await.unwrap();
        let waited = tokio::time::timeout(std::time::Duration::from_millis(20), reads.next()).await;
        assert!(waited.is_err(), "la trame n'était pas encore complète");

        feeding.write_all(rest).await.unwrap();
        let (channel, arrived) = reads.next().await.unwrap().unwrap();
        assert_eq!((channel, &arrived[..]), (Channel::Control, &b"whole"[..]));
    }

    #[tokio::test]
    async fn each_end_knows_the_process_at_the_other() {
        let (accepted, connected) = both_ends().await;
        // Both ends live in this very process.
        assert_eq!(accepted.peer_process(), Some(std::process::id()));
        assert_eq!(connected.peer_process(), Some(std::process::id()));
    }

    #[tokio::test]
    async fn a_name_already_held_cannot_be_claimed_again() {
        let drawn = random::alphanumeric_string(NAME_DRAWN);
        let holder = LinkListener::claim(&drawn, &IN_TESTS).unwrap();
        assert!(LinkListener::claim(&drawn, &IN_TESTS).is_err());

        // The failed attempt took nothing away from the holder.
        let name = holder.name().to_string();
        let accepting = tokio::spawn(holder.accept());
        let (_, mut writes) = connect(&name).await.unwrap().split();
        writes.send(Channel::Service, b"still here").await.unwrap();
        let (mut reads, _) = accepting.await.unwrap().unwrap().split();
        let (_, arrived) = reads.next().await.unwrap().unwrap();
        assert_eq!(&arrived[..], b"still here");
    }

    #[tokio::test]
    async fn two_links_never_share_a_name() {
        let one = LinkListener::create(IN_TESTS).unwrap();
        let other = LinkListener::create(IN_TESTS).unwrap();
        assert_ne!(one.name(), other.name());
    }

    #[tokio::test]
    async fn a_link_already_connected_takes_nobody_else() {
        let listener = LinkListener::create(IN_TESTS).unwrap();
        let name = listener.name().to_string();
        let accepting = tokio::spawn(listener.accept());
        let _first = connect(&name).await.unwrap();
        let _accepted = accepting.await.unwrap().unwrap();
        assert!(connect(&name).await.is_err());
    }

    #[test]
    fn access_lists_give_the_system_everything_and_the_other_account_read_and_write() {
        assert_eq!(Access::SystemOnly.access_list().unwrap(), "D:P(A;;GA;;;SY)");
        assert_eq!(
            Access::SystemAnd {
                user_sid: "S-1-5-21-1004336348-1177238915-682003330-1001".to_string()
            }
            .access_list()
            .unwrap(),
            "D:P(A;;GA;;;SY)(A;;GRGW;;;S-1-5-21-1004336348-1177238915-682003330-1001)"
        );
        assert_eq!(
            Access::SystemAndInteractive.access_list().unwrap(),
            "D:P(A;;GA;;;SY)(A;;GRGW;;;IU)"
        );
    }

    #[test]
    fn only_a_sid_is_written_into_an_access_list() {
        for sid in ["S-1-5-18", "S-1-5-21-1004336348-1177238915-682003330-1001"] {
            assert!(is_a_sid(sid), "{sid}");
        }
        for not_a_sid in [
            "",
            "BA",
            "IU",
            "S-1-",
            "S-1-5-",
            "S-1-5--18",
            "s-1-5-18",
            "S-2-5-18",
            "S-1-5-18)(A;;GA;;;WD",
            "S-1-5-18 ",
        ] {
            assert!(!is_a_sid(not_a_sid), "{not_a_sid:?}");
        }
    }

    #[test]
    fn a_link_for_an_account_that_is_not_a_sid_is_never_made() {
        let refused = LinkListener::create(Access::SystemAnd {
            user_sid: "S-1-5-18)(A;;GA;;;WD".to_string(),
        })
        .err()
        .expect("un identifiant forgé doit être refusé");
        assert_eq!(refused.kind(), io::ErrorKind::InvalidInput);
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn only_its_own_account_can_reach_the_socket() {
        use std::os::unix::fs::PermissionsExt;
        use std::path::Path;

        let listener = LinkListener::create(IN_TESTS).unwrap();
        let socket = Path::new(listener.name());
        let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(socket), 0o600);
        assert_eq!(mode(socket.parent().unwrap()), 0o700);
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn nothing_is_left_behind_once_the_name_is_no_longer_needed() {
        use std::path::PathBuf;

        let abandoned = LinkListener::create(IN_TESTS).unwrap();
        let directory = PathBuf::from(abandoned.name())
            .parent()
            .unwrap()
            .to_path_buf();
        drop(abandoned);
        assert!(!directory.exists());

        let listener = LinkListener::create(IN_TESTS).unwrap();
        let name = listener.name().to_string();
        let directory = PathBuf::from(&name).parent().unwrap().to_path_buf();
        let accepting = tokio::spawn(listener.accept());
        let _connected = connect(&name).await.unwrap();
        let _accepted = accepting.await.unwrap().unwrap();
        assert!(!directory.exists());
    }
}
