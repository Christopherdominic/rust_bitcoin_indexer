# Amiable Bitcoin Indexer

A Bitcoin blockchain indexer written in Rust. It reads blocks from a Bitcoin Core node over JSON-RPC, stores blocks, transactions, inputs and outputs in PostgreSQL, and tracks which outputs have been spent. After catching up with the chain, it keeps running and indexes each new block as Bitcoin Core announces it over ZMQ. An HTTP API exposes the indexed data, and a Next.js frontend in [`frontend/`](frontend/) lets you browse it.

```
╭────────────────────────────────────────╮
│  ₿  Amiable Bitcoin Indexer            │
│     Bitcoin Core  →  PostgreSQL        │
╰────────────────────────────────────────╯

━━ Connecting
  ✔ PostgreSQL connected
  ✔ Bitcoin Core RPC ready
    rpc            http://127.0.0.1:18443

━━ Chain sync
    node tip       812,345
    last indexed   812,340
  • Synchronizing blocks 812,341 → 812,345

  ▸ Block #812,341
    hash 00000000000000000002a7c4…
    size 1.52 MB
  ✔ Indexed block #812,341 · 3,104 tx
  ...

━━ API
  ✔ API listening
    url            http://127.0.0.1:3000

━━ Live mode
  ✔ ZMQ listener connected
  ◌ Listening for new blocks…
```

## Features

