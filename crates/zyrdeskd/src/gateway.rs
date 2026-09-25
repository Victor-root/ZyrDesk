//! The tunnel end the service holds.
//!
//! This is the one door open on this computer. Everything a session
//! needs goes through it, in a single encrypted connection: that is what
//! leaves a single rule to write in a firewall, and an engine that never
//! opens a socket at all.
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
//! Each session that comes through brings its own engine up. It is asked
//! for with the first word of the session, and started then in the
//! session that owns the screen, as this very program, on a local link
//! only the system may open; the tunnel then carries that link to the
//! far computer. When the session ends the link is let go of, the engine
//! lets go of whatever it held and goes, and what is left of it is taken
//! with the job it was started in.

// Outside Windows nothing calls this module: the service does not exist
// there. Its logic has nothing platform-specific about it and stays
// compiled and tested everywhere.
#![cfg_attr(not(windows), allow(dead_code))]

use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use tokio::runtime::Handle;
use tokio::sync::{mpsc, watch};
use tokio::task::{JoinHandle, JoinSet};
use zyr_control::link::{Access, Link, LinkListener};
use zyr_media::service::{ToEngine, ToService};
use zyr_proto::log::Log;
use zyr_proto::net::TUNNEL_PORT;
use zyr_proto::paths;
use zyr_proto::session::WantedScreen;
use zyr_proto::sifting::Sifting;
use zyr_transport::junction::{Aloud, Say};
use zyr_transport::{
    AllowedPeers, Bytes, Connection, EndpointError, Fingerprint, Identity, Junction, Knocking,
    Media, TunnelEndpoint, authorized, is_card,
};
use zyr_tunnel::{Answers, ServiceSide, Tunnel, aside, nudge, service_channel};

use crate::engine::{Engine, Film};
use crate::machine::{Door, Machine};
use crate::said::{self, Said};

/// What this module's lines are filed under.
const TAG: &str = "gateway";

/// How often a session in progress is looked over.
const SESSION_WATCH: Duration = Duration::from_secs(2);

/// Every network interface: the computer is reachable from wherever the
/// other one is.
const EVERY_INTERFACE: IpAddr = IpAddr::V4(Ipv4Addr::UNSPECIFIED);

/// How often the list of authorised devices is worked out again.
const AUTHORIZED_REFRESH: Duration = Duration::from_secs(5);

/// How long an engine just started is given to reach its link.
///
/// Starting a program in another session and having it open a pipe
/// takes a fraction of a second; past this, it is not coming, and the
/// far computer is told so rather than left waiting.
const ENGINE_PATIENCE: Duration = Duration::from_secs(10);

/// How often an engine being waited for on its link is looked at, in
/// case it has already gone.
///
/// An engine that falls over as it starts, on an FFmpeg it cannot load
/// for instance, never reaches its link: without looking, the far
/// computer waited the whole of the patience above for a refusal that
/// was known in the first second.
const ENGINE_LOOKED_AT: Duration = Duration::from_millis(50);

/// How long an engine whose link has closed is given to go by itself.
///
/// What it does in that time is let go of every key and button it was
/// holding down for the far computer, which is the one thing that must
/// not be skipped: taken before, it would leave a key held on this
/// computer. Past this, it is taken with the job it was started in.
const ENGINE_GOES: Duration = Duration::from_secs(2);

/// How long the door waits, when it closes, for every session to let its
/// engine go.
const CLOSING_PATIENCE: Duration = Duration::from_secs(3);

/// Starts the engine of one session, told the link it is to serve it on.
pub trait Launcher: Send + Sync {
    /// Called where blocking is allowed: starting a program in another
    /// session waits on Windows.
    fn launch(&self, link: &str) -> io::Result<Box<dyn Launched>>;
}

/// An engine started, for as long as it is held: letting go of it
/// without waiting takes it.
pub trait Launched: Send {
    /// The process, as the system numbers it: what the other end of the
    /// engine's link has to be.
    fn process(&self) -> u32;

    /// Whether it has gone already, asked without waiting.
    fn gone(&self) -> bool;

    /// Waits at most that long for it to go by itself, and says with
    /// which code. Nothing when it had to be taken.
    fn let_go(self: Box<Self>, within: Duration) -> io::Result<Option<u32>>;
}

/// Where no engine can be started, which is everywhere but Windows: the
/// engine films a Windows screen.
#[cfg(not(windows))]
pub struct NotHere;

#[cfg(not(windows))]
impl Launcher for NotHere {
    fn launch(&self, _link: &str) -> io::Result<Box<dyn Launched>> {
        Err(io::Error::other(
            "le moteur ne tourne que sous Windows, où il filme l'écran",
        ))
    }
}

/// One session coming through the door, as its own channel answers for
/// this computer.
struct Attending {
    /// The sessions coming through this door, so what one of them asks
    /// of this computer outlives the asking.
    sessions: Arc<Sessions>,
    /// This computer, for the asks that are about it rather than about
    /// the session: its journal, its screens.
    machine: Machine,
    /// This computer's fingerprint, which its journal opens on.
    fingerprint: Fingerprint,
    /// The engine serving this session, once there is one.
    engine: Arc<Engine>,
    log: Log,
}

