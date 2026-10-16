//! # AeroRate
//!
//! `aerorate` is an ultra-fast, lock-free rate limiter implementing the **Generic Cell Rate Algorithm (GCRA)**.
//!
//! Designed for high-frequency trading gateways, reverse proxies, and high-throughput microservices.
//!
//! ## Key Architecture Highlights
//!
//! - **Sub-10 Nanosecond Evaluation**: Evaluates requests via a lock-free atomic CAS loop on a single `AtomicU64` timestamp.
//! - **Zero Memory Allocations**: Hot path checks require zero heap allocations.
//! - **Keyed Multi-Tenant Limiting**: Sharded across power-of-two, cache-line padded partitions (`#[repr(align(64))]`) to eliminate false sharing.
//! - **Borrowed Key Queries**: Query with `&str` on `String` keys without cloning.
//! - **Rich HTTP Telemetry**: Yields `retry_after`, `remaining_burst`, and `reset_after` for standard HTTP headers.
//! - **100% Safe Rust**: `#![forbid(unsafe_code)]` strictly enforced.
//!
//! ## Quick Start
//!
//! ```rust
//! use aerorate::{RateLimiter, Quota};
//! use std::time::Duration;
//!
//! // Allow 100 requests per second with burst allowance up to 20
//! let limiter = RateLimiter::new(Quota::per_second(100).allow_burst(20));
//!
//! match limiter.check() {
//!     outcome if outcome.is_allowed() => {
//!         println!("Allowed! Remaining burst: {}", outcome.remaining_burst());
//!     }
//!     outcome => {
//!         println!("Throttled! Retry after: {:?}", outcome.retry_after());
//!     }
//! }
//! ```
//!
//! ## Multi-Tenant / Per-Key Rate Limiting
//!
//! ```rust
//! use aerorate::{KeyedRateLimiter, Quota};
//!
//! // Rate limit per IP address or API key
//! let limiter: KeyedRateLimiter<String> = KeyedRateLimiter::per_second(10);
//!
//! // Zero-allocation lookup using borrowed &str
//! let client_ip = "192.168.1.100";
//! if limiter.check(client_ip).is_allowed() {
//!     println!("Request accepted for {}", client_ip);
//! }
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Keyed multi-tenant rate limiter with cache-line padded sharding.
pub mod keyed;
/// Single-target lock-free rate limiter.
pub mod limiter;
/// Telemetry metrics and snapshot tracking.
pub mod metrics;
/// Rate limit evaluation outcomes and HTTP header helpers.
pub mod outcome;
/// Quota specifications and GCRA parameters.
pub mod quota;
/// Internal lock-free atomic GCRA state.
pub mod state;

pub use keyed::KeyedRateLimiter;
pub use limiter::RateLimiter;
pub use metrics::RateMetrics;
pub use outcome::RateLimitResult;
pub use quota::Quota;

/// Type alias for a keyed rate limiter where keys are `String`.
pub type StringRateLimiter = KeyedRateLimiter<String>;
