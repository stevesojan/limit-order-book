//! Order types and identifiers.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type OrderId = Uuid;
pub type Price = i64;
pub type Quantity = u64;
pub type Timestamp = i64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Buy,
    Sell,
}

impl Side {
    pub fn as_str(self) -> &'static str {
        match self {
            Side::Buy => "buy",
            Side::Sell => "sell",
        }
    }

    pub fn opposite(self) -> Self {
        match self {
            Side::Buy => Side::Sell,
            Side::Sell => Side::Buy,
        }
    }
}

/// Resting or in-flight order state inside the book.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Order {
    pub id: OrderId,
    pub side: Side,
    /// `None` for market orders (never rest).
    pub price: Option<Price>,
    pub original_quantity: Quantity,
    pub remaining: Quantity,
    pub timestamp: Timestamp,
    pub cancelled: bool,
}

impl Order {
    pub fn limit(
        id: OrderId,
        side: Side,
        price: Price,
        quantity: Quantity,
        timestamp: Timestamp,
    ) -> Self {
        Self {
            id,
            side,
            price: Some(price),
            original_quantity: quantity,
            remaining: quantity,
            timestamp,
            cancelled: false,
        }
    }

    pub fn market(id: OrderId, side: Side, quantity: Quantity, timestamp: Timestamp) -> Self {
        Self {
            id,
            side,
            price: None,
            original_quantity: quantity,
            remaining: quantity,
            timestamp,
            cancelled: false,
        }
    }

    pub fn is_market(&self) -> bool {
        self.price.is_none()
    }
}

/// Incoming command to the matching engine (CLI / replay / API).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case")]
pub enum OrderInput {
    AddLimit {
        order_id: OrderId,
        side: Side,
        price: Price,
        quantity: Quantity,
        timestamp: Timestamp,
    },
    AddMarket {
        order_id: OrderId,
        side: Side,
        quantity: Quantity,
        timestamp: Timestamp,
    },
    Cancel {
        order_id: OrderId,
        timestamp: Timestamp,
    },
    Modify {
        order_id: OrderId,
        side: Side,
        price: Price,
        quantity: Quantity,
        timestamp: Timestamp,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderLocation {
    pub side: Side,
    pub price: Price,
}
