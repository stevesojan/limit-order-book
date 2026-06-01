//! Price-time priority matching engine.

use uuid::Uuid;

use crate::book::{OrderBook, PriceLevel};
use crate::orders::{Order, OrderId, OrderInput, Price, Quantity, Side, Timestamp};
use crate::trades::Trade;

/// Result of processing a single command.
#[derive(Debug, Default, Clone)]
pub struct MatchResult {
    pub trades: Vec<Trade>,
    pub accepted_order: Option<Order>,
    pub cancelled_order: Option<Order>,
    pub modified_order: Option<Order>,
}

/// Matching engine wrapping an [`OrderBook`].
#[derive(Debug)]
pub struct MatchingEngine {
    book: OrderBook,
    next_trade_seq: u64,
}

impl MatchingEngine {
    pub fn new() -> Self {
        Self {
            book: OrderBook::new(),
            next_trade_seq: 0,
        }
    }

    pub fn book(&self) -> &OrderBook {
        &self.book
    }

    pub fn book_mut(&mut self) -> &mut OrderBook {
        &mut self.book
    }

    pub fn process(&mut self, input: OrderInput) -> MatchResult {
        match input {
            OrderInput::AddLimit {
                order_id,
                side,
                price,
                quantity,
                timestamp,
            } => self.add_limit(order_id, side, price, quantity, timestamp),
            OrderInput::AddMarket {
                order_id,
                side,
                quantity,
                timestamp,
            } => self.add_market(order_id, side, quantity, timestamp),
            OrderInput::Cancel {
                order_id,
                timestamp,
            } => self.cancel(order_id, timestamp),
            OrderInput::Modify {
                order_id,
                side,
                price,
                quantity,
                timestamp,
            } => self.modify(order_id, side, price, quantity, timestamp),
        }
    }

    fn add_limit(
        &mut self,
        id: OrderId,
        side: Side,
        price: Price,
        quantity: Quantity,
        timestamp: Timestamp,
    ) -> MatchResult {
        let mut incoming = Order::limit(id, side, price, quantity, timestamp);
        let mut trades = Vec::new();
        self.match_incoming(&mut incoming, &mut trades);

        let accepted = if incoming.remaining > 0 && !incoming.cancelled {
            let resting = incoming.clone();
            self.book.insert_resting(resting);
            Some(incoming)
        } else if incoming.remaining > 0 {
            Some(incoming)
        } else {
            None
        };

        MatchResult {
            trades,
            accepted_order: accepted,
            ..Default::default()
        }
    }

    fn add_market(
        &mut self,
        id: OrderId,
        side: Side,
        quantity: Quantity,
        timestamp: Timestamp,
    ) -> MatchResult {
        let mut incoming = Order::market(id, side, quantity, timestamp);
        let mut trades = Vec::new();
        self.match_incoming(&mut incoming, &mut trades);
        // Unfilled market quantity is cancelled (not rested)
        MatchResult {
            trades,
            accepted_order: Some(incoming),
            ..Default::default()
        }
    }

    fn cancel(&mut self, order_id: OrderId, _timestamp: Timestamp) -> MatchResult {
        let cancelled = self.book.cancel_order(order_id);
        MatchResult {
            cancelled_order: cancelled,
            ..Default::default()
        }
    }

    fn modify(
        &mut self,
        order_id: OrderId,
        side: Side,
        price: Price,
        quantity: Quantity,
        timestamp: Timestamp,
    ) -> MatchResult {
        let (old_ts, old_price) = self
            .book
            .get_order(order_id)
            .map(|o| (o.timestamp, o.price))
            .unwrap_or((timestamp, Some(price)));

        let mut result = self.cancel(order_id, timestamp);
        let effective_ts = if old_price == Some(price) {
            old_ts
        } else {
            timestamp
        };

        let add = self.add_limit(order_id, side, price, quantity, effective_ts);
        result.trades = add.trades;
        result.modified_order = add.accepted_order.clone();
        result.accepted_order = add.accepted_order;
        result
    }

    /// Match incoming order against contra side while prices cross.
    fn match_incoming(&mut self, incoming: &mut Order, trades: &mut Vec<Trade>) {
        loop {
            if incoming.remaining == 0 {
                break;
            }

            let contra = incoming.side.opposite();
            let best = self.book.best_price(contra);
            let Some(best_price) = best else {
                break;
            };

            if !Self::prices_cross(incoming, best_price) {
                break;
            }

            let maker_id = self
                .book
                .front_active(contra, best_price)
                .expect("best price implies active order");

            let (maker_price, maker_remaining, maker_ts) = {
                let maker = self.book.get_order(maker_id).expect("maker must exist");
                (
                    maker.price.expect("resting limit"),
                    maker.remaining,
                    maker.timestamp,
                )
            };

            let fill_qty = incoming.remaining.min(maker_remaining);
            let trade_price = maker_price;
            let trade_ts = incoming.timestamp.max(maker_ts);

            let (buy_id, sell_id) = match incoming.side {
                Side::Buy => (incoming.id, maker_id),
                Side::Sell => (maker_id, incoming.id),
            };

            trades.push(self.make_trade(buy_id, sell_id, trade_price, fill_qty, trade_ts));

            incoming.remaining -= fill_qty;
            if let Some(remaining) = self.book.reduce_remaining(maker_id, fill_qty) {
                if remaining == 0 {
                    self.book.remove_filled_order(maker_id);
                }
            }
            self.book.prune_inactive_head(contra, best_price);
        }
    }

