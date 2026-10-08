-- Unconfirmed transactions, mirrored from Bitcoin Core's mempool.
--
-- Kept apart from the confirmed tables on purpose: rows here are a
-- snapshot of Core's mempool, never canonical chain state, and they never
-- change `outputs.spent`. A transaction leaves these tables when Core
-- drops it from its mempool, whether it was mined, evicted or replaced.

CREATE TABLE IF NOT EXISTS mempool_transactions (
    id BIGSERIAL PRIMARY KEY,

    txid TEXT NOT NULL UNIQUE,

    version INTEGER NOT NULL,
    lock_time BIGINT NOT NULL,

    -- As reported by Bitcoin Core's getrawmempool.
    fee BIGINT NOT NULL,
    vsize BIGINT NOT NULL,
    entered_at BIGINT NOT NULL,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS mempool_inputs (
    id BIGSERIAL PRIMARY KEY,

    mempool_transaction_id BIGINT NOT NULL
        REFERENCES mempool_transactions(id)
        ON DELETE CASCADE,

    vin INTEGER NOT NULL,

    prev_txid TEXT NOT NULL,
    prev_vout BIGINT NOT NULL,

    script_sig TEXT,

    sequence BIGINT NOT NULL,

    UNIQUE(mempool_transaction_id, vin)
);

CREATE INDEX IF NOT EXISTS idx_mempool_inputs_prev_outpoint
ON mempool_inputs(prev_txid, prev_vout);

CREATE TABLE IF NOT EXISTS mempool_outputs (
    id BIGSERIAL PRIMARY KEY,

    mempool_transaction_id BIGINT NOT NULL
        REFERENCES mempool_transactions(id)
        ON DELETE CASCADE,

    vout INTEGER NOT NULL,

    value BIGINT NOT NULL,

    script_pubkey TEXT NOT NULL,

    address TEXT,

    UNIQUE(mempool_transaction_id, vout)
);

CREATE INDEX IF NOT EXISTS idx_mempool_outputs_address
ON mempool_outputs(address);
