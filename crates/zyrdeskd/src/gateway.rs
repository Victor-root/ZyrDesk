//! The tunnel end the service holds.
//!
//! This is the one door open on this computer. Everything a session
//! needs goes through it: the engine's seven ports are multiplexed into
//! a single encrypted connection. That is what lets the engine close
//! back onto the local machine, where nothing on the network can reach
//! it, and what leaves a single rule to write in a firewall.
//!
//! Who may come in is decided by fingerprint. Three things put a
//! fingerprint on that list: it was written down, its owner announced
//! itself on this local network while this computer was trusting it, or
//! the server of the account presented it with a signed ticket. The list
//! is read again as the service runs, so one more computer appearing on
//! the network does not mean cutting the session in progress, and asking
//! a small file every few seconds costs nothing next to watching the
//! filesystem on every platform. A ticket wakes the reading at once: the
//! computer it presents knocks a moment later.
//!
//! The door also answers for the engine on ZyrDesk's own channel: the
//! far computer hands over the code its engine is waiting for, and it is
//! passed on here. That is the whole of what replaced a code shown on
//! one screen and typed on the other.

use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::runtime::Handle;
use tokio::task::{JoinHandle, JoinSet};
use zyr_engine_host::Credentials;
use zyr_engine_host::api::{Asked, EngineApi};
use zyr_engine_host::config::minimum_fps_target;
use zyr_proto::log::Log;
use zyr_proto::net::{EnginePorts, TUNNEL_PORT};
use zyr_proto::paths;
use zyr_proto::session::{Serving, WantedScreen};
use zyr_transport::{
    AllowedPeers, EndpointError, Fingerprint, Identity, MediaProfile, TunnelEndpoint, authorized,
};
use zyr_tunnel::{Answers, Tunnel};

use crate::machine::Machine;

/// Every network interface: the computer is reachable from wherever the
/// other one is.
const EVERY_INTERFACE: IpAddr = IpAddr::V4(Ipv4Addr::UNSPECIFIED);

/// Where the engine listens, and the only place the tunnel hands it
/// anything.
const ENGINE: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

/// How often the list of authorised devices is worked out again.
const AUTHORIZED_REFRESH: Duration = Duration::from_secs(5);

/// How long a pairing code is offered to the local engine.
///
/// The far computer starts its own engine and then hands the code over,
/// so the two arrive within a hair of each other and in no fixed order.
/// The engine refuses a code as long as nobody is asking it for one, so
/// it is offered again until somebody is.
const PAIRING_PATIENCE: Duration = Duration::from_secs(10);

/// Pause between two offers.
const PAIRING_RETRY: Duration = Duration::from_millis(200);

/// The screen the engine is filming right now, under its own name for
/// it.
///
/// Shared between the door, which can move it while the engine runs, and
/// the watch that holds the engine, which starts that engine over when
/// what it is filming is not what should be filmed. One answer in one
/// place: two would be an engine that starts over for ever, or one that
/// never does.
pub type Filming = Arc<Mutex<Option<String>>>;

/// How the engine is serving right now.
///
/// Shared for the same reason as the screen above, and between the same
/// two: the door moves the rate a still screen is served at while the
/// engine runs, and the watch that holds the engine starts it over when
/// how it serves is not how it should. The way the screen is captured is
/// in here too, and that one nothing moves in a running engine.
pub type ServingNow = Arc<Mutex<Serving>>;

/// The local engine, as the tunnel has to see it.
pub struct AtHand {
    pub ports: EnginePorts,
    pub credentials: Credentials,
    /// Whether it was started filming the screen this computer grows for
    /// itself rather than one of its own.
    ///
    /// Decided before the engine started, because that is the one moment
    /// it reads which screen to film, and carried here because a session
    /// only borrows that screen where the engine is already looking at
    /// it. Borrowing it otherwise would move somebody's desktop onto a
    /// screen nobody is filming.
    pub films_the_grown_screen: bool,
    /// Where the engine writes down the screens it can see.
    ///
    /// The one authority on what this computer's screens are called: the
    /// identifier is a digest the engine alone computes, and working it
    /// out again here would be a copy that is wrong on the first machine
    /// nobody tested.
    pub engine_log: PathBuf,
    /// Which screen the engine is filming, starting with the one it was
    /// aimed at.
    ///
    /// Carried rather than asked for again: what a session wants is
    /// written down the instant it asks, and comparing the ask against
    /// the note would answer « you have it » to a session whose engine is
    /// not there yet. Shared, because asking the engine to film another
    /// screen moves it without anything starting over.
    pub filming: Filming,
    /// How the engine is serving, starting with how it was started.
    ///
    /// Carried for the same reason as the screen above, and shared for
    /// the same reason: asking the engine to serve a still screen
    /// otherwise moves it without anything starting over.
    pub serving: ServingNow,
}

/// The local engine, and the one thing a far computer may ask of it.
struct Attending {
    ports: EnginePorts,
    api: Arc<EngineApi>,
    /// Whether the engine is filming the screen this computer grew.
    films_the_grown_screen: bool,
    /// Where the engine writes down the screens it can see, which is
    /// where the list this computer offers is read from.
    engine_log: PathBuf,
    /// Which screen the engine is filming.
    filming: Filming,
    /// How it is serving right now.
    serving: ServingNow,
    /// The sessions coming through this door, so what one of them asks
    /// of this computer outlives the asking.
    sessions: Arc<Sessions>,
    /// This computer, for the two asks that are about it rather than
    /// about a session: how its own engine serves, and its journal.
    machine: Machine,
    /// This computer's fingerprint, which its journal opens on.
    fingerprint: Fingerprint,
    log: Log,
}

impl Answers for Attending {
    fn engine(&self) -> EnginePorts {
        self.ports
    }

