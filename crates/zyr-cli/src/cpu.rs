//! Processor time this program has consumed.
//!
//! The bench has to say what the tunnel costs in computation. Reading
//! that figure by hand out of a task manager is tedious and unreliable:
//! it samples, it rounds, and it has to be read while the measurement
//! runs. The program can do it itself, over exactly the window it cares
//! about.

use std::time::{Duration, Instant};

/// Starting reading, to compare against later.
#[derive(Debug, Clone, Copy)]
pub struct Stopwatch {
    processor: Duration,
    clock: Instant,
}

impl Stopwatch {
    /// Takes the starting reading. Returns `None` when the platform
    /// cannot answer, in which case the bench says nothing rather than
    /// making something up.
    pub fn start() -> Option<Self> {
        Some(Self {
            processor: processor_time()?,
            clock: Instant::now(),
        })
    }

    /// Share of one core taken since the start, as a percentage.
    ///
    /// A hundred percent is one saturated core. Two full cores give two
    /// hundred.
    ///
    /// The count covers the whole program, threads included: two pieces
    /// of work carried out at once by the same program cannot be told
    /// apart. That is why the bench runs its bursts one after the other.
    pub fn load(&self) -> Option<f64> {
        let consumed = processor_time()?.checked_sub(self.processor)?;
        share_of_one_core(consumed, self.clock.elapsed())
    }
}

/// Share of one core that a computing time takes over a given span.
fn share_of_one_core(consumed: Duration, elapsed: Duration) -> Option<f64> {
    let elapsed = elapsed.as_secs_f64();
    if elapsed <= 0.0 {
        return None;
    }
    Some(consumed.as_secs_f64() * 100.0 / elapsed)
}

/// Number of cores, to put the reading in context.
pub fn cores() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

/// Cumulative processor time of the process, threads included.
#[cfg(windows)]
fn processor_time() -> Option<Duration> {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();

    // Safe: the four writes target local variables whose lifetime spans
    // the call, and the current process is always valid.
    let obtained = unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        )
    };
    if obtained == 0 {
        return None;
    }

    // The system counts in hundred-nanosecond slices, spread over two
    // thirty-two-bit halves.
    fn hundreds_of_nanoseconds(time: FILETIME) -> u64 {
        (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)
    }
    let slices = hundreds_of_nanoseconds(kernel) + hundreds_of_nanoseconds(user);
    Some(Duration::from_nanos(slices.saturating_mul(100)))
}

/// The same under Linux, for the development machine.
#[cfg(target_os = "linux")]
fn processor_time() -> Option<Duration> {
    // The kernel counts in clock ticks, whose unit as exposed to
    // programs is a hundred per second whatever the machine.
    const TICKS_PER_SECOND: u64 = 100;

    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    // The program name may contain spaces, but it sits in brackets: the
    // numeric fields start after the last one.
    let after_name = &stat[stat.rfind(')')? + 1..];
    let fields: Vec<&str> = after_name.split_whitespace().collect();
    // After the name comes the state, then fields 3 to 13; time spent in
    // user space is the twelfth, time in kernel space the thirteenth.
    let user: u64 = fields.get(11)?.parse().ok()?;
    let kernel: u64 = fields.get(12)?.parse().ok()?;
    Some(Duration::from_nanos(
        (user + kernel).saturating_mul(1_000_000_000 / TICKS_PER_SECOND),
    ))
}

#[cfg(not(any(windows, target_os = "linux")))]
fn processor_time() -> Option<Duration> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_processor_time_is_readable_and_never_goes_backwards() {
        let first = processor_time().expect("platform without processor time");
        let mut sum = 0u64;
        for i in 0..3_000_000u64 {
            sum = sum.wrapping_add(i * i);
        }
        assert_ne!(sum, u64::MAX);
        let second = processor_time().unwrap();
        assert!(second >= first, "{second:?} after {first:?}");
    }

    #[test]
    fn sustained_work_shows_in_the_load() {
        let stopwatch = Stopwatch::start().unwrap();
        let start = Instant::now();
        let mut sum = 0u64;
        // Long enough to clear the system counter's granularity, which
        // moves in slices of a few milliseconds.
        while start.elapsed() < Duration::from_millis(120) {
            for i in 0..100_000u64 {
                sum = sum.wrapping_add(i * i);
            }
        }
        assert_ne!(sum, u64::MAX);

        let load = stopwatch.load().unwrap();
        assert!(load > 50.0, "{load:.1}% for a loop taking a whole core");
    }

    #[test]
    fn the_share_of_one_core_reads_as_a_percentage() {
        // A second of computing in a second is one saturated core.
        let full = share_of_one_core(Duration::from_secs(1), Duration::from_secs(1)).unwrap();
        assert!((full - 100.0).abs() < 0.001, "{full}");

        // A quarter second in a second is a quarter of a core.
        let quarter =
            share_of_one_core(Duration::from_millis(250), Duration::from_secs(1)).unwrap();
        assert!((quarter - 25.0).abs() < 0.001, "{quarter}");

        // Two full cores for one second give two hundred.
        let double = share_of_one_core(Duration::from_secs(2), Duration::from_secs(1)).unwrap();
        assert!((double - 200.0).abs() < 0.001, "{double}");

        // With no time elapsed, there is nothing to report.
        assert!(share_of_one_core(Duration::from_secs(1), Duration::ZERO).is_none());
    }

    #[test]
    fn the_core_count_is_plausible() {
        assert!(cores() >= 1);
    }
}