impl Answers for Attending {
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
    /// counts. It is woken from here rather than from the session on
    /// screen, and this session's engine is told to film it: on a
    /// computer with no screen at all, because there is nothing else to
    /// film; and on one whose own screens draw nothing larger than
    /// themselves, whose desktop moves onto it for the length of the
    /// session.
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
        // Asked of no other session first. A session that asks for this
        // computer's own screen gets this computer's own screen, and what
        // that costs a second viewer is a picture changing size, against a
        // first viewer served the wrong screen altogether.
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
            // And the grown screen goes with it, in that order: a desk
            // that had been moved onto it leaves it standing there empty,
            // and a session served from it would be served a bare
            // wallpaper instead of the desktop it asked for.
            self.put_the_grown_screen_away();
        }
        match hold_the_desk_for(wanted) {
            // What says somebody's screens are not the way they left them
            // is the note the errand writes, and never the asking: a
            // session that wanted this computer's own screen leaves
            // nothing behind to put back, and claiming otherwise has the
            // watch announce a desk coming home that never left.
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
        // signed-in person may not.
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
                self.wake_the_one_it_grew(screen).map(|()| {
                    self.film_the_grown_screen();
                    (screen.wide, screen.high)
                })
            }
            Some(screen)
                if showing != Some((screen.wide, screen.high))
                    && crate::screen::the_main_screen_is_stuck() =>
            {
                self.grow_one_for_this_session(screen)
            }
            _ => None,
        };
        if grown.is_none()
            && let Err(e) = self.engine.no_longer_the_grown_screen()
        {
            self.log.write(&format!(
                "this session's engine could not be told to film this computer's own screen: {e}"
            ));
        }
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
    fn journal(&self, sift: &str) -> Result<String, String> {
        self.log.write(&format!(
            "a computer asked this one for its journal{}, and it was handed over",
            if sift.is_empty() {
                String::new()
            } else {
                format!(" sifted through « {sift} »")
            }
        ));
        Ok(self
            .machine
            .journal(self.fingerprint, &self.log, &Sifting::of(sift)))
    }

    /// Hands over what this computer has measured of its own access to
    /// the Internet, whole.
    ///
    /// Read from disk and not gathered like the journal above: it holds
    /// one measurement a second, kept apart for exactly that reason, and
    /// asking for it from here is what spares the walk to this machine
    /// that a remote desktop already exists to spare.
    fn reach_log(&self) -> Result<String, String> {
        self.log
            .write("a computer asked this one for what it reaches, and it was handed over");
        match std::fs::read_to_string(paths::reach_log()) {
            Ok(text) => Ok(text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Ok("(rien de mesuré pour l'instant)".to_string())
            }
            Err(e) => Err(format!("le relevé n'a pas pu être lu : {e}")),
        }
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

    /// Says what shape this computer's pointer has right now.
    ///
    /// Nothing is written down about it, here or anywhere: it is asked
    /// several times a second while a hand is moving over there, it is
    /// worth nothing a moment later, and a journal carrying it would
    /// carry nothing else.
    fn pointer(&self) -> Result<zyr_proto::session::Pointer, String> {
        Ok(crate::pointer::shape(&self.log))
    }

    /// Says which screens this computer is showing on.
    ///
    /// Read from what this session's engine said when it started and
    /// whenever its screens changed: it is the one that films them, and
    /// the names it gives them are the ones it will be asked for.
    ///
    /// A session served from the screen this computer grows for itself is
    /// offered none of this. There is no screen of its own to choose
    /// between, which is the whole reason it grows one.
    fn screens(&self) -> Result<String, String> {
        if self.engine.films_the_grown_screen() {
            self.log.write(
                "a session asked which screens this computer has: it is served from the screen it \
                 grew for itself, so there is none to choose between",
            );
            return Ok(String::new());
        }
        let screens = self.engine.screens();
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

    /// Serves this session's picture from that screen from now on.
    ///
    /// Asked of the engine that runs, which changes the screen it films
    /// where it stands: nobody restarts, nobody reconnects, and the
    /// picture is on the other screen within the second.
    ///
    /// A session served from the screen this computer grew has one screen
    /// to give and no choice to offer; asking for its main screen is
    /// asking for what it is already getting.
    fn film_this_screen(&self, id: Option<String>) -> Result<(), String> {
        if self.engine.films_the_grown_screen() {
            return Ok(());
        }
        let named = id
            .clone()
            .unwrap_or_else(|| "this computer's main screen".to_string());
        let wanted = match id {
            Some(id) => Film::This(id),
            None => Film::Main,
        };
        self.engine.film(wanted).map_err(|refused| {
            self.log.write(&format!(
                "a session asked to be served from {named}, and its engine could not be told: \
                 {refused}"
            ));
            refused
        })?;
        self.log.write(&format!(
            "a session asked to be served from {named}, and its engine changes screen where it \
             stands"
        ));
        Ok(())
    }

    /// Takes what was copied on the far computer, and hands back what was
    /// copied here.
    ///
    /// Both halves in one message, because a clipboard is one thing and
    /// the two computers are equals about it: whichever of them somebody
    /// copied on, the other has to end up holding it.
    ///
    /// Nothing is written down about which of the two it came from, and
    /// nothing needs to be: what arrives goes on this computer's
    /// clipboard, this computer's clipboard then reads back with the very
    /// stamp it came with, and the next turn has nothing to say. It is
    /// the same stamp that stops what somebody copied from bouncing
    /// between two machines for the length of a session.
    ///
    /// The clipboard is only ever touched while a session is open, and
    /// the helper that touches it stops a few seconds after the last
    /// question. A computer nobody is watching keeps its clipboard to
    /// itself.
    fn clipboard(
        &self,
        pushing: Option<zyr_proto::clipboard::Clip>,
        seen: Option<zyr_proto::clipboard::Stamp>,
    ) -> Result<Option<zyr_proto::clipboard::Clip>, String> {
        if let Some(coming) = &pushing {
            crate::clipboard::give_it(coming, &self.log)?;
        }
        let held = crate::clipboard::what_this_computer_has(&self.log);
        // Nothing when the far computer already holds what is here, which
        // is almost every turn, and nothing again when this computer's
        // clipboard is empty: an empty clipboard here must never empty
        // the one over there.
        Ok(held.filter(|clip| Some(clip.stamp()) != seen))
    }

    /// Hands over a piece of a file this computer's clipboard named, and
    /// takes a piece of one the far clipboard named.
    ///
    /// Both ways in one message, like the clipboard above. Which of the
    /// two halves is doing anything depends on which computer somebody
    /// copied on and which they are pasting on, and neither end decides
    /// that.
    ///
    /// What is wanted back is what a paste under way on this computer is
    /// still missing, and nothing at all when nobody here is pasting.
    /// That nothing is the whole of how the far end learns it may stop
    /// sending.
    fn pieces(
        &self,
        asking: Option<zyr_tunnel::aside::Wanted>,
        giving: Option<zyr_tunnel::aside::Given>,
    ) -> Result<
        (
            Option<zyr_tunnel::aside::Given>,
            Option<zyr_tunnel::aside::Wanted>,
        ),
        String,
    > {
        if let Some(piece) = giving
            && let Err(refused) = crate::transfer::take(&piece, &self.log)
        {
            // The paste stops rather than asking for that piece again:
            // what refuses is a disk, and a disk does not change its mind
            // between two turns. The transfer is dropped by `take`, so
            // the far end is told to stop sending on this very answer.
            self.log
                .write(&format!("files: the paste here stops: {refused}"));
        }
        // A piece that cannot be read costs that ask and not the session:
        // the file was moved, or the clipboard has gone on to something
        // else, and the far end asks again or gives up on its own.
        let given = match asking.map(crate::clipboard::a_piece_of) {
            Some(Ok(piece)) => Some(piece),
            Some(Err(refused)) => {
                self.log
                    .write(&format!("files: nothing was handed over: {refused}"));
                None
            }
            None => None,
        };
        Ok((given, crate::clipboard::what_a_paste_here_wants(&self.log)))
    }
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

    /// Moves this session onto the screen this computer grew, its own
    /// screens drawing nothing larger than themselves.
    ///
    /// Three steps, and each undone when the next will not go: the screen
    /// is woken at the size asked for, the desktop is moved onto it, and
    /// the engine is told to film it.
    fn grow_one_for_this_session(&self, screen: WantedScreen) -> Option<(u32, u32)> {
        self.log.write(
            "this computer's own screen will not draw the size this session asks for, so the one \
             it grew is woken, the desktop moves onto it and the engine films it",
        );
        self.wake_the_one_it_grew(screen)?;
        let Some(showing) = self.move_the_desktop_onto_it(screen) else {
            self.put_the_grown_screen_away();
            return None;
        };
        self.film_the_grown_screen();
        Some(showing)
    }

    /// Tells this session's engine to film the screen this computer grew,
    /// as soon as it can see it.
    ///
    /// Said and not undone when the engine cannot be told: the desktop is
    /// on that screen alone by now, so it is the main screen too, and an
    /// engine filming the main screen films it all the same.
    fn film_the_grown_screen(&self) {
        if let Err(e) = self.engine.film(Film::Grown) {
            self.log.write(&format!(
                "this session's engine could not be told to film the screen this computer grew: \
                 {e}"
            ));
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

/// What every session coming through the door shares.
struct AtTheDoor {
    sessions: Arc<Sessions>,
    machine: Machine,
    fingerprint: Fingerprint,
    launcher: Arc<dyn Launcher>,
    log: Log,
}

impl AtTheDoor {
    /// What answers for this computer to one session, with that
    /// session's engine.
    fn attending(&self, engine: Arc<Engine>) -> Attending {
        Attending {
            sessions: self.sessions.clone(),
            machine: self.machine.clone(),
            fingerprint: self.fingerprint,
            engine,
            log: self.log.clone(),
        }
    }
}

/// The open door, and the sessions coming through it.
///
/// Closing it lets every session's engine go before anything is taken;
/// dropping it takes everything at once.
pub struct Gateway {
    runtime: Handle,
    tasks: Vec<JoinHandle<()>>,
    /// Takes the knocks, and holds the sessions they open.
    serving: Option<JoinHandle<()>>,
    /// Tells every session the door is closing.
    closing: watch::Sender<bool>,
    sessions: Arc<Sessions>,
    /// Where the junction this door stands on is held for the account,
    /// and taken back when the door closes.
    door: Door,
    log: Log,
}

/// The sessions this door has taken in, as the rest of the service needs
/// to know about them.
#[derive(Debug, Default)]
struct Sessions {
    open: AtomicUsize,
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
    /// by the watch that holds the door, on its own thread, which is
    /// where it is acted on.
    desk_held: AtomicBool,
}

/// One session, counted for as long as it lasts.
///
/// A guard and not two lines around the body: a session that ends by
/// anything other than a clean return would otherwise be counted as open
/// for as long as the door stands, and nothing would ever notice. It is
/// handed to the session's own body and named there, so that it lasts
/// exactly as long as the session and not a moment less.
struct Counted {
    sessions: Arc<Sessions>,
    media: Media,
    /// What this computer reaches outside itself, written down for as
    /// long as the session lasts and dropped with it.
    _outside: crate::outside::Watching,
}

impl Counted {
    fn one(sessions: &Arc<Sessions>, media: &Media, log: &Log) -> Self {
        sessions.open.fetch_add(1, Ordering::Relaxed);
        Self {
            sessions: sessions.clone(),
            media: media.clone(),
            _outside: crate::outside::watch(log),
        }
    }
}

impl Drop for Counted {
    fn drop(&mut self) {
        // What the last session asked of this computer goes with it. A
        // session that follows and asks nothing must not inherit the
        // silence of the one before, nor the window of a rate nobody is
        // asking for any more.
        if self.sessions.open.fetch_sub(1, Ordering::Relaxed) == 1 {
            self.sessions.hushing.store(false, Ordering::Relaxed);
            self.media.serving_nobody();
        }
    }
}

impl Drop for Gateway {
    fn drop(&mut self) {
        for task in self.tasks.iter().chain(&self.serving) {
            task.abort();
        }
        self.door.closed();
    }
}

impl Gateway {
    /// Opens the tunnel and serves whoever is authorised, starting each
    /// session's engine with `launcher`.
    pub fn open(
        runtime: &Handle,
        launcher: Arc<dyn Launcher>,
        machine: Machine,
        log: &Log,
    ) -> io::Result<Self> {
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
        // Named one by one. A refused session and an empty list look
        // too much alike for a count to be enough.
        for device in &starting {
            log.write(&format!("{device} may come in"));
        }

        // The door stands on a junction: what the server presents is
        // reached through a card, and everything else comes in as it
        // always did.
        let identity = Arc::new(identity);
        let say: Say = Arc::new({
            let log = log.clone();
            move |aloud, line: &str| match aloud {
                Aloud::Says => log.write(line),
                Aloud::Hunts => log.debug(|| line.to_string()),
            }
        });
        // On the product's own port unless asked otherwise: a port the
        // system picks is only reachable through a meeting the server
        // arranges, which names it, and that is what the switch is for.
        let wire = machine.remembered.wire();
        let port = if wire.fixed_port { TUNNEL_PORT } else { 0 };
        let junction = Junction::bind(
            SocketAddr::new(EVERY_INTERFACE, port),
            identity.clone(),
            say,
            wire.marking,
        )
        .map_err(io::Error::other)?;
        let endpoint =
            TunnelEndpoint::host_at(&identity, allowed.clone(), machine.door.media(), &junction)
                .map_err(io::Error::other)?;

        log.write(&format!(
            "tunnel open on {}, fingerprint of this computer {}",
            junction
                .local_address()
                .map_or_else(|_| format!("port {TUNNEL_PORT}"), |at| at.to_string()),
            identity.fingerprint()
        ));
        machine.door.opened(junction.clone());

        let sessions = Arc::new(Sessions::default());
        let door = machine.door.clone();
        let incoming = machine.incoming.clone();
        let at_the_door = Arc::new(AtTheDoor {
            sessions: sessions.clone(),
            machine: machine.clone(),
            fingerprint: identity.fingerprint(),
            launcher,
            log: log.about(TAG),
        });
        let (closing, closed) = watch::channel(false);
        Ok(Self {
            runtime: runtime.clone(),
            tasks: vec![runtime.spawn(keep_the_list_fresh(
                list,
                allowed,
                starting,
                machine,
                log.clone(),
            ))],
            serving: Some(runtime.spawn(serve(
                endpoint,
                junction,
                at_the_door,
                incoming,
                closed,
                log.clone(),
            ))),
            closing,
            sessions,
            door,
            log: log.about(TAG),
        })
    }

    /// Closes the door, letting every session's engine go first.
    ///
    /// Blocks the calling thread, which must not be one of the runtime's:
    /// it is the supervisor's, which has nothing else to do meanwhile.
    ///
    /// What has not gone within the patience is taken, and waited for
    /// until it is: left running, it would go on holding the port the
    /// next door opens on, and the engines of its sessions with it.
    ///
    /// The patience is counted inside the runtime, the only place its
    /// timers exist: one made on this thread brought the whole service
    /// down every time the door closed.
    pub fn close(mut self) {
        let _ = self.closing.send(true);
        let Some(mut serving) = self.serving.take() else {
            return;
        };
        self.runtime.block_on(async {
            if tokio::time::timeout(CLOSING_PATIENCE, &mut serving)
                .await
                .is_err()
            {
                self.log.write(&format!(
                    "sessions still letting their engine go after {} s were taken with the door",
                    CLOSING_PATIENCE.as_secs()
                ));
                serving.abort();
                let _ = serving.await;
            }
        });
    }

    /// Whether somebody is being served right this moment.
    pub fn a_session_is_open(&self) -> bool {
        self.sessions.open.load(Ordering::Relaxed) > 0
    }

    /// Whether a session in progress asked this computer to go quiet.
    pub fn silence_was_asked_for(&self) -> bool {
        self.sessions.hushing.load(Ordering::Relaxed)
    }

    /// Whether a session still has this computer's desk with nobody left
    /// watching it.
    ///
    /// Asked by the watch that holds the door, which is on a thread where
    /// rearranging a desktop is allowed to take its time. A session that
    /// ends properly says so itself and this never fires; this is for the
    /// sessions that do not, which is every one whose computer was closed,
    /// unplugged or crashed, and those are exactly the ones after which
    /// somebody's screens would stay the way a stranger left them.
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

/// Takes in the devices that connect, one session each, until the door
/// closes, and then waits for every session to have let its engine go.
///
/// Only the knock itself is waited for here: a connection is handed over
/// before its handshake finishes, on purpose, since waiting for one to
/// finish is waiting for it to fail as often as it is waiting for it to
/// succeed. Doing that in this loop would hold up every other computer
/// waiting to knock for as long as the slowest one takes to give up,
/// which is a denial of service anyone could trigger by simply being
/// slow.
async fn serve(
    endpoint: TunnelEndpoint,
    junction: Junction,
    door: Arc<AtTheDoor>,
    incoming: crate::incoming::Incoming,
    mut closing: watch::Receiver<bool>,
    log: Log,
) {
    // What each session is handed, apart from what this loop waits on.
    let told = closing.clone();
    let mut sessions = JoinSet::new();
    loop {
        tokio::select! {
            knock = endpoint.accept_knock(|_| true) => match knock {
                Ok(knocking) => {
                    sessions.spawn(take_the_knock(
                        knocking,
                        junction.clone(),
                        door.clone(),
                        incoming.clone(),
                        told.clone(),
                        log.clone(),
                    ));
                    while sessions.try_join_next().is_some() {}
                }
                // A refused device is not the end of the door: it must
                // not stop this computer from taking in the next one,
                // which is otherwise a denial of service anyone could
                // trigger.
                Err(EndpointError::Closed) => {
                    log.write("the tunnel is closed, no longer taking anyone in");
                    break;
                }
                // `accept_knock` only ever fails this way in practice;
                // kept for the variants the type allows but this call
                // cannot produce, read the same as a handshake failing.
                Err(e) => log.write(&format!("connection refused: {e}")),
            },
            _ = closing.wait_for(|closing| *closing) => break,
        }
    }
    while sessions.join_next().await.is_some() {}
}

/// Waits out one knock's handshake and, once it stands, serves the
/// session it opens.
///
/// Spawned rather than awaited in [`serve`]'s own loop: a handshake that
/// goes quiet halfway through takes as long to give up as any connection
/// does, and that must not hold up the next computer's turn to knock.
async fn take_the_knock(
    knocking: Knocking,
    junction: Junction,
    door: Arc<AtTheDoor>,
    incoming: crate::incoming::Incoming,
    mut closing: watch::Receiver<bool>,
    log: Log,
) {
    let taken = tokio::select! {
        taken = knocking.taken() => taken,
        _ = closing.wait_for(|closing| *closing) => return,
    };
    let connection = match taken {
        Ok(connection) => connection,
        Err(e) => {
            log.write(&format!("connection refused: {e}"));
            return;
        }
    };
    let _counted = Counted::one(&door.sessions, &door.machine.door.media(), &log);
    // Absent only when the certificate presented could not be read back
    // into a fingerprint, which authorisation itself already requires:
    // this never actually misses, and is not worth refusing a session
    // over if it ever did.
    let _held = connection
        .peer_fingerprint()
        .map(|peer| incoming.arrived(peer, connection.remote_address(), connection.clone()));
    one_session(connection, junction, door, closing, log).await
}

/// One connection, from its first question to its end.
async fn one_session(
    connection: Connection,
    junction: Junction,
    door: Arc<AtTheDoor>,
    mut closing: watch::Receiver<bool>,
    log: Log,
) {
    let from = connection.remote_address();
    let engine = Arc::new(Engine::default());
    let answering: Arc<dyn Answers> = Arc::new(door.attending(engine.clone()));

    // Most connections ask a question or two and go; only the first word
    // of a session brings an engine up.
    let opening = tokio::select! {
        opening = aside::until_a_session_opens(&connection, answering.clone(), Some(&log)) => opening,
        _ = closing.wait_for(|closing| *closing) => return,
    };
    let opening = match opening {
        Ok(opening) => opening,
        Err(e) => {
            log.debug(|| format!("{from} went without opening a session: {e}"));
            return;
        }
    };
    let serving = opening.serving();
    door.machine.door.media().serving(serving);
    log.write(&format!(
        "a session opening here asks to be served at {} kbps, {} images a second, and this \
         computer's tunnel is held open for that",
        serving.bits_per_second / 1_000,
        serving.frames_per_second
    ));

    let brought_up = tokio::select! {
        brought_up = bring_up_the_engine(&connection, &engine, &door, &log) => brought_up,
        _ = closing.wait_for(|closing| *closing) => return,
    };
    let running = match brought_up {
        Ok(running) => running,
        Err(refused) => {
            log.write(&format!(
                "the session from {from} was not opened: {refused}"
            ));
            let _ = opening.refused(&refused).await;
            return;
        }
    };
    if let Err(e) = opening.opened().await {
        log.write(&format!(
            "the session from {from} went before it was told it was open: {e}"
        ));
        // The link first, as at every end: it is what tells the engine to
        // go, and an engine never told is only ever taken.
        drop(running.link);
        let_the_engine_go(running.launched, &log).await;
        return;
    }

    // A computer the server presented speaks to its card: the road it
    // really takes is the junction's to say.
    let road = if is_card(from) {
        junction.road(from).map_or_else(
            || " through the junction".to_string(),
            |road| {
                format!(
                    " through {}, round trip {} ms",
                    road.through,
                    road.round_trip.as_millis()
                )
            },
        )
    } else {
        String::new()
    };
    let watched = connection.clone();
    let mut tunnel = Tunnel::host(
        connection,
        answering,
        running.link,
        running.service,
        Some(log.clone()),
    );
    log.write(&format!(
        "session open with {from}{road}, {} bytes of room in a packet",
        watched.carrying().usable_datagram
    ));

    // Heard for as long as the link stands, which is as long as the
    // tunnel: it ends by itself once the tunnel has let go of the link.
    tokio::spawn(listen_to_the_engine(
        running.from_engine,
        engine,
        door.machine.door.media(),
        log.clone(),
    ));
    let named = from.to_string();
    let outcome = tokio::select! {
        outcome = watch_over(&mut tunnel, &watched, &junction, &named, &log) => Some(outcome),
        _ = closing.wait_for(|closing| *closing) => None,
    };
    let carried = said::carried(&tunnel.reading(), &watched.carrying());
    match outcome {
        Some(Ok(())) => log.write(&format!("session ended, {carried}")),
        Some(Err(e)) => log.write(&format!("session ended: {e}, {carried}")),
        None => log.write(&format!("session closed with the door, {carried}")),
    }
    // The link first, which is what tells the engine its session is over,
    // and the engine after it.
    tunnel.close().await;
    let_the_engine_go(running.launched, &log).await;
    // The card is the account's to give back, and it does so when the
    // server says the session is over. A tunnel gives up half a minute
    // after the last packet, which is long after another session may
    // have taken the same card.
}

/// A session's engine, up and connected.
struct Running {
    link: Link,
    /// The tunnel's half of the service's channel to the engine.
    service: ServiceSide,
    /// What the engine says to the service.
    from_engine: mpsc::Receiver<Bytes>,
    launched: Box<dyn Launched>,
}

/// Starts this session's engine, waits for it on its link, and says its
/// first words to it: how large a datagram the tunnel takes, and which
/// screen to film.
///
/// Answers why it could not, in words the far computer shows.
async fn bring_up_the_engine(
    connection: &Connection,
    engine: &Arc<Engine>,
    door: &AtTheDoor,
    log: &Log,
) -> Result<Running, String> {
    // Taken from what the path can never stop carrying, less the byte
    // that names the channel of every datagram.
    let datagram_budget = connection
        .guaranteed_usable_datagram()
        .and_then(|usable| usable.checked_sub(1))
        .ok_or("le chemin n'annonce aucune taille de datagramme")?;
    let listener = LinkListener::create(Access::SystemOnly).map_err(|e| {
        format!(
            "la liaison du moteur n'a pas pu être créée : {}",
            with_its_code(&e)
        )
    })?;
    let name = listener.name().to_string();
    let launcher = door.launcher.clone();
    let launched = tokio::task::spawn_blocking(move || launcher.launch(&name))
        .await
        .map_err(|e| format!("le moteur n'a pas pu être lancé : {e}"))?
        .map_err(|e| format!("le moteur n'a pas pu être lancé : {}", with_its_code(&e)))?;
    let process = launched.process();
    log.write(&format!(
        "the engine of this session was started, process {process}, and is waited for on its link"
    ));

    let accepting = tokio::time::timeout(ENGINE_PATIENCE, listener.accept());
    tokio::pin!(accepting);
    let mut looking = tokio::time::interval(ENGINE_LOOKED_AT);
    let accepted = loop {
        tokio::select! {
            // An engine that reached its link and went in the same
            // instant came: why it went is on its link.
            biased;
            accepted = &mut accepting => break Some(accepted),
            _ = looking.tick() => if launched.gone() {
                break None;
            },
        }
    };
    let link = match accepted {
        Some(Ok(Ok(link))) => link,
        Some(Ok(Err(e))) => {
            let refused = format!(
                "le moteur n'a pas pu rejoindre sa liaison : {}",
                with_its_code(&e)
            );
            let_the_engine_go(launched, log).await;
            return Err(refused);
        }
        Some(Err(_)) => {
            let_the_engine_go(launched, log).await;
            return Err(format!(
                "le moteur n'a pas rejoint sa liaison en {} secondes",
                ENGINE_PATIENCE.as_secs()
            ));
        }
        None => {
            let_the_engine_go(launched, log).await;
            return Err("le moteur s'est arrêté avant de rejoindre sa liaison".to_string());
        }
    };
    // The name of a link is no secret to whoever may open one, the
    // system account: the one expected has to be the one that came.
    if link.peer_process() != Some(process) {
        log.write(&format!(
            "the engine's link was taken by process {} and not by the engine, process {process}: \
             dropped",
            link.peer_process()
                .map_or_else(|| "unknown".to_string(), |other| other.to_string())
        ));
        drop(link);
        let_the_engine_go(launched, log).await;
        return Err("la liaison du moteur a été prise par un autre programme".to_string());
    }

    let (service, spoken) = service_channel();
    engine.connected(spoken.to_link);
    let first_words = engine
        .tell(&ToEngine::Setup { datagram_budget })
        .and_then(|()| engine.film_now());
    if let Err(refused) = first_words {
        drop(link);
        let_the_engine_go(launched, log).await;
        return Err(refused);
    }
    log.write(&format!(
        "the engine of this session is on its link, told {datagram_budget} bytes a datagram"
    ));
    Ok(Running {
        link,
        service,
        from_engine: spoken.from_link,
        launched,
    })
}

/// Hears what the engine says, for as long as its link stands.
async fn listen_to_the_engine(
    mut from_engine: mpsc::Receiver<Bytes>,
    engine: Arc<Engine>,
    media: Media,
    log: Log,
) {
    while let Some(said) = from_engine.recv().await {
        let message = match ToService::decode(&said) {
            Ok(message) => message,
            Err(e) => {
                log.write(&format!(
                    "the engine said something this service cannot read ({} bytes): {e}",
                    said.len()
                ));
                continue;
            }
        };
        let said = engine.heard(message);
        if let Some(serving) = said.serving {
            media.serving(serving);
        }
        for line in said.lines {
            log.write(&line);
        }
    }
}

/// Lets a session's engine go, giving it the time to let go of what it
/// holds, and says how it went.
async fn let_the_engine_go(launched: Box<dyn Launched>, log: &Log) {
    let process = launched.process();
    let went = tokio::task::spawn_blocking(move || launched.let_go(ENGINE_GOES)).await;
    log.write(&match went {
        Ok(Ok(Some(code))) => format!("the engine, process {process}, went with code {code}"),
        Ok(Ok(None)) => format!(
            "the engine, process {process}, was still there after {} s and was taken",
            ENGINE_GOES.as_secs()
        ),
        Ok(Err(e)) => format!(
            "the engine, process {process}, could not be waited for, and was taken: {}",
            with_its_code(&e)
        ),
        Err(e) => format!("the engine, process {process}, was taken: {e}"),
    });
}

/// An error, with the system's own number for it when it gave one: the
/// number is what names the fault in Microsoft's documentation.
pub(crate) fn with_its_code(e: &io::Error) -> String {
    match e.raw_os_error() {
        Some(code) => format!("{e} (0x{:08X})", code as u32),
        None => e.to_string(),
    }
}

/// Waits for the session to end, saying what it throws away while it
/// lasts.
///
/// This computer is the one sending the picture, so it is the one whose
/// losses are seen at the other end, and until this existed its journal
/// said nothing of them: a session that froze left its whole
/// explanation on the machine that was only watching. What is worth
/// saying is decided in [`crate::said`], the same way as for the ways
/// this computer opens.
async fn watch_over(
    tunnel: &mut Tunnel,
    connection: &zyr_transport::Connection,
    junction: &Junction,
    named: &str,
    log: &Log,
) -> io::Result<()> {
    let mut said = Said::from(connection.round_trip());
    let mut watch = tokio::time::interval(SESSION_WATCH);
    watch.tick().await;
    loop {
        tokio::select! {
            // Both are cancel-safe: waiting on the pumps is waiting on a
            // set of tasks, and a tick that is not taken is simply the
            // next one.
            outcome = tunnel.wait() => return outcome,
            _ = watch.tick() => {
                let reading = tunnel.reading();
                let path = connection.carrying();
                for line in said.what_changed(named, &reading, &path) {
                    log.write(&line);
                }
                nudge_if_recovering(junction, connection, named, log);
            }
        }
    }
}

/// Nudges the connection awake the moment its road answers again after
/// going quiet: the road can recover long before the connection above it
/// would notice on its own, having no reason to expect anything back
/// before its own doubling retries say to.
fn nudge_if_recovering(
    junction: &Junction,
    connection: &zyr_transport::Connection,
    named: &str,
    log: &Log,
) {
    let from = connection.remote_address();
    if !is_card(from) || !junction.recovered(from) {
        return;
    }
    match nudge(connection) {
        Ok(()) => log.write(&format!(
            "{named}: the road came back after being quiet, nudging the connection so it does \
             not wait out its own retry timer"
        )),
        Err(e) => log.write(&format!(
            "{named}: the road came back after being quiet, but the connection could not be \
             nudged: {e}"
        )),
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
        let remembered = crate::preferences::Remembered::at(folder.join("preferences.conf"));
        let machine = Machine {
            hosting: crate::machine::Hosting::new(),
            ways: crate::ways::Ways::new(log.clone(), remembered.clone()),
            incoming: crate::incoming::Incoming::default(),
            remembered,
            neighbours: zyr_lan::Found::new(),
            account: crate::account::Account::at(folder.join("account.conf"), log),
            door: crate::machine::Door::default(),
        };
        (machine, folder)
    }

    #[test]
    fn a_neighbour_is_let_in_without_anyone_writing_it_down() {
        // That is the whole point of the local network: two ZyrDesks
        // switched on on the same network reach each other without
        // anything being copied out.
        let devices = joined(vec![fingerprint(1)], vec![fingerprint(2)]);
        assert_eq!(devices, vec![fingerprint(1), fingerprint(2)]);
    }

    #[test]
    fn a_device_both_written_down_and_seen_is_one_device() {
        // Otherwise the same fingerprint would go twice into the list
        // the transport looks up on every connection.
        let devices = joined(vec![fingerprint(1)], vec![fingerprint(1), fingerprint(2)]);
        assert_eq!(devices, vec![fingerprint(1), fingerprint(2)]);
    }

    #[test]
    fn only_what_changed_in_the_list_is_worth_a_line() {
        // The list is made again every five seconds: the journal must
        // only carry the moments when it moves, or there will be
        // nothing else left to read in it.
        let one = fingerprint(1);
        let two = fingerprint(2);
        assert!(apart(&[one, two], &[one, two]).is_empty());
        assert!(apart(&[], &[]).is_empty());

        let arriving = apart(&[one], &[one, two]);
        assert_eq!(arriving.len(), 1);
        assert!(arriving[0].starts_with(&two.to_string()), "{arriving:?}");
        assert!(arriving[0].contains("may now come in"), "{arriving:?}");

        let leaving = apart(&[one, two], &[one]);
        assert_eq!(leaving.len(), 1);
        assert!(leaving[0].contains("may no longer come in"), "{leaving:?}");
    }

    #[test]
    fn trust_turned_off_leaves_only_what_was_written_down() {
        let (machine, folder) = machine("sans-confiance");
        assert!(machine.remembered.trust_local_network());
        machine.remembered.set_trust_local_network(false).unwrap();

        // Nothing the network announces may come in any more: that is
        // the only effect expected of this switch.
        let devices = let_in(vec![fingerprint(1)], &machine);
        assert_eq!(devices, vec![fingerprint(1)]);

        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_computer_presented_by_a_ticket_is_let_in_whatever_the_network_says() {
        // This is what lets a computer of the account in across the
        // Internet: nothing is written down, nothing announces itself,
        // the server has introduced it. And trust in the local network
        // changes nothing about that.
        let (machine, folder) = machine("ticket");
        machine.remembered.set_trust_local_network(false).unwrap();
        machine
            .account
            .admit(fingerprint(2), zyr_broker::now() + 60);

        let devices = let_in(vec![fingerprint(1)], &machine);
        assert_eq!(devices, vec![fingerprint(1), fingerprint(2)]);

        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_refusal_carries_the_systems_own_number_as_microsoft_writes_it() {
        let denied = io::Error::from_raw_os_error(5);
        assert_eq!(with_its_code(&denied), format!("{denied} (0x00000005)"));
        // An HRESULT reads the way the documentation prints it, sign bit
        // and all, rather than as a negative number nobody can look up.
        let hresult = io::Error::from_raw_os_error(0x8007_0005_u32 as i32);
        assert!(
            with_its_code(&hresult).ends_with("(0x80070005)"),
            "{}",
            with_its_code(&hresult)
        );
        // A refusal the system did not number carries no number.
        assert_eq!(with_its_code(&io::Error::other("refusé")), "refusé");
    }

    /// An engine standing in for the real one: it joins the link it is
    /// given from a thread of its own, says what a real one says when it
    /// starts, echoes the control stream, and writes down what it is told.
    #[derive(Default)]
    struct StandIn {
        told: Arc<std::sync::Mutex<Vec<ToEngine>>>,
        let_go: Arc<AtomicBool>,
    }

    /// The screens it says it can film: two of this computer's own, and
    /// the one it grows for itself.
    fn its_screens() -> Vec<zyr_media::service::Display> {
        let display = |id: &str, main: bool, name: &str| zyr_media::service::Display {
            id: id.to_string(),
            main,
            width: 1920,
            height: 1080,
            name: name.to_string(),
        };
        vec![
            display(r"MONITOR\GSM5B7F\0003", true, "ROG PG279Q"),
            display(r"\\.\DISPLAY2", false, "Dell U2412M"),
            display(r"MONITOR\MTT1337\0007", false, "VDD by MTT"),
        ]
    }

    struct StoodIn {
        thread: std::thread::JoinHandle<()>,
        let_go: Arc<AtomicBool>,
    }

    impl Launcher for StandIn {
        fn launch(&self, link: &str) -> io::Result<Box<dyn Launched>> {
            use zyr_control::link::Channel;

            let link = link.to_string();
            let told = self.told.clone();
            let thread = std::thread::spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                runtime.block_on(async {
                    let link = zyr_control::link::connect(&link).await.unwrap();
                    let (mut reads, mut writes) = link.split();
                    let ready = ToService::Ready {
                        encodable: zyr_media::codec::CodecSet::empty()
                            .with(zyr_media::codec::VideoCodec::Hevc),
                        encoders: "hevc_nvenc".to_string(),
                        displays: its_screens(),
                    };
                    let serving = ToService::Serving {
                        kbps: 42_000,
                        fps: 90,
                    };
                    for message in [ready, serving] {
                        writes
                            .send(Channel::Service, &message.encode())
                            .await
                            .unwrap();
                    }
                    while let Ok(Some((channel, payload))) = reads.next().await {
                        match channel {
                            Channel::Service => told
                                .lock()
                                .unwrap()
                                .push(ToEngine::decode(&payload).unwrap()),
                            Channel::Control => {
                                writes.send(Channel::Control, &payload).await.unwrap()
                            }
                            Channel::Video | Channel::Audio => {}
                        }
                    }
                });
            });
            Ok(Box::new(StoodIn {
                thread,
                let_go: self.let_go.clone(),
            }))
        }
    }

    impl Launched for StoodIn {
        fn process(&self) -> u32 {
            // Its thread is in this very process, which is what the link
            // says of the other end.
            std::process::id()
        }

        fn gone(&self) -> bool {
            self.thread.is_finished()
        }

        fn let_go(self: Box<Self>, within: Duration) -> io::Result<Option<u32>> {
            self.let_go.store(true, Ordering::Relaxed);
            let deadline = std::time::Instant::now() + within;
            while !self.gone() {
                if std::time::Instant::now() >= deadline {
                    return Ok(None);
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Ok(Some(0))
        }
    }

    /// An engine that goes before it ever reaches its link, as one that
    /// cannot load FFmpeg does.
    struct GoesAtOnce;

    struct WentAtOnce;

    impl Launcher for GoesAtOnce {
        fn launch(&self, _link: &str) -> io::Result<Box<dyn Launched>> {
            Ok(Box::new(WentAtOnce))
        }
    }

    impl Launched for WentAtOnce {
        fn process(&self) -> u32 {
            std::process::id()
        }

        fn gone(&self) -> bool {
            true
        }

        fn let_go(self: Box<Self>, _within: Duration) -> io::Result<Option<u32>> {
            Ok(Some(3))
        }
    }

    async fn soon(done: impl Fn() -> bool) {
        tokio::time::timeout(Duration::from_secs(10), async {
            while !done() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("never happened");
    }

    /// One session through this door, its engine started by `launcher`,
    /// on a real connection over loopback, and the far computer's end of
    /// that connection.
    struct Served {
        session: JoinHandle<()>,
        connection: Connection,
        machine: Machine,
        folder: PathBuf,
        /// What the connection stands on, and what keeps the door from
        /// closing under the session.
        _standing: (TunnelEndpoint, TunnelEndpoint, watch::Sender<bool>),
    }

    async fn a_session_served_by(launcher: Arc<dyn Launcher>, what: &str) -> Served {
        use zyr_transport::{Marking, MediaProfile};

        let (machine, folder) = machine(what);
        let log = Log::open(&folder.join("service.log")).unwrap();
        let this = Arc::new(Identity::generate().unwrap());
        let far = Identity::generate().unwrap();
        let junction = Junction::bind(
            "127.0.0.1:0".parse().unwrap(),
            this.clone(),
            Arc::new(|_, _: &str| {}),
            Marking::Ecn,
        )
        .unwrap();
        let endpoint =
            TunnelEndpoint::host_at(&this, far.fingerprint(), machine.door.media(), &junction)
                .unwrap();
        let towards = TunnelEndpoint::client(
            &far,
            this.fingerprint(),
            MediaProfile::default(),
            "127.0.0.1:0".parse().unwrap(),
        )
        .unwrap();
        let (taken, reached) = tokio::join!(
            endpoint.accept(),
            towards.connect(junction.local_address().unwrap())
        );

        let door = Arc::new(AtTheDoor {
            sessions: Arc::default(),
            machine: machine.clone(),
            fingerprint: this.fingerprint(),
            launcher,
            log: log.clone(),
        });
        let (closing, closed) = watch::channel(false);
        let session = tokio::spawn(one_session(taken.unwrap(), junction, door, closed, log));
        Served {
            session,
            connection: reached.unwrap(),
            machine,
            folder,
            _standing: (endpoint, towards, closing),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_session_brings_its_engine_up_and_lets_it_go_at_its_end() {
        use zyr_control::link::{Channel, connect};
        use zyr_transport::MediaProfile;

        let stand_in = Arc::new(StandIn::default());
        let Served {
            session,
            connection,
            machine,
            folder,
            _standing,
        } = a_session_served_by(stand_in.clone(), "session").await;

        // The first word of the session, answered once its engine is up.
        let asked = MediaProfile {
            bits_per_second: 30_000_000,
            frames_per_second: 60,
        };
        aside::ask_to_open(&connection, asked).await.unwrap();
        let told = stand_in.told.clone();
        soon(|| told.lock().unwrap().len() >= 2).await;
        // It heard first how large a datagram is, the path's floor less
        // the byte naming the channel, then to film the main screen.
        assert_eq!(
            told.lock().unwrap()[..2],
            [
                ToEngine::Setup {
                    datagram_budget: 1161
                },
                ToEngine::Film {
                    display: String::new()
                }
            ]
        );

        // The player, on the way's link: what it says reaches the engine
        // and comes back.
        let listener = LinkListener::create(Access::SystemAndInteractive).unwrap();
        let name = listener.name().to_string();
        let (side, _way) = service_channel();
        let tunnel = Tunnel::client(connection.clone(), listener, side, None);
        let (mut reads, mut writes) = connect(&name).await.unwrap().split();
        writes.send(Channel::Control, b"hello").await.unwrap();
        let (channel, echoed) = reads.next().await.unwrap().unwrap();
        assert_eq!((channel, &echoed[..]), (Channel::Control, &b"hello"[..]));

        // What the engine serves sizes this computer's tunnel.
        let media = machine.door.media();
        soon(|| media.now().bits_per_second == 42_000_000).await;
        assert_eq!(media.now().frames_per_second, 90);

        // The screens are the engine's, the grown one left out, and
        // another is filmed where the engine stands.
        let listed = aside::ask_what_screens_it_has(&connection).await.unwrap();
        let screens = zyr_proto::session::far_screens_read(&listed);
        assert_eq!(screens.len(), 2, "{listed}");
        assert_eq!(screens[1].id, r"\\.\DISPLAY2");
        aside::ask_to_film_this_screen(&connection, Some(screens[1].id.clone()))
            .await
            .unwrap();
        soon(|| told.lock().unwrap().len() >= 3).await;
        assert_eq!(
            told.lock().unwrap()[2],
            ToEngine::Film {
                display: screens[1].id.clone()
            }
        );

        // The player leaves: the session ends, the engine's link closes,
        // and it is let go of.
        drop((reads, writes));
        tokio::time::timeout(Duration::from_secs(10), session)
            .await
            .expect("the session never ended")
            .unwrap();
        assert!(stand_in.let_go.load(Ordering::Relaxed));
        drop(tunnel);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_engine_that_goes_before_its_link_is_refused_at_once() {
        let served = a_session_served_by(Arc::new(GoesAtOnce), "gone").await;

        // Told why, and long before the patience a silent engine is given
        // has run out.
        let refusal = tokio::time::timeout(
            ENGINE_PATIENCE / 5,
            aside::ask_to_open(&served.connection, zyr_transport::MediaProfile::default()),
        )
        .await
        .expect("the refusal waited out the engine's patience")
        .unwrap_err()
        .to_string();
        assert!(refusal.contains("s'est arrêté"), "{refusal}");
        tokio::time::timeout(Duration::from_secs(10), served.session)
            .await
            .expect("the session never ended")
            .unwrap();
        let _ = std::fs::remove_dir_all(&served.folder);
    }

    /// A door standing on nothing but `serving`, the task that holds its
    /// sessions, told to close through `closing`.
    fn a_door_holding(
        runtime: &tokio::runtime::Runtime,
        serving: JoinHandle<()>,
        closing: watch::Sender<bool>,
        what: &str,
    ) -> (Gateway, PathBuf) {
        let (machine, folder) = machine(what);
        let gateway = Gateway {
            runtime: runtime.handle().clone(),
            tasks: Vec::new(),
            serving: Some(serving),
            closing,
            sessions: Arc::default(),
            door: machine.door.clone(),
            log: Log::open(&folder.join("service.log")).unwrap(),
        };
        (gateway, folder)
    }

    #[test]
    fn a_door_closes_as_soon_as_its_sessions_have_gone() {
        // Closed from a thread outside the runtime, as the supervisor
        // closes it.
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let (closing, mut closed) = watch::channel(false);
        let serving = runtime.spawn(async move {
            let _ = closed.wait_for(|closing| *closing).await;
        });
        let (gateway, folder) = a_door_holding(&runtime, serving, closing, "closed");

        let started = std::time::Instant::now();
        gateway.close();
        assert!(started.elapsed() < CLOSING_PATIENCE);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_door_that_closes_takes_what_would_not_go_in_time() {
        /// Says when it is dropped, which is when its task is.
        struct Dropped(Arc<AtomicBool>);

        impl Drop for Dropped {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Relaxed);
            }
        }

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let dropped = Arc::new(AtomicBool::new(false));
        let stuck = Dropped(dropped.clone());
        // A session that never lets its engine go, whatever it is told.
        let serving = runtime.spawn(async move {
            let _stuck = stuck;
            std::future::pending::<()>().await;
        });
        let (closing, _closed) = watch::channel(false);
        let (gateway, folder) = a_door_holding(&runtime, serving, closing, "closing");

        let started = std::time::Instant::now();
        gateway.close();
        assert!(started.elapsed() >= CLOSING_PATIENCE);
        // Taken with the door rather than left running behind it, holding
        // the port the next door opens on.
        assert!(dropped.load(Ordering::Relaxed));
        let _ = std::fs::remove_dir_all(&folder);
    }
}
