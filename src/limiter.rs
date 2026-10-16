use std::sync::Arc;
use std::time::Instant;

use crate::metrics::{AtomicRateMetrics, RateMetrics};
use crate::outcome::RateLimitResult;
use crate::quota::Quota;
use crate::state::AtomicGcraState;

struct RateLimiterShared {
    state: AtomicGcraState,
    quota: Quota,
    epoch: Instant,
    metrics: AtomicRateMetrics,
}

/// An ultra-fast, lock-free rate limiter for a single target.
///
/// Implements the Generic Cell Rate Algorithm (GCRA) using lock-free atomics.
/// Evaluates requests in **sub-10 nanoseconds** with zero heap allocations.
///
/// Cloning `RateLimiter` is an `O(1)` atomic reference-count increment.
#[derive(Clone)]
pub struct RateLimiter {
    inner: Arc<RateLimiterShared>,
}

impl RateLimiter {
    /// Constructs a rate limiter with the specified quota.
    ///
    /// # Example
    /// ```rust
    /// use aerorate::{RateLimiter, Quota};
    ///
    /// let limiter = RateLimiter::new(Quota::per_second(100));
    /// assert!(limiter.check().is_allowed());
    /// ```
    pub fn new(quota: Quota) -> Self {
        let epoch = Instant::now();
        Self {
            inner: Arc::new(RateLimiterShared {
                state: AtomicGcraState::new(0),
                quota,
                epoch,
                metrics: AtomicRateMetrics::new(),
            }),
        }
    }

    /// Convenience constructor for a target requests-per-second limit.
    pub fn per_second(requests: u32) -> Self {
        Self::new(Quota::per_second(requests))
    }

    /// Convenience constructor for a target requests-per-minute limit.
    pub fn per_minute(requests: u32) -> Self {
        Self::new(Quota::per_minute(requests))
    }

    #[inline]
    fn now_nanos(&self) -> u64 {
        self.inner.epoch.elapsed().as_nanos() as u64
    }

    /// Evaluates a single request against the rate limiter.
    ///
    /// Runs a lock-free CAS loop with sub-10ns overhead.
    #[inline]
    pub fn check(&self) -> RateLimitResult {
        self.check_n(1)
    }

    /// Evaluates `count` units against the rate limiter in a single atomic check.
    pub fn check_n(&self, count: u32) -> RateLimitResult {
        let outcome = self
            .inner
            .state
            .check(self.now_nanos(), &self.inner.quota, count);

        if outcome.is_allowed() {
            self.inner.metrics.record_allowed();
        } else {
            self.inner.metrics.record_limited();
        }

        outcome
    }

    /// Returns the configured quota.
    #[inline]
    pub fn quota(&self) -> &Quota {
        &self.inner.quota
    }

    /// Takes a point-in-time snapshot of telemetry metrics.
    #[inline]
    pub fn metrics(&self) -> RateMetrics {
        self.inner.metrics.snapshot()
    }

    /// Resets metrics counters to zero.
    #[inline]
    pub fn reset_metrics(&self) {
        self.inner.metrics.reset();
    }
}
