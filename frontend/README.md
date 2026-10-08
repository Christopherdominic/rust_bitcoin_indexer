# Amiable Indexer — Frontend

Explorer UI for the Amiable Bitcoin Indexer. It is a Next.js (App Router) client that talks **directly from the browser** to the existing Axum REST API. There is no backend, proxy or mock data in this app.

```
Bitcoin Core (regtest) ─ RPC/ZMQ ─▶ Rust indexer ─▶ PostgreSQL ─▶ Axum REST API ─▶ this UI
```

## Setup

```bash
cd frontend
cp .env.example .env.local      # NEXT_PUBLIC_API_URL=http://127.0.0.1:3000
npm install
npm run dev                     # http://localhost:3001
```

The indexer API must be running (`cargo run` in the repo root serves it on `127.0.0.1:3000`). The UI runs on **port 3001** so the two don't collide.

| Script              | Purpose                          |
| ------------------- | -------------------------------- |
| `npm run dev`       | Dev server on :3001              |
| `npm run build`     | Production build                 |
| `npm start`         | Serve the production build :3001 |
| `npm run typecheck` | `tsc --noEmit`                   |
| `npm run lint`      | ESLint                           |

`NEXT_PUBLIC_API_URL` is inlined at build time, so rebuild after changing it.

## Pages

| Route              | Data                                                          |
| ------------------ | ------------------------------------------------------------- |
| `/`                | Search, indexer stats (`/api/status`), latest 10 blocks       |
| `/blocks`          | Paginated block list (`?page=n`, 25 per page)                 |
| `/block/[height]`  | Block header + transactions. Also accepts a 64-char block hash |
| `/tx/[txid]`       | Transaction summary, inputs, outputs with spent/unspent state |
| `/address/[addr]`  | Balance, UTXOs, transaction history (received/sent), watch toggle |
| `/utxos`           | Indexed UTXO set (newest 100, the API's limit)                |
| `/watch`           | Watch list: add, view activity, remove                        |

The header's **LIVE** indicator reflects real polling of `GET /api/status` every 10 s and switches to **OFFLINE** when the API can't be reached.

## Layout

```
app/          routes; dynamic pages are thin server wrappers around client views
components/   Header, SearchBar, StatsCards, BlocksTable, UtxoTable, HashDisplay,
              CopyButton, NetworkStatus, WatchButton, Pagination,
              LoadingState / EmptyState / ErrorState, ui.tsx primitives
lib/api.ts    the only place that calls fetch; typed endpoint functions + ApiError
lib/types.ts  response types mirroring src/api/handlers.rs
lib/format.ts sats/BTC/timestamp/hash formatting
lib/use-api.ts small fetch-on-key hook
```
