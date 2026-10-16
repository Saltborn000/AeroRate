# 🏛️ AeroRate Architecture & Technical Design

This document details the internal design and mathematical foundations behind **AeroRate**, explaining how it achieves **11.5+ million checks/sec** with sub-10ns execution time using the **Generic Cell Rate Algorithm (GCRA)**.

---

## 🎯 Design Philosophy

Traditional rate limiting algorithms (Token Bucket, Leaky Bucket, Sliding Window Log) present severe performance hurdles in high-throughput distributed systems:

1. **State Bloat**: They require tracking at least two variables: current tokens (`f64`/`u64`) and last update timestamp (`Instant`).
2. **Synchronization Overhead**: Mutating multiple variables atomically requires an exclusive **Mutex**, forcing threads into kernel lock contention.
3. **Floating Point Roundoff**: Refill math (`tokens += elapsed * fill_rate`) introduces floating-point operations on the hot path.

**AeroRate eliminates all of these bottlenecks by using GCRA.**

---

## 🔬 The Generic Cell Rate Algorithm (GCRA)

GCRA (also known as the **Virtual Scheduling Algorithm**) was standardized by the ATM Forum for high-speed telecommunications switching.

Instead of tracking tokens, GCRA models traffic arrival as a single scalar timestamp:
**The Theoretical Arrival Time (TAT)**.

### Parameters
* **Emission Interval ($\tau$)**: Nominal duration between each allowed unit.
  $$\tau = \frac{\text{Period}}{\text{Rate}}$$
* **Burst Tolerance ($\tau \times \text{Burst}$)**: Maximum accumulated credit allowed before throttling.
* **Theoretical Arrival Time ($TAT$)**: The time when the next cell is theoretically scheduled to arrive.

### Decision Logic (Executed via Lock-Free CAS)

When a request arrives at time $t$:

```
                             Request Arrives at time t
                                        │
                                        ▼
                           Arrival Time = max(TAT, t)
                                        │
                                        ▼
                            New TAT = Arrival + tau
                                        │
                                        ▼
                     Earliest Allowed = New TAT - Tolerance
                                        │
                                        ▼
                               t < Earliest Allowed ?
                                      /        \
                                    YES         NO
                                    /             \
                                   ▼               ▼
                           [THROTTLED (429)]    [ALLOWED (200)]
                           Do not mutate TAT    Atomic CAS: TAT = New TAT
```

---

## ⚡ The Lock-Free Atomic CAS Loop

Because GCRA requires only **one single state variable** ($TAT$), we can store it in a single `AtomicU64`:

```rust
let mut current_tat = self.tat_nanos.load(Ordering::Relaxed);
loop {
    let arrival = current_tat.max(now_nanos);
    let new_tat = arrival.saturating_add(increment);
    let earliest_allowed = new_tat.saturating_sub(burst_tolerance);

    if now_nanos < earliest_allowed {
        // Rate limited! Zero memory mutations needed.
        return RateLimitResult::Limited { ... };
    }

    match self.tat_nanos.compare_exchange_weak(
        current_tat,
        new_tat,
        Ordering::AcqRel,
        Ordering::Relaxed,
    ) {
        Ok(_) => return RateLimitResult::Allowed { ... },
        Err(actual) => current_tat = actual,
    }
}
```

* **Zero Locks**: If uncontended, this executes in **1 atomic load + 1 atomic CAS instruction (~3-5ns)**.
* **Zero Allocations**: Evaluates entirely in CPU registers and L1 cache.

---

## 🛡️ Multi-Tenant Sharding (`KeyedRateLimiter`)

For per-IP or per-tenant rate limiting:
1. **64 Partitions (Shards)**: Power-of-two mask (`hash & 63`) computes target shard in 1 clock cycle.
2. **Cache-Line Padding (`#[repr(align(64))]`)**: Guarantees each shard lock sits on its own dedicated 64-byte L1 CPU cache line, preventing false sharing across cores.
3. **Read-Lock Optimization**: Once an IP/key is registered, incoming checks acquire only a shared `read()` lock on the shard. The internal state updates concurrently via the atomic CAS loop!
