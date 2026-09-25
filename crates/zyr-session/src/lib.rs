//! Opening a session on a remote computer, end to end.
//!
//! The service opens a way to the remote computer and hands back the
//! name of the local link its player connects to: through that link, the
//! far computer's engine. Before any picture is asked for, the far
//! computer is told what the player cannot say for itself: which of its
//! screens to film, whether its speakers go quiet, and what screen to
//! show on. The player is then started by whoever asked, with the link's
//! name, since it lives in their process: the window draws its picture,
//! the command line counts it.
//!
//! This lives apart from the command line and the interface because both
//! do exactly the same thing here, and the difference between them is
//! only how they say it: one prints, the other draws.
//!
//! Progress is reported as it happens rather than returned at the end:
//! opening a way takes seconds, and a window with nothing to say for all
//! of them looks stuck.

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use zyr_codec::Ffmpeg;
use zyr_control::{Answer, CHANNEL, Request, Service, WayId};
use zyr_media::codec::CodecChoice;
use zyr_media::control::Wanted as PlayerWants;
use zyr_proto::paths;
use zyr_proto::session::{Codec, SessionSettings, WantedScreen};
use zyr_transport::{Fingerprint, MediaProfile};

/// What is being asked for.
pub struct Wanted {
    /// Address of the remote computer, as the person wrote it.
    pub host: String,
    /// Fingerprint the remote computer is recognised by.
    pub peer: Fingerprint,
    pub settings: SessionSettings,
    /// Whether the far computer's speakers fall silent for the length of
    /// the session.
    ///
    /// Asked from here because the choice belongs here: whoever takes
    /// control of a machine in another room is the one who knows that the
    /// room should go quiet, and a setting on that machine would have to
    /// be walked over to, which is the one thing remote control exists to
    /// spare. It travels on the product's own channel, never through an
    /// engine, and the far computer gives its sound back when the way
    /// closes, whatever became of this end.
    pub hush_the_far_speakers: bool,
    /// Whether the far computer is asked for a screen of its own making
    /// to carry this session's picture.
    ///
    /// False means leave that machine exactly as it is: no virtual
    /// screen, no resolution changed under whoever is sitting in front of
    /// it. The size then comes back from that computer, since nothing
    /// here can know what is plugged in there.
    pub wants_a_screen_over_there: bool,
    /// How much larger than life that screen is asked to draw, in per
    /// cent, when one is asked for at all.
    ///
    /// Nought names none and takes whatever that computer recommends for
    /// a screen that size. A session that mirrors the screen it is
    /// watched on knows the number and owes it: the size makes the
    /// picture sharp, and this makes what is in it the size it is here.
    pub far_magnification: u32,
    /// Which of the far computer's own screens to be served from, under
    /// that computer's own name for it.
    ///
    /// Nothing named is its main screen, which is what every session asks
    /// for until somebody says otherwise. A machine with two screens
    /// plugged in shows one of them, and the choice belongs to whoever is
    /// watching: they are the one looking at it, and they are not in the
    /// room to lean over and drag a window across.
    ///
    /// Asked before the picture, so that the first picture is already of
    /// that screen. Its engine changes screen where it stands afterwards,
    /// asked on the same way.
    pub far_screen: Option<String>,
    /// Whether the session may only be opened on this local network,
    /// with nothing asked of any server.
    ///
    /// Two computers of one account are ordinarily put in touch by the
    /// server, even two on the same network, because a meeting brings
    /// more roads than an address does. This asks for one road and one
    /// only: the address this network announced. Chosen by whoever opens
    /// the session, because they are the one who knows that the machine
    /// they want is in the next room.
    pub only_here: bool,
}

