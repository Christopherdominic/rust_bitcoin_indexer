CREATE TABLE IF NOT EXISTS outputs (
    id BIGSERIAL PRIMARY KEY,

    transaction_id BIGINT NOT NULL
        REFERENCES transactions(id)
        ON DELETE CASCADE,

    vout INTEGER NOT NULL,

    value BIGINT NOT NULL,

    script_pubkey TEXT NOT NULL,

    address TEXT,

    spent BOOLEAN NOT NULL DEFAULT FALSE,

    spent_by_txid TEXT,
    spent_by_vin INTEGER,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE(transaction_id, vout)
);

CREATE INDEX IF NOT EXISTS idx_outputs_transaction_id
ON outputs(transaction_id);

CREATE INDEX IF NOT EXISTS idx_outputs_address
ON outputs(address);

CREATE INDEX IF NOT EXISTS idx_outputs_spent
ON outputs(spent);