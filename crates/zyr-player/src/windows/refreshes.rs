//! The screen's refreshes, timed by a thread that waits for each one.
//!
//! A window that goes to the screen straight, without the compositor, is
//! told nothing by its swap chain: neither when the screen refreshes nor
//! what it showed. The screen's output still says when it refreshes: a
//! thread waiting on it wakes at each refresh, and the moment it wakes is
//! when the refresh was, give or take how long waking takes.
//!
//! The thread follows the window from screen to screen. A refresh lasts
//! the middle of the newest steps from one wake to the next, each over
//! the refreshes it spans: a wake late enough to miss a refresh moves
//! neither the length nor the count.
//!
//! The wait sometimes returns without waiting, again and again, which
//! made a refresh last no time at all and the pictures never come due:
//! the session froze. A wake sooner than any screen refreshes is not a
//! refresh; a run of them and the screen is let go of, its refreshes
//! untold, until it can be timed again.

use std::collections::VecDeque;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory2, DXGI_CREATE_FACTORY_FLAGS, IDXGIFactory2, IDXGIOutput,
};
use windows::Win32::Graphics::Gdi::{HMONITOR, MONITOR_DEFAULTTONEAREST, MonitorFromWindow};
use windows::core::w;
use zyr_media::trace::{Seconds, Spread, ms};
use zyr_proto::log::{Log, Seldom};

use super::{Multimedia, failure};
use crate::lock;

/// Steps from one wake to the next kept, and how many at least before
/// a refresh's length is told.
const STEPS_KEPT: usize = 32;
const STEPS_LEAST: usize = 8;

/// How long the thread waits before it looks for the screen again, when
/// none can be waited on.
const AGAIN_AFTER: Duration = Duration::from_millis(100);

/// No screen refreshes sooner than this after the last refresh: a wake
/// that soon is a wait that did not wait.
const SHORTEST: Duration = Duration::from_millis(1);

/// Wakes in a row that did not wait, after which the screen is taken for
/// one that cannot be timed.
const UNWAITED_AT_MOST: u32 = 8;

/// A refresh of the screen: its number since the thread started on this
/// screen, when it was, and how long one lasts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Refresh {
    pub count: u64,
    pub at: Instant,
    pub period: Duration,
}

/// The thread timing the refreshes of the screen a window is on.
pub(super) struct Refreshes {
    latest: Arc<Mutex<Option<Refresh>>>,
    stop: Arc<AtomicBool>,
}

impl Refreshes {
    /// Starts timing the refreshes of the screen the window `hwnd` is
    /// on: a number the thread only asks which screen it is on with.
    pub(super) fn follow(hwnd: isize, log: &Log) -> Self {
        let latest = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let (shared, stopped, own_log) = (latest.clone(), stop.clone(), log.clone());
        // Not joined: the thread is a refresh at most from seeing it is
        // to stop, and a screen gone dark could keep it waiting longer
        // than a closing window should.
        let started = std::thread::Builder::new()
            .name("zyr-player-refreshes".to_string())
            .spawn(move || run(hwnd, &shared, &stopped, &own_log));
        if let Err(e) = started {
            log.write(&format!(
                "the screen's refreshes cannot be timed, no thread for it: {e}"
            ));
        }
        Self { latest, stop }
    }

    /// The last refresh timed, once a refresh's length is known.
    pub(super) fn latest(&self) -> Option<Refresh> {
        *lock(&self.latest)
    }
}