/// What is happening, as it happens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// The way is open, and the far computer is being told what the
    /// session wants of it.
    Reached,
    /// The far computer would not change which of its screens it serves
    /// from, and the session goes on regardless.
    ///
    /// What it costs is being served the screen it is already on, which
    /// is what every session got before this was offered.
    FarScreenLeftAlone { refused: String },
    /// The far computer would not silence its own speakers, and the
    /// session goes on regardless.
    ///
    /// Worth saying and never worth failing over: a far computer that
    /// cannot go quiet, because nobody is signed in on it or because
    /// Windows would not have it, still has a perfectly good session to
    /// give.
    SpeakersLeftAlone { refused: String },
    /// The far computer would not wake its virtual screen, and the
    /// session goes on regardless.
    ///
    /// What it costs is the sharpness of a picture larger than that
    /// computer's own screen: without the virtual screen it serves what
    /// its own screen can draw and this end stretches the rest, which is
    /// what every session did before that screen existed.
    ScreenLeftAlone { refused: String },
    /// The far computer said what it will be showing, and it is not what
    /// this end had asked for.
    ///
    /// What a session set to leave that computer's screen alone is
    /// entirely built on: its size is unknown here until it says it, and
    /// asking for a picture of any other size would scale it for
    /// nothing.
    ScreenOverThere { wide: u32, high: u32 },
    /// This computer has nothing to play the session's sound through.
    ///
    /// Worth saying out loud: the session is silent and the person is
    /// owed the reason, which is a sound card missing here and not
    /// anything the far computer did.
    NoSoundCardHere,
}

/// Stops the opening where it stands when the person has let it go.
///
/// Written once and called between the steps that can take seconds. An
/// opening only asked about at its very end is an opening a person
/// cannot close, and the close is a click on the cross of the window
/// they are watching it in.
fn carry_on(still_wanted: &dyn Fn() -> bool) -> Result<(), Error> {
    if still_wanted() {
        return Ok(());
    }
    Err(Error::Abandoned)
}

/// How long a wait for the service lasts before looking up to ask
/// whether the session is still wanted.
///
/// Short enough that a click to close is felt almost at once, long enough
/// that the wait is not a spin.
const WATCH_STEP: Duration = Duration::from_millis(100);

/// Why an ask of the service came back with no answer.
///
/// Two very different things, and the whole point is telling them apart.
/// Nearly every ask below is one a session survives: the far computer
/// would not silence its speakers, would not change screen, and the
/// picture is worth having anyway, so the refusal is written down as a
/// `Step` and stepped over. Somebody closing the window is not one of
/// those, and reported as a refusal it was read as one: the opening
/// wrote « les enceintes restent allumées » and carried on towards a
/// picture nobody was waiting for any more.
enum GaveUp {
    /// The service, or the far computer through it, said no.
    Said(String),
    /// The person let go of the opening while this was waiting.
    Abandoned,
}

impl GaveUp {
    /// What was refused, the abandonment travelling on out instead.
    ///
    /// For the asks a session survives. The refusal is theirs to write
    /// down; the `?` is what keeps the other one from being written down
    /// as though it were one.
    fn refusal(self) -> Result<String, Error> {
        match self {
            GaveUp::Said(reason) => Ok(reason),
            GaveUp::Abandoned => Err(Error::Abandoned),
        }
    }

    /// The same for the asks a session does not survive: the refusal
    /// under the name that says which ask it was.
    fn or(self, said: impl FnOnce(String) -> Error) -> Error {
        match self {
            GaveUp::Said(reason) => said(reason),
            GaveUp::Abandoned => Error::Abandoned,
        }
    }
}

/// The service answering something else entirely, said the one way.
fn unexpected(answer: Answer) -> String {
    format!("réponse inattendue du service : {answer}")
}

/// Waits for the service to answer, and lets go the moment the person
/// does.
///
/// Every ask of the service made while a session is opening goes through
/// here, and one of them is why it exists: opening a way races addresses
/// for half a minute, or waits on a meeting the server arranges, and
/// that is where an opening spends nearly all its time. `carry_on`
/// guards the ground between the asks, which was enough for exactly as
/// long as the asks themselves were quick; towards a computer that never
/// answers, the cross was read half a minute after it was clicked, and a
/// cross that does nothing is a cross nobody believes twice.
///
/// The answer is driven in short spells rather than waited for whole, so
/// the question can be put between two of them. What the person leaves
/// behind is a channel with an answer still coming on it; the only thing
/// ever said on it afterwards is the way going back, which `Drop` says
/// and whose answer it does not read. A way let go of before it was so
/// much as named here belongs to nobody at all, and the service's own
/// sweep of the ways nobody claimed is what closes it.
fn answered(
    runtime: &tokio::runtime::Runtime,
    service: &mut Service,
    request: &Request,
    still_wanted: &dyn Fn() -> bool,
) -> Result<Answer, GaveUp> {
    let mut asking = std::pin::pin!(service.ask(request));
    loop {
        // The spell is counted inside the runtime and not around it: a
        // timer made where no runtime is running has nothing to wake it,
        // and says so by taking the whole program down with it.
        let spell = async { tokio::time::timeout(WATCH_STEP, asking.as_mut()).await };
        match runtime.block_on(spell) {
            Ok(answered) => return answered.map_err(|e| GaveUp::Said(e.to_string())),
            // Still coming. The one thing worth doing with the pause is
            // looking up.
            Err(_) => {
                if !still_wanted() {
                    return Err(GaveUp::Abandoned);
                }
            }
        }
    }
}

