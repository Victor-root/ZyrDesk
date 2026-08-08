//! The channel the service listens on, and the only file naming the
//! mechanism that carries it.
//!
//! Windows has named pipes, which is what the service uses: they carry
//! an access list, they exist without a port, and nothing on the network
//! can reach them. Elsewhere there is a socket file, which behaves the
//! same and lets everything here be tested off Windows.
//!
//! Who may speak is decided when the channel is created, not on each
//! message. The service runs as the system account, and the interface
//! runs as the person sitting at the machine: without an access list
//! saying so, that person could not drive their own computer.

use std::io;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

/// Name of the channel the product uses.
pub const CHANNEL: &str = "ZyrDesk";

/// Longest message accepted, generous for a line of fields.
///
/// Without a ceiling, anything able to reach the channel could hold the
/// service on a line that never ends.
const LONGEST_MESSAGE: u64 = 8 * 1024;

/// One exchange with the other side, message by message.
pub struct Conversation<S> {
    lines: BufReader<S>,
}

impl<S> Conversation<S>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    fn over(stream: S) -> Self {
        Self {
            lines: BufReader::new(stream),
        }
    }

    /// Waits for the next message. `None` once the other side is gone.
    pub async fn hear(&mut self) -> io::Result<Option<String>> {
        let mut line = String::new();
        let read = (&mut self.lines)
            .take(LONGEST_MESSAGE)
            .read_line(&mut line)
            .await?;
        if read == 0 {
            return Ok(None);
        }
        if !line.ends_with('\n') {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "message trop long",
            ));
        }
        Ok(Some(line.trim_end().to_string()))
    }

    /// Sends one message, ended so the other side knows it is whole.
    pub async fn say(&mut self, message: &str) -> io::Result<()> {
        let stream = self.lines.get_mut();
        stream.write_all(message.as_bytes()).await?;
        stream.write_all(b"\n").await?;
        stream.flush().await
    }
}

#[cfg(windows)]
mod mechanism {
    use std::ffi::c_void;
    use std::io;

    use tokio::net::windows::named_pipe::{
        ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions,
    };

    use super::Conversation;

    pub type Heard = Conversation<NamedPipeServer>;
    pub type Spoken = Conversation<NamedPipeClient>;

    /// Where a channel of that name lives, in the shape Windows reserves
    /// for pipes.
    fn address(channel: &str) -> String {
        format!(r"\\.\pipe\{channel}")
    }

    /// Who may speak to the service.
    ///
    /// The system account and the administrators keep full control; the
    /// person logged in at the machine may read and write, which is what
    /// driving their own computer requires. Nothing is granted to anyone
    /// else, and nothing at all to the network: the list Windows would
    /// have picked leaves the interface unable to write a single
    /// message.
    const ACCESS: &str = "D:(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGW;;;IU)";

    /// The access list, held while a pipe instance is created with it,
    /// and given back to Windows afterwards.
    struct AccessList(*mut c_void);

    impl AccessList {
        fn build() -> io::Result<Self> {
            let text: Vec<u16> = ACCESS.encode_utf16().chain(std::iter::once(0)).collect();
            let mut descriptor: windows_sys::Win32::Security::PSECURITY_DESCRIPTOR =
                std::ptr::null_mut();
            // SAFETY: the text is null-terminated UTF-16 and the output
            // pointer is ours; Windows fills it or says why not.
            let built = unsafe {
                windows_sys::Win32::Security::Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    text.as_ptr(),
                    windows_sys::Win32::Security::Authorization::SDDL_REVISION_1,
                    &mut descriptor,
                    std::ptr::null_mut(),
                )
            };
            if built == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(Self(descriptor))
        }
    }

    impl Drop for AccessList {
        fn drop(&mut self) {
            // SAFETY: the pointer comes from the call above, which
            // allocates it, and is given back exactly once.
            unsafe { windows_sys::Win32::Foundation::LocalFree(self.0) };
        }
    }

    /// Creates one instance of the pipe, ready to be connected to.
    fn instance(channel: &str, first: bool) -> io::Result<NamedPipeServer> {
        let list = AccessList::build()?;
        let mut attributes = windows_sys::Win32::Security::SECURITY_ATTRIBUTES {
            nLength: size_of::<windows_sys::Win32::Security::SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: list.0,
            bInheritHandle: 0,
        };
        // SAFETY: the attributes live until the call returns, which is
        // all Windows reads them for.
        unsafe {
            ServerOptions::new()
                // Only the first instance claims the name: without this,
                // anything started earlier could sit on it and answer in
                // the service's place.
                .first_pipe_instance(first)
                .create_with_security_attributes_raw(
                    address(channel),
                    &raw mut attributes as *mut c_void,
                )
        }
    }

    /// The channel, open and waiting.
    ///
    /// One instance always stands ready to be connected to. Windows
    /// refuses a program that finds none, rather than making it wait:
    /// the replacement is therefore made and put in place before the
    /// one being connected to is ever awaited, so the door is never
    /// caught, even for an instant, with nothing listening on it.
    pub struct Door {
        channel: String,
        waiting: NamedPipeServer,
    }

    impl Door {
        pub fn open(channel: &str) -> io::Result<Self> {
            Ok(Self {
                channel: channel.to_string(),
                waiting: instance(channel, true)?,
            })
        }

        pub async fn accept(&mut self) -> io::Result<Heard> {
            let spare = instance(&self.channel, false)?;
            let connecting = std::mem::replace(&mut self.waiting, spare);
            connecting.connect().await?;
            Ok(Conversation::over(connecting))
        }
    }

    pub async fn call(channel: &str) -> io::Result<Spoken> {
        Ok(Conversation::over(
            ClientOptions::new().open(address(channel))?,
        ))
    }
}

