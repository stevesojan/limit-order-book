use lob_engine::orders::{Order, Side};
use lob_engine::persistence::events::{BookEvent, EventStore};
use lob_engine::replay::replay_events;
use tempfile::NamedTempFile;
use uuid::Uuid;

#[test]
fn jsonl_roundtrip_and_replay() {
    let sell_id = Uuid::new_v4();
    let buy_id = Uuid::new_v4();
    let events = vec![
        BookEvent::OrderAccepted {
            order: Order::limit(sell_id, Side::Sell, 100, 100, 1),
        },
        BookEvent::OrderAccepted {
            order: Order::limit(buy_id, Side::Buy, 100, 40, 2),
        },
    ];

    let file = NamedTempFile::new().unwrap();
    let mut store = EventStore::new();
    store.append_all(events.clone());
    store.save_jsonl(file.path()).unwrap();

    let loaded = EventStore::load_jsonl(file.path()).unwrap();
    let a = replay_events(loaded.events());
    let b = replay_events(&events);
    assert_eq!(a.trades.len(), b.trades.len());
    assert_eq!(a.trades[0].quantity, b.trades[0].quantity);
}
