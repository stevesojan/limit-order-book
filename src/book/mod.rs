//! Order book: `BTreeMap<Price, VecDeque<OrderId>>` with O(1) order lookup index.
//!
//! # Complexity
//!
//! | Operation              | Complexity                          |
//! |------------------------|-------------------------------------|
//! | Best bid/ask           | O(1) amortized (`BTreeMap` ends)    |
//! | Insert at price level  | O(log P) + O(1) queue push          |
//! | Match (pop front)      | O(log P) when level empties         |
//! | Cancel / modify lookup | O(1) `HashMap` → O(K) queue remove  |
//! | Top-N snapshot         | O(N) levels, no full book scan      |
//!
//! Cancel removes by scanning the price-level queue (K = orders at level).
//! Matching only touches the queue head — strict FIFO at each level.

use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap, VecDeque};

use crate::orders::{Order, OrderId, OrderLocation, Price, Quantity, Side};

/// Aggregated size at a price level (snapshot API).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriceLevel {
    pub price: Price,
    pub quantity: Quantity,
}

/// Location of a resting order for O(1) index lookup (then O(K) removal at level).
#[derive(Debug, Clone)]
struct Location {
    side: Side,
    price: Price,
}

/// In-memory limit order book.
#[derive(Debug, Default)]
pub struct OrderBook {
    /// Bids: highest price first (`Reverse` key).
    bids: BTreeMap<Reverse<Price>, VecDeque<OrderId>>,
    /// Asks: lowest price first.
    asks: BTreeMap<Price, VecDeque<OrderId>>,
    orders: HashMap<OrderId, Order>,
    locations: HashMap<OrderId, Location>,
}

impl OrderBook {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn order_count(&self) -> usize {
        self.orders.len()
    }

    pub fn get_order(&self, id: OrderId) -> Option<&Order> {
        self.orders.get(&id)
    }

    pub fn location(&self, id: OrderId) -> Option<OrderLocation> {
        self.locations.get(&id).map(|loc| OrderLocation {
            side: loc.side,
            price: loc.price,
        })
    }

    /// Insert resting limit order at tail of FIFO queue. O(log P) + O(1).
    pub fn insert_resting(&mut self, order: Order) {
        debug_assert!(order.price.is_some());
        debug_assert!(!order.cancelled);
        debug_assert!(order.remaining > 0);

        let price = order.price.expect("limit order");
        let id = order.id;
        let side = order.side;

        let queue = match side {
            Side::Buy => self.bids.entry(Reverse(price)).or_default(),
            Side::Sell => self.asks.entry(price).or_default(),
        };
        queue.push_back(id);

        self.locations.insert(id, Location { side, price });
        self.orders.insert(id, order);
    }

    /// Remove empty price level after last order consumed. O(log P).
    fn maybe_remove_level(&mut self, side: Side, price: Price) {
        let empty = match side {
            Side::Buy => self.bids.get(&Reverse(price)).is_none_or(|q| q.is_empty()),
            Side::Sell => self.asks.get(&price).is_none_or(|q| q.is_empty()),
        };
        if empty {
            match side {
                Side::Buy => {
                    self.bids.remove(&Reverse(price));
                }
                Side::Sell => {
                    self.asks.remove(&price);
                }
            }
        }
    }

    /// Front of FIFO queue at price, skipping cancelled tombstones. Does not remove. O(1) amortized.
    pub fn front_active(&self, side: Side, price: Price) -> Option<OrderId> {
        let queue = match side {
            Side::Buy => self.bids.get(&Reverse(price))?,
            Side::Sell => self.asks.get(&price)?,
        };
        queue.iter().find(|&&id| self.is_active(id)).copied()
    }

    /// Remove inactive orders from queue head (cancelled tombstones). O(1) amortized per tombstone.
    pub fn prune_inactive_head(&mut self, side: Side, price: Price) {
        loop {
            let head_id = match side {
                Side::Buy => self
                    .bids
                    .get(&Reverse(price))
                    .and_then(|q| q.front().copied()),
                Side::Sell => self.asks.get(&price).and_then(|q| q.front().copied()),
            };
            let Some(id) = head_id else { break };
            if self.is_active(id) {
                break;
            }
            let queue = match side {
                Side::Buy => self.bids.get_mut(&Reverse(price)),
                Side::Sell => self.asks.get_mut(&price),
            };
            if let Some(queue) = queue {
                queue.pop_front();
                if queue.is_empty() {
                    self.maybe_remove_level(side, price);
                }
            }
            self.locations.remove(&id);
            self.orders.remove(&id);
        }
    }

