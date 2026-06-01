//! Append-only event sourcing for audit and replay.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use crate::orders::{Order, OrderId, OrderInput, Price, Quantity, Side, Timestamp};
use crate::trades::Trade;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "PascalCase")]
pub enum BookEvent {
    OrderAccepted {
        order: Order,
    },
    OrderModified {
        order_id: OrderId,
        side: Side,
        price: Price,
        quantity: Quantity,
        timestamp: Timestamp,
    },
    OrderCancelled {
        order_id: OrderId,
        timestamp: Timestamp,
    },
    TradeExecuted {
        trade: Trade,
    },
}

impl BookEvent {
    pub fn from_input_and_result(
        input: &OrderInput,
        trades: &[Trade],
        accepted: Option<&Order>,
        cancelled: Option<&Order>,
    ) -> Vec<Self> {
        let mut events = Vec::new();

        if let Some(order) = accepted.filter(|o| o.remaining > 0 && !o.is_market()) {
            events.push(BookEvent::OrderAccepted {
                order: order.clone(),
            });
        }

        if let OrderInput::Modify { .. } = input {
            if let OrderInput::Modify {
                order_id,
                side,
                price,
                quantity,
                timestamp,
            } = input
            {
                events.push(BookEvent::OrderModified {
                    order_id: *order_id,
                    side: *side,
                    price: *price,
                    quantity: *quantity,
                    timestamp: *timestamp,
                });
            }
        }

        if cancelled.is_some() {
            if let OrderInput::Cancel {
                order_id,
                timestamp,
            } = input
            {
                events.push(BookEvent::OrderCancelled {
                    order_id: *order_id,
                    timestamp: *timestamp,
                });
            }
        }

        for trade in trades {
            events.push(BookEvent::TradeExecuted {
                trade: trade.clone(),
            });
        }

        events
    }
}

/// In-memory and file-backed event store.
#[derive(Debug, Default)]
pub struct EventStore {
    events: Vec<BookEvent>,
}

impl EventStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn append(&mut self, event: BookEvent) {
        self.events.push(event);
    }

    pub fn append_all(&mut self, events: impl IntoIterator<Item = BookEvent>) {
        self.events.extend(events);
    }

    pub fn events(&self) -> &[BookEvent] {
        &self.events
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn save_jsonl<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = File::create(path)?;
        for event in &self.events {
            let line = serde_json::to_string(event)?;
            writeln!(file, "{line}")?;
        }
        Ok(())
    }

    pub fn load_jsonl<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::open(path.as_ref())
            .with_context(|| format!("open event log {}", path.as_ref().display()))?;
        let reader = BufReader::new(file);
        let mut events = Vec::new();
        for (i, line) in reader.lines().enumerate() {
            let line = line.with_context(|| format!("read line {}", i + 1))?;
            if line.trim().is_empty() {
                continue;
            }
            let event: BookEvent =
                serde_json::from_str(&line).with_context(|| format!("parse line {}", i + 1))?;
            events.push(event);
        }
        Ok(Self { events })
    }
}
