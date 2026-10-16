use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crate::outcome::RateLimitResult;
use crate::quota::Quota;

/// Lock-free GCRA state using a single atomic Theoretical Arrival Time (TAT).
#[derive(Debug)]
pub struct AtomicGcraState {
    pub(crate) tat_nanos: AtomicU64,
    pub(crate) last_touched_nanos: AtomicU64,
}

impl AtomicGcraState {
    /// Constructs a new atomic GCRA state initialized to `now_nanos`.
    pub fn new(now_nanos: u64) -> Self {
        Self {
            tat_nanos: AtomicU64::new(now_nanos),
            last_touched_nanos: AtomicU64::new(now_nanos),
        }
    }

    /// Evaluates `count` cells against the quota using a lock-free CAS loop.
    pub fn check(&self, now_nanos: u64, quota: &Quota, count: u32) -> RateLimitResult {
        let n = count.max(1) as u64;
        let increment = quota.emission_interval_nanos.saturating_mul(n);
        let mut current_tat = self.tat_nanos.load(Ordering::Relaxed);

        loop {
            let arrival = current_tat.max(now_nanos);
            let new_tat = arrival.saturating_add(increment);
            let earliest_allowed = new_tat.saturating_sub(quota.burst_tolerance_nanos);

            if now_nanos < earliest_allowed {
                let wait_nanos = earliest_allowed - now_nanos;
                let reset_nanos = current_tat.saturating_sub(now_nanos);

                return RateLimitResult::Limited {
                    retry_after: Duration::from_nanos(wait_nanos),
                    reset_after: Duration::from_nanos(reset_nanos),
                };
            }

            match self.tat_nanos.compare_exchange_weak(
                current_tat,
                new_tat,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    self.last_touched_nanos.store(now_nanos, Ordering::Relaxed);
                    let diff = new_tat.saturating_sub(now_nanos);
                    let remaining_nanos = quota.burst_tolerance_nanos.saturating_sub(diff);
                    let remaining = (remaining_nanos / quota.emission_interval_nanos) as u32;

                    return RateLimitResult::Allowed {
                        remaining_burst: remaining,
                        reset_after: Duration::from_nanos(diff),
                    };
                }
                Err(actual) => current_tat = actual,
            }
        }
    }

    /// Checks if this state has been idle longer than `max_idle_nanos`.
    pub fn is_idle(&self, now_nanos: u64, max_idle_nanos: u64) -> bool {
        let touched = self.last_touched_nanos.load(Ordering::Relaxed);
        now_nanos.saturating_sub(touched) >= max_idle_nanos
    }
}
