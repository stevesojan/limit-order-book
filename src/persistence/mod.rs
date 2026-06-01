//! PostgreSQL persistence and append-only event store.

pub mod events;
pub mod postgres;

pub use events::{BookEvent, EventStore};
pub use postgres::PostgresStore;
