//! The viewer's keys and pointer, played on this computer in the order
//! they came.
//!
//! Everything the viewer holds down is remembered, so that it can be let
//! go of when the viewer can no longer do it: when the session ends,
//! when the link is lost, and when the player has said nothing for as
//! long as the whole product waits before calling a link dead. A key
//! left down would otherwise repeat on this computer with nobody there
//! to lift it.
//!
//! What is counted is how many of each kind came, never which: a key is
//! never written anywhere.

use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use zyr_media::input::{Held, InputEvent};
use zyr_proto::log::Log;

use crate::parts::{Injected, Injector, MakeInjector};
use crate::picture::Mapping;
use crate::session::{self, Event};
use crate::throttle::Throttle;

/// How often the counts are written.
const REPORT_EVERY: Duration = Duration::from_secs(10);

/// What the input thread is told.
pub(crate) enum Command {
    /// Something the viewer did.
    Event(InputEvent),
    /// The player said something else: it is still there.
    Heard,
    /// Where the picture lies on this computer's desktop now.
    Map(Mapping),
    /// Let go of everything held, for that reason, and stop.
    Quit(&'static str),
}

/// Starts the input thread, which lets go of what is held after
/// `silence` without a word from the player (the product's
/// [`zyr_proto::net::UNHEARD_LIMIT`]).
pub(crate) fn start(
    make: MakeInjector,
    silence: Duration,
    events: mpsc::Sender<Event>,
    log: Log,
) -> std::io::Result<(mpsc::Sender<Command>, JoinHandle<()>)> {
    let (commands, received) = mpsc::channel();
    let thread = session::spawn("input", events, move || {
        Player::new(make(), log).run(&received, silence);
    })?;
    Ok((commands, thread))
}

/// How many of each kind came, and what became of them.
#[derive(Debug, Default)]
struct Counts {
    keys: u64,
    pointer_at: u64,
    pointer_by: u64,
    buttons: u64,
    wheels: u64,
    release_alls: u64,
    /// Pointer positions that came before any picture: nowhere to put
    /// them.
    unplaced: u64,
    /// Held keys and buttons let go of by the engine itself.
    released: u64,
    failed: u64,
}

impl Counts {
    fn said(&self) -> String {
        format!(
            "input: {} keys, {} pointer positions, {} pointer moves, {} buttons, {} wheel turns, \
             {} release-alls; {} released by the engine, {} unplaced, {} failed",
            self.keys,
            self.pointer_at,
            self.pointer_by,
            self.buttons,
            self.wheels,
            self.release_alls,
            self.released,
            self.unplaced,
            self.failed
        )
    }
}

struct Player {
    injector: Box<dyn Injector>,
    held: Held,
    mapping: Option<Mapping>,
    counts: Counts,
    failures: Throttle,
    log: Log,
}

impl Player {
    fn new(injector: Box<dyn Injector>, log: Log) -> Self {
        Self {
            injector,
            held: Held::new(),
            mapping: None,
            counts: Counts::default(),
            failures: Throttle::new(REPORT_EVERY),
            log,
        }
    }

    fn run(mut self, commands: &mpsc::Receiver<Command>, silence: Duration) {
        let mut heard = Instant::now();
        let mut report = heard + REPORT_EVERY;
        loop {
            let silence_ends = heard + silence;
            let wake = report.min(silence_ends);
            match commands.recv_timeout(wake.saturating_duration_since(Instant::now())) {
                Ok(Command::Event(event)) => {
                    heard = Instant::now();
                    self.play(event);
                }
                Ok(Command::Heard) => heard = Instant::now(),
                Ok(Command::Map(mapping)) => self.mapping = Some(mapping),
                Ok(Command::Quit(why)) => {
                    self.let_go(why);
                    break;
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    self.let_go("the engine is gone");
                    break;
                }
            }
            let now = Instant::now();
            if now >= silence_ends {
                self.let_go("the player fell silent");
                // Once: nothing is held any more, until the viewer
                // presses something again.
                heard = now;
            }
            if now >= report {
                self.log.debug(|| self.counts.said());
                report = now + REPORT_EVERY;
            }
        }
        self.log.write(&self.counts.said());
    }

