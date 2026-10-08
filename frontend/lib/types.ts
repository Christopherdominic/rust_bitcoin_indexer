// Shapes returned by the Amiable Indexer Axum API (src/api/handlers.rs).
// All monetary values are integer satoshis.

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

export interface BlockDetail extends BlockSummary {
  transactions: BlockTransaction[];
}

export interface TransactionInfo {
  txid: string;
  block_height: number;
  position: number;
  version: number;
  lock_time: number;
  is_coinbase: boolean;
}

export interface TransactionInput {
  vin: number;
  /** null for coinbase inputs */
  prev_txid: string | null;
  prev_vout: number | null;
  script_sig: string | null;
  sequence: number;
}

export interface TransactionOutput {
  vout: number;
  value: number;
  script_pubkey: string;
  spent: boolean;
  spent_by_txid: string | null;
  spent_by_vin: number | null;
}

export interface TransactionDetail {
  transaction: TransactionInfo;
  inputs: TransactionInput[];
  outputs: TransactionOutput[];
}

export interface Utxo {
  txid: string;
  vout: number;
  value: number;
  script_pubkey: string;
}

export interface AddressSummary {
  address: string;
  received: number;
  spent: number;
  balance: number;
  transaction_count: number;
}

export type AddressDirection = "received" | "sent" | "self";

/**
 * One confirmed transaction's effect on an address's balance. A spend that
 * sends change back to the address is a single row with both sides netted.
 */
export interface AddressTransaction {
  txid: string;
  block_height: number;
  timestamp: number;
  position: number;
  direction: AddressDirection;
  /** sats paid to the address by this transaction's outputs */
  received: number;
  /** sats of the address's earlier outputs consumed by this transaction's inputs */
  sent: number;
  /** received − sent */
  net: number;
  /** |net| */
  value: number;
  /** every output this transaction paid to the address has been spent */
  spent: boolean;
}

export type SearchResultType = "block" | "transaction" | "address";

export interface SearchResult {
  result_type: SearchResultType;
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
