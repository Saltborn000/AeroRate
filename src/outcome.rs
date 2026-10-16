use std::time::Duration;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// The outcome of evaluating a request against the rate limiter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum RateLimitResult {
    /// The request is permitted within the configured quota.
    Allowed {
        /// Number of additional burst requests that can be accommodated immediately.
        remaining_burst: u32,
        /// Duration until the quota bucket completely resets/refills.
        reset_after: Duration,
    },
    /// The request exceeded the quota and was rejected/throttled.
    Limited {
        /// Duration the caller must wait before retrying (for HTTP `Retry-After` header).
        retry_after: Duration,
        /// Duration until the bucket completely resets.
        reset_after: Duration,
    },
}

impl RateLimitResult {
    /// Returns `true` if the request was allowed.
    #[inline]
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed { .. })
    }

    /// Returns `true` if the request was throttled/limited.
    #[inline]
    pub fn is_limited(&self) -> bool {
        matches!(self, Self::Limited { .. })
    }

    /// Returns the retry delay if limited, or `None` if allowed.
    #[inline]
    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            Self::Limited { retry_after, .. } => Some(*retry_after),
            Self::Allowed { .. } => None,
        }
    }

    /// Returns the remaining burst capacity. Returns `0` if limited.
    #[inline]
    pub fn remaining_burst(&self) -> u32 {
        match self {
            Self::Allowed {
                remaining_burst, ..
            } => *remaining_burst,
            Self::Limited { .. } => 0,
        }
    }

    /// Returns the duration until the quota resets to its full burst capacity.
    #[inline]
    pub fn reset_after(&self) -> Duration {
        match self {
            Self::Allowed { reset_after, .. } | Self::Limited { reset_after, .. } => *reset_after,
        }
    }
}
