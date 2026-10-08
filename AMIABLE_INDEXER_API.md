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

Public demo deployments use a different base URL. The frontend should
keep the base URL in an environment variable rather than hard-code it.

Example:

``` env
VITE_API_URL=http://127.0.0.1:3000
```

or for Next.js:

``` env
NEXT_PUBLIC_API_URL=http://127.0.0.1:3000
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

  GET                     `/api/transactions/{txid}/status`         Confirmed /
                                                                    unconfirmed status

  GET                     `/api/mempool`                            Unconfirmed
                                                                    transactions

  GET                     `/api/mempool/{txid}`                     Unconfirmed
                                                                    transaction detail

  GET                     `/api/utxos/summary`                      Outputs by
                                                                    category

  GET                     `/api/addresses/{address}`                Address summary

  GET                     `/api/addresses/{address}/utxos`          Address UTXOs

  GET                     `/api/addresses/{address}/transactions`   Received and spent
                                                                    history for address

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

Returns the 100 most recently created UTXOs: indexed outputs that are
unspent and not provably unspendable.

``` json
{
  "txid": "...",
  "vout": 0,
  "value": 1250000000,
  "script_pubkey": "OP_0 OP_PUSHBYTES_20 ...",
  "script_type": "p2wpkh",
  "is_coinbase": true,
  "block_height": 337,
  "confirmations": 14,
  "mature": false
}
```

All `value` fields are in **satoshis**.

UTXO semantics:

- **Provably unspendable** outputs are never UTXOs. This follows Bitcoin
  Core's rule: the script starts with `OP_RETURN` (0x6a) or is longer
  than 10,000 bytes. They are still stored and returned by
  `/api/transactions/{txid}`.
- **`script_type`** is `p2pkh`, `p2sh`, `p2wpkh`, `p2wsh`, `p2tr`,
  `op_return` or `unknown`. `unknown` means "not one of these templates"
  (P2PK, bare multisig, anchors, future witness versions, nonstandard);
  it says nothing about whether the output can be spent.
- **Coinbase maturity:** a coinbase output can only be spent in a block
  at least 100 blocks above the one that created it, so `mature` is
  `false` for coinbase outputs with fewer than 100 confirmations
  (counted against the indexer's tip). Bitcoin Core's wallet is one block
  more conservative and shows them as immature until 101 confirmations.
- `mature: true` means no consensus rule stops the output from being spent
  in the next block. Whether anyone can produce a valid signature is not
  something an indexer can know.

### GET `/api/utxos/summary`

Classifies every indexed output:

``` json
{
  "indexed_height": 350,
  "coinbase_maturity": 100,
  "outputs":              { "count": 730, "value": 1755000000000 },
  "provably_unspendable": { "count": 351, "value": 0 },
  "spent":                { "count": 19,  "value": 95000000000 },
  "utxos":                { "count": 360, "value": 1660000000000 },
  "immature_coinbase":    { "count": 100, "value": 500000000000 },
  "spendable":            { "count": 260, "value": 1160000000000 },
  "utxos_by_script_type": [ { "script_type": "p2wpkh", "count": 355, "value": 1659000000000 } ]
}
```

Always `outputs = provably_unspendable + spent + utxos` and
`utxos = immature_coinbase + spendable`.

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
  "immature_balance": 1250000000,
  "spendable_balance": 0,
  "transaction_count": 1
}
```

`received`, `spent`, and `balance` are satoshi amounts derived from
indexed outputs. `balance` is the sum of the address's UTXOs and always
equals `immature_balance + spendable_balance`, where `immature_balance`
is coinbase outputs with fewer than 100 confirmations (see `/api/utxos`).

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

Returns every confirmed transaction that pays to **or spends from** the
address, newest first, **one row per transaction**.

Example response (a receive, then a spend with change back):

