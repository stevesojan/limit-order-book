//! PostgreSQL integration tests (require running Postgres 17).

use lob_engine::matching::MatchingEngine;
use lob_engine::orders::{OrderInput, Side};
use lob_engine::persistence::postgres::PostgresStore;
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires DATABASE_URL and PostgreSQL"]
async fn persist_order_and_trade() {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://lob:lob@localhost:5432/lob".into());
    let store = PostgresStore::connect(&url).await.expect("connect");

    let mut engine = MatchingEngine::new();
    let sell = Uuid::new_v4();
    let buy = Uuid::new_v4();

    engine.process(OrderInput::AddLimit {
        order_id: sell,
        side: Side::Sell,
        price: 100,
        quantity: 50,
        timestamp: 1,
    });

    let order = engine.book().get_order(sell).unwrap().clone();
    store.persist_order(&order).await.expect("order");

    let result = engine.process(OrderInput::AddLimit {
        order_id: buy,
        side: Side::Buy,
        price: 100,
        quantity: 20,
        timestamp: 2,
    });

    assert_eq!(result.trades.len(), 1);
    store.persist_trade(&result.trades[0]).await.expect("trade");
}