    /// Offers the far computer's code to the engine, and keeps offering
    /// it after answering.
    ///
    /// The engine only takes a code while a client is asking it for one,
    /// and reports success either way (`patches/MANIFEST.md`). The far
    /// engine was started before the code was sent, but started is not
    /// yet asking: a code offered in that gap is swallowed with a
    /// straight face, and stopping there left the real request, arriving
    /// a moment later, waiting for a code nobody would offer again. So
    /// the first successful offer answers the caller, and the offering
    /// goes on quietly for the rest of the patience: offering a code
    /// nobody is waiting for does nothing, which is exactly why it is
    /// safe to insist.
    fn hand_over_the_code(&self, pin: &str, name: &str) -> Result<(), String> {
        let deadline = Instant::now() + PAIRING_PATIENCE;
        loop {
            let refused = match self.api.submit_pin(pin, name) {
                Ok(()) => break,
                Err(e) => e.to_string(),
            };
            if Instant::now() >= deadline {
                self.log
                    .write(&format!("pairing refused to {name}: {refused}"));
                return Err(refused);
            }
            std::thread::sleep(PAIRING_RETRY);
        }
        self.log
            .write(&format!("pairing code offered to the engine for {name}"));

        let api = self.api.clone();
        let pin = pin.to_string();
        let name = name.to_string();
        std::thread::spawn(move || {
            while Instant::now() < deadline {
                std::thread::sleep(PAIRING_RETRY);
                let _ = api.submit_pin(&pin, &name);
            }
        });
        Ok(())
    }

    /// Presses Ctrl+Alt+Suppr on this computer, for the far one.
    ///
    /// It goes nowhere near the engine, and could not: the way an engine
    /// types is exactly the way Windows refuses for this combination.
    /// This is the service pressing it in its own process, which is the
    /// one thing on this machine Windows will take it from.
    fn secure_attention(&self) -> Result<(), String> {
        match press_it(&self.log) {
            Ok(()) => {
                self.log
                    .write("Ctrl+Alt+Suppr pressed for the far computer");
                Ok(())
            }
            Err(e) => {
                let refused = e.to_string();
                self.log
                    .write(&format!("Ctrl+Alt+Suppr not pressed: {refused}"));
                Err(refused)
            }
        }
    }

    /// Silences this computer's speakers for the length of the session,
    /// or lets them play again.
    ///
    /// Written down rather than acted on. Two reasons, and they are both
    /// about who is in charge of the sound: several sessions can be open
    /// at once and any one of them may have asked, and the sound has to
    /// come back when the last one goes, whatever became of the computer
    /// that asked. Both are answered by the watch that reads this every
    /// half second, and by nothing else.
    fn hush_the_speakers(&self, quiet: bool) -> Result<(), String> {
        self.sessions.hushing.store(quiet, Ordering::Relaxed);
        self.log.write(if quiet {
            "the far computer asked this one's speakers to be silent"
        } else {
            "the far computer asked this one's speakers to play again"
        });
        Ok(())
    }

    /// Puts this computer's lock screen up, for the far one.
    ///
    /// The mirror of Ctrl+Alt+Suppr just above, and the mirror in every
    /// sense: that one only a service may press, and this one only a
    /// program sitting on the interactive desktop may ask for. So it goes
    /// out to the session that owns the screen and comes back, where the
    /// other stays in this process.
    fn lock_the_screen(&self) -> Result<(), String> {
        // Said before the order goes out, so the journal carries the
        // moment it was asked as well as the moment it was done. What
        // happens between the two is the picture standing still, and
        // lining that stretch up against what the engine says about its
        // capture is the only way to tell which of the two is at fault.
        self.log
            .write("the far computer asked this one to lock itself");
        let asked_at = std::time::Instant::now();
        match lock_it() {
            Ok(took) => {
                self.log.write(&format!(
                    "this computer locked itself after {} ms ({took})",
                    asked_at.elapsed().as_millis()
                ));
                Ok(())
            }
            Err(e) => {
                let refused = e.to_string();
                self.log
                    .write(&format!("this computer not locked: {refused}"));
                Err(refused)
            }
        }
    }