#[derive(Debug)]
pub enum Error {
    /// FFmpeg is not all there: these of its files are missing from
    /// `vendor/ffmpeg`, and the player decodes the picture with it.
    EngineMissing(Vec<PathBuf>),
    /// The service could not be asked, or refused.
    Service(String),
    /// The person closed the window on the opening before there was a
    /// picture, so it was let go of.
    ///
    /// Not a failure, and the one road out of here that has nothing to
    /// show anybody: whoever asked for the session is the one who asked
    /// for this too.
    Abandoned,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::EngineMissing(files) => write!(
                f,
                "FFmpeg introuvable, l'image ne peut pas être décodée sans lui : il manque {}",
                files
                    .iter()
                    .map(|file| file.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Error::Service(reason) => f.write_str(reason),
            Error::Abandoned => f.write_str("ouverture abandonnée avant l'image"),
        }
    }
}

impl std::error::Error for Error {}

/// A way open towards the far computer, ready for its player.
pub struct Opened {
    /// Name of the local link the player connects to: through it, the
    /// far computer's engine.
    pub link: String,
    /// What the player is to ask for: what was wanted, at the size the
    /// far computer said it will be showing.
    pub settings: SessionSettings,
    /// The way itself, given back to the service when this is dropped.
    pub way: Driving,
}

/// What the player asks the far computer's engine for, out of a session's
/// settings.
///
/// `steady` is whether a still screen is sent again at the full rate: the
/// viewer's choice, since only the person looking can tell whether a
/// pointer moving over a still desktop feels smooth. The far computer
/// draws its pointer into the picture when this end draws none of its
/// own, which is a session whose mouse is relative, and always sends its
/// sound: a player with nowhere to play it lets it go.
pub fn player_wants(settings: &SessionSettings, steady: bool) -> PlayerWants {
    let at_most = |value: u32| u16::try_from(value).unwrap_or(u16::MAX);
    PlayerWants {
        width: at_most(settings.width),
        height: at_most(settings.height),
        fps: at_most(settings.fps),
        bitrate_kbps: settings.bitrate_kbps,
        codec: match settings.codec {
            Codec::Auto => CodecChoice::Auto,
            Codec::H264 => CodecChoice::H264,
            Codec::Hevc => CodecChoice::Hevc,
            Codec::Av1 => CodecChoice::Av1,
        },
        draw_pointer: !settings.absolute_mouse,
        audio: true,
        steady,
    }
}

/// Opens a way to a session, reporting what happens as it happens.
///
/// `still_wanted` is asked at every step of the opening that can take
/// seconds, and answered « no » it gives up where it stands and comes
/// back `Abandoned`: an opening is watched on a screen with a cross in
/// its corner, and a cross that does nothing for half a minute is a
/// cross nobody believes twice. A way opened by then is given back on
/// the way out.
pub fn open(
    wanted: &Wanted,
    told: &mut dyn FnMut(Step),
    still_wanted: &dyn Fn() -> bool,
) -> Result<Opened, Error> {
    opened_on(&paths::ffmpeg_dir(), CHANNEL, wanted, told, still_wanted)
}

