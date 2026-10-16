use std::time::Duration;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Defines the rate allowance and burst capacity for rate limiting.
///
/// Uses Generic Cell Rate Algorithm (GCRA) parameters internally:
/// - **Emission Interval ($\tau$)**: Time between each unit allowance.
/// - **Burst Tolerance ($\tau \times \text{burst}$)**: Max transient deviation allowed before limiting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Quota {
    pub(crate) emission_interval_nanos: u64,
    pub(crate) burst_tolerance_nanos: u64,
    pub(crate) max_burst: u32,
}

impl Quota {
    /// Creates a quota for `requests` per `period` with a default burst equal to `requests`.
    pub fn with_period(requests: u32, period: Duration) -> Self {
        let reqs = requests.max(1);
        let period_nanos = period.as_nanos().max(1) as u64;
        let emission_interval_nanos = (period_nanos / reqs as u64).max(1);
        let max_burst = reqs;
        let burst_tolerance_nanos = emission_interval_nanos.saturating_mul(max_burst as u64);

        Self {
            emission_interval_nanos,
            burst_tolerance_nanos,
            max_burst,
        }
    }

    /// Creates a quota for a target number of requests per second.
    ///
    /// # Example
    /// ```rust
    /// use aerorate::Quota;
    ///
    /// // 100 requests per second with burst allowance of up to 100
    /// let quota = Quota::per_second(100);
    /// ```
    pub fn per_second(requests: u32) -> Self {
        Self::with_period(requests, Duration::from_secs(1))
    }

    /// Creates a quota for a target number of requests per minute.
    pub fn per_minute(requests: u32) -> Self {
        Self::with_period(requests, Duration::from_secs(60))
    }

    /// Creates a quota for a target number of requests per hour.
    pub fn per_hour(requests: u32) -> Self {
        Self::with_period(requests, Duration::from_secs(3600))
    }

    /// Customizes the maximum burst allowance.
    ///
    /// If `burst` is 1, requests must be evenly spaced over the interval.
    /// Higher burst values allow callers to expend credits in short bursts.
    ///
    /// # Example
    /// ```rust
    /// use aerorate::Quota;
    ///
    /// // Sustained rate of 50 req/sec, but allowing an instantaneous burst of up to 10
    /// let quota = Quota::per_second(50).allow_burst(10);
    /// ```
    #[must_use]
    pub fn allow_burst(mut self, burst: u32) -> Self {
        let b = burst.max(1);
        self.max_burst = b;
        self.burst_tolerance_nanos = self.emission_interval_nanos.saturating_mul(b as u64);
        self
    }

    /// Returns the emission interval between requests.
    #[inline]
    pub fn emission_interval(&self) -> Duration {
        Duration::from_nanos(self.emission_interval_nanos)
    }

    /// Returns the maximum burst limit.
    #[inline]
    pub fn max_burst(&self) -> u32 {
        self.max_burst
    }
}
