//! Deterministic event replay.

use anyhow::Result;
use std::path::Path;

use crate::matching::{MatchResult, MatchingEngine};
use crate::orders::OrderInput;
use crate::persistence::events::{BookEvent, EventStore};
use crate::trades::Trade;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReplayOutput {
    pub trades: Vec<Trade>,
    pub event_count: usize,
}

/// Replay events from JSONL file into a fresh engine (no DB).
pub fn replay_from_file<P: AsRef<Path>>(path: P) -> Result<ReplayOutput> {
    let store = EventStore::load_jsonl(path)?;
    Ok(replay_events(store.events()))
}

/// Replay in-memory event slice.
pub fn replay_events(events: &[BookEvent]) -> ReplayOutput {
    let mut engine = MatchingEngine::new();
    let mut all_trades = Vec::new();

    for event in events {
        match event {
            BookEvent::OrderAccepted { order } => {
                if let Some(price) = order.price {
                    let result = engine.process(OrderInput::AddLimit {
                        order_id: order.id,
                        side: order.side,
                        price,
                        quantity: order.remaining,
                        timestamp: order.timestamp,
                    });
                    all_trades.extend(result.trades);
                }
            }
            BookEvent::OrderModified {
                order_id,
                side,
                price,
                quantity,
                timestamp,
            } => {
                let result = engine.process(OrderInput::Modify {
                    order_id: *order_id,
                    side: *side,
                    price: *price,
                    quantity: *quantity,
                    timestamp: *timestamp,
                });
                all_trades.extend(result.trades);
            }
            BookEvent::OrderCancelled {
                order_id,
                timestamp,
            } => {
                let _ = engine.process(OrderInput::Cancel {
                    order_id: *order_id,
                    timestamp: *timestamp,
                });
            }
            BookEvent::TradeExecuted { .. } => {
                // Trades are outputs of matching; replay reconstructs via orders only.
                // Stored trade events validate audit trail, not re-applied.
            }
        }
    }

    ReplayOutput {
        trades: all_trades,
        event_count: events.len(),
    }
}

/// Full command replay from order inputs (deterministic).
pub fn replay_commands(inputs: &[OrderInput]) -> (MatchingEngine, Vec<Trade>) {
    let mut engine = MatchingEngine::new();
    let mut trades = Vec::new();
    for input in inputs {
        let result: MatchResult = engine.process(input.clone());
        trades.extend(result.trades);
    }
    (engine, trades)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orders::{Order, Side};
    use uuid::Uuid;

    #[test]
    fn replay_is_deterministic() {
        let id = Uuid::new_v4();
        let events = vec![
            BookEvent::OrderAccepted {
                order: Order::limit(id, Side::Sell, 100, 50, 1),
            },
            BookEvent::OrderAccepted {
                order: Order::limit(Uuid::new_v4(), Side::Buy, 100, 30, 2),
            },
        ];
        let a = replay_events(&events);
        let b = replay_events(&events);
        assert_eq!(a.trades.len(), b.trades.len());
        for (ta, tb) in a.trades.iter().zip(b.trades.iter()) {
            assert_eq!(ta.price, tb.price);
            assert_eq!(ta.quantity, tb.quantity);
            assert_eq!(ta.buy_order_id, tb.buy_order_id);
            assert_eq!(ta.sell_order_id, tb.sell_order_id);
        }
    }
}