    /// Puts this computer's desk where a session wants it, and answers
    /// what it ends up showing.
    ///
    /// A size named is the size its main screen takes, its whole desk
    /// having been written down first so it can be given back. No size
    /// named is a session asking for this computer's own desk, which is
    /// the one its owner left and not the one an earlier session left
    /// behind: that one is given back here, before the answer is worked
    /// out.
    ///
    /// The screen this computer grew for itself is the exception on both
    /// counts, and it is for the machine that has no screen at all: there
    /// is no desk to write down, nothing to give back, and it is woken
    /// from here rather than from the session on screen.
    ///
    /// A refusal is written down rather than swallowed, and the session
    /// goes on anyway at the other end: a computer that will not take the
    /// size serves the one it has and the picture is stretched over
    /// there, which is what every computer did before this existed.
    fn screen_for_a_session(
        &self,
        wanted: Option<WantedScreen>,
    ) -> Result<Option<(u32, u32)>, String> {
        // Said before the errand goes out, like the lock above. What
        // became of it is written from the session that owns the screen,
        // since that is the only place any of it can be known, and it
        // lands in this journal a moment before this call is back.
        self.log.write(&match wanted {
            Some(screen) => format!(
                "a session asks this computer's main screen for {screen}, and its desk is written \
                 down first"
            ),
            None => "a session asks this computer to keep its own screen".to_string(),
        });
        // Its own screen is the one its owner left, and that is the desk
        // written down before an earlier session moved it, never the one
        // that session left behind. Switching a session from the client's
        // resolution to the host's is exactly this case: the way carrying
        // the size closes and the next one opens in the same second, so
        // nothing in between ever put the desk back, and a 4K host went on
        // serving 1920x1200 of itself for the rest of the evening.
        //
        // Asked of no other session first, and that is a change. It used
        // to be done only while nothing else was open, so as never to pull
        // the desk from under somebody else; but a session closing and the
        // next one opening in the same second are counted together for as
        // long as the first takes to be shown out, and the switch lands in
        // exactly that second. It came out a coin toss: the same three
        // clicks worked one evening and did nothing the next. A session
        // that asks for this computer's own screen gets this computer's
        // own screen, and what that costs a second viewer is a picture
        // changing size, against a first viewer served the wrong screen
        // altogether. That the two can even overlap is [O1], still open.
        if wanted.is_none() && !crate::screen::noted_before().is_empty() {
            self.log.write(
                "this session wants this computer's own screen, so the desk an earlier one took is \
                 given back first",
            );
            match give_the_desk_back() {
                Ok(took) => self.log.write(&format!(
                    "the desk was put back from the session on screen ({took})"
                )),
                Err(e) => self
                    .log
                    .write(&format!("this computer's desk was left as it was: {e}")),
            }
            // And the grown screen goes with it, in that order. A desk
            // that had been moved onto it leaves it standing there empty,
            // and the engine on such a computer is aimed at it: left
            // awake, it would film an empty screen and this session would
            // be served a bare wallpaper instead of the desktop it asked
            // for. Asleep, the engine falls back to a real screen, which
            // is exactly what this session wants.
            self.put_the_grown_screen_away();
        }
        match hold_the_desk_for(wanted) {
            // What says somebody's screens are not the way they left them
            // is the note the errand writes, and never the asking: a
            // session that wanted this computer's own screen leaves
            // nothing behind to put back, and claiming otherwise has the
            // watch below announce a desk coming home that never left.
            Ok(took) => {
                self.sessions
                    .desk_held
                    .store(!crate::screen::noted_before().is_empty(), Ordering::Relaxed);
                self.log.write(&format!(
                    "the desk was set from the session on screen ({took})"
                ));
            }
            // Never fails a session. A computer that will not take the
            // size serves the one it has and the picture is stretched at
            // the other end, which is what every session did before any
            // of this existed.
            Err(e) => self
                .log
                .write(&format!("this computer's desk was left as it was: {e}")),
        }
        // The screen this computer grows for itself is woken from here
        // rather than from the session on screen: starting a display
        // device is administrator work, which a service has and a
        // signed-in person may not. The engine is already aimed at it,
        // that having been settled when it started, which is the one
        // moment it reads which screen to film.
        //
        // Two computers need it, and they need different things of it. One
        // has nothing plugged in at all, so the grown screen is the only
        // thing there is to film and Windows puts the desktop on it
        // unasked. The other has screens that draw nothing larger than
        // themselves, so it is woken at the size asked for and the desktop
        // is moved onto it, which is the errand below.
        let showing = crate::screen::showing_now();
        let grown = match wanted {
            Some(screen) if showing.is_none() => {
                self.log.write(
                    "no screen is plugged into this computer, so the one it grew for itself is \
                     woken for this session",
                );
                self.wake_the_one_it_grew(screen)
                    .map(|()| (screen.wide, screen.high))
            }
            Some(screen)
                if self.films_the_grown_screen && showing != Some((screen.wide, screen.high)) =>
            {
                self.log.write(
                    "this computer's own screens draw nothing larger than themselves, so the one \
                     it grew is woken at the size asked for and the desktop moves onto it",
                );
                self.wake_the_one_it_grew(screen)
                    .and_then(|()| self.move_the_desktop_onto_it(screen))
            }
            _ => None,
        };
        // What this computer ends up showing, read from what the session
        // on screen just wrote down rather than worked out here: what was
        // asked for and what Windows did are two different things, and a
        // service cannot see a screen to tell them apart. The grown
        // screen is the exception and has to be: it is not on any desk a
        // session could have looked at.
        let showing = grown.or_else(crate::screen::showing_now);
        self.log.write(&match showing {
            Some((wide, high)) => format!("this computer is showing {wide}x{high}"),
            None => "this computer could not say what it is showing, so the session keeps what it \
                     guessed"
                .to_string(),
        });
        Ok(showing)
    }

    /// Sets whether this computer resends a still screen at full rate,
    /// because a session asked.
    ///
    /// Written down first, because the note is what the next engine will
    /// read, and then asked of the engine that is running: it changes the
    /// floor it keeps up where it stands, which costs it a new encoder and
    /// costs the session watching it nothing at all. How the running
    /// engine serves is moved with it, so the watch that holds that engine
    /// sees nothing to start over for.
    ///
    /// One road still ends in a restart, and the answer says so: an engine
    /// that cannot be asked, which is one of an older build or one that
    /// has stopped answering. The note then differs from how the engine
    /// serves, the watch starts it over, and starting over takes this very
    /// tunnel with it, so the session that asked is told to wait and come
    /// back rather than left to find out from a way that broke under it.
    ///
    /// Doing nothing at all when it already serves that way, which is the
    /// ordinary case: every session asks, and almost none of them changes
    /// anything.
    fn serve_steady(&self, rate: bool) -> Result<zyr_tunnel::Settled, String> {
        let mut serving = self.machine.remembered.serving();
        if serving.steady_rate != rate {
            serving.steady_rate = rate;
            self.machine.remembered.set_serving(serving).map_err(|e| {
                let refused = e.to_string();
                self.log.write(&format!(
                    "the rate this computer serves at is unchanged: {refused}"
                ));
                refused
            })?;
        }
        // Weighed against how the engine serves **now**, and never against
        // the note that may have just been written: the note is what the
        // next engine will read, and answering from it would tell a session
        // it has what it asked for while the engine that is running still
        // serves the other way. Read and let go of before anything is
        // asked of the engine, like the screen below and for the same
        // reason.
        let served = *self.serving.lock().expect("façon de servir");
        if served.steady_rate == rate {
            return Ok(zyr_tunnel::Settled::Already);
        }
        let asked = Asked {
            minimum_fps_target: Some(minimum_fps_target(rate)),
            ..Asked::default()
        };
        match self.api.serve_as_asked(&asked) {
            Ok(()) => {
                self.serving.lock().expect("façon de servir").steady_rate = rate;
                self.log.write(&format!(
                    "a session asked this computer to {} resending a still screen, and its engine \
                     is changing floor where it stands",
                    if rate { "start" } else { "stop" }
                ));
                Ok(zyr_tunnel::Settled::Already)
            }
            Err(refused) => {
                self.log.write(&format!(
                    "a session asked this computer to {} resending a still screen, and its engine \
                     could not be asked to change floor ({refused}), so it starts over instead",
                    if rate { "start" } else { "stop" }
                ));
                Ok(zyr_tunnel::Settled::StartingOver)
            }
        }
    }

