# Limit Order Book (LOB) Engine

Production-oriented, low-latency matching engine in Rust with **strict price-time priority**, PostgreSQL persistence, append-only event sourcing, deterministic replay, Criterion benchmarks, and Behave BDD acceptance tests.

Runs natively on **Linux** and **Windows**.

## Architecture

```mermaid
flowchart TB
    CLI[CLI / Clap] --> SVC[EngineService]
    SVC --> ME[MatchingEngine]
    ME --> MAP["HashMap InstrumentId → InstrumentMatcher"]
    MAP --> OB[OrderBook per instrument]
    OB --> BIDS["BTreeMap Reverse Price → VecDeque OrderId"]
    OB --> ASKS["BTreeMap Price → VecDeque OrderId"]
    OB --> IDX["HashMap OrderId → Location"]
    ME --> OIDX["HashMap OrderId → InstrumentId"]
    SVC --> CMD[data/commands.jsonl]
    SVC --> ES[data/events.jsonl]
    SVC --> PG[(PostgreSQL 17)]
    CMD --> REPLAY[Replay Engine]
    REPLAY --> ME
```

## Multi-instrument design

Each symbol (`AAPL`, `MSFT`, `BTCUSD`, …) has an **independent** [`OrderBook`](src/book/mod.rs). The [`MatchingEngine`](src/matching/mod.rs) routes commands with **O(1)** `HashMap<InstrumentId, InstrumentMatcher>` lookup; matching logic inside each book is unchanged.

| Layer | Lookup | Matching |
|-------|--------|----------|
| Engine | O(1) by `instrument_id` | Only within that book |
| Book | O(log P) price levels | Price-time FIFO per level |

Orders **never** match across instruments: `BUY AAPL` vs `SELL MSFT` at the same price produces no trade.

**Performance:** Instrument routing is one `HashMap` get per command—negligible vs book operations. Benchmarks include 1 / 10 / 100 instruments (`cargo bench`, group `multi_instrument_insert`).

## Data Structures & Complexity

| Structure | Role | Why |
|-----------|------|-----|
| `BTreeMap<Reverse<Price>, VecDeque<OrderId>>` | Bids | O(log P) best bid, ascending iteration via `Reverse` |
| `BTreeMap<Price, VecDeque<OrderId>>` | Asks | O(log P) best ask |
| `VecDeque<OrderId>` | FIFO per level | O(1) tail insert, O(1) head match |
| `HashMap<OrderId, Order>` | Order state | O(1) quantity updates |
| `HashMap<OrderId, Location>` | Cancel/modify index | O(1) lookup → O(K) queue removal at level |

| Operation | Complexity |
|-----------|------------|
| Add limit (no match) | O(log P) |
| Match against front | O(1) per fill at level + O(log P) if level empties |
| Best bid/ask | O(L) scan of stale heads; typically O(1) |
| Cancel | O(1) lookup + O(K) retain at price level |
| Top-N snapshot | O(N) levels |

**Partial fills** update quantity in place at the front of the FIFO queue (orders are not popped until fully filled).

## Setup (Ubuntu 24.04)

```bash
# Rust stable
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# PostgreSQL 17
sudo apt update
sudo apt install -y postgresql-17 postgresql-client-17
sudo -u postgres psql -c "CREATE USER lob WITH PASSWORD 'lob' CREATEDB;"
sudo -u postgres psql -c "CREATE DATABASE lob OWNER lob;"

export DATABASE_URL=postgres://lob:lob@localhost:5432/lob

# Build & test
git clone <repo>
cd "Limit Order Book"
cargo build --release
cargo test
```

### Windows