    fn prices_cross(incoming: &Order, best_contra: Price) -> bool {
        match (incoming.side, incoming.price) {
            (Side::Buy, Some(limit)) => best_contra <= limit,
            (Side::Buy, None) => true,
            (Side::Sell, Some(limit)) => best_contra >= limit,
            (Side::Sell, None) => true,
        }
    }

    fn make_trade(
        &mut self,
        buy_id: OrderId,
        sell_id: OrderId,
        price: Price,
        quantity: Quantity,
        timestamp: Timestamp,
    ) -> Trade {
        self.next_trade_seq += 1;
        Trade {
            trade_id: Uuid::new_v4(),
            buy_order_id: buy_id,
            sell_order_id: sell_id,
            price,
            quantity,
            timestamp,
        }
    }

    pub fn best_bid(&self) -> Option<Price> {
        self.book.best_bid()
    }

    pub fn best_ask(&self) -> Option<Price> {
        self.book.best_ask()
    }

    pub fn spread(&self) -> Option<i64> {
        self.book.spread()
    }

    pub fn top_bids(&self, n: usize) -> Vec<PriceLevel> {
        self.book.top_bids(n)
    }

    pub fn top_asks(&self, n: usize) -> Vec<PriceLevel> {
        self.book.top_asks(n)
    }
}

impl Default for MatchingEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn ts(n: i64) -> i64 {
        n
    }

    #[test]
    fn price_time_priority_same_price() {
        let mut engine = MatchingEngine::new();
        let buy1 = Uuid::new_v4();
        let buy2 = Uuid::new_v4();
        let sell = Uuid::new_v4();

        engine.process(OrderInput::AddLimit {
            order_id: buy1,
            side: Side::Buy,
            price: 101,
            quantity: 100,
            timestamp: ts(1),
        });
        engine.process(OrderInput::AddLimit {
            order_id: buy2,
            side: Side::Buy,
            price: 101,
            quantity: 100,
            timestamp: ts(2),
        });

        let result = engine.process(OrderInput::AddLimit {
            order_id: sell,
            side: Side::Sell,
            price: 101,
            quantity: 150,
            timestamp: ts(3),
        });

        assert_eq!(result.trades.len(), 2);
        assert_eq!(result.trades[0].quantity, 100);
        assert_eq!(result.trades[0].buy_order_id, buy1);
        assert_eq!(result.trades[1].quantity, 50);
        assert_eq!(result.trades[1].buy_order_id, buy2);

        let remaining = engine.book().get_order(buy2).unwrap();
        assert_eq!(remaining.remaining, 50);
    }

    #[test]
    fn match_across_levels() {
        let mut engine = MatchingEngine::new();

        engine.process(OrderInput::AddLimit {
            order_id: Uuid::new_v4(),
            side: Side::Sell,
            price: 100,
            quantity: 50,
            timestamp: ts(1),
        });
        engine.process(OrderInput::AddLimit {
            order_id: Uuid::new_v4(),
            side: Side::Sell,
            price: 101,
            quantity: 50,
            timestamp: ts(2),
        });

        let result = engine.process(OrderInput::AddLimit {
            order_id: Uuid::new_v4(),
            side: Side::Buy,
            price: 101,
            quantity: 70,
            timestamp: ts(3),
        });

        assert_eq!(result.trades.len(), 2);
        assert_eq!(result.trades[0].price, 100);
        assert_eq!(result.trades[0].quantity, 50);
        assert_eq!(result.trades[1].price, 101);
        assert_eq!(result.trades[1].quantity, 20);
        assert_eq!(engine.book().best_ask(), Some(101));
    }

    #[test]
    fn market_order_no_rest() {
        let mut engine = MatchingEngine::new();

        engine.process(OrderInput::AddLimit {
            order_id: Uuid::new_v4(),
            side: Side::Sell,
            price: 100,
            quantity: 30,
            timestamp: ts(1),
        });

        let result = engine.process(OrderInput::AddMarket {
            order_id: Uuid::new_v4(),
            side: Side::Buy,
            quantity: 100,
            timestamp: ts(2),
        });

        assert_eq!(result.trades.len(), 1);
        assert_eq!(result.trades[0].quantity, 30);
        assert_eq!(result.accepted_order.as_ref().unwrap().remaining, 70);
        assert!(engine.book().order_count() <= 1);
    }
}