/// The same, with FFmpeg looked for in `ffmpeg` and through the service
/// listening on `channel`: the product's own, or ones a test stands in
/// for.
fn opened_on(
    ffmpeg: &Path,
    channel: &str,
    wanted: &Wanted,
    told: &mut dyn FnMut(Step),
    still_wanted: &dyn Fn() -> bool,
) -> Result<Opened, Error> {
    // First, and before anything is asked of anybody: without FFmpeg the
    // player has nothing to decode with, and a far computer woken for a
    // picture that can never be shown is a far computer disturbed for
    // nothing.
    let missing = Ffmpeg::missing_from(ffmpeg);
    if !missing.is_empty() {
        return Err(Error::EngineMissing(missing));
    }

    let (mut driving, link) =
        Driving::towards(channel, wanted, still_wanted).map_err(|gone| gone.or(Error::Service))?;
    told(Step::Reached);

    // The screen first: it is the one the whole picture is made of, and a
    // far computer that refuses it still has a picture to give, of the
    // screen it is already on.
    if let Err(gone) = driving.film_this_far_screen(wanted.far_screen.clone(), still_wanted) {
        told(Step::FarScreenLeftAlone {
            refused: gone.refusal()?,
        });
    }

    // Asked before the picture is: a session that never shows one has
    // still said it, and the far computer gives its sound back when the
    // way closes either way.
    if let Err(gone) = driving.hush_the_far_speakers(wanted.hush_the_far_speakers, still_wanted) {
        told(Step::SpeakersLeftAlone {
            refused: gone.refusal()?,
        });
    }

    // And the virtual screen over there, asked for the size this session
    // is about to ask of the picture. Before the picture, because the
    // engine can only film a screen that is already there, and asked at
    // all because that screen sleeps between sessions: a machine nobody
    // is looking at has the screens its owner plugged in and no others.
    //
    // Asked with nothing wanted as well: that is how a session that
    // leaves the far computer as it is learns what it will be showing,
    // and how a screen an earlier session grew is put back to sleep.
    let mut settings = wanted.settings;
    let asked_for = wanted.wants_a_screen_over_there.then_some(WantedScreen {
        wide: settings.width,
        high: settings.height,
        scale: wanted.far_magnification,
    });
    match driving.far_screen(asked_for, still_wanted) {
        // What that computer says it will be showing wins over what this
        // end guessed. It is the only one that knows: a session asking it
        // to keep its own screen has no way to work that size out from
        // here, and a session that asked for a size is told the same one
        // back.
        Ok(Some((wide, high))) => {
            if (wide, high) != (settings.width, settings.height) {
                settings.width = wide;
                settings.height = high;
                told(Step::ScreenOverThere { wide, high });
            }
        }
        Ok(None) => {}
        Err(gone) => told(Step::ScreenLeftAlone {
            refused: gone.refusal()?,
        }),
    }

    if !zyr_sound::anything_to_play_through() {
        told(Step::NoSoundCardHere);
    }

    // The far computer has been asked everything it is asked before a
    // picture: what it answered took seconds, and a person who closed the
    // window during them is not to be handed a session now.
    carry_on(still_wanted)?;
    Ok(Opened {
        link,
        settings,
        way: driving,
    })
}

/// The service, and the way it holds for a session.
///
/// The way goes back to the service when this is dropped.
///
/// [`Driving::hold`] is called, and this dropped, on a plain thread and
/// never inside an async task: what they ask of the service is waited for
/// on a runtime of this guard's own, and tokio refuses to wait that way on
/// a thread already running one.
pub struct Driving {
    runtime: tokio::runtime::Runtime,
    service: Service,
    way: WayId,
}