``` json
[
  {
    "txid": "5f1c…",
    "block_height": 345,
    "timestamp": 1790899950,
    "position": 1,
    "direction": "sent",
    "received": 60000000,
    "sent": 100000000,
    "net": -40000000,
    "value": 40000000,
    "spent": false
  },
  {
    "txid": "bfff47f13f277b51ab598520377d580129f884c4dd5f0586234df0c6a2efe721",
    "block_height": 337,
    "timestamp": 1790896950,
    "position": 1,
    "direction": "received",
    "received": 100000000,
    "sent": 0,
    "net": 100000000,
    "value": 100000000,
    "spent": true
  }
]
```

Field meanings (all amounts in satoshis):

  Field         Meaning
  ------------- ------------------------------------------------------------
  `received`    Sum of this transaction's outputs paying to the address
  `sent`        Sum of the address's earlier outputs this transaction's inputs consume
  `net`         `received - sent`: the change in the address's balance
  `direction`   `"received"` if `net > 0`, `"sent"` if `net < 0`, `"self"` if `net = 0`
  `value`       `abs(net)`
  `spent`       `true` when every output this transaction paid to the address has since been spent; `false` if it paid nothing to the address

Change outputs are netted, not double counted: spending a 1 BTC output
and receiving 0.6 BTC change in the same transaction is one row with
`sent = 100000000`, `received = 60000000`, `net = -40000000`.

Summed over the history, `received` and `sent` equal the address
summary's `received` and `spent`, and the number of rows equals its
`transaction_count`.

### Unconfirmed transactions (mempool)

The indexer mirrors Bitcoin Core's mempool in separate tables. Mempool
data is a snapshot, not canonical chain state: it never changes the UTXO
set, address balances or address history, which are confirmed-only.

#### GET `/api/transactions/{txid}/status`

``` json
{ "txid": "…", "status": "unconfirmed", "block_height": null,
  "block_hash": null, "confirmations": null, "entered_at": 1790900000 }
```

After the transaction is mined:

``` json
{ "txid": "…", "status": "confirmed", "block_height": 341,
  "block_hash": "…", "confirmations": 1, "entered_at": null }
```

Returns 404 if the transaction is neither indexed nor in the mempool
(for example evicted, replaced, or not yet seen). `confirmations` is
counted against the indexer's tip.

#### GET `/api/mempool`

Unconfirmed transactions, newest first:

``` json
[
  { "txid": "…", "fee": 2820, "vsize": 141, "fee_rate": 20.0,
    "entered_at": 1790900000, "input_count": 1, "output_count": 2,
    "output_value": 99997180 }
]
```

`fee` and `vsize` come from Bitcoin Core; `fee_rate` is sat/vB.

#### GET `/api/mempool/{txid}`

``` json
{
  "transaction": { "txid": "…", "version": 2, "lock_time": 340, "fee": 2820,
                   "vsize": 141, "fee_rate": 20.0, "entered_at": 1790900000 },
  "inputs": [
    { "vin": 0, "prev_txid": "…", "prev_vout": 0, "script_sig": "",
      "sequence": 4294967293, "prev_value": 100000000,
      "prev_address": "bcrt1q…", "prev_source": "confirmed" }
  ],
  "outputs": [
    { "vout": 0, "value": 99997180, "script_pubkey": "OP_0 OP_PUSHBYTES_20 …",
      "address": "bcrt1q…", "spent_by_mempool_txid": null }
  ]
}
```

`prev_source` is `"confirmed"`, `"mempool"` (spends another unconfirmed
transaction) or `null` (output unknown to the indexer). Returns 404 once
the transaction is confirmed; use `/api/transactions/{txid}` then.

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

`transaction_count` and `latest_*` include both receiving and spending
transactions, matching `/api/addresses/{address}/transactions`.

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

Reports the indexer's actual state. Every value is either measured during
the request or recorded by the running indexer; anything that cannot be
determined right now is `null` instead of guessed.

