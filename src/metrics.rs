use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// High-throughput atomic telemetry counters for rate limiter operations.
#[derive(Debug, Default)]
pub struct AtomicRateMetrics {
    allowed: AtomicU64,
    limited: AtomicU64,
}

impl AtomicRateMetrics {
    /// Creates a new metrics tracker with counters set to zero.
    pub const fn new() -> Self {
        Self {
            allowed: AtomicU64::new(0),
            limited: AtomicU64::new(0),
        }
    }

    /// Records an allowed request.
    #[inline]
    pub fn record_allowed(&self) {
        self.allowed.fetch_add(1, Ordering::Relaxed);
    }

    /// Records a throttled/limited request.
    #[inline]
    pub fn record_limited(&self) {
        self.limited.fetch_add(1, Ordering::Relaxed);
    }

    /// Takes a consistent snapshot of the current metrics.
    pub fn snapshot(&self) -> RateMetrics {
        RateMetrics {
            allowed: self.allowed.load(Ordering::Relaxed),
            limited: self.limited.load(Ordering::Relaxed),
        }
    }

    /// Resets all counters to zero.
    pub fn reset(&self) {
        self.allowed.store(0, Ordering::Relaxed);
        self.limited.store(0, Ordering::Relaxed);
    }
}

/// An immutable, point-in-time snapshot of rate limiter telemetry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct RateMetrics {
    /// Total number of requests allowed.
    pub allowed: u64,
    /// Total number of requests limited/throttled.
    pub limited: u64,
}

impl RateMetrics {
    /// Returns the total number of evaluated requests (`allowed + limited`).
    #[inline]
    pub fn total_requests(&self) -> u64 {
        self.allowed.saturating_add(self.limited)
    }

    /// Returns the fraction of requests that were limited in the range `[0.0, 1.0]`.
    #[inline]
    pub fn limited_ratio(&self) -> f64 {
        let total = self.total_requests();
        if total == 0 {
            0.0
        } else {
            self.limited as f64 / total as f64
        }
    }

    /// Returns the fraction of requests that were allowed in the range `[0.0, 1.0]`.
    #[inline]
    pub fn allowed_ratio(&self) -> f64 {
        let total = self.total_requests();
        if total == 0 {
            1.0
        } else {
            self.allowed as f64 / total as f64
        }
    }
}

impl fmt::Display for RateMetrics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "RateMetrics {{ allowed: {}, limited: {}, throttle_rate: {:.2}% }}",
            self.allowed,
            self.limited,
            self.limited_ratio() * 100.0
        )
    }
}
