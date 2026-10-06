# Amiable Bitcoin Indexer API

Frontend Integration Guide

## 1. Overview

Amiable Bitcoin Indexer exposes indexed Bitcoin regtest data through a
JSON HTTP API. A frontend should communicate only with this API; it does
not need access to Bitcoin Core RPC, ZMQ, PostgreSQL, or environment
secrets.

Current demo network: **Bitcoin Core regtest**

## 2. Base URL

Local development:

`http://127.0.0.1:3000`

Current temporary demo tunnel:

`https://girlfriend-remedy-dash-precipitation.trycloudflare.com`

The `trycloudflare.com` URL is temporary and may change whenever the
Quick Tunnel is restarted. The frontend should therefore keep the base
URL in an environment variable rather than hard-code it.

Example:

``` env
VITE_API_URL=https://girlfriend-remedy-dash-precipitation.trycloudflare.com
```

or for Next.js:

``` env
NEXT_PUBLIC_API_URL=https://girlfriend-remedy-dash-precipitation.trycloudflare.com
```

## 3. Endpoint Summary

  -----------------------------------------------------------------------------------------
  Method                  Endpoint                                  Purpose
  ----------------------- ----------------------------------------- -----------------------
  GET                     `/health`                                 API health check

  GET                     `/api/status`                             Indexed database
                                                                    statistics

  GET                     `/api/blocks`                             Latest indexed blocks

  GET                     `/api/blocks/{height}`                    Block by height

  GET                     `/api/blocks/hash/{hash}`                 Block by hash

  GET                     `/api/transactions/{txid}`                Transaction details

  GET                     `/api/utxos`                              Recent indexed unspent
                                                                    outputs

  GET                     `/api/addresses/{address}`                Address summary

  GET                     `/api/addresses/{address}/utxos`          Address UTXOs

  GET                     `/api/addresses/{address}/transactions`   Indexed receiving
                                                                    activity for address

  GET                     `/api/search/{query}`                     Universal explorer
                                                                    search

  POST                    `/api/watch/{address}`                    Add an indexed address
                                                                    to watch list

  GET                     `/api/watch`                              List watched addresses

  GET                     `/api/watch/{address}`                    Get watched-address
                                                                    activity

  DELETE                  `/api/watch/{address}`                    Remove address from
                                                                    watch list
  -----------------------------------------------------------------------------------------

## 4. Blocks

### GET `/api/blocks`

Returns indexed blocks newest first.

Query parameters:

-   `limit` --- optional; default `20`; accepted by API up to `100`.
-   `offset` --- optional; default `0`.

Example:

``` http
GET /api/blocks?limit=5&offset=0
```

Example response:

``` json
[
  {
    "height": 337,
    "hash": "6331d24c9982f77f95ac06df94601d77ae399fc497b61e464e6662a03848b189",
    "previous_hash": "424617679fc9805d5877cc9ce58ca6e5e458b33b07bd22c890f2bc88c2735664",
    "timestamp": 1790896950,
    "tx_count": 1
  }
]
```

### GET `/api/blocks/{height}`

Example:

``` http
GET /api/blocks/337
```

Returns block metadata plus its indexed transactions.

### GET `/api/blocks/hash/{hash}`

Example:

``` http
GET /api/blocks/hash/6331d24c9982f77f95ac06df94601d77ae399fc497b61e464e6662a03848b189
```

Use either block endpoint for a block-detail page.

## 5. Transactions

### GET `/api/transactions/{txid}`

Example:

``` http
GET /api/transactions/bfff47f13f277b51ab598520377d580129f884c4dd5f0586234df0c6a2efe721
```

The transaction response contains transaction metadata together with its
indexed inputs and outputs.

Typical fields include:

``` json
{
  "transaction": {
    "txid": "...",
    "block_height": 337,
    "position": 0,
    "version": 2,
    "lock_time": 0,
    "is_coinbase": true
  },
  "inputs": [],
  "outputs": []
}
```

Input/output arrays contain the values stored by the indexer, including
outpoints, scripts, values and spend state where applicable.

## 6. Global UTXOs

### GET `/api/utxos`

Returns up to the current API limit of recent indexed outputs whose
`spent` flag is false.

Example:

``` http
GET /api/utxos
```

Typical item:

``` json
{
  "txid": "...",
  "vout": 0,
  "value": 1250000000,
  "script_pubkey": "OP_0 OP_PUSHBYTES_20 ..."
}
```

All `value` fields are in **satoshis**.

## 7. Addresses

### GET `/api/addresses/{address}`

Returns an indexed address summary.

Example:

``` http
GET /api/addresses/bcrt1qeahru55e2w5snywq62rjv4txs32gs54w4m7x3t
```

Example response:

