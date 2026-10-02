// Shapes returned by the Amiable Indexer Axum API (src/api/handlers.rs).
// All monetary values are integer satoshis.

export interface IndexerStatus {
  indexed_height: number | null;
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

/** One indexed output paying to the address, with the block that created it. */
export interface AddressOutput {
  txid: string;
  block_height: number;
  timestamp: number;
  value: number;
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
