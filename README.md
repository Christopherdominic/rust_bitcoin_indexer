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

Apply the SQL files in `src/migrations/` to the database configured by `DATABASE_URL`, in numeric order, before starting the indexer. For example, from the project root:

```sh
for migration in src/migrations/*.sql; do
  psql "$DATABASE_URL" -f "$migration"
done
```

These scripts create the tables and indexes needed by the application. They are not applied automatically at startup. The spent-state columns on `outputs` are updated when a later indexed transaction spends an output.

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
- Database migrations are provided as SQL files but are not applied automatically; run them before starting the application.
