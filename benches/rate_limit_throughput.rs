use std::hint::black_box;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use aerorate::{KeyedRateLimiter, Quota, RateLimiter};
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};

fn bench_single_limiter(c: &mut Criterion) {
    let mut group = c.benchmark_group("single_limiter");

    let limiter = RateLimiter::new(Quota::per_second(10_000_000));

    group.throughput(Throughput::Elements(1));
    group.bench_function("lock_free_atomic_check", |b| {
        b.iter(|| {
            black_box(limiter.check());
        });
    });

    group.finish();
}

fn bench_keyed_limiter(c: &mut Criterion) {
    let mut group = c.benchmark_group("keyed_limiter");

    let limiter: KeyedRateLimiter<String> =
        KeyedRateLimiter::with_shards(Quota::per_second(1_000_000), 64);

    for i in 0..1_000 {
        limiter.check(&format!("client:{i}"));
    }

    group.throughput(Throughput::Elements(1));
    group.bench_function("borrowed_str_lookup", |b| {
        let mut i = 0;
        b.iter(|| {
            let key = format!("client:{}", i % 1_000);
            let result = limiter.check(black_box(&key));
            i += 1;
            black_box(result)
        });
    });

    for threads in [2, 4, 8] {
        group.bench_with_input(
            BenchmarkId::new("concurrent_threads", threads),
            &threads,
            |b, &num_threads| {
                let shared = Arc::new(KeyedRateLimiter::<u64>::with_shards(
                    Quota::per_second(10_000_000),
                    64,
                ));

                for i in 0..1_000 {
                    shared.check(&i);
                }

                b.iter(|| {
                    let mut handles = Vec::with_capacity(num_threads);
                    for t in 0..num_threads {
                        let l = Arc::clone(&shared);
                        handles.push(thread::spawn(move || {
                            for i in 0..1_000 {
                                let key = (t as u64 * 100 + i) % 1_000;
                                black_box(l.check(&key));
                            }
                        }));
                    }
                    for h in handles {
                        h.join().unwrap();
                    }
                });
            },
        );
    }

    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default().warm_up_time(Duration::from_millis(500)).measurement_time(Duration::from_secs(1));
    targets = bench_single_limiter, bench_keyed_limiter
}
criterion_main!(benches);
