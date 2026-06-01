//! Command-line interface (Clap).

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use uuid::Uuid;

use crate::matching::MatchingEngine;
use crate::orders::{OrderInput, Side};
use crate::persistence::events::{BookEvent, EventStore};
use crate::persistence::postgres::{default_database_url, PostgresStore};
use crate::replay::{replay_commands, replay_from_file};

#[derive(Parser, Debug)]
#[command(
    name = "lob-engine",
    about = "Low-latency limit order book matching engine"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// PostgreSQL connection URL
    #[arg(long, env = "DATABASE_URL")]
    pub database_url: Option<String>,

    /// Append events to JSONL file
    #[arg(long, default_value = "data/events.jsonl")]
    pub event_log: PathBuf,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Add a limit order
    AddOrder {
        #[arg(long)]
        side: SideArg,
        #[arg(long)]
        price: i64,
        #[arg(long)]
        quantity: u64,
        #[arg(long)]
        order_id: Option<Uuid>,
        #[arg(long, default_value_t = default_timestamp())]
        timestamp: i64,
        #[arg(long, default_value = "limit")]
        order_type: OrderTypeArg,
    },
    /// Cancel a resting order
    CancelOrder {
        #[arg(long)]
        order_id: Uuid,
        #[arg(long, default_value_t = default_timestamp())]
        timestamp: i64,
    },
    /// Modify a resting order (cancel + replace)
    ModifyOrder {
        #[arg(long)]
        order_id: Uuid,
        #[arg(long)]
        side: SideArg,
        #[arg(long)]
        price: i64,
        #[arg(long)]
        quantity: u64,
        #[arg(long, default_value_t = default_timestamp())]
        timestamp: i64,
    },
    /// Replay events or commands from JSON file
    Replay {
        /// JSON array of commands or JSONL events
        file: PathBuf,
        #[arg(long)]
        commands: bool,
    },
    /// Print book snapshot
    Snapshot {
        #[arg(long, default_value_t = 5)]
        depth: usize,
    },
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum SideArg {
    Buy,
    Sell,
}

impl From<SideArg> for Side {
    fn from(s: SideArg) -> Self {
        match s {
            SideArg::Buy => Side::Buy,
            SideArg::Sell => Side::Sell,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, clap::ValueEnum)]
pub enum OrderTypeArg {
    #[default]
    Limit,
    Market,
}

fn default_timestamp() -> i64 {
    chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
}

pub async fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::AddOrder {
            side,
            price,
            quantity,
            order_id,
            timestamp,
            ref order_type,
        } => {
            let id = order_id.unwrap_or_else(Uuid::new_v4);
            let input = match order_type {
                OrderTypeArg::Limit => OrderInput::AddLimit {
                    order_id: id,
                    side: side.into(),
                    price,
                    quantity,
                    timestamp,
                },
                OrderTypeArg::Market => OrderInput::AddMarket {
                    order_id: id,
                    side: side.into(),
                    quantity,
                    timestamp,
                },
            };
            let mut service = build_service(&cli).await?;
            let result = service.process(input).await?;
            print_match_result(&result);
        }
        Commands::CancelOrder {
            order_id,
            timestamp,
        } => {
            let mut service = build_service(&cli).await?;
            let result = service
                .process(OrderInput::Cancel {
                    order_id,
                    timestamp,
                })
                .await?;
            print_match_result(&result);
        }
        Commands::ModifyOrder {
            order_id,
            ref side,
            price,
            quantity,
            timestamp,
        } => {
            let mut service = build_service(&cli).await?;
            let result = service
                .process(OrderInput::Modify {
                    order_id,
                    side: (*side).into(),
                    price,
                    quantity,
                    timestamp,
                })
                .await?;
            print_match_result(&result);
        }
        Commands::Replay { file, commands } => {
            tracing::info!(path = %file.display(), "Replay started");
            if commands {
                let data = std::fs::read_to_string(&file)?;
                let inputs: Vec<OrderInput> = serde_json::from_str(&data)?;
                let (engine, trades) = replay_commands(&inputs);
                for t in &trades {
                    println!(
                        "TRADE {} buy={} sell={} price={} qty={}",
                        t.trade_id, t.buy_order_id, t.sell_order_id, t.price, t.quantity
                    );
                }
                println!("Replayed {} commands", inputs.len());
                println!("Trades: {}", trades.len());
                println!("Best bid: {:?}", engine.best_bid());
                println!("Best ask: {:?}", engine.best_ask());
            } else {
                let out = replay_from_file(&file)?;
                println!("Replayed {} events", out.event_count);
                println!("Trades reconstructed: {}", out.trades.len());
            }
            tracing::info!("Replay finished");
        }
        Commands::Snapshot { depth } => {
            let service = build_service(&cli).await?;
            let engine = service.engine();
            println!("Best bid: {:?}", engine.best_bid());
            println!("Best ask: {:?}", engine.best_ask());
            println!("Spread: {:?}", engine.spread());
            println!("Bids:");
            for lvl in engine.top_bids(depth) {
                println!("  {} -> {}", lvl.price, lvl.quantity);
            }
            println!("Asks:");
            for lvl in engine.top_asks(depth) {
                println!("  {} -> {}", lvl.price, lvl.quantity);
            }
        }
    }
    Ok(())
}