    /// Serves the session's picture at that rate from now on.
    ///
    /// Asked of the engine that runs, which costs it a new encoder and
    /// costs the session nothing: the rate was negotiated when the stream
    /// started, and that being the one road the engines had, a change of
    /// it used to be the picture stopped and started. Nothing is written
    /// down, because there is nothing to write: a rate is asked of the
    /// stream that runs, and the next one announces its own.
    ///
    /// A refusal is an engine that cannot be asked, and it is said rather
    /// than swallowed: the far end then opens its picture again, which is
    /// what every change of rate cost before this existed.
    fn serve_at(&self, kbps: u32) -> Result<(), String> {
        let asked = Asked {
            bitrate_kbps: Some(kbps),
            ..Asked::default()
        };
        self.api.serve_as_asked(&asked).map_err(|e| {
            let refused = e.to_string();
            self.log.write(&format!(
                "a session asked to be served at {kbps} kbps, and this computer's engine could \
                 not be asked ({refused})"
            ));
            refused
        })?;
        self.log.write(&format!(
            "a session asked to be served at {kbps} kbps, and this computer's engine is changing \
             rate where it stands"
        ));
        Ok(())
    }

    /// Hands this computer's journal over, whole.
    ///
    /// The same page the person sitting here would read, gathered the
    /// same way: a journal read from another computer that differed from
    /// the one read on the spot would be worth nothing to compare, and
    /// comparing the two is the whole reason for asking.
    ///
    /// Said in this computer's own journal as it goes out. Somebody
    /// reading a machine from elsewhere leaves a trace on it, like every
    /// other thing a far computer may ask for here.
    fn journal(&self) -> Result<String, String> {
        self.log
            .write("a computer asked this one for its journal, and it was handed over");
        Ok(self.machine.journal(self.fingerprint, &self.log))
    }

    /// Empties this computer's journal, because a far one asked.
    ///
    /// The line saying so is written after the emptying and not before,
    /// so the page opens on the moment it was cleared rather than on
    /// nothing at all. It is the same order the window uses on its own
    /// machine, and for the same reason.
    fn empty_the_journal(&self) -> Result<(), String> {
        let refused = zyr_proto::journal::emptied();
        self.log
            .write("a computer asked this one to empty its journal");
        if refused.is_empty() {
            return Ok(());
        }
        let reason = format!(
            "une partie du journal n'a pas pu être vidée : {}",
            refused.join(" ; ")
        );
        self.log.write(&reason);
        Err(reason)
    }

    /// Says which pictures this computer's engine can make.
    ///
    /// Read from what that engine wrote down when it started, and never
    /// worked out here: the engine tries every encoder the machine might
    /// have and writes down the ones that answered, so it is the one
    /// authority on what this graphics card can do. A copy of its
    /// reasoning would be wrong on the first machine nobody tested.
    ///
    /// Nothing read is « it has not said », which the far end shows as no
    /// opinion rather than as a machine that can encode nothing.
    fn codecs(&self) -> Result<String, String> {
        let named = what_this_engine_can_encode();
        self.log.write(&format!(
            "a session asked what this computer can encode: {}",
            if named.is_empty() {
                "its engine has not said".to_string()
            } else {
                named.clone()
            }
        ));
        Ok(named)
    }

    /// Says which screens this computer is showing on.
    ///
    /// Read from what the engine wrote down when it started, like the
    /// codecs just above and for the same reason: the identifier a screen
    /// is asked for by is a digest that engine alone computes, and a copy
    /// of that recipe that drifts by one byte names nothing at all.
    ///
    /// A computer filmed on the screen it grows for itself offers none of
    /// this. It has no screen of its own to choose between, which is the
    /// whole reason it grows one, and offering the grown one would offer
    /// the very screen the session is already being served from.
    fn screens(&self) -> Result<String, String> {
        if self.films_the_grown_screen {
            self.log.write(
                "a session asked which screens this computer has: it is filmed on the screen it \
                 grew for itself, so there is none to choose between",
            );
            return Ok(String::new());
        }
        let screens = crate::screen::on_this_computer(&self.engine_log);
        self.log.write(&format!(
            "a session asked which screens this computer has: {}",
            if screens.is_empty() {
                "its engine has not said".to_string()
            } else {
                screens
                    .iter()
                    .map(|screen| format!("{} ({})", screen.name, screen.id))
                    .collect::<Vec<_>>()
                    .join(" ; ")
            }
        ));
        Ok(zyr_proto::session::far_screens_written(&screens))
    }