impl Drop for Refreshes {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn run(window: isize, latest: &Mutex<Option<Refresh>>, stop: &AtomicBool, log: &Log) {
    // Woken at each refresh as soon as can be: how late it wakes is how
    // far off each refresh is dated.
    let _playback = Multimedia::join(w!("Playback"), log);
    let hwnd = HWND(window as *mut c_void);
    let mut watched: Option<(HMONITOR, IDXGIOutput)> = None;
    let mut clock = Clock::default();
    let mut refused = Seldom::default();
    let mut unwaited = Seldom::default();
    while !stop.load(Ordering::Relaxed) {
        // SAFETY: any window handle gets an answer, the nearest screen.
        let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
        if watched.as_ref().is_none_or(|(on, _)| *on != monitor) {
            clock = Clock::default();
            *lock(latest) = None;
            watched = output_of(monitor, log).map(|output| (monitor, output));
        }
        let Some((_, output)) = &watched else {
            std::thread::sleep(AGAIN_AFTER);
            continue;
        };
        // SAFETY: a wait on an output this thread holds.
        match unsafe { output.WaitForVBlank() } {
            Ok(()) => {
                let now = Instant::now();
                if let Some(refresh) = clock.woke(now) {
                    *lock(latest) = Some(refresh);
                }
                if let Some(line) = clock.second(now) {
                    log.debug(&line);
                }
                if clock.waits_for_nothing() {
                    unwaited.note(log, now, |times| {
                        format!(
                            "the screen's refreshes cannot be timed: WaitForVBlank returns \
                             without waiting ({times} since last said)"
                        )
                    });
                    watched = None;
                    *lock(latest) = None;
                    std::thread::sleep(AGAIN_AFTER);
                }
            }
            Err(e) => {
                refused.note(log, Instant::now(), |times| {
                    format!(
                        "the screen's refreshes cannot be timed: {} ({times} since last said)",
                        failure("WaitForVBlank", &e)
                    )
                });
                watched = None;
                *lock(latest) = None;
                std::thread::sleep(AGAIN_AFTER);
            }
        }
    }
}

/// The output of the graphics card that is `monitor`.
fn output_of(monitor: HMONITOR, log: &Log) -> Option<IDXGIOutput> {
    // SAFETY: a factory asked for with no flag.
    let factory: IDXGIFactory2 = match unsafe { CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0)) } {
        Ok(factory) => factory,
        Err(e) => {
            log.write(&failure("CreateDXGIFactory2, to time the screen", &e));
            return None;
        }
    };
    // SAFETY: enumerating the factory's cards and their outputs, each of
    // which ends with an error.
    let output = (0..)
        .map_while(|index| unsafe { factory.EnumAdapters1(index) }.ok())
        .flat_map(|adapter| {
            (0..).map_while(move |index| unsafe { adapter.EnumOutputs(index) }.ok())
        })
        // SAFETY: a getter on an output just obtained.
        .find(|output| unsafe { output.GetDesc() }.is_ok_and(|desc| desc.Monitor == monitor));
    match &output {
        Some(output) => {
            // SAFETY: a getter on the output just found.
            let name = unsafe { output.GetDesc() }.map_or_else(
                |_| "a screen".to_string(),
                |desc| {
                    let name = desc.DeviceName;
                    let end = name.iter().position(|c| *c == 0).unwrap_or(name.len());
                    String::from_utf16_lossy(&name[..end])
                },
            );
            log.debug(&format!("the refreshes of {name} are timed as they come"));
        }
        None => log
            .write("the screen the window is on is no card's output: its refreshes are not timed"),
    }
    output
}

/// Refreshes worked out from the moments the thread woke at them.
#[derive(Default)]
struct Clock {
    count: u64,
    last: Option<Instant>,
    /// The newest steps, each over the refreshes it spans.
    steps: VecDeque<Duration>,
    period: Option<Duration>,
    seconds: Seconds,
    /// How far each wake was from where the rhythm put its refresh.
    off: Spread,
    missed: u32,
    /// Wakes that came sooner than any refresh: in a row, and over the
    /// second.
    hurried: u32,
    unwaited: u32,
}

impl Clock {
    /// The thread woke at a refresh at `at`: that refresh, once a
    /// refresh's length is known.
    fn woke(&mut self, at: Instant) -> Option<Refresh> {
        self.seconds.started(at);
        if let Some(last) = self.last {
            let step = at.saturating_duration_since(last);
            if step < SHORTEST {
                self.hurried += 1;
                self.unwaited += 1;
                return None;
            }
            self.hurried = 0;
            self.last = Some(at);
            let spans = self.period.map_or(1, |period| {
                (step.as_secs_f64() / period.as_secs_f64()).round().max(1.0) as u32
            });
            if let Some(period) = self.period {
                self.off.add(step.abs_diff(period * spans));
            }
            self.missed += spans - 1;
            self.count += u64::from(spans);
            if self.steps.len() == STEPS_KEPT {
                self.steps.pop_front();
            }
            self.steps.push_back(step / spans);
            if self.steps.len() >= STEPS_LEAST {
                let mut sorted: Vec<Duration> = self.steps.iter().copied().collect();
                sorted.sort_unstable();
                self.period = Some(sorted[sorted.len() / 2]);
            }
        } else {
            self.last = Some(at);
        }
        Some(Refresh {
            count: self.count,
            at,
            period: self.period?,
        })
    }