#[cfg(not(windows))]
mod mechanism {
    use std::io;
    use std::path::PathBuf;

    use tokio::net::{UnixListener, UnixStream};

    use super::Conversation;

    pub type Heard = Conversation<UnixStream>;
    pub type Spoken = Conversation<UnixStream>;

    /// Where a channel of that name lives, among the product's files.
    fn address(channel: &str) -> PathBuf {
        zyr_proto::paths::data_dir().join(format!("{channel}.sock"))
    }

    pub struct Door {
        listener: UnixListener,
    }

    impl Door {
        pub fn open(channel: &str) -> io::Result<Self> {
            let path = address(channel);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            // A socket file outlives the program that made it: left
            // behind by a crash, it would refuse every later start.
            let _ = std::fs::remove_file(&path);
            Ok(Self {
                listener: UnixListener::bind(&path)?,
            })
        }

        pub async fn accept(&mut self) -> io::Result<Heard> {
            let (stream, _) = self.listener.accept().await?;
            Ok(Conversation::over(stream))
        }
    }

    pub async fn call(channel: &str) -> io::Result<Spoken> {
        Ok(Conversation::over(
            UnixStream::connect(address(channel)).await?,
        ))
    }
}

pub use mechanism::{Door, Heard, Spoken};

/// Joins the service. Fails at once when it is not running.
pub async fn call(channel: &str) -> io::Result<Spoken> {
    mechanism::call(channel).await
}

#[cfg(test)]
mod tests {
    use super::*;

    use tokio::task::JoinSet;

    /// Each test gets its own channel: they run at the same time, and a
    /// shared name would make them fight over it.
    fn a_channel_of_its_own(what: &str) -> String {
        format!("zyrdesk-test-{}-{what}", std::process::id())
    }

    #[tokio::test]
    async fn a_message_comes_out_the_way_it_went_in() {
        let channel = a_channel_of_its_own("echo");
        let mut door = Door::open(&channel).unwrap();
        let listening = tokio::spawn(async move {
            let mut heard = door.accept().await.unwrap();
            let message = heard.hear().await.unwrap().unwrap();
            heard.say(&format!("écho {message}")).await.unwrap();
            // Nothing more is said: the caller sees the channel close.
            assert_eq!(heard.hear().await.unwrap(), None);
        });

        let mut speaking = call(&channel).await.unwrap();
        speaking.say("bonjour").await.unwrap();
        assert_eq!(
            speaking.hear().await.unwrap(),
            Some("écho bonjour".to_string())
        );
        drop(speaking);
        listening.await.unwrap();
    }

    #[tokio::test]
    async fn a_message_without_end_is_refused_rather_than_awaited() {
        let channel = a_channel_of_its_own("endless");
        let mut door = Door::open(&channel).unwrap();
        let listening = tokio::spawn(async move {
            let mut heard = door.accept().await.unwrap();
            assert!(heard.hear().await.is_err());
        });

        let mut speaking = call(&channel).await.unwrap();
        let endless = "x".repeat(LONGEST_MESSAGE as usize + 1);
        // Written without its ending: the service must give up on it
        // rather than hold a task on a line that never comes.
        let _ = speaking.lines.get_mut().write_all(endless.as_bytes()).await;
        listening.await.unwrap();
    }

    #[tokio::test]
    async fn programs_arriving_at_the_same_instant_are_all_taken_in() {
        // Sequential calls do not exercise the door's weak point: the
        // instant between one instance being connected to and the next
        // being made ready. Only calls fired together, with nothing
        // ordering them, land in that gap if it exists.
        const CALLERS: usize = 12;
        let channel = a_channel_of_its_own("crowd");
        let mut door = Door::open(&channel).unwrap();
        let listening = tokio::spawn(async move {
            for _ in 0..CALLERS {
                let mut heard = door.accept().await.unwrap();
                let message = heard.hear().await.unwrap().unwrap();
                heard.say(&message).await.unwrap();
            }
        });

        let mut callers = JoinSet::new();
        for turn in 0..CALLERS {
            let channel = channel.clone();
            callers.spawn(async move {
                let mut speaking = call(&channel).await.expect("porte occupée");
                speaking.say(&turn.to_string()).await.unwrap();
                assert_eq!(speaking.hear().await.unwrap(), Some(turn.to_string()));
            });
        }
        while let Some(result) = callers.join_next().await {
            result.unwrap();
        }
        listening.await.unwrap();
    }

    #[tokio::test]
    async fn several_programs_are_taken_in_one_after_another() {
        let channel = a_channel_of_its_own("queue");
        let mut door = Door::open(&channel).unwrap();
        let listening = tokio::spawn(async move {
            for _ in 0..3 {
                let mut heard = door.accept().await.unwrap();
                let message = heard.hear().await.unwrap().unwrap();
                heard.say(&message).await.unwrap();
            }
        });

        for turn in 0..3 {
            let mut speaking = call(&channel).await.unwrap();
            speaking.say(&format!("tour {turn}")).await.unwrap();
            assert_eq!(speaking.hear().await.unwrap(), Some(format!("tour {turn}")));
        }
        listening.await.unwrap();
    }
}