    /// Remove order from queue after full fill. O(K) worst case at level.
    pub fn remove_filled_order(&mut self, id: OrderId) {
        let Some(loc) = self.locations.remove(&id) else {
            return;
        };
        self.remove_from_queue(loc.side, loc.price, id);
        self.orders.remove(&id);
    }

    /// Peek best active order on a side without removing. O(1) amortized.
    pub fn best_price(&self, side: Side) -> Option<Price> {
        match side {
            Side::Buy => {
                for (&Reverse(price), queue) in &self.bids {
                    if queue.iter().any(|id| self.is_active(*id)) {
                        return Some(price);
                    }
                }
                None
            }
            Side::Sell => {
                for (&price, queue) in &self.asks {
                    if queue.iter().any(|id| self.is_active(*id)) {
                        return Some(price);
                    }
                }
                None
            }
        }
    }

    fn is_active(&self, id: OrderId) -> bool {
        self.orders
            .get(&id)
            .is_some_and(|o| !o.cancelled && o.remaining > 0)
    }

    pub fn best_bid(&self) -> Option<Price> {
        self.best_price(Side::Buy)
    }

    pub fn best_ask(&self) -> Option<Price> {
        self.best_price(Side::Sell)
    }

    pub fn spread(&self) -> Option<i64> {
        match (self.best_bid(), self.best_ask()) {
            (Some(bid), Some(ask)) => Some(ask - bid),
            _ => None,
        }
    }

    /// Top N bid levels (highest first). O(N) levels.
    pub fn top_bids(&self, n: usize) -> Vec<PriceLevel> {
        self.top_levels(Side::Buy, n)
    }

    /// Top N ask levels (lowest first). O(N) levels.
    pub fn top_asks(&self, n: usize) -> Vec<PriceLevel> {
        self.top_levels(Side::Sell, n)
    }

    fn top_levels(&self, side: Side, n: usize) -> Vec<PriceLevel> {
        let mut out = Vec::with_capacity(n.min(64));

        match side {
            Side::Buy => {
                for (&Reverse(price), queue) in &self.bids {
                    let qty = self.aggregate_level(queue);
                    if qty > 0 {
                        out.push(PriceLevel {
                            price,
                            quantity: qty,
                        });
                        if out.len() >= n {
                            break;
                        }
                    }
                }
            }
            Side::Sell => {
                for (&price, queue) in &self.asks {
                    let qty = self.aggregate_level(queue);
                    if qty > 0 {
                        out.push(PriceLevel {
                            price,
                            quantity: qty,
                        });
                        if out.len() >= n {
                            break;
                        }
                    }
                }
            }
        }
        out
    }

    fn aggregate_level(&self, queue: &VecDeque<OrderId>) -> Quantity {
        queue
            .iter()
            .filter_map(|id| self.orders.get(id))
            .filter(|o| !o.cancelled && o.remaining > 0)
            .map(|o| o.remaining)
            .sum()
    }

    /// Mark order cancelled; remove from location index. Lazy removal from deque on match. O(1) map + O(K) scan.
    pub fn cancel_order(&mut self, id: OrderId) -> Option<Order> {
        let loc = self.locations.remove(&id)?;
        let order = self.orders.get_mut(&id)?;
        order.cancelled = true;
        let removed = order.clone();

        // Eagerly remove from queue to keep snapshots accurate
        self.remove_from_queue(loc.side, loc.price, id);
        self.orders.remove(&id);
        Some(removed)
    }

    fn remove_from_queue(&mut self, side: Side, price: Price, id: OrderId) {
        let queue = match side {
            Side::Buy => self.bids.get_mut(&Reverse(price)),
            Side::Sell => self.asks.get_mut(&price),
        };
        if let Some(queue) = queue {
            queue.retain(|&oid| oid != id);
            if queue.is_empty() {
                self.maybe_remove_level(side, price);
            }
        }
    }

    /// Update remaining quantity after partial fill. O(1).
    pub fn reduce_remaining(&mut self, id: OrderId, filled: Quantity) -> Option<Quantity> {
        let order = self.orders.get_mut(&id)?;
        order.remaining = order.remaining.saturating_sub(filled);
        Some(order.remaining)
    }

    pub fn orders_map(&self) -> &HashMap<OrderId, Order> {
        &self.orders
    }
}

#[cfg(test)]
mod book_tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn bid_priority_highest_first() {
        let mut book = OrderBook::new();
        book.insert_resting(Order::limit(Uuid::new_v4(), Side::Buy, 100, 10, 1));
        book.insert_resting(Order::limit(Uuid::new_v4(), Side::Buy, 101, 20, 2));
        assert_eq!(book.best_bid(), Some(101));
        let levels = book.top_bids(2);
        assert_eq!(levels[0].price, 101);
        assert_eq!(levels[1].price, 100);
    }
}