    /// Whether the wait has stopped waiting: too many wakes in a row came
    /// sooner than any refresh.
    fn waits_for_nothing(&self) -> bool {
        self.hurried >= UNWAITED_AT_MOST
    }

    /// What the last second timed, once it is over.
    fn second(&mut self, now: Instant) -> Option<String> {
        if !self.seconds.over(now) {
            return None;
        }
        let line = format!(
            "refreshes: one every {:.3} ms, woken {} ms off the rhythm (median/95th/worst), \
             {} passed over unwoken, {} wakes that did not wait",
            self.period.map_or(0.0, ms),
            self.off,
            self.missed,
            self.unwaited
        );
        self.off.clear();
        self.missed = 0;
        self.unwaited = 0;
        Some(line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIXTY_HZ: Duration = Duration::from_nanos(16_666_667);

    #[test]
    fn a_refresh_is_told_once_enough_of_them_were_timed() {
        let mut clock = Clock::default();
        let start = Instant::now();
        let told: Vec<Option<Refresh>> = (0..10u32)
            .map(|n| clock.woke(start + SIXTY_HZ * n))
            .collect();
        assert!(told[..STEPS_LEAST].iter().all(Option::is_none));
        let refresh = told[STEPS_LEAST].unwrap();
        assert_eq!(refresh.count, STEPS_LEAST as u64);
        assert_eq!(refresh.period, SIXTY_HZ);
    }

    #[test]
    fn a_wake_that_missed_a_refresh_moves_neither_the_count_nor_the_length() {
        let mut clock = Clock::default();
        let start = Instant::now();
        for n in 0..20u32 {
            clock.woke(start + SIXTY_HZ * n);
        }
        let refresh = clock.woke(start + SIXTY_HZ * 21).unwrap();
        assert_eq!(refresh.count, 21);
        assert_eq!(refresh.period, SIXTY_HZ);
        assert_eq!(clock.missed, 1);
    }

    #[test]
    fn late_wakes_leave_the_length_where_most_of_them_put_it() {
        let mut clock = Clock::default();
        let start = Instant::now();
        let mut last = None;
        for n in 0..40u32 {
            let late = if n % 5 == 0 {
                Duration::from_micros(900)
            } else {
                Duration::ZERO
            };
            last = clock.woke(start + SIXTY_HZ * n + late);
        }
        assert_eq!(last.unwrap().period, SIXTY_HZ);
    }

    #[test]
    fn a_wait_that_does_not_wait_is_not_a_refresh() {
        let mut clock = Clock::default();
        let start = Instant::now();
        for n in 0..20u32 {
            clock.woke(start + SIXTY_HZ * n);
        }
        let last = start + SIXTY_HZ * 19;
        for n in 1..UNWAITED_AT_MOST {
            assert_eq!(clock.woke(last + Duration::from_micros(u64::from(n))), None);
            assert!(!clock.waits_for_nothing());
        }
        let refresh = clock.woke(start + SIXTY_HZ * 20).unwrap();
        assert_eq!(refresh.period, SIXTY_HZ);
        assert_eq!(refresh.count, 20);
        assert_eq!(clock.missed, 0);
    }

    #[test]
    fn a_run_of_waits_that_do_not_wait_lets_the_screen_go() {
        let mut clock = Clock::default();
        let start = Instant::now();
        for n in 0..20u32 {
            clock.woke(start + SIXTY_HZ * n);
        }
        let last = start + SIXTY_HZ * 19;
        for n in 1..=UNWAITED_AT_MOST {
            clock.woke(last + Duration::from_micros(u64::from(n)));
        }
        assert!(clock.waits_for_nothing());
    }
}
