//! What the tests of this brick stand in for: the service, the far
//! computer behind it, and FFmpeg.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use zyr_control::{Answer, Door, Reached, Request, WayId};
use zyr_player::Ffmpeg;
use zyr_proto::fact::Fact;
use zyr_proto::session::SessionSettings;

use crate::Wanted;

pub(crate) fn wanted() -> Wanted {
    Wanted {
        host: "192.168.1.20".to_string(),
        peer: "0829cc7ecb9e9ba53cd36e6f342268ddf3c8ef05a49d1d7944ac6332c89cf237"
            .parse()
            .unwrap(),
        settings: SessionSettings::default(),
        hush_the_far_speakers: true,
        wants_a_screen_over_there: true,
        far_magnification: 150,
        far_screen: Some(r"MONITOR\GSM5B7F\0003".to_string()),
        only_here: false,
    }
}

/// A service of its own for a test, on a channel of its own, which
/// answers each request as `answering` says and writes down what it
/// was asked, in order.
///
/// It takes one call after another, as the real one does: a picture
/// brought back opens a way again, and calls again to ask for it.
pub(crate) fn a_service(
    what: &str,
    answering: impl Fn(&Request) -> Option<Answer> + Send + 'static,
) -> (String, Arc<Mutex<Vec<Request>>>) {
    let channel = format!("zyr-session-test-{}-{what}", std::process::id());
    let listening = channel.clone();
    let asked = Arc::new(Mutex::new(Vec::new()));
    let writing = Arc::clone(&asked);
    let (opened, when_open) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("the test service's runtime starts");
        runtime.block_on(async move {
            let mut door = Door::open(&listening).expect("the test channel opens");
            opened.send(()).expect("the test channel is announced");
            while let Ok(mut heard) = door.accept().await {
                while let Ok(Some(line)) = heard.hear().await {
                    let request = Request::parse(&line).expect("a readable request");
                    let answer = answering(&request);
                    writing.lock().unwrap().push(request);
                    match answer {
                        Some(answer) => heard
                            .say(&answer.to_string())
                            .await
                            .expect("the answer is sent"),
                        // Held without an answer: a service still
                        // chasing the far computer.
                        None => std::future::pending::<()>().await,
                    }
                }
            }
        });
    });
    when_open.recv().expect("the test channel is open");
    (channel, asked)
}

/// What an ordinary far computer answers.
pub(crate) fn willing(request: &Request) -> Option<Answer> {
    Some(match request {
        Request::Reach { .. } => Answer::Reached(Reached {
            way: WayId(7),
            link: r"\\.\pipe\ZyrDesk-link-8fKq2Lr0aZ3x9Wm1".to_string(),
        }),
        Request::Hush { .. } => Answer::Refused(nobody_signed_in()),
        Request::FarScreen { .. } => Answer::Showing {
            size: Some((2560, 1440)),
        },
        _ => Answer::Done,
    })
}

/// What a far computer with nobody signed in answers when asked to
/// silence its speakers.
pub(crate) fn nobody_signed_in() -> Fact {
    Fact::new("far.hush_failed").with("detail", "nobody is signed in on this computer")
}

/// Waits for the service to have been asked `count` things.
pub(crate) fn until_asked(asked: &Mutex<Vec<Request>>, count: usize) -> Vec<Request> {
    let began = Instant::now();
    loop {
        let so_far = asked.lock().unwrap().clone();
        if so_far.len() >= count || began.elapsed() > Duration::from_secs(5) {
            return so_far;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// A folder holding FFmpeg, as far as looking for it goes: empty
/// files under the names its libraries are opened by. Gone with the
/// test.
pub(crate) struct FfmpegHere(pub(crate) PathBuf);

impl FfmpegHere {
    pub(crate) fn new(what: &str) -> Self {
        let folder =
            std::env::temp_dir().join(format!("zyr-session-ffmpeg-{}-{what}", std::process::id()));
        std::fs::create_dir_all(&folder).expect("the test folder is made");
        for file in Ffmpeg::missing_from(&folder) {
            std::fs::write(file, b"").expect("a test file is written");
        }
        Self(folder)
    }
}

impl Drop for FfmpegHere {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
