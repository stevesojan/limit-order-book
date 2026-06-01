//! Integration-style tests for matching engine.

use lob_engine::matching::MatchingEngine;
use lob_engine::orders::{OrderInput, Side};
use uuid::Uuid;

#[test]
fn cancel_removes_from_book() {
    let mut engine = MatchingEngine::new();
    let id = Uuid::new_v4();
    engine.process(OrderInput::AddLimit {
        order_id: id,
        side: Side::Buy,
        price: 99,
        quantity: 50,
        timestamp: 1,
    });
    let result = engine.process(OrderInput::Cancel {
        order_id: id,
        timestamp: 2,
    });
    assert!(result.cancelled_order.is_some());
    assert!(engine.book().get_order(id).is_none());
}

#[test]
fn higher_buy_price_matches_first() {
    let mut engine = MatchingEngine::new();
    engine.process(OrderInput::AddLimit {
        order_id: Uuid::new_v4(),
        side: Side::Sell,
        price: 100,
        quantity: 10,
        timestamp: 1,
    });
    let result = engine.process(OrderInput::AddLimit {
        order_id: Uuid::new_v4(),
        side: Side::Buy,
        price: 105,
        quantity: 10,
        timestamp: 2,
    });
    assert_eq!(result.trades.len(), 1);
    assert_eq!(result.trades[0].price, 100);
}

#[test]
fn snapshot_spread() {
    let mut engine = MatchingEngine::new();
    engine.process(OrderInput::AddLimit {
        order_id: Uuid::new_v4(),
        side: Side::Buy,
        price: 99,
        quantity: 10,
        timestamp: 1,
    });
    engine.process(OrderInput::AddLimit {
        order_id: Uuid::new_v4(),
        side: Side::Sell,
        price: 101,
        quantity: 10,
        timestamp: 2,
    });
    assert_eq!(engine.spread(), Some(2));
}
