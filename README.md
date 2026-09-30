# Amiable Bitcoin Indexer

A Rust application that connects to a Bitcoin Core-compatible JSON-RPC node, fetches the node's latest block, and stores block, transaction, input, and output data in PostgreSQL. It also marks previously indexed outputs as spent when an input references them.

> **Current behavior:** each run processes only the latest block reported by the node. It does not backfill historical blocks or continuously poll for new blocks.

## Requirements

- Rust toolchain with Cargo (Rust 2024 edition support)
- A reachable Bitcoin Core-compatible node with JSON-RPC enabled
- PostgreSQL database

## Configuration

Create a `.env` file in the project root (it is ignored by Git) with:

```dotenv
BITCOIN_RPC_URL=http://127.0.0.1:18443
BITCOIN_RPC_USER=your_rpc_user
BITCOIN_RPC_PASSWORD=your_rpc_password
DATABASE_URL=postgresql://user:password@localhost:5432/bitcoin_indexer
```

The RPC settings also accept `RPC_URL`, `RPC_USER`, and `RPC_PASSWORD` as fallback variable names. `DATABASE_URL` is required. Use values for your own node and database; do not commit real credentials.

## Database schema

Create the following tables in the database configured by `DATABASE_URL` before starting the indexer:

```sql
CREATE TABLE blocks (
    id BIGSERIAL PRIMARY KEY,
    height BIGINT NOT NULL UNIQUE,
    hash TEXT NOT NULL,
    previous_hash TEXT NOT NULL,
    timestamp BIGINT NOT NULL,
    tx_count INTEGER NOT NULL
);

CREATE TABLE transactions (
    id BIGSERIAL PRIMARY KEY,
    txid TEXT NOT NULL UNIQUE,
    block_id BIGINT NOT NULL REFERENCES blocks(id),
    position INTEGER NOT NULL,
    version INTEGER NOT NULL,
    lock_time BIGINT NOT NULL,
    is_coinbase BOOLEAN NOT NULL
);

CREATE TABLE inputs (
    id BIGSERIAL PRIMARY KEY,
    transaction_id BIGINT NOT NULL REFERENCES transactions(id),
    vin INTEGER NOT NULL,
    prev_txid TEXT,
    prev_vout BIGINT,
    script_sig TEXT NOT NULL,
    sequence BIGINT NOT NULL,
    UNIQUE (transaction_id, vin)
);

CREATE TABLE outputs (
    id BIGSERIAL PRIMARY KEY,
    transaction_id BIGINT NOT NULL REFERENCES transactions(id),
    vout INTEGER NOT NULL,
    value BIGINT NOT NULL,
    script_pubkey TEXT NOT NULL,
    spent BOOLEAN NOT NULL DEFAULT FALSE,
    spent_by_txid TEXT,
    spent_by_vin INTEGER,
    UNIQUE (transaction_id, vout)
);
```

The spent-state columns on `outputs` are updated when a later indexed transaction spends an output.

## Run

From the project root:

```sh
cargo run
```

To compile without starting the indexer:

```sh
cargo build
```

The process connects to PostgreSQL first, then queries the node for its current block height, fetches and decodes that block, and writes its records to the database. Both services must be reachable and the node's RPC credentials must be valid.

## Limitations

- Only the current tip block is processed on each invocation; missed blocks are not automatically backfilled.
- Inputs are stored with their previous transaction references. Output spend status is updated only when the referenced output has already been indexed.
- Schema migrations are not currently managed by the application; provision the tables before running it.
