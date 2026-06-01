//! Trade execution records.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::orders::{OrderId, Price, Quantity, Timestamp};

pub type TradeId = Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trade {
    pub trade_id: TradeId,
    pub buy_order_id: OrderId,
    pub sell_order_id: OrderId,
    pub price: Price,
    pub quantity: Quantity,
    pub timestamp: Timestamp,
}
