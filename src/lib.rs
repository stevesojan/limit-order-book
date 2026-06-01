//! Low-latency limit order book matching engine.
//!
//! # Architecture
//!
//! - [`book::OrderBook`] — price levels (`BTreeMap`) + FIFO queues (`VecDeque`)
//! - [`matching::MatchingEngine`] — price-time priority matching
//! - [`persistence`] — PostgreSQL + append-only event log
//! - [`replay`] — deterministic reconstruction from events

pub mod book;
pub mod cli;
pub mod matching;
pub mod orders;
pub mod persistence;
pub mod replay;
pub mod trades;

pub use matching::MatchingEngine;
pub use orders::{Order, OrderId, OrderInput, Side};
pub use trades::Trade;
