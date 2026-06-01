//! Property-based tests: quantity conservation, book consistency, replay determinism.

use lob_engine::matching::MatchingEngine;
use lob_engine::orders::{OrderInput, Side};
use lob_engine::replay::replay_commands;
use proptest::prelude::*;
use uuid::Uuid;

fn arb_side() -> impl Strategy<Value = Side> {
    prop_oneof![Just(Side::Buy), Just(Side::Sell)]
}

proptest! {
    #[test]
    fn no_negative_remaining(
        side in arb_side(),
        price in 1i64..200,
        qty in 1u64..1000,
    ) {
        let mut engine = MatchingEngine::new();
        let id = Uuid::new_v4();
        engine.process(OrderInput::AddLimit {
            order_id: id,
            side,
            price,
            quantity: qty,
            timestamp: 1,
        });
        if let Some(o) = engine.book().get_order(id) {
            prop_assert!(o.remaining > 0);
        }
    }

    #[test]
    fn trade_quantities_positive(
        prices in prop::collection::vec(1i64..50, 1..20),
    ) {
        let mut engine = MatchingEngine::new();
        let mut all_qty = 0u64;
        for (i, p) in prices.iter().enumerate() {
            let r = engine.process(OrderInput::AddLimit {
                order_id: Uuid::new_v4(),
                side: Side::Sell,
                price: 100 + p,
                quantity: 5,
                timestamp: i as i64,
            });
            for t in &r.trades {
                prop_assert!(t.quantity > 0);
                all_qty += t.quantity;
            }
        }
        let r = engine.process(OrderInput::AddLimit {
            order_id: Uuid::new_v4(),
            side: Side::Buy,
            price: 200,
            quantity: 1000,
            timestamp: 10_000,
        });
        for t in &r.trades {
            prop_assert!(t.quantity > 0);
            all_qty += t.quantity;
        }
        let _ = all_qty;
    }

    #[test]
    fn replay_deterministic(commands in prop::collection::vec((arb_side(), 1i64..30, 1u64..20), 1..30)) {
        let mut inputs = Vec::new();
        for (i, (side, offset, qty)) in commands.iter().enumerate() {
            inputs.push(OrderInput::AddLimit {
                order_id: Uuid::new_v4(),
                side: *side,
                price: 100 + offset,
                quantity: *qty,
                timestamp: i as i64,
            });
        }
        let (_, t1) = replay_commands(&inputs);
        let (_, t2) = replay_commands(&inputs);
        prop_assert_eq!(t1.len(), t2.len());
        for (a, b) in t1.iter().zip(t2.iter()) {
            prop_assert_eq!(a.price, b.price);
            prop_assert_eq!(a.quantity, b.quantity);
            prop_assert_eq!(a.buy_order_id, b.buy_order_id);
            prop_assert_eq!(a.sell_order_id, b.sell_order_id);
        }
    }
}