    /// Serves this computer's picture from that screen from now on.
    ///
    /// Written down first, because the note is what the next engine will
    /// read, and then asked of the engine that is running: it changes the
    /// screen it films where it stands, which costs it the same
    /// reinitialization of its capture as a desktop switch, and costs the
    /// session watching it nothing at all. Nobody restarts, nobody
    /// reconnects, and the picture is on the other screen within the
    /// second.
    ///
    /// Two roads still end in a restart, and the answer says so: an engine
    /// that cannot be asked, which is one of an older build or one that
    /// has stopped answering, and a computer whose main screen has never
    /// been named, which is one that has never finished starting an
    /// engine. Starting over takes this very tunnel with it, so the
    /// session that asked is told to wait and come back rather than left
    /// to find out from a way that broke under it.
    ///
    /// Doing nothing at all when it is already the screen being filmed,
    /// which is the ordinary case: every session asks, and almost none of
    /// them changes anything.
    fn film_this_screen(&self, id: Option<String>) -> Result<zyr_tunnel::Settled, String> {
        // A computer filmed on the screen it grew has one screen to give
        // and no choice to offer; a session asking for its main screen is
        // asking for what it is already getting.
        if self.films_the_grown_screen {
            return Ok(zyr_tunnel::Settled::Already);
        }
        crate::screen::film_this_screen(id.as_deref()).map_err(|refused| {
            self.log.write(&format!(
                "the screen this computer is served from is unchanged: {refused}"
            ));
            refused
        })?;
        // Weighed against the screen the engine is filming **now**, and
        // never against the note that was just written: the note is what
        // the next engine will read, and answering from it would tell a
        // session it has what it asked for while the engine that is
        // running is still on the other screen.
        let should_be = crate::screen::the_screen_to_film();
        // Read and let go of before anything is asked of the engine: the
        // watch that holds that engine reads this too, and a lock held
        // across a question asked over a socket is that watch standing
        // still for as long as the answer takes.
        let filming = self.filming.lock().expect("écran filmé").clone();
        if should_be == filming {
            return Ok(zyr_tunnel::Settled::Already);
        }
        let named = id.as_deref().unwrap_or("this computer's main screen");
        if let Some(screen) = should_be.clone() {
            let asked = Asked {
                display: Some(screen),
                ..Asked::default()
            };
            match self.api.serve_as_asked(&asked) {
                Ok(()) => {
                    *self.filming.lock().expect("écran filmé") = should_be;
                    self.log.write(&format!(
                        "a session asked to be served from {named}, and this computer's engine is \
                         changing screen where it stands"
                    ));
                    return Ok(zyr_tunnel::Settled::Already);
                }
                Err(refused) => self.log.write(&format!(
                    "this computer's engine could not be asked to change screen ({refused}), so \
                     it starts over instead"
                )),
            }
        }
        self.log.write(&format!(
            "a session asked to be served from {named}, so this computer's engine starts over"
        ));
        Ok(zyr_tunnel::Settled::StartingOver)
    }
}

/// What the local engine wrote down about its own encoders, in the
/// product's own spelling.
fn what_this_engine_can_encode() -> String {
    zyr_engine_host::encoders::found_for(&paths::logs_dir())
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" ")
}

impl Attending {
    /// Wakes the screen this computer grew, at that size, saying what
    /// came of it.
    fn wake_the_one_it_grew(&self, screen: WantedScreen) -> Option<()> {
        match wake_the_grown_screen((screen.wide, screen.high)) {
            Ok(said) => {
                for line in said {
                    self.log.write(&line);
                }
                Some(())
            }
            Err(refused) => {
                self.log.write(&format!(
                    "the screen this computer grew stayed as it was: {refused}"
                ));
                None
            }
        }
    }

    /// Puts the screen this computer grew back to sleep, if it is awake.
    ///
    /// Asked when a session wants this computer's own screen back, after
    /// the desk itself has gone home: the two go in that order or Windows
    /// decides where the desktop lands and the arrangement put back a
    /// moment earlier is undone.
    fn put_the_grown_screen_away(&self) {
        if crate::screen::asleep() {
            return;
        }
        // Nobody is asked whether somebody wants it: the session asking
        // for this computer's own screen is the one that wants it gone,
        // and it is being answered right now.
        match sleep_the_grown_screen() {
            Ok(said) => {
                for line in said {
                    self.log.write(&line);
                }
            }
            Err(refused) => self.log.write(&format!(
                "the screen this computer grew would not go to sleep: {refused}"
            )),
        }
        // Asked of the device rather than taken on trust, because a
        // refusal has a consequence somebody will see: this computer is
        // filmed on that screen, and a screen left awake with nothing on
        // it is a session served a bare wallpaper. Said plainly here, so
        // the journal explains what the person is looking at.
        if !crate::screen::asleep() {
            self.log.write(
                "the screen this computer grew is still awake, so this session is served that \
                 screen rather than the desktop; it goes away when the last session does",
            );
        }
    }

    /// Moves this computer's desktop onto that screen, from the session
    /// that owns the screens, and answers what it ends up showing.
    ///
    /// Answered from what that session writes down rather than from what
    /// was asked for, like everything else about screens here: a desktop
    /// that did not move is a session served the wrong size, and the far
    /// end has to be told the size that really arrived.
    fn move_the_desktop_onto_it(&self, screen: WantedScreen) -> Option<(u32, u32)> {
        match take_the_grown_screen(screen) {
            Ok(took) => self.log.write(&format!(
                "the desktop was moved from the session on screen ({took})"
            )),
            Err(e) => {
                self.log.write(&format!(
                    "this computer's desktop was left where it is: {e}"
                ));
                return None;
            }
        }
        crate::screen::showing_now()
    }
}

/// Notes this computer's desk and puts its main screen where a session
/// wants it, saying what it cost.
///
/// From the session that owns the screen and never from here: everything
/// Windows says about the arrangement of screens is answered for the
/// window station of whoever asks, and the service's carries none.
#[cfg(windows)]
fn hold_the_desk_for(wanted: Option<WantedScreen>) -> io::Result<String> {
    crate::session::hold_the_desk_for(wanted).map(|took| took.to_string())
}

#[cfg(not(windows))]
fn hold_the_desk_for(_wanted: Option<WantedScreen>) -> io::Result<String> {
    Err(io::Error::other("cet ordinateur n'a pas d'écran à régler"))
}

/// Puts this computer's desk back where it was noted, saying what it cost.
///
/// From here as well as from the watch that holds the engine, because a
/// session asking for this computer's own screen cannot wait for that
/// watch: it is answered with the size this computer shows, and the
/// answer is what the far end opens its picture at.
#[cfg(windows)]
fn give_the_desk_back() -> io::Result<String> {
    crate::session::give_the_desk_back().map(|took| took.to_string())
}