``` json
{
  "address": "bcrt1qeahru55e2w5snywq62rjv4txs32gs54w4m7x3t",
  "received": 1250000000,
  "spent": 0,
  "balance": 1250000000,
  "transaction_count": 1
}
```

`received`, `spent`, and `balance` are satoshi amounts derived from
indexed outputs. `balance` means the sum of indexed outputs currently
marked unspent; it is not a wallet-spendability or coinbase-maturity
calculation.

### GET `/api/addresses/{address}/utxos`

Example response:

``` json
[
  {
    "txid": "bfff47f13f277b51ab598520377d580129f884c4dd5f0586234df0c6a2efe721",
    "vout": 0,
    "value": 1250000000,
    "script_pubkey": "OP_0 OP_PUSHBYTES_20 cf6e3e529953a90991c0d28726556684548852ae"
  }
]
```

### GET `/api/addresses/{address}/transactions`

Example response:

``` json
[
  {
    "txid": "bfff47f13f277b51ab598520377d580129f884c4dd5f0586234df0c6a2efe721",
    "block_height": 337,
    "timestamp": 1790896950,
    "value": 1250000000,
    "spent": false
  }
]
```

Current limitation: this endpoint represents transactions that created
indexed outputs for the address. It is not yet a complete
incoming-and-outgoing address history.

## 8. Universal Search

### GET `/api/search/{query}`

Use this endpoint for the explorer search bar.

Supported indexed values:

-   block height
-   exact block hash
-   exact transaction ID
-   exact indexed address

Block-height response:

``` json
{
  "result_type": "block",
  "value": "337"
}
```

Transaction response:

``` json
{
  "result_type": "transaction",
  "value": "bfff47f13f277b51ab598520377d580129f884c4dd5f0586234df0c6a2efe721"
}
```

Address response:

``` json
{
  "result_type": "address",
  "value": "bcrt1qeahru55e2w5snywq62rjv4txs32gs54w4m7x3t"
}
```

If nothing matches, the API returns:

`404 Not Found`

Recommended frontend routing:

-   `block` → block page
-   `transaction` → transaction page
-   `address` → address page

## 9. Address Watch

Address Watch lets a client register an address already known to the
indexer and query its latest indexed activity.

### POST `/api/watch/{address}`

Example:

``` http
POST /api/watch/bcrt1qeahru55e2w5snywq62rjv4txs32gs54w4m7x3t
```

Example response:

``` json
{
  "address": "bcrt1qeahru55e2w5snywq62rjv4txs32gs54w4m7x3t"
}
```

Current implementation only registers an address if it already exists in
indexed outputs.

### GET `/api/watch`

Example:

``` json
[
  {
    "address": "bcrt1qeahru55e2w5snywq62rjv4txs32gs54w4m7x3t"
  }
]
```

### GET `/api/watch/{address}`

Example response:

``` json
{
  "address": "bcrt1qeahru55e2w5snywq62rjv4txs32gs54w4m7x3t",
  "watching": true,
  "received": 1250000000,
  "balance": 1250000000,
  "transaction_count": 1,
  "latest_txid": "bfff47f13f277b51ab598520377d580129f884c4dd5f0586234df0c6a2efe721",
  "latest_block_height": 337
}
```

This is a polling API, not a push-notification API. The underlying
indexer updates as new blocks are processed, so polling this endpoint
will reflect newly indexed activity.

### DELETE `/api/watch/{address}`

Success:

`204 No Content`

Requesting watch status afterward returns:

`404 Not Found`

## 10. Status and Health

### GET `/health`

Simple service-health endpoint.

### GET `/api/status`

Returns indexer database statistics. Current fields include:

``` ts
interface IndexerStatus {
  indexed_height: number | null;
  blocks: number;
  transactions: number;
  inputs: number;
  outputs: number;
  unspent_outputs: number;
}
```

The endpoint describes indexed database state; it should not be
interpreted as a direct Bitcoin Core node-status response.

## 11. TypeScript Types

``` ts
export interface BlockSummary {
  height: number;
  hash: string;
  previous_hash: string;
  timestamp: number;
  tx_count: number;
}

export interface BlockTransaction {
  txid: string;
  position: number;
}

export interface BlockResponse extends BlockSummary {
  transactions: BlockTransaction[];
}

export interface AddressSummary {
  address: string;
  received: number;
  spent: number;
  balance: number;
  transaction_count: number;
}

export interface AddressUtxo {
  txid: string;
  vout: number;
  value: number;
  script_pubkey: string;
}

export interface AddressTransaction {
  txid: string;
  block_height: number;
  timestamp: number;
  value: number;
  spent: boolean;
}

export interface SearchResult {
  result_type: "block" | "transaction" | "address";
  value: string;
}

export interface WatchedAddress {
  address: string;
}

export interface WatchActivity {
  address: string;
  watching: boolean;
  received: number;
  balance: number;
  transaction_count: number;
  latest_txid: string | null;
  latest_block_height: number | null;
}

export interface IndexerStatus {
  indexed_height: number | null;
  blocks: number;
  transactions: number;
  inputs: number;
  outputs: number;
  unspent_outputs: number;
}
```