Install [Rust](https://rustup.rs) and PostgreSQL 17, then:

```powershell
$env:DATABASE_URL = "postgres://lob:lob@localhost:5432/lob"
cargo build --release
cargo test
```

Skip database for local dev:

```powershell
$env:LOB_SKIP_DB = "1"
```

## CLI state between runs

Each `cargo run` is a **new process**. The engine reloads the book from **`data/commands.jsonl`** (append-only command log) on startup, then applies your new command. Audit events append to **`data/events.jsonl`**.

To reset local state:

```bash
rm -f data/commands.jsonl data/events.jsonl   # Linux
del data\commands.jsonl data\events.jsonl      # Windows
```

## CLI Examples

```bash
# Limit buy on AAPL
cargo run -- add-order --instrument-id AAPL --side buy --price 101 --quantity 100

# Limit sell on same instrument (matches if price crosses)
cargo run -- add-order --instrument-id AAPL --side sell --price 101 --quantity 50

# Different instrument — no cross-match
cargo run -- add-order --instrument-id MSFT --side buy --price 300 --quantity 100

# Market sell
cargo run -- add-order --instrument-id AAPL --side sell --order-type market --quantity 50

# Cancel
cargo run -- cancel-order --order-id <UUID>

# Modify (cancel + replace; keeps timestamp if price unchanged)
cargo run -- modify-order --instrument-id AAPL --order-id <UUID> --side buy --price 101 --quantity 200

# Snapshot for one instrument only
cargo run -- snapshot --instrument-id AAPL --depth 5

# Replay command log (authoritative for determinism)
cargo run -- replay examples/commands_price_time.json --commands

# Replay event JSONL
cargo run -- replay data/events.jsonl

# Structured logs
RUST_LOG=info cargo run -- add-order --side buy --price 100 --quantity 10
```

## Business Rules (Price-Time)

- **Buy**: higher price first; then earlier timestamp.
- **Sell**: lower price first; then earlier timestamp.
- **Market orders**: match immediately; never rest; unfilled qty cancelled.

Example (from spec):

```
BUY: 100@101 (t=1), 100@101 (t=2)
SELL: 150@101 (t=3)
→ Trade 100 with first buy; second buy has 50 remaining
```

## Event Sourcing

Append-only events (`data/events.jsonl`):

- `OrderAccepted`
- `OrderModified`
- `OrderCancelled`
- `TradeExecuted`

PostgreSQL tables: `orders`, `trades`, `events` (see `migrations/`).

**Replay**: use `--commands` JSON for bit-identical reconstruction; event replay re-applies resting `OrderAccepted` states.

## Benchmarks

```bash
cargo bench
# HTML report: target/criterion/report/index.html
```

Measures: insert, match burst, cancel, replay (5k commands). Release profile uses LTO.

Example interpretation (release, modern CPU):

| Benchmark | Typical throughput |
|-----------|-------------------|
| Insert 10k limits | 100k–500k orders/sec |
| Match burst | depends on trade count |
| Replay 5k | 200k+ commands/sec |

Run stress tests:

```bash
cargo test --test stress --release -- --ignored
```

## Memory & Performance Notes

- **Allocations**: `Uuid::new_v4()` per order/trade; hot path uses `Vec` growth for trades only.
- **Clones**: `Order::clone` on cancel path; minimize in production with `Arc` or arena allocators.
- **Cache locality**: `VecDeque` + contiguous `Order` map is cache-friendlier than pointer-heavy intrusive lists.
- **Future optimizations**: slab allocator for orders, fixed-point prices as `i32`, single-threaded lock-free book, `mmap` event log, CPU pinning, SIMD not applicable here.

### Trade-offs

| Choice | Pros | Cons |
|--------|------|------|
| `BTreeMap` vs hash for prices | Ordered iteration for snapshots | O(log P) vs O(1) |
| `VecDeque` vs linked list | Cache locality, simple | O(K) cancel at level |
| Tombstone cancel | O(1) mark | Was replaced by eager queue retain |

## Testing

```bash
cargo test                    # unit + integration + proptest
cargo test --test stress --release -- --ignored
cargo test --test postgres_integration -- --ignored  # needs Postgres

# BDD (Python 3 + behave)
cargo build --release
pip install behave
cd tests/bdd && behave features
```

## CI

GitHub Actions (`.github/workflows/ci.yml`):

- `cargo fmt --check`
- `cargo clippy -- -D warnings`
- `cargo test` on Ubuntu 24.04 + Windows
- `cargo bench -- --sample-size 10`
- PostgreSQL integration (optional job)
- Behave BDD

## Project Layout

```
src/
  book/          # Order book
  matching/      # Matching engine
  orders/        # Types
  trades/        # Trade records
  persistence/   # SQLx + events
  replay/        # Replay
  cli/           # Clap + EngineService
tests/
  bdd/           # Behave features
  *.rs           # integration / proptest / stress
benches/         # Criterion
migrations/      # SQLx migrations
examples/        # Sample replay files
```

## License

MIT
