//! Criterion benchmarks: insertion, matching, cancellation, replay.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use lob_engine::matching::MatchingEngine;
use lob_engine::orders::{OrderInput, Side};
use lob_engine::replay::replay_commands;
use uuid::Uuid;

fn bench_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("insert_limit");
    for size in [1_000, 10_000] {
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &n| {
            b.iter(|| {
                let mut engine = MatchingEngine::new();
                for i in 0..n {
                    let side = if i % 2 == 0 { Side::Buy } else { Side::Sell };
                    let price = 100 + (i % 10) as i64;
                    let _ = engine.process(OrderInput::AddLimit {
                        order_id: Uuid::new_v4(),
                        side,
                        price,
                        quantity: 10,
                        timestamp: i as i64,
                    });
                }
                black_box(engine.best_bid());
            });
        });
    }
    group.finish();
}

fn bench_matching(c: &mut Criterion) {
    c.bench_function("match_burst", |b| {
        b.iter(|| {
            let mut engine = MatchingEngine::new();
            for i in 0..500 {
                let _ = engine.process(OrderInput::AddLimit {
                    order_id: Uuid::new_v4(),
                    side: Side::Sell,
                    price: 100,
                    quantity: 5,
                    timestamp: i,
                });
            }
            let result = engine.process(OrderInput::AddLimit {
                order_id: Uuid::new_v4(),
                side: Side::Buy,
                price: 100,
                quantity: 2500,
                timestamp: 10_000,
            });
            black_box(result.trades.len());
        });
    });
}

fn bench_cancel(c: &mut Criterion) {
    c.bench_function("cancel_order", |b| {
        b.iter(|| {
            let mut engine = MatchingEngine::new();
            let id = Uuid::new_v4();
            let _ = engine.process(OrderInput::AddLimit {
                order_id: id,
                side: Side::Buy,
                price: 100,
                quantity: 100,
                timestamp: 1,
            });
            let _ = engine.process(OrderInput::Cancel {
                order_id: id,
                timestamp: 2,
            });
        });
    });
}

fn bench_replay(c: &mut Criterion) {
    let mut inputs = Vec::new();
    for i in 0..5_000 {
        inputs.push(OrderInput::AddLimit {
            order_id: Uuid::new_v4(),
            side: if i % 2 == 0 { Side::Buy } else { Side::Sell },
            price: 100 + (i % 5) as i64,
            quantity: 1,
            timestamp: i as i64,
        });
    }
    c.bench_function("replay_5000", |b| {
        b.iter(|| {
            let (_, trades) = replay_commands(black_box(&inputs));
            black_box(trades.len());
        });
    });
}

criterion_group!(
    benches,
    bench_insert,
    bench_matching,
    bench_cancel,
    bench_replay
);
criterion_main!(benches);