impl Driving {
    /// Joins the service on `channel` and asks it for a way to that
    /// computer, handing back the name of the link its player connects
    /// to.
    fn towards(
        channel: &str,
        wanted: &Wanted,
        still_wanted: &dyn Fn() -> bool,
    ) -> Result<(Self, String), GaveUp> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| GaveUp::Said(e.to_string()))?;
        let mut service = runtime
            .block_on(Service::join_on(channel))
            .map_err(|e| GaveUp::Said(e.to_string()))?;
        // The window the transport keeps open follows the session that
        // was actually asked for, not a nominal one.
        let request = Request::Reach {
            host: wanted.host.clone(),
            peer: wanted.peer,
            media: MediaProfile {
                bits_per_second: u64::from(wanted.settings.bitrate_kbps) * 1000,
                frames_per_second: wanted.settings.fps,
            },
            only_here: wanted.only_here,
        };
        let reached = match answered(&runtime, &mut service, &request, still_wanted)? {
            Answer::Reached(reached) => reached,
            Answer::Refused(reason) => return Err(GaveUp::Said(reason)),
            other => return Err(GaveUp::Said(unexpected(other))),
        };
        Ok((
            Self {
                runtime,
                service,
                way: reached.way,
            },
            reached.link,
        ))
    }

    /// Ties the way to this process, once its player plays.
    ///
    /// Until then the way is an attempt under way, which the service
    /// keeps out of the sessions it lists. From then on it is a session:
    /// named to whoever asks, and closed by the service should this
    /// process go without a word.
    ///
    /// A refusal is answered in words to show, and is never fatal: the
    /// way still closes with its player's link. What it costs is a
    /// session the service does not list, which a window opened
    /// afterwards cannot find.
    pub fn hold(&mut self) -> Result<(), String> {
        let request = Request::Hold {
            way: self.way,
            process: std::process::id(),
        };
        match self.runtime.block_on(self.service.ask(&request)) {
            Ok(Answer::Done) => Ok(()),
            Ok(Answer::Refused(reason)) => Err(reason),
            Ok(other) => Err(unexpected(other)),
            Err(e) => Err(e.to_string()),
        }
    }

    /// Asks the far computer to wake its virtual screen for a picture
    /// like that one, or, with nothing asked for, to leave its own screen
    /// alone.
    ///
    /// Answers the size that computer will be showing, which is the one
    /// ask that comes back with something: a session told to leave that
    /// machine as it is cannot know what that is until it asks.
    fn far_screen(
        &mut self,
        wanted: Option<WantedScreen>,
        still_wanted: &dyn Fn() -> bool,
    ) -> Result<Option<(u32, u32)>, GaveUp> {
        let way = self.way;
        match self.ask(&Request::FarScreen { way, wanted }, still_wanted)? {
            Answer::Showing { size } => Ok(size),
            Answer::Refused(reason) => Err(GaveUp::Said(reason)),
            other => Err(GaveUp::Said(unexpected(other))),
        }
    }

    /// Asks the far computer to serve its picture from that screen.
    fn film_this_far_screen(
        &mut self,
        id: Option<String>,
        still_wanted: &dyn Fn() -> bool,
    ) -> Result<(), GaveUp> {
        let way = self.way;
        self.asked(&Request::FilmFarScreen { way, id }, still_wanted)
    }

    /// Asks the far computer to silence its speakers, or to let them
    /// play again.
    fn hush_the_far_speakers(
        &mut self,
        quiet: bool,
        still_wanted: &dyn Fn() -> bool,
    ) -> Result<(), GaveUp> {
        let way = self.way;
        self.asked(&Request::Hush { way, quiet }, still_wanted)
    }

    /// One ask of the service that is either done or refused, and nothing
    /// else.
    fn asked(&mut self, request: &Request, still_wanted: &dyn Fn() -> bool) -> Result<(), GaveUp> {
        match self.ask(request, still_wanted)? {
            Answer::Done => Ok(()),
            Answer::Refused(reason) => Err(GaveUp::Said(reason)),
            other => Err(GaveUp::Said(unexpected(other))),
        }
    }

    /// One ask of the service, waited for without losing sight of the
    /// person watching it happen.
    fn ask(
        &mut self,
        request: &Request,
        still_wanted: &dyn Fn() -> bool,
    ) -> Result<Answer, GaveUp> {
        answered(&self.runtime, &mut self.service, request, still_wanted)
    }

    /// Gives the way back at the end of the session. The service would
    /// close it on its own once its player's link closes; saying so frees
    /// it at once, and frees one no player ever came to.
    fn let_go(&mut self) {
        let request = Request::Release { way: self.way };
        let _ = self.runtime.block_on(self.service.ask(&request));
    }
}

