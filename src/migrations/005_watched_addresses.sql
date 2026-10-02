CREATE TABLE IF NOT EXISTS watched_addresses (
    id BIGSERIAL PRIMARY KEY,
    address TEXT NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_watched_addresses_address
ON watched_addresses(address);