use std::borrow::Borrow;
use std::collections::HashMap;
use std::hash::{BuildHasher, Hash};
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::RwLock;

use crate::metrics::{AtomicRateMetrics, RateMetrics};
use crate::outcome::RateLimitResult;
use crate::quota::Quota;
use crate::state::AtomicGcraState;

#[repr(align(64))]
struct CachePadded<T> {
    value: T,
}

impl<T> CachePadded<T> {
    pub const fn new(value: T) -> Self {
        Self { value }
    }
}

impl<T> Deref for CachePadded<T> {
    type Target = T;
    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T> DerefMut for CachePadded<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

struct KeyedShard<K, S> {
    states: RwLock<HashMap<K, AtomicGcraState, S>>,
}

struct KeyedLimiterShared<K, S> {
    shards: Box<[CachePadded<KeyedShard<K, S>>]>,
    shard_mask: usize,
    quota: Quota,
    epoch: Instant,
    hasher: S,
    metrics: AtomicRateMetrics,
}

/// A multi-tenant, sharded rate limiter keyed by identifier (IP, user ID, API key).
///
/// Lock-striped across 64 power-of-two, cache-line padded shards (`#[repr(align(64))]`).
/// Reads execute concurrently under shared locks with lock-free atomic GCRA evaluation.
///
/// Cloning `KeyedRateLimiter` is an `O(1)` atomic reference-count increment.
pub struct KeyedRateLimiter<K, S = ahash::RandomState> {
    inner: Arc<KeyedLimiterShared<K, S>>,
}

impl<K, S> Clone for KeyedRateLimiter<K, S> {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<K> KeyedRateLimiter<K, ahash::RandomState>
where
    K: Eq + Hash,
{
    /// Creates a keyed rate limiter with default 64 shards and AHash.
    pub fn new(quota: Quota) -> Self {
        Self::with_shards(quota, 64)
    }

    /// Creates a keyed rate limiter with a custom number of shards (auto-rounded to power of 2).
    pub fn with_shards(quota: Quota, num_shards: usize) -> Self {
        Self::with_shards_and_hasher(quota, num_shards, ahash::RandomState::new())
    }

    /// Convenience constructor for a target rate per second per key.
    pub fn per_second(requests: u32) -> Self {
        Self::new(Quota::per_second(requests))
    }

    /// Convenience constructor for a target rate per minute per key.
    pub fn per_minute(requests: u32) -> Self {
        Self::new(Quota::per_minute(requests))
    }
}

impl<K, S> KeyedRateLimiter<K, S>
where
    K: Eq + Hash,
    S: BuildHasher + Clone,
{
    /// Constructs a keyed rate limiter with custom shards and custom hash builder.
    pub fn with_shards_and_hasher(quota: Quota, num_shards: usize, hasher: S) -> Self {
        let shards_count = num_shards.max(1).next_power_of_two();
        let mut shards = Vec::with_capacity(shards_count);

        for _ in 0..shards_count {
            shards.push(CachePadded::new(KeyedShard {
                states: RwLock::new(HashMap::with_hasher(hasher.clone())),
            }));
        }

        Self {
            inner: Arc::new(KeyedLimiterShared {
                shards: shards.into_boxed_slice(),
                shard_mask: shards_count - 1,
                quota,
                epoch: Instant::now(),
                hasher,
                metrics: AtomicRateMetrics::new(),
            }),
        }
    }

    #[inline]
    fn now_nanos(&self) -> u64 {
        self.inner.epoch.elapsed().as_nanos() as u64
    }

    #[inline]
    fn select_shard<Q>(&self, key: &Q) -> &KeyedShard<K, S>
    where
        Q: Hash + ?Sized,
    {
        let hash = self.inner.hasher.hash_one(key);
        let index = (hash as usize) & self.inner.shard_mask;
        &self.inner.shards[index]
    }

    /// Evaluates a single request for the given key.
    ///
    /// Accepts borrowed keys (`&str` for `String`, `&[u8]` for `Vec<u8>`).
    /// Memory allocation occurs only once when a new key is first seen.
    pub fn check<Q>(&self, key: &Q) -> RateLimitResult
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ToOwned<Owned = K> + ?Sized,
    {
        self.check_n(key, 1)
    }

    /// Evaluates `count` units for the given key.
    pub fn check_n<Q>(&self, key: &Q, count: u32) -> RateLimitResult
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ToOwned<Owned = K> + ?Sized,
    {
        let shard = self.select_shard(key);
        let now = self.now_nanos();

        // 1. Fast read-lock path for established keys
        {
            let guard = shard.states.read();
            if let Some(state) = guard.get(key) {
                let outcome = state.check(now, &self.inner.quota, count);
                if outcome.is_allowed() {
                    self.inner.metrics.record_allowed();
                } else {
                    self.inner.metrics.record_limited();
                }
                return outcome;
            }
        }

        // 2. Write-lock path for first-time key registration
        let mut guard = shard.states.write();
        let state = guard
            .entry(key.to_owned())
            .or_insert_with(|| AtomicGcraState::new(now));

        let outcome = state.check(now, &self.inner.quota, count);
        if outcome.is_allowed() {
            self.inner.metrics.record_allowed();
        } else {
            self.inner.metrics.record_limited();
        }

        outcome
    }

    /// Prunes keys that have been idle longer than `max_idle`.
    ///
    /// Returns the number of idle keys purged.
    pub fn clean_idle(&self, max_idle: Duration) -> usize {
        let now = self.now_nanos();
        let max_idle_nanos = max_idle.as_nanos() as u64;
        let mut total_purged = 0;

        for shard in self.inner.shards.iter() {
            let mut guard = shard.states.write();
            let initial_len = guard.len();
            guard.retain(|_, state| !state.is_idle(now, max_idle_nanos));
            total_purged += initial_len - guard.len();
        }

        total_purged
    }

    /// Returns the total number of tracked keys across all shards.
    pub fn len(&self) -> usize {
        self.inner
            .shards
            .iter()
            .map(|s| s.states.read().len())
            .sum()
    }

    /// Returns `true` if no keys are currently tracked.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clears all tracked keys from all shards.
    pub fn clear(&self) {
        for shard in self.inner.shards.iter() {
            shard.states.write().clear();
        }
    }

    /// Returns the configured quota.
    #[inline]
    pub fn quota(&self) -> &Quota {
        &self.inner.quota
    }

    /// Returns the number of shards.
    #[inline]
    pub fn num_shards(&self) -> usize {
        self.inner.shards.len()
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