``` json
{
  "network": "regtest",
  "core_height": 350,
  "indexed_height": 350,
  "blocks_behind": 0,
  "synced": true,
  "bitcoin_core": "connected",
  "database": "connected",
  "zmq": {
    "blocks": { "status": "connected", "last_message_at": 1790900000 },
    "transactions": { "status": "connected", "last_message_at": 1790900042 }
  },
  "mempool_transactions": 2,
  "blocks": 351,
  "transactions": 365,
  "inputs": 370,
  "outputs": 730,
  "unspent_outputs": 360
}
```

How each value is determined:

  Field                    Source
  ------------------------ -----------------------------------------------------------
  `bitcoin_core`           `getblockchaininfo` is called on every request (3 s timeout)
  `network`, `core_height` That same call; `null` when Core is unreachable
  `synced`                 Indexed tip **hash** equals Core's best block hash (a same-height reorg is not synced)
  `blocks_behind`          `core_height − indexed_height`, never negative
  `database`               The response itself was built from PostgreSQL; if it is down the endpoint returns 500
  `zmq.*.status`           ZMQ socket monitor events for that endpoint
  `zmq.*.last_message_at`  Time the indexer last received a message on that endpoint

`zmq` `"connected"` means the ZMQ session with Bitcoin Core's publisher
is established. It does not prove Core is publishing on it; use
`last_message_at` for that. `transactions` is `"not_configured"` when
`ZMQ_TX_URL` is not set.

The API only starts after the initial historical sync, so the status is
unavailable while that sync runs.

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
  immature_balance: number;
  spendable_balance: number;
  transaction_count: number;
}

/** Same shape for /api/utxos and /api/addresses/{address}/utxos. */
export interface AddressUtxo {
  txid: string;
  vout: number;
  value: number;
  script_pubkey: string;
  script_type: "p2pkh" | "p2sh" | "p2wpkh" | "p2wsh" | "p2tr" | "op_return" | "unknown";
  is_coinbase: boolean;
  block_height: number;
  confirmations: number;
  mature: boolean;
}

export interface AddressTransaction {
  txid: string;
  block_height: number;
  timestamp: number;
  position: number;
  direction: "received" | "sent" | "self";
  received: number;
  sent: number;
  net: number;
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

export type ServiceState = "connected" | "unreachable";

export type ZmqEndpointState =
  | "not_started"    // initial sync still running
  | "connecting"     // subscribed, no session yet
  | "connected"
  | "disconnected"   // ZMQ keeps retrying
  | "not_configured";

export interface ZmqEndpointStatus {
  status: ZmqEndpointState;
  /** unix seconds of the last message received on this endpoint */
  last_message_at: number | null;
}

export interface IndexerStatus {
  /** Bitcoin Core's chain, e.g. "regtest"; null if Core is unreachable */
  network: string | null;
  core_height: number | null;
  indexed_height: number | null;
  /** core_height − indexed_height, never negative; null if Core is unreachable */
  blocks_behind: number | null;
  /** indexed tip hash equals Core's best block hash; null if Core is unreachable */
  synced: boolean | null;
  bitcoin_core: ServiceState;
  database: ServiceState;
  zmq: { blocks: ZmqEndpointStatus; transactions: ZmqEndpointStatus };
  mempool_transactions: number;
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
6.  Any public demo URL is for demonstration/development only and may
    change.
7.  The API currently has no authentication or rate limiting; do not
    treat a public demo deployment as a production service.
8.  Address Watch is currently a stored watch list plus queryable
    indexed activity, not Telegram/email/webhook push notifications.

## 17. Quick Integration Test

``` bash
API_URL="http://127.0.0.1:3000"

curl -s "$API_URL/api/status" | jq
curl -s "$API_URL/api/blocks?limit=5&offset=0" | jq
curl -s "$API_URL/api/search/337" | jq
```

If these return JSON, the frontend can reach the Amiable Bitcoin Indexer
API.
