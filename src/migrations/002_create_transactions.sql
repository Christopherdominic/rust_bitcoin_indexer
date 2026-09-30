CREATE TABLE IF NOT EXISTS transactions (
    id BIGSERIAL PRIMARY KEY,

    txid TEXT NOT NULL UNIQUE,

    block_id BIGINT NOT NULL
        REFERENCES blocks(id)
        ON DELETE CASCADE,

    position INTEGER NOT NULL,

    version INTEGER NOT NULL,

    lock_time BIGINT NOT NULL,

    is_coinbase BOOLEAN NOT NULL,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE(block_id, position)
);

CREATE INDEX IF NOT EXISTS idx_transactions_block_id
ON transactions(block_id);