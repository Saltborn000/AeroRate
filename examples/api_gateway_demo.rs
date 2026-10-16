use std::sync::Arc;
use std::thread;
use std::time::Instant;

use aerorate::{KeyedRateLimiter, Quota, RateLimitResult};

fn main() {
    println!("============================================================");
    println!("     ⚡ AeroRate - Enterprise API Gateway Demo              ");
    println!("============================================================");

    // 1. Configure per-client rate limiter (50 req/sec sustained, burst of 10)
    let gateway_limiter: KeyedRateLimiter<String> =
        KeyedRateLimiter::with_shards(Quota::per_second(50).allow_burst(10), 64);

    // 2. Simulate single client requests
    println!("\n[Testing Single Client Burst]");
    let client_ip = "192.168.1.42";

    for req_num in 1..=12 {
        let result = gateway_limiter.check(client_ip);
        match result {
            RateLimitResult::Allowed {
                remaining_burst,
                reset_after,
            } => {
                println!(
                    "  Req #{req_num}: ✅ ALLOWED (HTTP 200) | Remaining: {remaining_burst} | Reset in: {reset_after:?}"
                );
            }
            RateLimitResult::Limited {
                retry_after,
                reset_after,
            } => {
                println!(
                    "  Req #{req_num}: 🛑 429 TOO MANY REQUESTS | Retry-After: {retry_after:?} | Reset in: {reset_after:?}"
                );
            }
        }
    }

    // 3. Simulate high-throughput concurrent traffic across 8 worker threads
    println!("\n[Simulating 8 Concurrent Gateway Workers Handling 80,000 Requests]");
    let shared_limiter = Arc::new(gateway_limiter);
    let mut workers = Vec::new();
    let start = Instant::now();

    for thread_id in 0..8 {
        let l = Arc::clone(&shared_limiter);
        workers.push(thread::spawn(move || {
            for i in 0..10_000 {
                let ip = format!("10.0.{}.{}", thread_id, i % 500);
                let _ = l.check(&ip);
            }
        }));
    }

    for w in workers {
        w.join().unwrap();
    }
    let elapsed = start.elapsed();

    let ops_per_sec = (80_000.0 / elapsed.as_secs_f64()) as u64;
    println!("Completed 80,000 multi-threaded checks in {:?}", elapsed);
    println!("Gateway Throughput: {} checks/sec", ops_per_sec);

    // 4. Telemetry snapshot
    let stats = shared_limiter.metrics();
    println!("\n[Gateway Telemetry Snapshot]");
    println!("  Total Requests Evaluated : {}", stats.total_requests());
    println!("  Allowed Requests         : {}", stats.allowed);
    println!("  Limited / Throttled      : {}", stats.limited);
    println!(
        "  Throttle Rate            : {:.2}%",
        stats.limited_ratio() * 100.0
    );
    println!("  Active Client IPs Tracked: {}", shared_limiter.len());

    println!("\n✅ AeroRate enterprise gateway showcase completed successfully!");
}
