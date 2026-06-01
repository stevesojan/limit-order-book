//! SQLx PostgreSQL integration.

use crate::orders::{Order, Side};
use crate::persistence::events::BookEvent;
use crate::trades::Trade;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

pub struct PostgresStore {
    pool: PgPool,
}

impl PostgresStore {
    pub async fn connect(database_url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(database_url)
            .await
            .context("connect to PostgreSQL")?;

        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .context("run migrations")?;

        Ok(Self { pool })
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn persist_order(&self, order: &Order) -> Result<()> {
        let side = order.side.as_str();
        let price: Option<i64> = order.price;
        let ts = timestamp_to_dt(order.timestamp)?;

        sqlx::query(
            r#"
            INSERT INTO orders (order_id, side, price, quantity, timestamp)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (order_id) DO UPDATE
            SET side = EXCLUDED.side,
                price = EXCLUDED.price,
                quantity = EXCLUDED.quantity,
                timestamp = EXCLUDED.timestamp
            "#,
        )
        .bind(order.id)
        .bind(side)
        .bind(price)
        .bind(order.original_quantity as i64)
        .bind(ts)
        .execute(&self.pool)
        .await
        .context("insert order")?;
        Ok(())
    }

    pub async fn persist_trade(&self, trade: &Trade) -> Result<()> {
        let ts = timestamp_to_dt(trade.timestamp)?;
        sqlx::query(
            r#"
            INSERT INTO trades (trade_id, buy_order_id, sell_order_id, price, quantity, timestamp)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (trade_id) DO NOTHING
            "#,
        )
        .bind(trade.trade_id)
        .bind(trade.buy_order_id)
        .bind(trade.sell_order_id)
        .bind(trade.price)
        .bind(trade.quantity as i64)
        .bind(ts)
        .execute(&self.pool)
        .await
        .context("insert trade")?;
        Ok(())
    }

    pub async fn persist_event(&self, seq: i64, event: &BookEvent) -> Result<()> {
        let payload = serde_json::to_value(event)?;
        sqlx::query(
            r#"
            INSERT INTO events (seq, event_type, payload)
            VALUES ($1, $2, $3)
            ON CONFLICT (seq) DO NOTHING
            "#,
        )
        .bind(seq)
        .bind(event_type_name(event))
        .bind(payload)
        .execute(&self.pool)
        .await
        .context("insert event")?;
        Ok(())
    }

    pub async fn load_events(&self) -> Result<Vec<(i64, BookEvent)>> {
        let rows = sqlx::query(
            r#"
            SELECT seq, payload FROM events ORDER BY seq ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            use sqlx::Row;
            let seq: i64 = row.get("seq");
            let payload: serde_json::Value = row.get("payload");
            let event: BookEvent = serde_json::from_value(payload)?;
            out.push((seq, event));
        }
        Ok(out)
    }
}

fn event_type_name(event: &BookEvent) -> &'static str {
    match event {
        BookEvent::OrderAccepted { .. } => "OrderAccepted",
        BookEvent::OrderModified { .. } => "OrderModified",
        BookEvent::OrderCancelled { .. } => "OrderCancelled",
        BookEvent::TradeExecuted { .. } => "TradeExecuted",
    }
}

fn timestamp_to_dt(ts: i64) -> Result<DateTime<Utc>> {
    DateTime::from_timestamp(ts / 1_000_000_000, (ts % 1_000_000_000) as u32)
        .or_else(|| DateTime::from_timestamp(ts, 0))
        .context("invalid timestamp")
}

pub fn side_from_str(s: &str) -> Option<Side> {
    match s {
        "buy" => Some(Side::Buy),
        "sell" => Some(Side::Sell),
        _ => None,
    }
}

pub fn default_database_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://lob:lob@localhost:5432/lob".to_string())
}
