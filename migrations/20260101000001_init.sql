CREATE TABLE IF NOT EXISTS orders (
    order_id UUID PRIMARY KEY,
    side VARCHAR(4) NOT NULL,
    price BIGINT,
    quantity BIGINT NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS trades (
    trade_id UUID PRIMARY KEY,
    buy_order_id UUID NOT NULL,
    sell_order_id UUID NOT NULL,
    price BIGINT NOT NULL,
    quantity BIGINT NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_trades_timestamp ON trades (timestamp);
CREATE INDEX IF NOT EXISTS idx_orders_timestamp ON orders (timestamp);

CREATE TABLE IF NOT EXISTS events (
    seq BIGSERIAL PRIMARY KEY,
    event_type VARCHAR(32) NOT NULL,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_events_created ON events (created_at);
