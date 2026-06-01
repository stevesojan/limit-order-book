//! Large-scale stress tests (run with `cargo test --test stress --release`).

use lob_engine::matching::MatchingEngine;
use lob_engine::orders::{OrderInput, Side};
use lob_engine::replay::replay_commands;
use std::time::Instant;
use uuid::Uuid;

#[test]
#[ignore = "long-running stress test"]
fn stress_100k_orders() {
    run_stress(100_000);
}

#[test]
#[ignore = "long-running stress test"]
fn stress_1m_orders() {
    run_stress(1_000_000);
}

fn run_stress(n: usize) {
    let mut engine = MatchingEngine::new();
    let start = Instant::now();
    for i in 0..n {
        let side = if i % 2 == 0 { Side::Buy } else { Side::Sell };
        let price = 100 + (i % 20) as i64;
        engine.process(OrderInput::AddLimit {
            order_id: Uuid::new_v4(),
            side,
            price,
            quantity: 1,
            timestamp: i as i64,
        });
    }
    let elapsed = start.elapsed();
    let rate = n as f64 / elapsed.as_secs_f64();
    eprintln!("{n} orders in {elapsed:?} ({rate:.0} orders/sec)");
    assert!(rate > 10_000.0, "expected >10k orders/sec, got {rate}");
}

#[test]
fn stress_replay_50k() {
    let n = 50_000;
    let inputs: Vec<_> = (0..n)
        .map(|i| OrderInput::AddLimit {
            order_id: Uuid::new_v4(),
            side: if i % 2 == 0 { Side::Buy } else { Side::Sell },
            price: 100 + (i % 10) as i64,
            quantity: 1,
            timestamp: i as i64,
        })
        .collect();
    let start = Instant::now();
    let (_, trades) = replay_commands(&inputs);
    let elapsed = start.elapsed();
    eprintln!(
        "replay {n} commands, {} trades, {:?}",
        trades.len(),
        elapsed
    );
}
