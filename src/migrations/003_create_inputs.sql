CREATE TABLE IF NOT EXISTS inputs (
    id BIGSERIAL PRIMARY KEY,

    transaction_id BIGINT NOT NULL
        REFERENCES transactions(id)
        ON DELETE CASCADE,

    vin INTEGER NOT NULL,

    prev_txid TEXT,
    prev_vout BIGINT,

    script_sig TEXT,

    sequence BIGINT NOT NULL,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE(transaction_id, vin)
);

CREATE INDEX IF NOT EXISTS idx_inputs_transaction_id
ON inputs(transaction_id);

CREATE INDEX IF NOT EXISTS idx_inputs_prev_outpoint
ON inputs(prev_txid, prev_vout);