#[cfg(not(windows))]
fn give_the_desk_back() -> io::Result<String> {
    Err(io::Error::other(
        "cet ordinateur n'a pas de bureau à rendre",
    ))
}

/// Moves this computer's desktop onto the screen it grew for itself,
/// saying what it cost.
///
/// From the session that owns the screens, and only once the service has
/// woken that screen: the two halves cannot be done from the same place.
#[cfg(windows)]
fn take_the_grown_screen(wanted: WantedScreen) -> io::Result<String> {
    crate::session::take_the_grown_screen(wanted).map(|took| took.to_string())
}

#[cfg(not(windows))]
fn take_the_grown_screen(_wanted: WantedScreen) -> io::Result<String> {
    Err(io::Error::other(
        "cet ordinateur n'a pas d'écran à faire pousser",
    ))
}

/// Puts that screen back to sleep, where there is one.
#[cfg(windows)]
fn sleep_the_grown_screen() -> Result<Vec<String>, String> {
    crate::screen::sleep_after_a_session(&|| true)
}

#[cfg(not(windows))]
fn sleep_the_grown_screen() -> Result<Vec<String>, String> {
    Err("cet ordinateur n'a pas d'écran virtuel".to_string())
}

/// Wakes the screen this computer grew for itself, for the one machine
/// that has nothing else to film.
#[cfg(windows)]
fn wake_the_grown_screen(size: (u32, u32)) -> Result<Vec<String>, String> {
    crate::screen::wake_for_a_session(size)
}

#[cfg(not(windows))]
fn wake_the_grown_screen(_size: (u32, u32)) -> Result<Vec<String>, String> {
    Err("cet ordinateur n'a pas d'écran virtuel".to_string())
}

/// Locks it, where there is a Windows to lock, saying what it cost.
#[cfg(windows)]
fn lock_it() -> io::Result<String> {
    crate::session::lock_the_screen().map(|took| took.to_string())
}

#[cfg(not(windows))]
fn lock_it() -> io::Result<String> {
    Err(io::Error::other(
        "cet ordinateur n'a pas d'écran de verrouillage à lever",
    ))
}

/// Presses it, where there is a Windows to press it on.
#[cfg(windows)]
fn press_it(log: &Log) -> io::Result<()> {
    crate::attention::press(log)
}

/// Outside Windows there is no such key and no service either. The
/// gateway stays compiled and tested everywhere, its logic having
/// nothing platform-specific about it.
#[cfg(not(windows))]
fn press_it(_log: &Log) -> io::Result<()> {
    Err(io::Error::other(
        "cet ordinateur n'a pas de Ctrl+Alt+Suppr à presser",
    ))
}

/// The open door, and the sessions coming through it.
///
/// Dropping it closes everything: the tunnel has no reason to outlive
/// the engine it serves.
#[derive(Debug)]
pub struct Gateway {
    tasks: Vec<JoinHandle<()>>,
    sessions: Arc<Sessions>,
}

/// The sessions this door has taken in, as the rest of the service needs
/// to know about them.
///
/// Two questions, and they are not the same one. What is open right now
/// says whether the engine may be disturbed at all. Whether anybody came
/// through at all says whether a screen the engine cannot put back is a
/// screen this run of it moved, or one it inherited already wrong from
/// the run before.
#[derive(Debug, Default)]
struct Sessions {
    open: AtomicUsize,
    ever: AtomicBool,
    /// Whether a session in progress asked this computer to go quiet.
    ///
    /// Not part of a session's own state on purpose: it is asked after
    /// the session stands, and what matters to the speakers is whether
    /// anybody at all is asking. It is cleared when the last session
    /// goes, so the next one starts from silence not being wanted.
    hushing: AtomicBool,
    /// Whether a session has this computer's desk, which is to say
    /// whether somebody's screens are not the way they left them.
    ///
    /// Here for the same reason as the hush, and put back the same way,
    /// but the putting back is not done where it is noticed: rearranging
    /// a desktop takes long enough that it has no business happening
    /// while a session is being torn down. What is written here is read
    /// by the watch that holds the engine, on its own thread, which is
    /// where it is acted on.
    desk_held: AtomicBool,
}

/// One session, counted for as long as it lasts.
///
/// A guard and not two lines around the body: a session that ends by
/// anything other than a clean return would otherwise be counted as open
/// for as long as the engine lives, and nothing would ever notice. It is
/// handed to the session's own body and named there, so that it lasts
/// exactly as long as the session and not a moment less.
struct Counted(Arc<Sessions>);

impl Counted {
    fn one(sessions: &Arc<Sessions>) -> Self {
        sessions.open.fetch_add(1, Ordering::Relaxed);
        sessions.ever.store(true, Ordering::Relaxed);
        Self(sessions.clone())
    }
}

impl Drop for Counted {
    fn drop(&mut self) {
        // What the last session asked of this computer's speakers goes
        // with it. A session that follows and asks nothing must not
        // inherit the silence of the one before.
        if self.0.open.fetch_sub(1, Ordering::Relaxed) == 1 {
            self.0.hushing.store(false, Ordering::Relaxed);
        }
    }
}