- **Historical sync:** on startup, indexes every block from the last indexed height (or genesis) up to the node's tip.
- **Live indexing:** subscribes to Bitcoin Core's `rawblock` ZMQ feed and syncs whenever a new block arrives.
- **Atomic writes:** each block, including all its transactions, inputs and outputs, is written in a single database transaction.
- **UTXO tracking:** when an input spends an already-indexed output, the output is marked `spent`, along with the spending `txid` and `vin`.
- **Idempotent:** upserts (`ON CONFLICT … DO UPDATE`) make it safe to re-index a block.
- **Address indexing:** each output's `scriptPubKey` is decoded to a regtest address (outputs without one, such as `OP_RETURN`, store `NULL`), so balances, UTXOs and history can be queried per address.
- **HTTP API:** JSON endpoints for indexer status, blocks, transactions, UTXOs, addresses and search. CORS is enabled so a browser frontend can call it directly.
- **Address watch:** a stored watch list of addresses, with their indexed activity queryable through the API.
- **Explorer frontend:** a Next.js UI for blocks, transactions, addresses, the UTXO set and the watch list (see [Frontend](#frontend)).
- **Readable terminal output:** colored, structured logs. Color turns off automatically when output is piped or `NO_COLOR` is set.

## How it works

```
 Bitcoin Core ──RPC──▶ sync_chain ──▶ index_block ──▶ PostgreSQL
      │                    ▲          (one DB tx per block)
      └──ZMQ rawblock──────┘
```

1. `sync_chain` compares the node's tip (`getblockcount`) with `MAX(height)` in the `blocks` table and indexes the missing range.
2. `index_block` fetches the raw block (`getblockhash` + `getblock` hex), decodes it with the `bitcoin` crate, then saves the block and, for each transaction, marks spent outputs and saves its inputs and outputs.
3. The ZMQ listener runs `sync_chain` again on every block notification. RPC and the database decide what still needs indexing, so a missed notification is caught on the next one.

## Requirements

- Rust (2024 edition) and Cargo
- Bitcoin Core with RPC enabled and ZMQ block publishing turned on
- PostgreSQL

### Bitcoin Core configuration

Add these to `bitcoin.conf` (the example uses regtest):

```ini
server=1
rpcuser=your_rpc_user
rpcpassword=your_rpc_password
zmqpubrawblock=tcp://127.0.0.1:28332
```

Blocks are fetched by height, so the node must still have the blocks you want to index. A pruned node can't serve blocks it has already pruned.

## Configuration

Create a `.env` file in the project root. Git ignores it.

```dotenv
BITCOIN_RPC_URL=http://127.0.0.1:18443
BITCOIN_RPC_USER=your_rpc_user
BITCOIN_RPC_PASSWORD=your_rpc_password
DATABASE_URL=postgresql://user:password@localhost:5432/bitcoin_indexer
ZMQ_BLOCK_URL=tcp://127.0.0.1:28332
```

| Variable               | Description                                  |
| ---------------------- | -------------------------------------------- |
| `BITCOIN_RPC_URL`      | Bitcoin Core JSON-RPC endpoint               |
| `BITCOIN_RPC_USER`     | RPC username                                 |
| `BITCOIN_RPC_PASSWORD` | RPC password                                 |
| `DATABASE_URL`         | PostgreSQL connection string                 |
| `ZMQ_BLOCK_URL`        | Bitcoin Core `zmqpubrawblock` endpoint       |

All five are required. Don't commit real credentials.

## Database setup

The migrations are **not** applied automatically. Run them in order before the first start:

```sh
for migration in src/migrations/*.sql; do
  psql "$DATABASE_URL" -f "$migration"
done
```

| Table          | Contents                                                                 |
| -------------- | ------------------------------------------------------------------------ |
| `blocks`       | height, hash, previous hash, timestamp, transaction count                |
| `transactions` | txid, block, position in block, version, lock time, coinbase flag        |
| `inputs`       | previous outpoint (`prev_txid`, `prev_vout`), `script_sig`, sequence     |
| `outputs`      | value (sats), `script_pubkey`, address, spent flag, spending `txid`/`vin` |
| `watched_addresses` | addresses on the watch list, with the time they were added          |

## Running

```sh
cargo run --release
```

The indexer catches up to the chain tip, then keeps running in live mode. Press `Ctrl+C` to stop it. On the next start it resumes from the last indexed height.

To turn off colored output:

```sh
NO_COLOR=1 cargo run --release
```

## HTTP API

The API starts after the historical sync finishes. It listens on `http://127.0.0.1:3000` and runs alongside the ZMQ listener. All responses are JSON.

| Method | Path                       | Description                                                                 |
| ------ | -------------------------- | --------------------------------------------------------------------------- |
| GET    | `/health`                  | Liveness check: `{"status":"ok"}`                                           |
| GET    | `/api/status`              | Highest indexed height plus row counts for blocks, txs, inputs, outputs and unspent outputs |
| GET    | `/api/blocks?limit=&offset=` | Blocks, newest first. `limit` defaults to 20 (max 100), `offset` to 0     |
| GET    | `/api/blocks/{height}`     | Block at a height, with its transaction IDs. Returns `404` if not indexed   |
| GET    | `/api/blocks/hash/{hash}`  | Same as above, looked up by block hash                                      |
| GET    | `/api/transactions/{txid}` | Transaction details with its inputs and outputs, including spend status. Returns `404` if not found |
| GET    | `/api/utxos`               | The 100 most recently indexed unspent outputs                               |
| GET    | `/api/addresses/{address}` | Received, spent, balance and transaction count. Returns `404` if the address has no indexed outputs |
| GET    | `/api/addresses/{address}/utxos` | Unspent outputs paying to the address                                 |
| GET    | `/api/addresses/{address}/transactions` | Indexed outputs paying to the address, with block height, timestamp and spend status |
| GET    | `/api/search/{query}`      | Resolves a block height, block hash, txid or address to `{"result_type", "value"}`. Returns `404` if nothing matches |
| POST   | `/api/watch/{address}`     | Adds an indexed address to the watch list. Returns `201`, or `404` if the address has no indexed outputs |
| GET    | `/api/watch`               | Watched addresses, newest first                                             |
| GET    | `/api/watch/{address}`     | Received, balance, transaction count and latest activity for a watched address. Returns `404` if not watched |
| DELETE | `/api/watch/{address}`     | Removes an address from the watch list. Returns `204`, or `404` if it wasn't watched |

Examples:

```sh
curl http://127.0.0.1:3000/api/status
curl "http://127.0.0.1:3000/api/blocks?limit=5&offset=0"
curl http://127.0.0.1:3000/api/transactions/<txid>
curl http://127.0.0.1:3000/api/search/<height|hash|txid|address>
curl -X POST http://127.0.0.1:3000/api/watch/<address>
```

Values are in satoshis. Database errors return `500`. Address watch is a stored list you query; it doesn't push notifications.

## Frontend

[`frontend/`](frontend/) is a Next.js + TypeScript + Tailwind explorer for the indexer. The browser calls the API above directly; there's no separate backend.

```sh
cd frontend
cp .env.example .env.local   # NEXT_PUBLIC_API_URL=http://127.0.0.1:3000
npm install
npm run dev                  # http://localhost:3001
```

Start the indexer first so the API is up. The frontend uses port 3001 to stay clear of the API on 3000.

| Page               | Shows                                                          |
| ------------------ | -------------------------------------------------------------- |
| `/`                | Search, indexer stats, latest blocks, live API status          |
| `/blocks`          | Paginated block list                                           |
| `/block/{height}`  | Block header and its transactions (also accepts a block hash)  |
| `/tx/{txid}`       | Inputs and outputs with spent/unspent state                    |
| `/address/{address}` | Balance, UTXOs, received outputs, watch/unwatch              |
| `/utxos`           | The indexed UTXO set                                           |
| `/watch`           | Watch list: add, view activity, remove                         |

See [`frontend/README.md`](frontend/README.md) for details.

## Project structure

```
src/
├── main.rs            # Startup: config, connections, sync, then API + live mode
├── config.rs          # Loads settings from environment / .env
├── ui.rs              # Terminal output formatting (presentation only)
├── api/               # HTTP API (axum): routes, handlers, shared state
├── rpc/bitcoin.rs     # Bitcoin Core RPC client wrapper
├── indexer/
│   ├── block.rs       # Fetch and decode raw blocks
│   └── sync.rs        # Chain sync and per-block indexing
├── zmq/listener.rs    # ZMQ new-block subscription
├── db/                # PostgreSQL persistence (blocks, txs, inputs, outputs, UTXO)
└── migrations/        # SQL schema
frontend/              # Next.js explorer UI (see Frontend above)
```

## Limitations

- **No reorg handling:** if a block at an already-indexed height is replaced, the indexer doesn't detect or roll it back.
- **Outputs are only marked spent if already indexed:** if you index from partway through the chain, spends of earlier outputs are logged as warnings.
- The API address (`127.0.0.1:3000`) is hard-coded, and the API has no authentication. Only `/api/blocks` is paginated; `/api/utxos` returns at most 100 rows.
- Addresses are decoded for **regtest** only (`Network::Regtest` is hard-coded). Outputs indexed before address decoding was added have a `NULL` address until their block is re-indexed.
- Blocks are indexed one at a time over RPC, so a full mainnet sync from genesis takes a long time.