async fn build_service(cli: &Cli) -> Result<EngineService> {
    let url = cli
        .database_url
        .clone()
        .unwrap_or_else(default_database_url);

    let pg = if std::env::var("LOB_SKIP_DB").is_ok() {
        None
    } else {
        match PostgresStore::connect(&url).await {
            Ok(store) => Some(store),
            Err(e) => {
                tracing::warn!(error = %e, "PostgreSQL unavailable; running without persistence");
                None
            }
        }
    };

    Ok(EngineService::new(pg, cli.event_log.clone()))
}

fn print_match_result(result: &crate::matching::MatchResult) {
    for t in &result.trades {
        println!(
            "TRADE {} buy={} sell={} price={} qty={}",
            t.trade_id, t.buy_order_id, t.sell_order_id, t.price, t.quantity
        );
    }
    if let Some(o) = &result.accepted_order {
        println!("ORDER {} {:?} remaining={}", o.id, o.side, o.remaining);
    }
    if result.cancelled_order.is_some() {
        println!("CANCELLED");
    }
}

/// Orchestrates matching, events, and optional PostgreSQL.
pub struct EngineService {
    engine: MatchingEngine,
    events: EventStore,
    postgres: Option<PostgresStore>,
    event_log_path: PathBuf,
    event_seq: i64,
}

impl EngineService {
    pub fn new(postgres: Option<PostgresStore>, event_log_path: PathBuf) -> Self {
        Self {
            engine: MatchingEngine::new(),
            events: EventStore::new(),
            postgres,
            event_log_path,
            event_seq: 0,
        }
    }

    pub fn engine(&self) -> &MatchingEngine {
        &self.engine
    }

    pub async fn process(&mut self, input: OrderInput) -> Result<crate::matching::MatchResult> {
        tracing::info!(?input, "Order received");
        let result = self.engine.process(input.clone());

        for trade in &result.trades {
            tracing::info!(
                trade_id = %trade.trade_id,
                price = trade.price,
                quantity = trade.quantity,
                "Trade generated"
            );
            if let Some(pg) = &self.postgres {
                pg.persist_trade(trade).await?;
            }
            self.events.append(BookEvent::TradeExecuted {
                trade: trade.clone(),
            });
            self.persist_event_to_db(BookEvent::TradeExecuted {
                trade: trade.clone(),
            })
            .await?;
        }

        if let Some(order) = &result.accepted_order {
            if let Some(pg) = &self.postgres {
                pg.persist_order(order).await?;
            }
            if order.remaining > 0 && order.price.is_some() {
                self.events.append(BookEvent::OrderAccepted {
                    order: order.clone(),
                });
                self.persist_event_to_db(BookEvent::OrderAccepted {
                    order: order.clone(),
                })
                .await?;
            }
        }

        if let OrderInput::Modify { .. } = &input {
            if let OrderInput::Modify {
                order_id,
                side,
                price,
                quantity,
                timestamp,
            } = &input
            {
                let ev = BookEvent::OrderModified {
                    order_id: *order_id,
                    side: *side,
                    price: *price,
                    quantity: *quantity,
                    timestamp: *timestamp,
                };
                self.events.append(ev.clone());
                self.persist_event_to_db(ev).await?;
            }
        }

        if result.cancelled_order.is_some() {
            if let OrderInput::Cancel {
                order_id,
                timestamp,
            } = &input
            {
                tracing::info!(%order_id, "Order cancelled");
                let ev = BookEvent::OrderCancelled {
                    order_id: *order_id,
                    timestamp: *timestamp,
                };
                self.events.append(ev.clone());
                self.persist_event_to_db(ev).await?;
            }
        }

        for trade in &result.trades {
            let _ = trade;
            tracing::info!(count = result.trades.len(), "Order matched");
        }

        self.events.save_jsonl(&self.event_log_path)?;
        Ok(result)
    }

    async fn persist_event_to_db(&mut self, event: BookEvent) -> Result<()> {
        if let Some(pg) = &self.postgres {
            self.event_seq += 1;
            pg.persist_event(self.event_seq, &event).await?;
        }
        Ok(())
    }
}
