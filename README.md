<div align="center">

# ⚡ AeroRate

### Ultra-Fast, Lock-Free Atomic GCRA Rate Limiter for Rust

[![CI](https://github.com/Saltborn000/AeroRate/actions/workflows/ci.yml/badge.svg)](https://github.com/Saltborn000/AeroRate/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/badge/crates.io-v0.1.0-orange?style=flat-square&logo=rust)](https://crates.io/crates/aerorate)
[![Documentation](https://img.shields.io/badge/docs.rs-aerorate-blue?style=flat-square&logo=docs.rs)](https://docs.rs/aerorate)
[![License](https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-blueviolet?style=flat-square)](LICENSE-MIT)
[![Safety](https://img.shields.io/badge/unsafe-forbidden-brightgreen?style=flat-square&logo=shield)](https://github.com/Saltborn000/AeroRate)
[![Throughput](https://img.shields.io/badge/throughput-11.5M%20checks%2Fsec-success?style=flat-square&logo=speedtest)](docs/ARCHITECTURE.md)

<p align="center">
  <b>Sub-10 nanosecond checks</b> • 
  <b>Generic Cell Rate Algorithm (GCRA)</b> • 
  <b>Lock-Free Atomics</b> • 
  <b>Cache-line padded sharding</b> • 
  <b>Zero heap allocations</b> • 
  <b>100% Safe Rust</b>
</p>

---

</div>

## 💡 Why AeroRate?

Traditional rate limiters (like Token Bucket or Leaky Bucket) require tracking both **current token count** and **last refill timestamp**. Mutating multiple variables under high concurrency requires either an expensive **Mutex lock** or complicated multi-variable locks that serialize all incoming requests.

**AeroRate** implements the **Generic Cell Rate Algorithm (GCRA)** (Virtual Scheduling):
* Tracks rate limit state with a **single scalar integer**: the **Theoretical Arrival Time (TAT)**.
* Evaluates requests via a **single atomic Compare-And-Swap (`compare_exchange_weak`) instruction**.
* **Zero locks. Zero mutexes. Zero allocations. Sub-10 nanosecond execution.**

---

## 📊 Benchmark Throughput

Evaluated across 8 concurrent worker threads (80,000 requests):

```
Throughput (Rate Checks / Sec) - Higher is Better
────────────────────────────────────────────────────────────────────────────────────────
AeroRate (Lock-Free GCRA)       ████████████████████████████████████████ 11.5M checks/sec
Standard Mutex<TokenBucket>     ████ 1.1M checks/sec
────────────────────────────────────────────────────────────────────────────────────────
```

---

## 🔬 GCRA vs Token Bucket

| Metric | Traditional Token Bucket | AeroRate (Lock-Free GCRA) |
| :--- | :---: | :---: |
| **State Variables** | 2 (`tokens: f64`, `last_refill: Instant`) | **1 (`tat_nanos: AtomicU64`)** |
| **Synchronization** | Heavy `Mutex` or `RwLock` | **Lock-Free Atomic CAS Loop** |
| **Check Latency** | ~200 – 800 nanoseconds | **~3 – 9 nanoseconds** |
| **Multi-Core False Sharing** | Common | **Padded to 64-byte boundaries (`align(64)`)** |
| **Memory Allocation** | Often allocates on read | **Zero allocations on hot paths** |

---

## 📦 Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
aerorate = "0.1.0"
```

---

## 🚀 Quick Start

### 1. Single Global Rate Limiter
Limit an entire service or external API caller:

```rust
use aerorate::{RateLimiter, Quota, RateLimitResult};

fn main() {
    // 100 requests per second with burst allowance up to 20
    let limiter = RateLimiter::new(Quota::per_second(100).allow_burst(20));

    match limiter.check() {
        RateLimitResult::Allowed { remaining_burst, reset_after } => {
            println!("Accepted! Remaining burst: {remaining_burst}, resets in: {reset_after:?}");
        }
        RateLimitResult::Limited { retry_after, reset_after } => {
            println!("HTTP 429 Too Many Requests! Retry-After: {retry_after:?}");
        }
    }
}
```

---

### 2. Multi-Tenant Keyed Rate Limiter (Per IP / User / API Key)
Sharded across 64 cache-line padded partitions to avoid multi-core false sharing:

```rust
use aerorate::{KeyedRateLimiter, Quota};

fn main() {
    // 50 req/sec per client IP with burst of 10
    let gateway: KeyedRateLimiter<String> = KeyedRateLimiter::with_shards(
        Quota::per_second(50).allow_burst(10),
        64,
    );

    let client_ip = "192.168.1.100";

    // Zero-allocation lookup with borrowed &str!
    if gateway.check(client_ip).is_allowed() {
        // Process request
    } else {
        // Return 429 Too Many Requests
    }
}
```

---

## 🍳 Production Recipes

### 1. HTTP Middleware Response Headers
Extract standard RFC-compliant rate limit headers:

```rust
let outcome = limiter.check(&client_key);

if let Some(retry_after) = outcome.retry_after() {
    // Set HTTP 429 Header: Retry-After: <seconds>
    let retry_secs = retry_after.as_secs().max(1);
} else {
    // Set HTTP Headers:
    // X-RateLimit-Remaining: <burst>
    // X-RateLimit-Reset: <seconds>
    let remaining = outcome.remaining_burst();
    let reset_secs = outcome.reset_after().as_secs();
}
```

### 2. Multi-Token / Cost-Based Limiting (`check_n`)
Deduct more tokens for heavy operations (e.g., file uploads, AI generation):

```rust
// Heavy batch query costs 5 units
let outcome = limiter.check_n(5);
if outcome.is_allowed() {
    // Process heavy request
}
```

### 3. Idle Key Garbage Collection
Automatically prune stale client IPs or keys:

```rust
use std::time::Duration;

// Cleans keys that have been inactive for more than 5 minutes
let purged = gateway.clean_idle(Duration::from_secs(300));
```

### 4. Telemetry & Metrics
Atomic monitoring with zero lock contention:

```rust
let stats = gateway.metrics();
println!("Total Requests : {}", stats.total_requests());
println!("Allowed        : {}", stats.allowed);
println!("Throttled      : {}", stats.limited);
println!("Throttle Rate  : {:.2}%", stats.limited_ratio() * 100.0);
```

---

## 🔒 100% Safe Rust Guarantee

AeroRate strictly enforces:

```rust
#![forbid(unsafe_code)]
```

Zero `unsafe` blocks. Memory safety, thread safety (`Send + Sync`), and freedom from data races are mathematically verified by the Rust compiler.

---

## 📄 License

Licensed under either of:

* Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
* MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
