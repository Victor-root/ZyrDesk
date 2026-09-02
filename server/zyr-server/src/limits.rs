//! Slowing down whoever tries too often.
//!
//! A password can be guessed, and a challenge can be asked for in a
//! loop. Each address gets a bucket of attempts that refills over a
//! minute; an empty bucket is a refusal with a code, never a block for
//! good, because a block for good is a way of keeping somebody out of
//! their own home.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Attempts left for one address, and when they were last counted.
struct Bucket {
    left: f64,
    counted: Instant,
}

pub struct Limiter {
    /// Attempts a full bucket holds, refilled over `WINDOW`.
    per_window: f64,
    buckets: Mutex<HashMap<IpAddr, Bucket>>,
}

/// Over what time a bucket refills entirely.
const WINDOW: Duration = Duration::from_secs(60);

/// Past this many addresses, the ones that have not tried for a while
/// are forgotten, so a sweep of the Internet does not fill memory.
const MOST_REMEMBERED: usize = 10_000;

impl Limiter {
    pub fn new(per_minute: u32) -> Self {
        Self {
            per_window: f64::from(per_minute.max(1)),
            buckets: Mutex::new(HashMap::new()),
        }
    }

    /// Whether that address may try once more, now.
    pub fn allows(&self, who: IpAddr) -> bool {
        self.allows_at(who, Instant::now())
    }

    fn allows_at(&self, who: IpAddr, now: Instant) -> bool {
        let mut buckets = self.buckets.lock().expect("seaux");
        if buckets.len() >= MOST_REMEMBERED {
            buckets.retain(|_, bucket| now.duration_since(bucket.counted) < WINDOW);
        }
        let refill = self.per_window / WINDOW.as_secs_f64();
        let bucket = buckets.entry(who).or_insert(Bucket {
            left: self.per_window,
            counted: now,
        });
        let since = now.duration_since(bucket.counted).as_secs_f64();
        bucket.left = (bucket.left + since * refill).min(self.per_window);
        bucket.counted = now;
        if bucket.left >= 1.0 {
            bucket.left -= 1.0;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn somebody() -> IpAddr {
        "203.0.113.7".parse().unwrap()
    }

    #[test]
    fn the_bucket_empties_then_refills_with_time() {
        let limiter = Limiter::new(3);
        let start = Instant::now();
        assert!(limiter.allows_at(somebody(), start));
        assert!(limiter.allows_at(somebody(), start));
        assert!(limiter.allows_at(somebody(), start));
        assert!(!limiter.allows_at(somebody(), start));
        // Vingt secondes plus tard, un tiers du seau est revenu : un
        // essai, et pas deux.
        let later = start + Duration::from_secs(20);
        assert!(limiter.allows_at(somebody(), later));
        assert!(!limiter.allows_at(somebody(), later));
        // Et une minute pleine remplit tout, sans jamais déborder.
        let much_later = later + Duration::from_secs(600);
        for _ in 0..3 {
            assert!(limiter.allows_at(somebody(), much_later));
        }
        assert!(!limiter.allows_at(somebody(), much_later));
    }

    #[test]
    fn addresses_do_not_share_a_bucket() {
        let limiter = Limiter::new(1);
        let now = Instant::now();
        assert!(limiter.allows_at(somebody(), now));
        assert!(!limiter.allows_at(somebody(), now));
        assert!(limiter.allows_at("198.51.100.2".parse().unwrap(), now));
    }
}