/// The way goes back whatever happens to whoever asked for it.
///
/// A guard and not a line at the end of the road that works: every road
/// out of a session, the opening given up half way included, has to give
/// the way back, and a way nobody gives back is a way the service only
/// closes once its patience for a player runs out, with the window
/// showing « Sessions ouvertes: 1 » over no session at all until then.
///
/// Releasing a way twice is not an error, which is what makes this safe
/// beside anything else that might already have said it.
impl Drop for Driving {
    fn drop(&mut self) {
        self.let_go();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    use zyr_control::{Door, Reached};

    use super::*;

    fn wanted() -> Wanted {
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
    fn a_service(
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
                .expect("exécuteur du service d'essai");
            runtime.block_on(async move {
                let mut door = Door::open(&listening).expect("canal d'essai");
                opened.send(()).expect("canal d'essai annoncé");
                let mut heard = door.accept().await.expect("un appel");
                while let Ok(Some(line)) = heard.hear().await {
                    let request = Request::parse(&line).expect("une demande lisible");
                    let answer = answering(&request);
                    writing.lock().unwrap().push(request);
                    match answer {
                        Some(answer) => heard.say(&answer.to_string()).await.expect("répondu"),
                        // Held without an answer: a service still
                        // chasing the far computer.
                        None => std::future::pending::<()>().await,
                    }
                }
            });
        });
        when_open.recv().expect("canal d'essai ouvert");
        (channel, asked)
    }

    /// What an ordinary far computer answers.
    fn willing(request: &Request) -> Option<Answer> {
        Some(match request {
            Request::Reach { .. } => Answer::Reached(Reached {
                way: WayId(7),
                link: r"\\.\pipe\ZyrDesk-link-8fKq2Lr0aZ3x9Wm1".to_string(),
            }),
            Request::Hush { .. } => {
                Answer::Refused("personne n'est connecté sur cet ordinateur".to_string())
            }
            Request::FarScreen { .. } => Answer::Showing {
                size: Some((2560, 1440)),
            },
            _ => Answer::Done,
        })
    }