impl Drop for Gateway {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

impl Gateway {
    /// Opens the tunnel and serves whoever is authorised.
    pub fn open(runtime: &Handle, engine: AtHand, machine: Machine, log: &Log) -> io::Result<Self> {
        // The transport registers with the runtime as it is built, so it
        // has to be built from inside it.
        let _guard = runtime.enter();

        let identity =
            Identity::load_or_create(&paths::identity_dir()).map_err(io::Error::other)?;
        let list = paths::authorized_devices();
        let starting = let_in(authorized::read(&list)?, &machine);
        let allowed: AllowedPeers = starting.iter().copied().collect();
        if starting.is_empty() {
            log.write(
                "nobody can reach this computer yet: no device written down, \
                 and no other ZyrDesk seen on the local network",
            );
        }
        // Nommés un par un. Une session refusée et une liste vide se
        // ressemblent trop pour qu'on se contente d'un nombre.
        for device in &starting {
            log.write(&format!("{device} may come in"));
        }

        let endpoint = TunnelEndpoint::host(
            &identity,
            allowed.clone(),
            MediaProfile::default(),
            SocketAddr::new(EVERY_INTERFACE, TUNNEL_PORT),
        )
        .map_err(io::Error::other)?;

        log.write(&format!(
            "tunnel open on port {TUNNEL_PORT}, fingerprint of this computer {}",
            identity.fingerprint()
        ));

        let sessions = Arc::new(Sessions::default());
        let attending: Arc<dyn Answers> = Arc::new(Attending {
            ports: engine.ports,
            api: Arc::new(EngineApi::new(engine.ports, engine.credentials)),
            films_the_grown_screen: engine.films_the_grown_screen,
            engine_log: engine.engine_log,
            filming: engine.filming,
            serving: engine.serving,
            sessions: sessions.clone(),
            machine: machine.clone(),
            fingerprint: identity.fingerprint(),
            log: log.clone(),
        });
        Ok(Self {
            tasks: vec![
                runtime.spawn(keep_the_list_fresh(
                    list,
                    allowed,
                    starting,
                    machine,
                    log.clone(),
                )),
                runtime.spawn(serve(endpoint, attending, sessions.clone(), log.clone())),
            ],
            sessions,
        })
    }

    /// Whether somebody is being served right this moment.
    pub fn a_session_is_open(&self) -> bool {
        self.sessions.open.load(Ordering::Relaxed) > 0
    }

    /// Whether a session in progress asked this computer to go quiet.
    pub fn silence_was_asked_for(&self) -> bool {
        self.sessions.hushing.load(Ordering::Relaxed)
    }

    /// Whether a session has been served since this door was opened.
    ///
    /// Which is to say since the engine started, the two being opened and
    /// closed together. It is what tells a screen the engine could not put
    /// back after a session from a screen it inherited wrong from the run
    /// before: only the first is worth acting on, and confusing them is
    /// how a service ends up restarting its engine in a circle.
    pub fn anyone_came_through(&self) -> bool {
        self.sessions.ever.load(Ordering::Relaxed)
    }

    /// Whether a session still has this computer's desk with nobody left
    /// watching it.
    ///
    /// Asked by the watch that holds the engine, which is on a thread
    /// where rearranging a desktop is allowed to take its time. A session
    /// that ends properly says so itself and this never fires; this is
    /// for the sessions that do not, which is every one whose computer
    /// was closed, unplugged or crashed, and those are exactly the ones
    /// after which somebody's screens would stay the way a stranger left
    /// them.
    pub fn the_desk_is_held_for_nobody(&self) -> bool {
        self.sessions.desk_held.load(Ordering::Relaxed)
            && self.sessions.open.load(Ordering::Relaxed) == 0
    }

