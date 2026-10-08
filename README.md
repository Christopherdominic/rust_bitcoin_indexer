# Amiable Bitcoin Indexer

A Bitcoin blockchain indexer written in Rust. It reads blocks from a Bitcoin Core node over JSON-RPC and stores blocks, transactions, inputs and outputs in PostgreSQL. It keeps the UTXO set's spent state up to date, handles chain reorganizations, and mirrors Bitcoin Core's mempool. After catching up with the chain, it keeps running and reacts to new blocks and transactions that Bitcoin Core announces over ZMQ. An HTTP API (Axum) exposes the indexed data, and a Next.js explorer in [`frontend/`](frontend/) lets you browse it.

It is built and tested against **Bitcoin Core on regtest** (for example in [Polar](https://lightningpolar.com/)). See [Limitations](#limitations) before pointing it at anything else.

**Bitcoin Core is the source of truth.** RPC decides what the canonical chain and the mempool contain. ZMQ messages only tell the indexer that something changed.

```
━━ Chain sync
    node tip       351
    last indexed   350
  • Synchronizing blocks 351 → 351

  ▸ Block #351
    hash 3a5f0e…
    size 1.23 KB
  ✔ Indexed block #351 · 2 tx

━━ Live mode
  • Connecting to Bitcoin Core ZMQ…
    blocks         tcp://127.0.0.1:28332
    transactions   tcp://127.0.0.1:28333
  ✔ ZMQ listener subscribed
  • Mempool updated: +1 unconfirmed, −0 removed (1 in mempool)
  ◌ Listening for new blocks…
```

## Architecture

```
Bitcoin Core
     │
     ├── RPC ───── canonical chain + mempool contents (source of truth)
     │
     └── ZMQ ───── rawblock / rawtx notifications ("something changed")
          │
          ▼
     Rust indexer ── one SQL transaction per block / rollback / mempool refresh
          │
          ▼
      PostgreSQL
          │
          ▼
       Axum API ──── 127.0.0.1:3000, JSON
          │
          ▼
      Frontend ───── Next.js explorer on :3001
```

## Project structure

```
src/
├── main.rs              Startup: config, PostgreSQL, RPC, historical sync, then API + ZMQ concurrently
├── config.rs            Reads settings from the environment / .env
├── ui.rs                Terminal output formatting (presentation only)
├── api/                 HTTP API
│   ├── routes.rs        Route table
│   ├── handlers.rs      Status, blocks, transactions, UTXOs, addresses, search, watch list
│   ├── mempool.rs       Mempool and confirmation-status endpoints
│   └── state.rs         Shared state: DB pool, RPC client for live status, ZMQ connection state
├── db/                  PostgreSQL persistence
│   ├── blocks.rs        Blocks and hash lookups
│   ├── transactions.rs  Transactions
│   ├── inputs.rs        Inputs (previous outpoints)
│   ├── outputs.rs       Outputs, address extraction, script classification
│   ├── utxo.rs          Marking outputs spent
│   ├── reorg.rs         Atomic rollback of orphaned blocks
│   └── mempool.rs       Mempool tables
├── indexer/             Indexing logic
│   ├── chain.rs         ChainSource trait: the canonical chain (Bitcoin Core, or a mock in tests)
│   ├── block.rs         Fetch and decode raw blocks
│   ├── sync.rs          Chain sync and per-block indexing with parent-hash checks
│   ├── reorg.rs         Reorg detection and common-ancestor search
│   └── mempool.rs       Mirror Bitcoin Core's mempool
├── rpc/bitcoin.rs       Bitcoin Core RPC client wrapper
├── zmq/
│   ├── listener.rs      ZMQ subscription: triggers chain and mempool syncs
│   └── status.rs        Per-endpoint ZMQ connection state, as reported by the socket
├── migrations/          SQL schema, applied in file-name order
└── test_support.rs      Test-only: in-memory chain/mempool and isolated test schemas
frontend/                Next.js explorer (see Frontend below)
AMIABLE_INDEXER_API.md   Detailed API guide for frontend developers
```

## Setup

### Requirements

- **Rust** stable with the 2024 edition (Rust 1.85 or newer) and Cargo
- **PostgreSQL**, any recent version (local, Docker, or hosted such as Neon)
- **Bitcoin Core** with RPC and ZMQ enabled, on **regtest**, either standalone or in Polar
- **Docker** to run the database tests (optional otherwise)
- **Node.js** to run the frontend (optional)

### Bitcoin Core with Polar

1. Create a network in Polar with a Bitcoin Core node and start it.
2. Select the bitcoind node and open its **Connect** tab. It lists the RPC host, port, username and password, and the ZMQ block and transaction ports. Those are the **host** ports to put in `.env`.
3. To run `bitcoin-cli` against it, exec into the container, for example:
   ```sh
   alias btc='docker exec polar-n1-backend1 bitcoin-cli -regtest -datadir=/home/bitcoin/.bitcoin'
   btc getblockchaininfo
   ```
   The container name depends on your network and node; `docker ps` shows it.

Polar's bitcoind already publishes `rawblock` and `rawtx` on separate ports, and runs with `-txindex`, which the indexer does not need.

### Bitcoin Core standalone

Add to `bitcoin.conf`:

```ini
regtest=1
server=1
rpcuser=change_me
rpcpassword=change_me
zmqpubrawblock=tcp://127.0.0.1:28332
zmqpubrawtx=tcp://127.0.0.1:28333
```

The indexer fetches historical blocks over RPC, so the node must still have them; a pruned node can't serve pruned blocks.

### Configuration

```sh
cp .env.example .env    # then fill in real values; .env is git-ignored
```

| Variable               | Required | Description |
| ---------------------- | -------- | ----------- |
| `BITCOIN_RPC_URL`      | yes      | Bitcoin Core JSON-RPC endpoint, e.g. `http://127.0.0.1:18443` |
| `BITCOIN_RPC_USER`     | yes      | RPC username |
| `BITCOIN_RPC_PASSWORD` | yes      | RPC password |
| `DATABASE_URL`         | yes      | PostgreSQL connection string |
| `ZMQ_BLOCK_URL`        | yes      | `zmqpubrawblock` endpoint |
| `ZMQ_TX_URL`           | no       | `zmqpubrawtx` endpoint. Without it, the mempool is refreshed only at startup and when a block arrives |

Never commit `.env`. The API never returns RPC credentials, `DATABASE_URL` or ZMQ endpoints. Keep RPC and ZMQ bound to localhost or a private network.

### Database migrations

Migrations are plain SQL files in [`src/migrations/`](src/migrations/). They are **not** applied automatically and are not tracked in a migrations table. Apply them in file-name order. Every file is written to be safe to re-run, so applying the whole directory again after pulling new ones is fine:

```sh
set -a; source .env; set +a
for m in src/migrations/*.sql; do
  psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -f "$m"
done
```

Without a local `psql`, use the Postgres image:

```sh
for m in src/migrations/*.sql; do
  docker run --rm -i postgres:16-alpine psql "$DATABASE_URL" -v ON_ERROR_STOP=1 < "$m"
done
```

Apply new migrations **before** starting a newer build of the indexer. For example, migration 007 adds `NOT NULL` columns that the indexer writes.

### Running the indexer

```sh
cargo run --release
```

The indexer catches up to Bitcoin Core's tip, starts the API on `http://127.0.0.1:3000`, then stays in live mode. Stop it with `Ctrl+C`. On restart it checks for reorgs and resumes from where it stopped. Set `NO_COLOR=1` for plain output.

### Running tests

Tests never need a Bitcoin node: chain and mempool behavior run against in-memory mocks. The database tests need a **throwaway** PostgreSQL, which they find through `TEST_DATABASE_URL` and only there. They never fall back to `DATABASE_URL`, and they refuse to run if the two are equal.

```sh
# Pure unit tests: no database needed. Database tests show as "ignored".
cargo test

# Everything, against a disposable Postgres in Docker:
docker run -d --name amiable-test-db \
  -e POSTGRES_USER=amiable -e POSTGRES_PASSWORD=amiable -e POSTGRES_DB=amiable_test \
  -p 127.0.0.1:55432:5432 postgres:16-alpine

export TEST_DATABASE_URL=postgres://amiable:amiable@127.0.0.1:55432/amiable_test
cargo test -- --include-ignored

docker rm -f amiable-test-db   # when finished
```

Each database test creates its own schema `amiable_test_<pid>_<n>`, applies the migrations there with `search_path` pinned to it, and drops it afterwards. A test that panics leaves its schema behind, which is harmless in a throwaway container. If `--include-ignored` is used without `TEST_DATABASE_URL`, the database tests fail rather than silently pass.

The tests cover persistence, UTXO spent-state transitions, OP_RETURN and script classification, coinbase handling and maturity, address extraction and history, reorg rollback (including a branch switch in the middle of indexing a block), mempool mirroring, status reporting, and migration 007's backfill.

## Indexing lifecycle

```
startup
  ↓
RPC catch-up ─┐
  ↓           │ every sync starts with
reorg check ◀─┘ a reorg check
  ↓
ZMQ subscription
  ↓
live indexing
  ↓
offline recovery (on the next start)
```

1. **Startup.** The indexer loads the config, connects to PostgreSQL and creates the RPC client.
2. **RPC catch-up** (`sync_chain`) indexes every height from the last indexed block (or genesis) up to Core's tip. Each block is fetched by hash and written in **one SQL transaction**: the block, its transactions, inputs and outputs, and the outputs it spends.
3. **Reorg check.** Every sync first compares the indexed tip's hash with Core's hash at that height.
   - If they differ, or Core's chain is now shorter, the indexer walks back to the **common ancestor**. It then rolls back everything above it in one SQL transaction, restoring outputs that orphaned transactions had spent, and indexes the new branch.
   - Before a block is written, its parent hash must match the indexed block below it. If Core switched branches in the middle of a sync, the sync restarts, up to 5 times.
   - If not even genesis matches, the indexer stops instead of deleting data. That means the database belongs to another network or a reset regtest chain.
4. **ZMQ subscription.** The API starts and the listener subscribes to `rawblock` (and `rawtx` if configured). It then runs one more sync to catch blocks that arrived in between.
5. **Live indexing.**
   - A `rawblock` message runs `sync_chain` and then a mempool refresh. A `rawtx` message runs a mempool refresh.
   - Messages that are already queued are drained together, so a burst costs one sync.
   - The message contents are never indexed directly; RPC decides what changed.
6. **Offline recovery.** Nothing depends on having seen every message. A missed notification, or downtime of any length, is fixed by the next sync, which checks for reorgs and continues from the database tip.

### Mempool lifecycle

```
rawtx → Core's mempool (getrawmempool) → UNCONFIRMED row in mempool_* tables
      → block mined → indexed as CONFIRMED → mempool row removed on the next refresh
```

- **What a refresh does:** `sync_mempool` makes the mempool tables mirror Core's mempool. It removes what Core no longer has (mined, evicted, expired, replaced) and fetches what's new. All writes happen in one SQL transaction.
- **No duplicates:** a transaction that is already confirmed is never stored as unconfirmed, and the API hides any mempool row whose transaction is already confirmed.
- **Reorgs:** a transaction from a reorged-out block becomes unconfirmed again once Core puts it back in its mempool.
- **Separate from confirmed data:** mempool data lives in its own tables and never changes `outputs.spent`, the UTXO set, balances or address history.

## Database model

```
blocks ─────────── one row per indexed block on the canonical chain
  ↓ ON DELETE CASCADE
transactions ───── txid, position, coinbase flag
  ├── inputs ───── prev_txid, prev_vout (NULL for coinbase)
  └── outputs ──── value, script_pubkey, address, script_type,
                   provably_unspendable, spent, spent_by_txid, spent_by_vin

mempool_transactions ── fee, vsize, entered_at   (snapshot of Core's mempool)
  ├── mempool_inputs
  └── mempool_outputs

watched_addresses
```

**How UTXO state is maintained:**

- **Spending.** When a block is indexed, each input marks the output it spends (`prev_txid:prev_vout`) as `spent`, recording `spent_by_txid` and `spent_by_vin`. This happens inside the block's SQL transaction, so a block is either fully applied or not at all.
- **Rollback.** On a reorg, outputs spent by transactions in orphaned blocks are reset to unspent *before* those blocks are deleted. Outputs created in orphaned blocks are removed by the cascade.
- **What counts as a UTXO:** `spent = FALSE AND provably_unspendable = FALSE`.
  - **Provably unspendable** follows Bitcoin Core's rule: the script starts with `OP_RETURN`, or is longer than 10,000 bytes. Such outputs are stored and appear in transaction details, but are never UTXOs.
  - **`script_type`** is `p2pkh`, `p2sh`, `p2wpkh`, `p2wsh`, `p2tr`, `op_return` or `unknown`. `unknown` means "not a recognised template", not "unspendable".
- **Coinbase maturity.** A coinbase output can be spent in a block at least 100 blocks above the one that created it. The API therefore reports it as immature until it has 100 confirmations, counted against the indexer's tip. This is computed at query time. Core's wallet is one block more conservative (101).

## API reference

All responses are JSON. All amounts are in satoshis. The API listens on `127.0.0.1:3000`. [`AMIABLE_INDEXER_API.md`](AMIABLE_INDEXER_API.md) has example responses and TypeScript types.

| Method | Path | Description |
| ------ | ---- | ----------- |
| GET | `/health` | Liveness: `{"status":"ok"}` |
| GET | `/api/status` | Network, Core tip, indexed tip, `blocks_behind`, `synced` (hash comparison), Bitcoin Core / database / per-endpoint ZMQ state, mempool size and row counts. Values that can't be determined right now are `null` |
| GET | `/api/blocks?limit=&offset=` | Blocks, newest first. `limit` 1–100 (default 20) |
| GET | `/api/blocks/{height}` | Block at a height with its txids. `404` if not indexed |
| GET | `/api/blocks/hash/{hash}` | Same, by block hash |
| GET | `/api/transactions/{txid}` | Confirmed transaction with inputs and outputs (including OP_RETURN) and spent state. `404` if not confirmed |
| GET | `/api/transactions/{txid}/status` | `unconfirmed` (with `entered_at`) or `confirmed` (with height, hash, confirmations). `404` if unknown |
| GET | `/api/utxos` | 100 newest UTXOs, with `script_type`, `is_coinbase`, `confirmations`, `mature` |
| GET | `/api/utxos/summary` | Count and value of all outputs, provably unspendable, spent, UTXOs, immature coinbase, spendable, and UTXOs by script type |
| GET | `/api/mempool` | Unconfirmed transactions: fee, vsize, fee rate, inputs/outputs |
| GET | `/api/mempool/{txid}` | Unconfirmed transaction detail. `404` once confirmed |
| GET | `/api/addresses/{address}` | Received, spent, balance (immature + spendable) and transaction count. `404` if never seen |
| GET | `/api/addresses/{address}/utxos` | The address's UTXOs |
| GET | `/api/addresses/{address}/transactions` | One row per confirmed transaction that pays to or spends from the address, with `received`, `sent`, `net` and `direction`. Change outputs are netted, not double-counted |
| GET | `/api/search/{query}` | Resolves a height, block hash, txid or address to `{"result_type","value"}` |
| POST | `/api/watch/{address}` | Add an indexed address to the watch list (`201`) |
| GET | `/api/watch` | Watched addresses |
| GET | `/api/watch/{address}` | Activity for a watched address: balance, transaction count, latest transaction |
| DELETE | `/api/watch/{address}` | Remove from the watch list (`204`) |

```sh
curl -s localhost:3000/api/status | jq
curl -s "localhost:3000/api/blocks?limit=5" | jq
curl -s localhost:3000/api/transactions/<txid>/status | jq
curl -s localhost:3000/api/addresses/<address>/transactions | jq
```

## Address Watch

Address Watch is a **stored list you poll**. Adding an address records it; `GET /api/watch/{address}` then reports its activity as indexed so far. It is **not** a push-notification service: there are no webhooks, emails or messages, and the indexer doesn't contact anyone when a watched address is used.

## Frontend

[`frontend/`](frontend/) is a Next.js + TypeScript + Tailwind explorer that calls the API directly from the browser.

```sh
cd frontend
cp .env.example .env.local   # NEXT_PUBLIC_API_URL=http://127.0.0.1:3000
npm install
npm run dev                  # http://localhost:3001
```

Pages: home (search, stats, latest blocks), `/blocks`, `/block/{height}`, `/tx/{txid}`, `/address/{address}` (balance, UTXOs, transaction history), `/utxos`, `/watch`. See [`frontend/README.md`](frontend/README.md).

## Limitations

This is a capstone project. It is **not** production-ready or mainnet-ready.

**Network and scale**
- **Regtest only for addresses:** addresses are decoded with `Network::Regtest`, which is hard-coded, so another network would get wrong addresses.
- **Slow initial sync:** blocks are fetched one at a time over blocking RPC, so a mainnet sync from genesis would take a very long time.
- **Mempool cost:** each mempool refresh reads Core's entire mempool. That's fine on regtest, but too heavy for mainnet.
- **No pagination:** address history and the mempool list are unpaginated. `/api/utxos` returns at most 100 rows.

**Operations**
- **Fixed API address, no protection:** the API address `127.0.0.1:3000` is hard-coded, and the API has no authentication or rate limiting.
- **Migrations are manual** and untracked.
- **Errors stop the process:** a chain-sync error (including 5 failed retries while the chain keeps changing) stops the indexer. Run it under a supervisor if it must stay up. Mempool refresh errors are only logged.
- **No status during the first sync:** the API starts only after the initial historical sync, so `/api/status` is unavailable during it. While Core is unreachable, each status request can take up to 3 seconds.

**Reorgs**
- **Pure disconnects are noticed late:** a block disconnect with no new block (for example `invalidateblock` on its own) is noticed at the next block or restart, because Core sends no ZMQ message for it.
- **Slow on deep reorgs:** the ancestor search makes one RPC call per height.
- **Orphaned data is deleted,** not archived.

**Data**
- **Partial-start spends:** outputs are only marked spent if the indexer stored them. When indexing starts partway through a chain, spends of earlier outputs are logged and ignored, and they don't count in address history.
- **Maturity can lag:** coinbase maturity is computed against the indexer's tip, so it can lag Core while the indexer is behind.
- **Migration 007 backfill gap:** rows backfilled by migration 007 can't apply the >10,000-byte unspendable rule. Re-index if that matters.
- **Duplicate coinbase txids:** the two historical duplicate coinbase txids on mainnet (BIP30) would be mis-stored by the `txid` upsert.
- **Frontend lags the API:** the frontend doesn't yet display mempool, node or ZMQ status, or coinbase maturity, although the API provides them.