    /// Waits for the service to have been asked `count` things.
    fn until_asked(asked: &Mutex<Vec<Request>>, count: usize) -> Vec<Request> {
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
    struct FfmpegHere(PathBuf);

    impl FfmpegHere {
        fn new(what: &str) -> Self {
            let folder = std::env::temp_dir()
                .join(format!("zyr-session-ffmpeg-{}-{what}", std::process::id()));
            std::fs::create_dir_all(&folder).expect("dossier d'essai");
            for file in Ffmpeg::missing_from(&folder) {
                std::fs::write(file, b"").expect("fichier d'essai");
            }
            Self(folder)
        }
    }

    impl Drop for FfmpegHere {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_missing_ffmpeg_is_reported_before_anything_is_asked() {
        // Checked first, since everything after it asks the far computer
        // to change something: the service stands ready, and hears
        // nothing at all.
        let (channel, asked) = a_service("sans-ffmpeg", willing);
        let nowhere = std::env::temp_dir().join(format!(
            "zyr-session-ffmpeg-{}-nulle-part",
            std::process::id()
        ));
        let mut steps = Vec::new();
        let outcome = opened_on(
            &nowhere,
            &channel,
            &wanted(),
            &mut |step| steps.push(step),
            &|| true,
        );
        assert!(
            matches!(&outcome, Err(Error::EngineMissing(files))
                if !files.is_empty() && files.iter().all(|file| file.starts_with(&nowhere))),
            "{:?}",
            outcome.err()
        );
        assert!(steps.is_empty(), "{steps:?}");
        assert!(asked.lock().unwrap().is_empty(), "{asked:?}");
    }

    #[test]
    fn an_opening_tells_the_far_computer_what_it_wants_in_order() {
        let (channel, asked) = a_service("ordre", willing);
        let ffmpeg = FfmpegHere::new("ordre");
        let wanted = wanted();
        let mut steps = Vec::new();
        let opened = opened_on(
            &ffmpeg.0,
            &channel,
            &wanted,
            &mut |step| steps.push(step),
            &|| true,
        )
        .expect("une voie ouverte");

        assert_eq!(opened.link, r"\\.\pipe\ZyrDesk-link-8fKq2Lr0aZ3x9Wm1");
        // The size the far computer said wins over the one asked for, and
        // nothing else of what was wanted moves.
        assert_eq!(
            (opened.settings.width, opened.settings.height),
            (2560, 1440)
        );
        assert_eq!(opened.settings.fps, wanted.settings.fps);
        assert_eq!(opened.settings.bitrate_kbps, wanted.settings.bitrate_kbps);

        let way = WayId(7);
        assert_eq!(
            asked.lock().unwrap().clone(),
            vec![
                Request::Reach {
                    host: wanted.host.clone(),
                    peer: wanted.peer,
                    media: MediaProfile {
                        bits_per_second: 20_000_000,
                        frames_per_second: 60,
                    },
                    only_here: false,
                },
                Request::FilmFarScreen {
                    way,
                    id: wanted.far_screen.clone(),
                },
                Request::Hush { way, quiet: true },
                Request::FarScreen {
                    way,
                    wanted: Some(WantedScreen {
                        wide: 1920,
                        high: 1080,
                        scale: 150,
                    }),
                },
            ]
        );
        // The refusal is written down and stepped over, and the size is
        // said since it is not what was asked.
        assert_eq!(steps[0], Step::Reached);
        assert!(
            steps.contains(&Step::SpeakersLeftAlone {
                refused: "personne n'est connecté sur cet ordinateur".to_string()
            }),
            "{steps:?}"
        );
        assert!(
            steps.contains(&Step::ScreenOverThere {
                wide: 2560,
                high: 1440
            }),
            "{steps:?}"
        );

        // The way is tied to this process once its player plays, and
        // given back when it is let go of.
        let Opened { mut way, .. } = opened;
        way.hold().expect("tenue");
        drop(way);
        let asked = until_asked(&asked, 6);
        assert_eq!(
            asked[4..],
            [
                Request::Hold {
                    way: WayId(7),
                    process: std::process::id()
                },
                Request::Release { way: WayId(7) }
            ]
        );
    }

    #[test]
    fn a_session_that_leaves_the_far_screen_alone_asks_for_none_and_takes_its_size() {
        let (channel, asked) = a_service("sans-ecran", willing);
        let ffmpeg = FfmpegHere::new("sans-ecran");
        let wanted = Wanted {
            wants_a_screen_over_there: false,
            ..wanted()
        };
        let opened =
            opened_on(&ffmpeg.0, &channel, &wanted, &mut |_| {}, &|| true).expect("ouverte");
        assert_eq!(
            (opened.settings.width, opened.settings.height),
            (2560, 1440)
        );
        assert!(
            asked.lock().unwrap().contains(&Request::FarScreen {
                way: WayId(7),
                wanted: None
            }),
            "{asked:?}"
        );
    }

    #[test]
    fn a_refused_way_is_the_end_of_the_opening() {
        let (channel, _) = a_service("refus", |_| {
            Some(Answer::Refused(
                "192.168.1.20 n'a pas répondu en 15 secondes".to_string(),
            ))
        });
        let ffmpeg = FfmpegHere::new("refus");
        let mut steps = Vec::new();
        let outcome = opened_on(
            &ffmpeg.0,
            &channel,
            &wanted(),
            &mut |step| steps.push(step),
            &|| true,
        );
        assert!(
            matches!(&outcome, Err(Error::Service(reason)) if reason.contains("n'a pas répondu")),
            "{:?}",
            outcome.err()
        );
        assert!(steps.is_empty(), "{steps:?}");
    }

    #[test]
    fn an_opening_let_go_of_after_the_way_gives_the_way_back() {
        // Let go of once the far computer has been asked everything: the
        // way was open by then, and is released rather than left to the
        // service's patience.
        let (channel, asked) = a_service("lachee-apres", willing);
        let ffmpeg = FfmpegHere::new("lachee-apres");
        let reached = std::sync::atomic::AtomicBool::new(false);
        let outcome = opened_on(
            &ffmpeg.0,
            &channel,
            &wanted(),
            &mut |step| {
                if step == Step::Reached {
                    reached.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            },
            &|| !reached.load(std::sync::atomic::Ordering::Relaxed),
        );
        assert!(
            matches!(outcome, Err(Error::Abandoned)),
            "{:?}",
            outcome.err()
        );
        let asked = until_asked(&asked, 5);
        assert_eq!(asked.last(), Some(&Request::Release { way: WayId(7) }));
    }

    #[test]
    fn an_abandoned_opening_does_not_wait_for_the_service_to_answer() {
        // A service that takes the call and never answers: seen from
        // here, it is exactly a computer being chased for thirty
        // seconds, and that is where the whole time of an opening
        // went. Nothing in that wait looked at the cross.
        let (channel, _) = a_service("lachee", |_| None);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let mut service = runtime
            .block_on(Service::join_on(&channel))
            .expect("joindre le service d'essai");

        // Let go at the third glance, which requires there to have been
        // three: the question is asked during the wait, and not once
        // before and then once after, which is what already existed.
        let looks = std::sync::atomic::AtomicUsize::new(0);
        let began = Instant::now();
        let outcome = answered(&runtime, &mut service, &Request::Standing, &|| {
            looks.fetch_add(1, std::sync::atomic::Ordering::Relaxed) < 2
        });

        assert!(matches!(outcome, Err(GaveUp::Abandoned)));
        assert!(
            looks.load(std::sync::atomic::Ordering::Relaxed) >= 3,
            "{looks:?}"
        );
        // And handed back almost at once, which is the heart of the
        // matter: without that, this came back half a minute after the
        // click, when the service had finished chasing.
        assert!(
            began.elapsed() < Duration::from_secs(5),
            "{:?}",
            began.elapsed()
        );
    }

    #[test]
    fn giving_up_never_reads_as_a_refusal() {
        // Half of these asks are made so that they can fail: the far
        // computer refuses to go quiet, to change screen, and the
        // picture is worth having all the same. Giving up takes the same
        // way back and is not one of those; handed back as a refusal, it
        // was written "les enceintes restent allumées" and the opening
        // carried on towards a picture nobody was waiting for any more.
        assert!(matches!(GaveUp::Abandoned.refusal(), Err(Error::Abandoned)));
        assert!(matches!(
            GaveUp::Abandoned.or(Error::Service),
            Error::Abandoned
        ));

        // A real refusal, for its part, goes through whole: it is what
        // gets written in the journal and on the opening screen.
        assert_eq!(
            GaveUp::Said("son écran ne se laisse pas filmer".to_string())
                .refusal()
                .unwrap(),
            "son écran ne se laisse pas filmer"
        );
        assert!(matches!(
            GaveUp::Said("refusé".to_string()).or(Error::Service),
            Error::Service(reason) if reason == "refusé"
        ));
    }

    #[test]
    fn the_player_asks_for_what_the_session_is_set_to() {
        let settings = SessionSettings {
            width: 2560,
            height: 1440,
            fps: 120,
            bitrate_kbps: 50_000,
            codec: Codec::Hevc,
            absolute_mouse: true,
            ..SessionSettings::default()
        };
        assert_eq!(
            player_wants(&settings, true),
            PlayerWants {
                width: 2560,
                height: 1440,
                fps: 120,
                bitrate_kbps: 50_000,
                codec: CodecChoice::Hevc,
                draw_pointer: false,
                audio: true,
                steady: true,
            }
        );
        // A relative mouse draws no pointer here, so the far computer
        // draws its own into the picture.
        let game = SessionSettings {
            absolute_mouse: false,
            ..settings
        };
        assert!(player_wants(&game, false).draw_pointer);
        // And a size no picture could carry is held at the largest one
        // that travels, rather than wrapped round to a tiny one.
        let absurd = SessionSettings {
            width: 70_000,
            ..settings
        };
        assert_eq!(player_wants(&absurd, false).width, u16::MAX);
    }

    #[test]
    fn every_failure_says_something_a_person_can_act_on() {
        let messages = [
            Error::EngineMissing(vec![PathBuf::from("/nowhere/vendor/ffmpeg/avcodec-63.dll")])
                .to_string(),
            Error::Service("192.168.1.20 ne répond pas".to_string()).to_string(),
        ];
        for message in messages {
            assert!(!message.is_empty());
            assert!(!message.starts_with("Error"), "{message}");
        }
        assert!(
            Error::EngineMissing(vec![PathBuf::from("avcodec-63.dll")])
                .to_string()
                .contains("avcodec-63.dll")
        );
    }
}
