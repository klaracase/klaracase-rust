//! Retry policy, plus the `Sleeper` and `Clock` seams.
//!
//! The policy:
//!
//! - Retry at most `max_retries` times on 429 and 503, for every method. Both statuses guarantee that
//!   nothing ran and nothing was charged.
//! - Never retry 402, any other 4xx, 500 or 502.
//! - The wait is `retry_after_seconds` from the body, else the `Retry-After` header in whole seconds,
//!   else exponential backoff with full jitter: a random wait between 0 and `min(8, 0.5 * 2^n)`
//!   seconds, where `n` is 0 for the first retry.
//! - A server-stated wait above 30 seconds is not slept. The error is returned at once.
//! - A transport error (connection, DNS, TLS, timeout) is retried for GET only, with the same backoff.

use std::future::Future;
use std::hash::{BuildHasher, RandomState};
use std::pin::Pin;
use std::time::{Duration, Instant};

/// Longest server-stated wait the client sleeps. A longer wait is returned as an error.
pub(crate) const MAX_STATED_WAIT_SECONDS: u64 = 30;
/// Cap on one computed backoff, in seconds.
const BACKOFF_CAP_SECONDS: f64 = 8.0;
/// First backoff upper bound, in seconds.
const BACKOFF_BASE_SECONDS: f64 = 0.5;

/// A boxed future that sleeps. Returned by [`Sleeper::sleep`].
#[doc(hidden)]
pub type SleepFuture<'a> = Pin<Box<dyn Future<Output = ()> + Send + 'a>>;

/// Waits between attempts. Inject your own in tests to avoid real sleeping.
///
/// This is a test seam, not part of the everyday API, so it is hidden from the rendered docs.
///
/// ```
/// use std::time::Duration;
/// use klaracase::{SleepFuture, Sleeper};
///
/// struct NoSleep;
///
/// impl Sleeper for NoSleep {
///     fn sleep(&self, _duration: Duration) -> SleepFuture<'_> {
///         Box::pin(async {})
///     }
/// }
/// ```
#[doc(hidden)]
pub trait Sleeper: Send + Sync {
    /// Returns a future that completes after `duration`.
    fn sleep(&self, duration: Duration) -> SleepFuture<'_>;
}

/// A monotonic clock. The [`wait_until_ready`](crate::CitationCheckDocuments::wait_until_ready)
/// timeout reads it. Inject your own in tests. A test seam, hidden from the rendered docs.
#[doc(hidden)]
pub trait Clock: Send + Sync {
    /// The current instant.
    fn now(&self) -> Instant;
}

/// The default [`Clock`]: [`Instant::now`].
#[doc(hidden)]
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// The default [`Sleeper`], built on `tokio::time::sleep`. Needs the `tokio` feature.
#[cfg(feature = "tokio")]
#[cfg_attr(docsrs, doc(cfg(feature = "tokio")))]
#[derive(Debug, Clone, Copy, Default)]
pub struct TokioSleeper;

#[cfg(feature = "tokio")]
impl Sleeper for TokioSleeper {
    fn sleep(&self, duration: Duration) -> SleepFuture<'_> {
        Box::pin(tokio::time::sleep(duration))
    }
}

/// Upper bound of the backoff before retry number `retry_index` (0 for the first retry), in seconds.
pub(crate) fn backoff_upper_bound(retry_index: u32) -> f64 {
    let exponent = retry_index.min(16);
    (BACKOFF_BASE_SECONDS * f64::from(2_u32.pow(exponent))).min(BACKOFF_CAP_SECONDS)
}

/// A backoff with full jitter for retry number `retry_index`.
pub(crate) fn backoff(retry_index: u32) -> Duration {
    Duration::from_secs_f64(backoff_upper_bound(retry_index) * unit_random())
}

/// A random number in `[0, 1)`. `RandomState` is seeded by the operating system, which is enough for
/// jitter and keeps the crate free of a random-number dependency.
fn unit_random() -> f64 {
    let bits = RandomState::new().hash_one(0_u8) >> 11;
    bits as f64 / (1_u64 << 53) as f64
}

/// Reads a `Retry-After` header holding whole seconds. An HTTP date gives `None`.
pub(crate) fn parse_retry_after(value: &str) -> Option<u64> {
    value.trim().parse::<u64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upper_bounds_double_and_cap_at_eight_seconds() {
        let bounds: Vec<f64> = (0..7).map(backoff_upper_bound).collect();
        assert_eq!(bounds, vec![0.5, 1.0, 2.0, 4.0, 8.0, 8.0, 8.0]);
    }

    #[test]
    fn a_huge_retry_index_does_not_overflow() {
        assert!((backoff_upper_bound(u32::MAX) - 8.0).abs() < f64::EPSILON);
    }

    #[test]
    fn jitter_stays_inside_the_bound() {
        for index in 0..8 {
            for _ in 0..200 {
                assert!(backoff(index).as_secs_f64() <= backoff_upper_bound(index));
            }
        }
    }

    #[test]
    fn retry_after_accepts_whole_seconds_only() {
        assert_eq!(parse_retry_after(" 42 "), Some(42));
        assert_eq!(parse_retry_after("Wed, 21 Oct 2026 07:28:00 GMT"), None);
        assert_eq!(parse_retry_after("1.5"), None);
    }
}