## 12. Frontend API Helper

``` ts
const API_URL =
  process.env.NEXT_PUBLIC_API_URL ?? "http://127.0.0.1:3000";

async function api<T>(path: string, options?: RequestInit): Promise<T> {
  const response = await fetch(`${API_URL}${path}`, options);

  if (response.status === 404) {
    throw new Error("Not found");
  }

  if (!response.ok) {
    throw new Error(`API request failed: ${response.status}`);
  }

  if (response.status === 204) {
    return undefined as T;
  }

  return response.json() as Promise<T>;
}

export function getBlocks(limit = 20, offset = 0) {
  return api<BlockSummary[]>(
    `/api/blocks?limit=${limit}&offset=${offset}`
  );
}

export function getBlock(height: number) {
  return api<BlockResponse>(`/api/blocks/${height}`);
}

export function getTransaction(txid: string) {
  return api(`/api/transactions/${encodeURIComponent(txid)}`);
}

export function getAddress(address: string) {
  return api<AddressSummary>(
    `/api/addresses/${encodeURIComponent(address)}`
  );
}

export function search(query: string) {
  return api<SearchResult>(
    `/api/search/${encodeURIComponent(query)}`
  );
}

export function watchAddress(address: string) {
  return api<WatchedAddress>(
    `/api/watch/${encodeURIComponent(address)}`,
    { method: "POST" }
  );
}

export function getWatchActivity(address: string) {
  return api<WatchActivity>(
    `/api/watch/${encodeURIComponent(address)}`
  );
}

export function unwatchAddress(address: string) {
  return api<void>(
    `/api/watch/${encodeURIComponent(address)}`,
    { method: "DELETE" }
  );
}
```

## 13. Suggested Explorer Pages

A frontend can build the following pages directly from the API:

-   **Home** --- `/api/status` + `/api/blocks`
-   **Block detail** --- `/api/blocks/{height}` or
    `/api/blocks/hash/{hash}`
-   **Transaction detail** --- `/api/transactions/{txid}`
-   **Address detail** --- `/api/addresses/{address}`, `/utxos`,
    `/transactions`
-   **Search bar** --- `/api/search/{query}`
-   **Watch list** --- `/api/watch`
-   **Watch detail** --- `/api/watch/{address}`

Suggested navigation:

``` text
Home
  |
  +-- Latest Blocks
  |      |
  |      +-- Block
  |             |
  |             +-- Transaction
  |
  +-- Search
  |      |
  |      +-- Block
  |      +-- Transaction
  |      +-- Address
  |
  +-- Watched Addresses
         |
         +-- Watch Activity
```

## 14. HTTP Status Handling

Frontend code should handle at least:

-   `200 OK` --- successful GET
-   `201 Created` --- address watch registration
-   `204 No Content` --- address removed from watch list
-   `400 Bad Request` --- malformed/empty request where applicable
-   `404 Not Found` --- requested indexed resource/search/watch not
    found
-   `500 Internal Server Error` --- database/API processing failure

## 15. CORS

CORS is currently enabled for development. Browser-based frontends can
call the API from a different origin.

The current development response permits broad origins. This should be
restricted to the actual frontend origin for a production deployment.

## 16. Important Integration Notes

1.  Bitcoin monetary values returned by the current API are in
    **satoshis**, not BTC.
2.  Block timestamps are Unix timestamps in seconds.
3.  The current blockchain network is **regtest**, so addresses use
    forms such as `bcrt1...`.
4.  Keep the API base URL in a frontend environment variable.
5.  Do not give the frontend Bitcoin RPC credentials, ZMQ endpoints,
    `DATABASE_URL`, or the indexer's `.env`.
6.  The current Cloudflare Quick Tunnel is for demonstration/development
    and can change after restart.
7.  The API currently has no authentication or rate limiting; do not
    treat the temporary public tunnel as a production service.
8.  Address Watch is currently a stored watch list plus queryable
    indexed activity, not Telegram/email/webhook push notifications.

## 17. Quick Integration Test

``` bash
API_URL="https://girlfriend-remedy-dash-precipitation.trycloudflare.com"

curl -s "$API_URL/api/status" | jq
curl -s "$API_URL/api/blocks?limit=5&offset=0" | jq
curl -s "$API_URL/api/search/337" | jq
```

If these return JSON, the frontend can reach the Amiable Bitcoin Indexer
API.