    /// Says the desk is back, so it is not asked for again on the next
    /// turn of that watch.
    pub fn the_desk_came_back(&self) {
        self.sessions.desk_held.store(false, Ordering::Relaxed);
    }
}

/// Takes in the devices that connect, one session each.
async fn serve(
    endpoint: TunnelEndpoint,
    attending: Arc<dyn Answers>,
    counting: Arc<Sessions>,
    log: Log,
) {
    let mut sessions = JoinSet::new();
    loop {
        match endpoint.accept().await {
            Ok(connection) => {
                let log = log.clone();
                let attending = attending.clone();
                let counted = Counted::one(&counting);
                sessions
                    .spawn(async move { one_session(connection, attending, counted, log).await });
                while sessions.try_join_next().is_some() {}
            }
            // A refused device is not the end of the door: it must not
            // stop this computer from taking in the next one, which is
            // otherwise a denial of service anyone could trigger.
            Err(EndpointError::Closed) => {
                log.write("the tunnel is closed, no longer taking anyone in");
                return;
            }
            Err(e) => log.write(&format!("connection refused: {e}")),
        }
    }
}

async fn one_session(
    connection: zyr_transport::Connection,
    attending: Arc<dyn Answers>,
    _counted: Counted,
    log: Log,
) {
    let from = connection.remote_address();
    let mut tunnel = match Tunnel::host(connection, ENGINE, attending).await {
        Ok(tunnel) => tunnel,
        Err(e) => {
            log.write(&format!("session from {from} not opened: {e}"));
            return;
        }
    };
    log.write(&format!("session open with {from}"));

    let outcome = tunnel.wait().await;
    let reading = tunnel.reading();
    match outcome {
        Ok(()) => log.write(&format!(
            "session ended, {} packets to the engine, {} to the tunnel",
            reading.to_engine, reading.to_tunnel
        )),
        Err(e) => log.write(&format!("session ended: {e}")),
    }
}

/// Works the list of authorised devices out again, so a computer that
/// has just appeared gets in without the service being restarted.
///
/// Every change is written down, and only the changes: the list is
/// worked out afresh every few seconds, and saying so each time would
/// bury everything else. What matters is the moment a computer starts or
/// stops being let in, which is exactly what a refused session needs
/// explaining.
async fn keep_the_list_fresh(
    list: PathBuf,
    allowed: AllowedPeers,
    starting: Vec<Fingerprint>,
    machine: Machine,
    log: Log,
) {
    let mut reported: Option<String> = None;
    let mut known = starting;
    loop {
        match authorized::read(&list) {
            Ok(written) => {
                if reported.take().is_some() {
                    log.write("authorised devices readable again");
                }
                let now = let_in(written, &machine);
                for said in apart(&known, &now) {
                    log.write(&said);
                }
                known = now.clone();
                allowed.replace_with(now);
            }
            // What was already allowed stays allowed: a file being
            // rewritten must not cut the session in progress.
            Err(e) => {
                let message = e.to_string();
                if reported.as_deref() != Some(message.as_str()) {
                    log.write(&format!("authorised devices unreadable: {message}"));
                    reported = Some(message);
                }
            }
        }
        tokio::select! {
            _ = tokio::time::sleep(AUTHORIZED_REFRESH) => {}
            _ = machine.account.admissions_changed() => {}
        }
    }
}

/// What changed between two states of the list, in words.
///
/// Nothing when nothing moved, which is the ordinary case a few times a
/// minute for as long as the service runs.
fn apart(before: &[Fingerprint], now: &[Fingerprint]) -> Vec<String> {
    let mut said = Vec::new();
    for device in now {
        if !before.contains(device) {
            said.push(format!("{device} may now come in"));
        }
    }
    for device in before {
        if !now.contains(device) {
            said.push(format!("{device} may no longer come in"));
        }
    }
    said
}

/// Everyone this computer lets in.
///
/// The devices written down, plus the ZyrDesk announcing themselves on
/// this local network when it is trusted, plus the computers the server
/// of the account presented with a ticket, for as long as the ticket
/// lives. The trust of the network spares anyone carrying a fingerprint
/// from one computer to the other, and it covers exactly what the
/// network already carries: a machine that can speak on it. Nothing
/// arriving from outside it is let in that way; across the Internet, the
/// account's ticket is what lets a computer in.
fn let_in(written: Vec<Fingerprint>, machine: &Machine) -> Vec<Fingerprint> {
    let seen = if machine.remembered.trust_local_network() {
        machine
            .neighbours
            .peers()
            .into_iter()
            .map(|peer| peer.fingerprint)
            .collect()
    } else {
        Vec::new()
    };
    joined(joined(written, seen), machine.account.admitted())
}

/// Two lists of fingerprints as one, without repeats.
///
/// The same computer is very often on both: written down once, and
/// announcing itself ever since.
fn joined(written: Vec<Fingerprint>, seen: Vec<Fingerprint>) -> Vec<Fingerprint> {
    let mut devices = written;
    for device in seen {
        if !devices.contains(&device) {
            devices.push(device);
        }
    }
    devices
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fingerprint(seed: u8) -> Fingerprint {
        format!("{seed:02x}").repeat(32).parse().unwrap()
    }

    fn machine(what: &str) -> (Machine, PathBuf) {
        let folder = std::env::temp_dir().join(format!(
            "zyrdeskd-gateway-{}-{what}",
            zyr_proto::random::alphanumeric_string(8)
        ));
        let log = Log::open(&folder.join("service.log")).expect("un journal");
        let machine = Machine {
            hosting: crate::machine::Hosting::new(),
            ways: crate::ways::Ways::new(log.clone()),
            remembered: crate::preferences::Remembered::at(folder.join("preferences.conf")),
            neighbours: zyr_lan::Found::new(),
            account: crate::account::Account::at(folder.join("account.conf"), log),
        };
        (machine, folder)
    }

    #[test]
    fn a_neighbour_is_let_in_without_anyone_writing_it_down() {
        // C'est tout l'intérêt du réseau local : deux ZyrDesk allumés
        // sur le même réseau se joignent sans rien recopier.
        let devices = joined(vec![fingerprint(1)], vec![fingerprint(2)]);
        assert_eq!(devices, vec![fingerprint(1), fingerprint(2)]);
    }

    #[test]
    fn a_device_both_written_down_and_seen_is_one_device() {
        // Sinon la même empreinte entrerait deux fois dans la liste que
        // le transport consulte à chaque connexion.
        let devices = joined(vec![fingerprint(1)], vec![fingerprint(1), fingerprint(2)]);
        assert_eq!(devices, vec![fingerprint(1), fingerprint(2)]);
    }

    #[test]
    fn only_what_changed_in_the_list_is_worth_a_line() {
        // La liste est refaite toutes les cinq secondes : le journal ne
        // doit porter que les moments où elle bouge, sinon il n'y aura
        // plus rien d'autre à y lire.
        let un = fingerprint(1);
        let deux = fingerprint(2);
        assert!(apart(&[un, deux], &[un, deux]).is_empty());
        assert!(apart(&[], &[]).is_empty());

        let arrive = apart(&[un], &[un, deux]);
        assert_eq!(arrive.len(), 1);
        assert!(arrive[0].starts_with(&deux.to_string()), "{arrive:?}");
        assert!(arrive[0].contains("may now come in"), "{arrive:?}");

        let part = apart(&[un, deux], &[un]);
        assert_eq!(part.len(), 1);
        assert!(part[0].contains("may no longer come in"), "{part:?}");
    }

    #[test]
    fn trust_turned_off_leaves_only_what_was_written_down() {
        let (machine, folder) = machine("sans-confiance");
        assert!(machine.remembered.trust_local_network());
        machine.remembered.set_trust_local_network(false).unwrap();

        // Rien de ce que le réseau annonce ne doit plus entrer : c'est
        // le seul effet attendu de cet interrupteur.
        let devices = let_in(vec![fingerprint(1)], &machine);
        assert_eq!(devices, vec![fingerprint(1)]);

        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_computer_presented_by_a_ticket_is_let_in_whatever_the_network_says() {
        // C'est ce qui fait entrer un ordinateur du compte à travers
        // Internet : rien n'est écrit, rien ne s'annonce, le serveur l'a
        // présenté. Et la confiance au réseau local n'y change rien.
        let (machine, folder) = machine("ticket");
        machine.remembered.set_trust_local_network(false).unwrap();
        machine
            .account
            .admit(fingerprint(2), zyr_broker::now() + 60);

        let devices = let_in(vec![fingerprint(1)], &machine);
        assert_eq!(devices, vec![fingerprint(1), fingerprint(2)]);

        let _ = std::fs::remove_dir_all(&folder);
    }
}