    fn play(&mut self, event: InputEvent) {
        let count = match event {
            InputEvent::Key { .. } => &mut self.counts.keys,
            InputEvent::PointerAt { .. } => &mut self.counts.pointer_at,
            InputEvent::PointerBy { .. } => &mut self.counts.pointer_by,
            InputEvent::Button { .. } => &mut self.counts.buttons,
            InputEvent::Wheel { .. } => &mut self.counts.wheels,
            InputEvent::ReleaseAll => &mut self.counts.release_alls,
        };
        *count += 1;
        if event == InputEvent::ReleaseAll {
            self.release();
            return;
        }
        let Some(injected) = self.placed(event) else {
            self.counts.unplaced += 1;
            return;
        };
        // A press that failed is remembered all the same: a release too
        // many costs nothing. A release that failed is not, so that the
        // key is let go of again later.
        let down = matches!(
            event,
            InputEvent::Key { down: true, .. } | InputEvent::Button { down: true, .. }
        );
        if self.inject(injected) || down {
            self.held.note(&event);
        }
    }

    /// The event as this computer plays it: nothing for a pointer
    /// position while no picture is placed, nor for a release-all, which
    /// is the releases it stands for.
    fn placed(&self, event: InputEvent) -> Option<Injected> {
        Some(match event {
            InputEvent::Key {
                scancode,
                extended,
                down,
            } => Injected::Key {
                scancode,
                extended,
                down,
            },
            InputEvent::PointerAt { x, y } => {
                let (x, y) = self.mapping.as_ref()?.absolute(x, y);
                Injected::PointerTo { x, y }
            }
            InputEvent::PointerBy { dx, dy } => Injected::PointerBy { dx, dy },
            InputEvent::Button { button, down } => Injected::Button { button, down },
            InputEvent::Wheel {
                vertical,
                horizontal,
            } => Injected::Wheel {
                vertical,
                horizontal,
            },
            InputEvent::ReleaseAll => return None,
        })
    }

    /// Plays one input, saying whether it was.
    fn inject(&mut self, what: Injected) -> bool {
        let Err(e) = self.injector.inject(what) else {
            return true;
        };
        self.counts.failed += 1;
        if let Some(unsaid) = self.failures.allow(Instant::now()) {
            self.log.write(&format!(
                "an input could not be played ({unsaid} more failures unsaid): {e}"
            ));
        }
        false
    }

    /// Plays the releases of everything held, saying how many.
    fn release(&mut self) -> usize {
        let ups = self.held.release_all();
        for up in ups
            .iter()
            .filter_map(|up| self.placed(*up))
            .collect::<Vec<_>>()
        {
            self.inject(up);
        }
        ups.len()
    }

