use std::sync::Arc;
use std::thread;
use std::time::Duration;

use aerorate::{KeyedRateLimiter, Quota, RateLimiter};

#[test]
fn test_single_limiter_burst_and_throttle() {
    // 5 requests per second, with burst of 5
    let limiter = RateLimiter::new(Quota::per_second(5).allow_burst(5));

    // First 5 requests must be allowed immediately
    for i in 0..5 {
        let res = limiter.check();
        assert!(res.is_allowed(), "Request {i} should be allowed");
    }

    // 6th request must be throttled
    let limited = limiter.check();
    assert!(limited.is_limited(), "6th request should be throttled");
    assert!(limited.retry_after().is_some());
    assert!(limited.retry_after().unwrap() > Duration::ZERO);

    // Wait for refill (1 emission interval ~200ms)
    thread::sleep(Duration::from_millis(220));

    // Should be allowed again
    let recovered = limiter.check();
    assert!(
        recovered.is_allowed(),
        "Request after delay should be allowed"
    );
}

#[test]
fn test_check_n_multiple_tokens() {
    let limiter = RateLimiter::new(Quota::per_second(10).allow_burst(10));

    // Consume 8 tokens at once
    let res = limiter.check_n(8);
    assert!(res.is_allowed());
    assert_eq!(res.remaining_burst(), 2);

    // Trying to consume 5 tokens when only 2 remain should fail
    let failed = limiter.check_n(5);
    assert!(failed.is_limited());

    // Consuming remaining 2 should succeed
    let ok = limiter.check_n(2);
    assert!(ok.is_allowed());
}

#[test]
fn test_keyed_rate_limiter_isolation() {
    let limiter: KeyedRateLimiter<String> = KeyedRateLimiter::per_second(2);

    // Client A uses up their quota
    assert!(limiter.check("client_a").is_allowed());
    assert!(limiter.check("client_a").is_allowed());
    assert!(limiter.check("client_a").is_limited());

    // Client B must NOT be affected by Client A
    assert!(limiter.check("client_b").is_allowed());
    assert!(limiter.check("client_b").is_allowed());
    assert!(limiter.check("client_b").is_limited());

    assert_eq!(limiter.len(), 2);
}

#[test]
fn test_keyed_clean_idle() {
    let limiter: KeyedRateLimiter<String> = KeyedRateLimiter::per_second(10);

    limiter.check("temp_user_1");
    limiter.check("temp_user_2");
    assert_eq!(limiter.len(), 2);

    thread::sleep(Duration::from_millis(60));

    // Clean keys idle for > 40ms
    let cleaned = limiter.clean_idle(Duration::from_millis(40));
    assert_eq!(cleaned, 2);
    assert_eq!(limiter.len(), 0);
}

#[test]
fn test_concurrent_torture_test() {
    let limiter = Arc::new(KeyedRateLimiter::<u64>::with_shards(
        Quota::per_second(50_000).allow_burst(100),
        32,
    ));

    let num_threads: u64 = 16;
    let ops_per_thread: u64 = 2_000;
    let mut handles = Vec::new();

    for t in 0..num_threads {
        let l = Arc::clone(&limiter);
        handles.push(thread::spawn(move || {
            for i in 0..ops_per_thread {
                let key = (t * 10 + i) % 50;
                let _ = l.check(&key);
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    let stats = limiter.metrics();
    assert_eq!(stats.total_requests(), num_threads * ops_per_thread);
    assert!(stats.allowed > 0);
    println!("Keyed torture test complete: {}", stats);
}

#[test]
fn test_telemetry_metrics() {
    let limiter = RateLimiter::new(Quota::per_second(2).allow_burst(2));

    assert!(limiter.check().is_allowed());
    assert!(limiter.check().is_allowed());
    assert!(limiter.check().is_limited());

    let metrics = limiter.metrics();
    assert_eq!(metrics.allowed, 2);
    assert_eq!(metrics.limited, 1);
    assert_eq!(metrics.total_requests(), 3);
    assert!((metrics.allowed_ratio() - 2.0 / 3.0).abs() < 0.001);
    assert!((metrics.limited_ratio() - 1.0 / 3.0).abs() < 0.001);

    limiter.reset_metrics();
    let reset = limiter.metrics();
    assert_eq!(reset.total_requests(), 0);
}