    /// Lets go of everything held, for the engine's own reason.
    fn let_go(&mut self, why: &str) {
        let released = self.release();
        if released > 0 {
            self.counts.released += released as u64;
            self.log.write(&format!(
                "{released} keys and buttons still held let go of: {why}"
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    use crate::end_to_end::TestLog;
    use crate::fake::RecordingInjector;
    use crate::picture::{Rect, Size};
    use zyr_media::input::Button;
    use zyr_proto::net::UNHEARD_LIMIT;

    #[test]
    fn events_are_played_in_order_and_pointers_placed_on_the_desktop() {
        let journal = TestLog::new("input-in-order");
        let (injector, recorded) = RecordingInjector::new();
        let mut player = Player::new(Box::new(injector), journal.log.clone());
        player.play(InputEvent::PointerAt { x: 0, y: 0 });
        player.mapping = Some(Mapping {
            picture: Size::new(1000, 500),
            placement: Rect::new(0, 0, 1000, 500),
            screen: Rect::new(1000, 0, 1000, 500),
            desktop: Rect::new(0, 0, 2000, 500),
        });
        player.play(InputEvent::Key {
            scancode: 0x1e,
            extended: false,
            down: true,
        });
        player.play(InputEvent::PointerAt { x: 0, y: 0 });
        player.play(InputEvent::Wheel {
            vertical: -120,
            horizontal: 0,
        });
        let played = recorded.taken();
        assert_eq!(player.counts.unplaced, 1);
        assert_eq!(played.len(), 3);
        assert!(matches!(played[0], Injected::Key { down: true, .. }));
        // The screen's left edge is the middle of the desktop.
        let Injected::PointerTo { x, y } = played[1] else {
            panic!("{:?}", played[1]);
        };
        assert!((32768..32800).contains(&x), "{x}");
        assert!((0..100).contains(&y), "{y}");
        assert!(matches!(played[2], Injected::Wheel { vertical: -120, .. }));
    }

    #[test]
    fn release_all_lifts_exactly_what_is_held() {
        let journal = TestLog::new("input-release-all");
        let (injector, recorded) = RecordingInjector::new();
        let mut player = Player::new(Box::new(injector), journal.log.clone());
        for event in [
            InputEvent::Key {
                scancode: 0x1d,
                extended: true,
                down: true,
            },
            InputEvent::Key {
                scancode: 0x2a,
                extended: false,
                down: true,
            },
            InputEvent::Key {
                scancode: 0x2a,
                extended: false,
                down: false,
            },
            InputEvent::Button {
                button: Button::X1,
                down: true,
            },
            InputEvent::ReleaseAll,
        ] {
            player.play(event);
        }
        let played = recorded.taken();
        assert_eq!(
            played[4..],
            [
                Injected::Key {
                    scancode: 0x1d,
                    extended: true,
                    down: false
                },
                Injected::Button {
                    button: Button::X1,
                    down: false
                },
            ]
        );
        player.let_go("test");
        assert_eq!(recorded.taken().len(), 6, "nothing left to let go of");
    }

    fn left(down: bool) -> Injected {
        Injected::Button {
            button: Button::Left,
            down,
        }
    }

    fn press_left(commands: &mpsc::Sender<Command>) {
        commands
            .send(Command::Event(InputEvent::Button {
                button: Button::Left,
                down: true,
            }))
            .unwrap();
    }

    #[test]
    fn stopping_lets_go_of_what_is_held() {
        let journal = TestLog::new("input-stopping");
        let (injector, recorded) = RecordingInjector::new();
        let (events, _) = mpsc::channel();
        let (commands, thread) = start(
            Box::new(move || Box::new(injector)),
            UNHEARD_LIMIT,
            events,
            journal.log.clone(),
        )
        .unwrap();
        press_left(&commands);
        commands.send(Command::Quit("the test ends")).unwrap();
        thread.join().unwrap();
        assert_eq!(recorded.taken(), vec![left(true), left(false)]);
    }

    #[test]
    fn a_silent_player_has_what_it_holds_let_go_of_once() {
        let journal = TestLog::new("input-silence");
        let (injector, recorded) = RecordingInjector::new();
        let silence = Duration::from_millis(100);
        let (events, _) = mpsc::channel();
        let (commands, thread) = start(
            Box::new(move || Box::new(injector)),
            silence,
            events,
            journal.log.clone(),
        )
        .unwrap();
        press_left(&commands);
        // Pings keep the player alive: nothing is let go of.
        for _ in 0..4 {
            thread::sleep(silence / 2);
            commands.send(Command::Heard).unwrap();
        }
        assert_eq!(recorded.taken(), vec![left(true)]);
        thread::sleep(silence * 3);
        assert_eq!(recorded.taken(), vec![left(true), left(false)]);
        thread::sleep(silence * 2);
        commands.send(Command::Quit("the test ends")).unwrap();
        thread.join().unwrap();
        assert_eq!(recorded.taken(), vec![left(true), left(false)]);
    }
}
