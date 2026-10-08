use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::{api::state::AppState, rpc::bitcoin::BlockchainInfo, zmq::status::EndpointSnapshot};

// ============================================================
// HEALTH
// ============================================================

#[derive(Serialize)]
pub struct HealthResponse {
    status: &'static str,
}

pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

// ============================================================
// STATUS
// ============================================================

/// How long the status endpoint waits for Bitcoin Core before reporting
/// it as unreachable.
const NODE_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ServiceState {
    Connected,
    Unreachable,
}

#[derive(Serialize)]
pub struct ZmqStatusResponse {
    blocks: EndpointSnapshot,
    transactions: EndpointSnapshot,
}

/// Every field is either measured during this request or recorded by the
/// running indexer. Values that cannot be determined right now are null
/// rather than guessed.
#[derive(Serialize)]
pub struct StatusResponse {
    /// Bitcoin Core's chain ("regtest", …); null if Core is unreachable.
    network: Option<String>,
    core_height: Option<i64>,
    indexed_height: Option<i64>,
    /// `core_height - indexed_height`, never negative; null if Core is unreachable.
    blocks_behind: Option<i64>,
    /// The indexed tip is Core's best block (same hash, not just same
    /// height); null if Core is unreachable.
    synced: Option<bool>,
    /// Result of asking Core for its tip during this request.
    bitcoin_core: ServiceState,
    /// This response was built from the database, so it is reachable.
    database: ServiceState,
    zmq: ZmqStatusResponse,
    mempool_transactions: i64,
    blocks: i64,
    transactions: i64,
    inputs: i64,
    outputs: i64,
    unspent_outputs: i64,
}

#[derive(sqlx::FromRow)]
struct DatabaseCounts {
    indexed_height: Option<i64>,
    indexed_hash: Option<String>,
    blocks: i64,
    transactions: i64,
    inputs: i64,
    outputs: i64,
    unspent_outputs: i64,
    mempool_transactions: i64,
}

/// Asks Core for its tip on a blocking thread, giving up after
/// [`NODE_TIMEOUT`]. `None` means unreachable (or no client attached).
async fn node_tip(state: &AppState) -> Option<BlockchainInfo> {
    let node = state.node.clone()?;
    let call = tokio::task::spawn_blocking(move || node.blockchain_info());

    match tokio::time::timeout(NODE_TIMEOUT, call).await {
        Ok(Ok(Ok(info))) => Some(info),
        _ => None,
    }
}

pub async fn status(State(state): State<AppState>) -> Result<Json<StatusResponse>, StatusCode> {
    let counts = sqlx::query_as::<_, DatabaseCounts>(
        r#"
        SELECT
            (SELECT MAX(height) FROM blocks) AS indexed_height,
            (SELECT hash FROM blocks ORDER BY height DESC LIMIT 1) AS indexed_hash,
            (SELECT COUNT(*) FROM blocks) AS blocks,
            (SELECT COUNT(*) FROM transactions) AS transactions,
            (SELECT COUNT(*) FROM inputs) AS inputs,
            (SELECT COUNT(*) FROM outputs) AS outputs,
            (SELECT COUNT(*) FROM outputs
             WHERE spent = FALSE
               AND provably_unspendable = FALSE) AS unspent_outputs,
            (SELECT COUNT(*) FROM mempool_transactions m
             WHERE NOT EXISTS (
                 SELECT 1 FROM transactions t WHERE t.txid = m.txid
             )) AS mempool_transactions
        "#,
    )
    .fetch_one(&state.pool)
    .await
    .map_err(|error| {
        eprintln!("Failed to read indexer status: {}", error);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let node = node_tip(&state).await;

    let (network, core_height, blocks_behind, synced) = match &node {
        Some(info) => {
            let core_height = info.blocks as i64;
            let behind = match counts.indexed_height {
                Some(indexed) => core_height - indexed,
                None => core_height + 1,
            };
            (
                Some(info.chain.clone()),
                Some(core_height),
                Some(behind.max(0)),
                Some(counts.indexed_hash.as_deref() == Some(info.best_block_hash.as_str())),
            )
        }
        None => (None, None, None, None),
    };

    Ok(Json(StatusResponse {
        network,
        core_height,
        indexed_height: counts.indexed_height,
        blocks_behind,
        synced,
        bitcoin_core: if node.is_some() {
            ServiceState::Connected
        } else {
            ServiceState::Unreachable
        },
        database: ServiceState::Connected,
        zmq: ZmqStatusResponse {
            blocks: state.zmq.blocks.snapshot(),
            transactions: state.zmq.transactions.snapshot(),
        },
        mempool_transactions: counts.mempool_transactions,
        blocks: counts.blocks,
        transactions: counts.transactions,
        inputs: counts.inputs,
        outputs: counts.outputs,
        unspent_outputs: counts.unspent_outputs,
    }))
}

// ============================================================
// BLOCKS
// ============================================================

#[derive(Serialize, sqlx::FromRow)]
pub struct BlockSummary {
    height: i64,
    hash: String,
    previous_hash: String,
    timestamp: i64,
    tx_count: i32,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct BlockTransaction {
    txid: String,
    position: i32,
}

#[derive(Serialize)]
pub struct BlockResponse {
    height: i64,
    hash: String,
    previous_hash: String,
    timestamp: i64,
    tx_count: i32,
    transactions: Vec<BlockTransaction>,
}

#[derive(Deserialize)]
pub struct BlocksQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

// ============================================================
// LATEST BLOCKS
// GET /api/blocks?limit=20
// ============================================================

pub async fn get_blocks(
    State(state): State<AppState>,
    Query(query): Query<BlocksQuery>,
) -> Result<Json<Vec<BlockSummary>>, StatusCode> {
    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    let offset = query.offset.unwrap_or(0).max(0);

    let blocks = sqlx::query_as::<_, BlockSummary>(
        r#"
        SELECT
            height,
            hash,
            previous_hash,
            timestamp,
            tx_count
        FROM blocks
        ORDER BY height DESC
        LIMIT $1
        OFFSET $2
        "#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.pool)
    .await
    .map_err(|error| {
        eprintln!("Failed to fetch blocks: {}", error);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(blocks))
}

// ============================================================
// BLOCK BY HEIGHT
// GET /api/blocks/{height}
// ============================================================

pub async fn get_block(
    State(state): State<AppState>,
    Path(height): Path<i64>,
) -> Result<Json<BlockResponse>, StatusCode> {
    let block = sqlx::query_as::<_, BlockSummary>(
        r#"
        SELECT
            height,
            hash,
            previous_hash,
            timestamp,
            tx_count
        FROM blocks
        WHERE height = $1
        "#,
    )
    .bind(height)
    .fetch_optional(&state.pool)
    .await
    .map_err(|error| {
        eprintln!("Failed to fetch block {}: {}", height, error);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let block = match block {
        Some(block) => block,
        None => return Err(StatusCode::NOT_FOUND),
    };

    build_block_response(&state, block).await
}

// ============================================================
// BLOCK BY HASH
// GET /api/blocks/hash/{hash}
// ============================================================

pub async fn get_block_by_hash(
    State(state): State<AppState>,
    Path(hash): Path<String>,
) -> Result<Json<BlockResponse>, StatusCode> {
    let block = sqlx::query_as::<_, BlockSummary>(
        r#"
        SELECT
            height,
            hash,
            previous_hash,
            timestamp,
            tx_count
        FROM blocks
        WHERE hash = $1
        "#,
    )
    .bind(&hash)
    .fetch_optional(&state.pool)
    .await
    .map_err(|error| {
        eprintln!("Failed to fetch block by hash {}: {}", hash, error);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let block = match block {
        Some(block) => block,
        None => return Err(StatusCode::NOT_FOUND),
    };

    build_block_response(&state, block).await
}

// ============================================================
// BUILD BLOCK DETAILS
// ============================================================

async fn build_block_response(
    state: &AppState,
    block: BlockSummary,
) -> Result<Json<BlockResponse>, StatusCode> {
    let transactions = sqlx::query_as::<_, BlockTransaction>(
        r#"
        SELECT
            t.txid,
            t.position
        FROM transactions t
        JOIN blocks b
            ON b.id = t.block_id
        WHERE b.height = $1
        ORDER BY t.position ASC
        "#,
    )
    .bind(block.height)
    .fetch_all(&state.pool)
    .await
    .map_err(|error| {
        eprintln!(
            "Failed to fetch transactions for block {}: {}",
            block.height, error
        );

        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(BlockResponse {
        height: block.height,
        hash: block.hash,
        previous_hash: block.previous_hash,
        timestamp: block.timestamp,
        tx_count: block.tx_count,
        transactions,
    }))
}

// ============================================================
// TRANSACTION
// ============================================================

#[derive(Serialize, sqlx::FromRow)]
pub struct TransactionInfo {
    txid: String,
    block_height: i64,
    position: i32,
    version: i32,
    lock_time: i64,
    is_coinbase: bool,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct InputResponse {
    vin: i32,
    prev_txid: Option<String>,
    prev_vout: Option<i64>,
    script_sig: Option<String>,
    sequence: i64,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct OutputResponse {
    vout: i32,
    value: i64,
    script_pubkey: String,
    spent: bool,
    spent_by_txid: Option<String>,
    spent_by_vin: Option<i32>,
}

#[derive(Serialize)]
pub struct TransactionResponse {
    transaction: TransactionInfo,
    inputs: Vec<InputResponse>,
    outputs: Vec<OutputResponse>,
}

pub async fn get_transaction(
    State(state): State<AppState>,
    Path(txid): Path<String>,
) -> Result<Json<TransactionResponse>, StatusCode> {
    let transaction = sqlx::query_as::<_, TransactionInfo>(
        r#"
            SELECT
                t.txid,
                b.height AS block_height,
                t.position,
                t.version,
                t.lock_time,
                t.is_coinbase
            FROM transactions t
            JOIN blocks b
                ON b.id = t.block_id
            WHERE t.txid = $1
            "#,
    )
    .bind(&txid)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let transaction = match transaction {
        Some(tx) => tx,
        None => return Err(StatusCode::NOT_FOUND),
    };

    let inputs = sqlx::query_as::<_, InputResponse>(
        r#"
        SELECT
            i.vin,
            i.prev_txid,
            i.prev_vout,
            i.script_sig,
            i.sequence
        FROM inputs i
        JOIN transactions t
            ON t.id = i.transaction_id
        WHERE t.txid = $1
        ORDER BY i.vin
        "#,
    )
    .bind(&txid)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let outputs = sqlx::query_as::<_, OutputResponse>(
        r#"
        SELECT
            o.vout,
            o.value,
            o.script_pubkey,
            o.spent,
            o.spent_by_txid,
            o.spent_by_vin
        FROM outputs o
        JOIN transactions t
            ON t.id = o.transaction_id
        WHERE t.txid = $1
        ORDER BY o.vout
        "#,
    )
    .bind(&txid)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(TransactionResponse {
        transaction,
        inputs,
        outputs,
    }))
}

// ============================================================
// UTXOS
// ============================================================

/// Outputs of a coinbase transaction can be spent only in a block at least
/// this many blocks above the one that created them (consensus rule).
pub const COINBASE_MATURITY: i64 = 100;

#[derive(Serialize, sqlx::FromRow)]
pub struct UtxoResponse {
    txid: String,
    vout: i32,
    value: i64,
    script_pubkey: String,
    /// p2pkh, p2sh, p2wpkh, p2wsh, p2tr or unknown.
    script_type: String,
    is_coinbase: bool,
    block_height: i64,
    /// Counted against the indexer's tip.
    confirmations: i64,
    /// `false` only for a coinbase output with fewer than
    /// [`COINBASE_MATURITY`] confirmations: no consensus rule stops it being
    /// spent in the next block. Whether anyone holds the keys is unknowable.
    mature: bool,
}

/// Columns shared by every UTXO listing. `$1` is [`COINBASE_MATURITY`].
macro_rules! utxo_select {
    ($where:literal) => {
        concat!(
            r#"
            SELECT
                t.txid,
                o.vout,
                o.value,
                o.script_pubkey,
                o.script_type,
                t.is_coinbase,
                b.height AS block_height,
                tip.height - b.height + 1 AS confirmations,
                (NOT t.is_coinbase OR tip.height - b.height + 1 >= $1) AS mature
            FROM outputs o
            JOIN transactions t
                ON t.id = o.transaction_id
            JOIN blocks b
                ON b.id = t.block_id
            CROSS JOIN (SELECT MAX(height) AS height FROM blocks) tip
            WHERE o.spent = FALSE
              AND o.provably_unspendable = FALSE
            "#,
            $where
        )
    };
}

pub async fn get_utxos(
    State(state): State<AppState>,
) -> Result<Json<Vec<UtxoResponse>>, StatusCode> {
    let utxos = sqlx::query_as::<_, UtxoResponse>(utxo_select!(
        r#"
        ORDER BY o.id DESC
        LIMIT 100
        "#
    ))
    .bind(COINBASE_MATURITY)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(utxos))
}

// ============================================================
// UTXO SUMMARY
// GET /api/utxos/summary
// ============================================================

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputTotals {
    count: i64,
    value: i64,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct ScriptTypeTotals {
    script_type: String,
    count: i64,
    value: i64,
}

/// How every indexed output is classified:
///
/// `outputs = provably_unspendable + spent + utxos` and
/// `utxos = immature_coinbase + spendable`.
#[derive(Serialize)]
pub struct UtxoSummaryResponse {
    indexed_height: Option<i64>,
    coinbase_maturity: i64,
    outputs: OutputTotals,
    provably_unspendable: OutputTotals,
    spent: OutputTotals,
    utxos: OutputTotals,
    immature_coinbase: OutputTotals,
    spendable: OutputTotals,
    utxos_by_script_type: Vec<ScriptTypeTotals>,
}

#[derive(sqlx::FromRow)]
struct SummaryRow {
    indexed_height: Option<i64>,
    outputs: i64,
    outputs_value: i64,
    unspendable: i64,
    unspendable_value: i64,
    spent: i64,
    spent_value: i64,
    utxos: i64,
    utxos_value: i64,
    immature: i64,
    immature_value: i64,
}

pub async fn get_utxo_summary(
    State(state): State<AppState>,
) -> Result<Json<UtxoSummaryResponse>, StatusCode> {
    let row = sqlx::query_as::<_, SummaryRow>(
        r#"
        WITH tip AS (
            SELECT MAX(height) AS height FROM blocks
        ),
        classified AS (
            SELECT
                o.value,
                o.provably_unspendable,
                o.spent,
                (o.spent = FALSE AND o.provably_unspendable = FALSE) AS utxo,
                (t.is_coinbase AND tip.height - b.height + 1 < $1) AS immature
            FROM outputs o
            JOIN transactions t
                ON t.id = o.transaction_id
            JOIN blocks b
                ON b.id = t.block_id
            CROSS JOIN tip
        )
        SELECT
            (SELECT height FROM tip) AS indexed_height,
            COUNT(*) AS outputs,
            COALESCE(SUM(value), 0)::BIGINT AS outputs_value,
            COUNT(*) FILTER (WHERE provably_unspendable) AS unspendable,
            COALESCE(SUM(value) FILTER (WHERE provably_unspendable), 0)::BIGINT AS unspendable_value,
            COUNT(*) FILTER (WHERE spent) AS spent,
            COALESCE(SUM(value) FILTER (WHERE spent), 0)::BIGINT AS spent_value,
            COUNT(*) FILTER (WHERE utxo) AS utxos,
            COALESCE(SUM(value) FILTER (WHERE utxo), 0)::BIGINT AS utxos_value,
            COUNT(*) FILTER (WHERE utxo AND immature) AS immature,
            COALESCE(SUM(value) FILTER (WHERE utxo AND immature), 0)::BIGINT AS immature_value
        FROM classified
        "#,
    )
    .bind(COINBASE_MATURITY)
    .fetch_one(&state.pool)
    .await
    .map_err(|error| {
        eprintln!("Failed to summarise UTXOs: {}", error);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let utxos_by_script_type = sqlx::query_as::<_, ScriptTypeTotals>(
        r#"
        SELECT
            script_type,
            COUNT(*) AS count,
            COALESCE(SUM(value), 0)::BIGINT AS value
        FROM outputs
        WHERE spent = FALSE
          AND provably_unspendable = FALSE
        GROUP BY script_type
        ORDER BY count DESC, script_type
        "#,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|error| {
        eprintln!("Failed to group UTXOs by script type: {}", error);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let totals = |count, value| OutputTotals { count, value };

    Ok(Json(UtxoSummaryResponse {
        indexed_height: row.indexed_height,
        coinbase_maturity: COINBASE_MATURITY,
        outputs: totals(row.outputs, row.outputs_value),
        provably_unspendable: totals(row.unspendable, row.unspendable_value),
        spent: totals(row.spent, row.spent_value),
        utxos: totals(row.utxos, row.utxos_value),
        immature_coinbase: totals(row.immature, row.immature_value),
        spendable: totals(
            row.utxos - row.immature,
            row.utxos_value - row.immature_value,
        ),
        utxos_by_script_type,
    }))
}

// ============================================================
// ADDRESS
// ============================================================

#[derive(Serialize)]
pub struct AddressResponse {
    address: String,
    received: i64,
    spent: i64,
    /// All unspent outputs: `immature_balance + spendable_balance`.
    balance: i64,
    /// Coinbase outputs still short of [`COINBASE_MATURITY`] confirmations.
    immature_balance: i64,
    spendable_balance: i64,
    transaction_count: i64,
}

/// One transaction's effect on an address's balance.
///
/// A transaction that both spends the address's coins and pays change back
/// to it is one row: `sent` and `received` are netted, never listed twice.
#[derive(Serialize)]
pub struct AddressTransactionResponse {
    txid: String,
    block_height: i64,
    timestamp: i64,
    position: i32,
    /// "received" (net > 0), "sent" (net < 0) or "self" (net = 0).
    direction: &'static str,
    /// Sats paid to the address by this transaction's outputs.
    received: i64,
    /// Sats of the address's earlier outputs consumed by this transaction's inputs.
    sent: i64,
    /// `received - sent`: the change in the address's balance.
    net: i64,
    /// `|net|`. For a plain receive this is the received amount, as before.
    value: i64,
    /// Every output this transaction paid to the address has been spent.
    /// `false` when it paid nothing to the address.
    spent: bool,
}

#[derive(sqlx::FromRow)]
struct AddressActivityRow {
    txid: String,
    block_height: i64,
    timestamp: i64,
    position: i32,
    received: i64,
    sent: i64,
    spent: bool,
}

impl From<AddressActivityRow> for AddressTransactionResponse {
    fn from(row: AddressActivityRow) -> Self {
        let net = row.received - row.sent;
        let direction = match net.signum() {
            1 => "received",
            -1 => "sent",
            _ => "self",
        };

        Self {
            txid: row.txid,
            block_height: row.block_height,
            timestamp: row.timestamp,
            position: row.position,
            direction,
            received: row.received,
            sent: row.sent,
            net,
            value: net.abs(),
            spent: row.spent,
        }
    }
}

/// Every confirmed transaction that pays to or spends from `address`,
/// newest first, one row per transaction.
async fn address_history(
    state: &AppState,
    address: &str,
) -> Result<Vec<AddressTransactionResponse>, sqlx::Error> {
    let rows = sqlx::query_as::<_, AddressActivityRow>(
        r#"
        WITH activity AS (
            -- Outputs paying to the address.
            SELECT
                o.transaction_id,
                o.value AS received,
                0::BIGINT AS sent,
                o.spent AS output_spent
            FROM outputs o
            WHERE o.address = $1

            UNION ALL

            -- Inputs spending one of the address's outputs.
            SELECT
                i.transaction_id,
                0::BIGINT AS received,
                o.value AS sent,
                NULL AS output_spent
            FROM outputs o
            JOIN transactions ot
                ON ot.id = o.transaction_id
            JOIN inputs i
                ON i.prev_txid = ot.txid
               AND i.prev_vout = o.vout
            WHERE o.address = $1
        )
        SELECT
            t.txid,
            b.height AS block_height,
            b.timestamp,
            t.position,
            SUM(a.received)::BIGINT AS received,
            SUM(a.sent)::BIGINT AS sent,
            COALESCE(BOOL_AND(a.output_spent), FALSE) AS spent
        FROM activity a
        JOIN transactions t
            ON t.id = a.transaction_id
        JOIN blocks b
            ON b.id = t.block_id
        GROUP BY t.id, t.txid, b.height, b.timestamp, t.position
        ORDER BY b.height DESC, t.position DESC
        "#,
    )
    .bind(address)
    .fetch_all(&state.pool)
    .await?;

    Ok(rows.into_iter().map(Into::into).collect())
}

// ============================================================
// ADDRESS SUMMARY
// GET /api/addresses/{address}
// ============================================================

pub async fn get_address(
    State(state): State<AppState>,
    Path(address): Path<String>,
) -> Result<Json<AddressResponse>, StatusCode> {
    let exists: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS(
            SELECT 1
            FROM outputs
            WHERE address = $1
        )
        "#,
    )
    .bind(&address)
    .fetch_one(&state.pool)
    .await
    .map_err(|error| {
        eprintln!("Failed to check address {}: {}", address, error);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if !exists {
        return Err(StatusCode::NOT_FOUND);
    }

    let received: i64 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(SUM(value), 0)::BIGINT
        FROM outputs
        WHERE address = $1
        "#,
    )
    .bind(&address)
    .fetch_one(&state.pool)
    .await
    .map_err(|error| {
        eprintln!("Failed to calculate received amount: {}", error);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let (balance, immature_balance): (i64, i64) = sqlx::query_as(
        r#"
        SELECT
            COALESCE(SUM(o.value), 0)::BIGINT,
            COALESCE(SUM(o.value) FILTER (
                WHERE t.is_coinbase
                  AND (SELECT MAX(height) FROM blocks) - b.height + 1 < $2
            ), 0)::BIGINT
        FROM outputs o
        JOIN transactions t
            ON t.id = o.transaction_id
        JOIN blocks b
            ON b.id = t.block_id
        WHERE o.address = $1
          AND o.spent = FALSE
          AND o.provably_unspendable = FALSE
        "#,
    )
    .bind(&address)
    .bind(COINBASE_MATURITY)
    .fetch_one(&state.pool)
    .await
    .map_err(|error| {
        eprintln!("Failed to calculate address balance: {}", error);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let spent = received - balance;

    let transaction_count = address_history(&state, &address)
        .await
        .map_err(|error| {
            eprintln!("Failed to count address transactions: {}", error);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .len() as i64;

    Ok(Json(AddressResponse {
        address,
        received,
        spent,
        balance,
        immature_balance,
        spendable_balance: balance - immature_balance,
        transaction_count,
    }))
}

// ============================================================
// ADDRESS UTXOS
// GET /api/addresses/{address}/utxos
// ============================================================

pub async fn get_address_utxos(
    State(state): State<AppState>,
    Path(address): Path<String>,
) -> Result<Json<Vec<UtxoResponse>>, StatusCode> {
    let utxos = sqlx::query_as::<_, UtxoResponse>(utxo_select!(
        r#"
          AND o.address = $2
        ORDER BY o.id DESC
        "#
    ))
    .bind(COINBASE_MATURITY)
    .bind(&address)
    .fetch_all(&state.pool)
    .await
    .map_err(|error| {
        eprintln!("Failed to fetch UTXOs for {}: {}", address, error);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(utxos))
}

// ============================================================
// ADDRESS TRANSACTIONS
// GET /api/addresses/{address}/transactions
// ============================================================

pub async fn get_address_transactions(
    State(state): State<AppState>,
    Path(address): Path<String>,
) -> Result<Json<Vec<AddressTransactionResponse>>, StatusCode> {
    let transactions = address_history(&state, &address).await.map_err(|error| {
        eprintln!(
            "Failed to fetch transactions for address {}: {}",
            address, error
        );

        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(transactions))
}

// ============================================================
// SEARCH
// ============================================================

#[derive(Serialize)]
pub struct SearchResponse {
    result_type: String,
    value: String,
}

// ============================================================
// UNIVERSAL SEARCH
// GET /api/search/{query}
// ============================================================

pub async fn search(
    State(state): State<AppState>,
    Path(query): Path<String>,
) -> Result<Json<SearchResponse>, StatusCode> {
    let query = query.trim();

    if query.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    // --------------------------------------------------------
    // 1. BLOCK HEIGHT
    // --------------------------------------------------------

    if let Ok(height) = query.parse::<i64>() {
        let exists: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM blocks
                WHERE height = $1
            )
            "#,
        )
        .bind(height)
        .fetch_one(&state.pool)
        .await
        .map_err(|error| {
            eprintln!("Block-height search failed: {}", error);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        if exists {
            return Ok(Json(SearchResponse {
                result_type: "block".to_string(),
                value: height.to_string(),
            }));
        }
    }

    // --------------------------------------------------------
    // 2. BLOCK HASH
    // --------------------------------------------------------

    let block_hash: Option<String> = sqlx::query_scalar(
        r#"
        SELECT hash
        FROM blocks
        WHERE hash = $1
        "#,
    )
    .bind(query)
    .fetch_optional(&state.pool)
    .await
    .map_err(|error| {
        eprintln!("Block-hash search failed: {}", error);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if let Some(hash) = block_hash {
        return Ok(Json(SearchResponse {
            result_type: "block".to_string(),
            value: hash,
        }));
    }

    // --------------------------------------------------------
    // 3. TRANSACTION ID
    // --------------------------------------------------------

    let txid: Option<String> = sqlx::query_scalar(
        r#"
        SELECT txid
        FROM transactions
        WHERE txid = $1
        "#,
    )
    .bind(query)
    .fetch_optional(&state.pool)
    .await
    .map_err(|error| {
        eprintln!("Transaction search failed: {}", error);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if let Some(txid) = txid {
        return Ok(Json(SearchResponse {
            result_type: "transaction".to_string(),
            value: txid,
        }));
    }

    // --------------------------------------------------------
    // 4. ADDRESS
    // --------------------------------------------------------

    let address: Option<String> = sqlx::query_scalar(
        r#"
        SELECT address
        FROM outputs
        WHERE address = $1
        LIMIT 1
        "#,
    )
    .bind(query)
    .fetch_optional(&state.pool)
    .await
    .map_err(|error| {
        eprintln!("Address search failed: {}", error);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if let Some(address) = address {
        return Ok(Json(SearchResponse {
            result_type: "address".to_string(),
            value: address,
        }));
    }

    Err(StatusCode::NOT_FOUND)
}

// ============================================================
// ADDRESS WATCH
// ============================================================

#[derive(Serialize, sqlx::FromRow)]
pub struct WatchedAddress {
    address: String,
}

#[derive(Serialize)]
pub struct WatchActivityResponse {
    address: String,
    watching: bool,
    received: i64,
    balance: i64,
    transaction_count: i64,
    latest_txid: Option<String>,
    latest_block_height: Option<i64>,
}

pub async fn watch_address(
    State(state): State<AppState>,
    Path(address): Path<String>,
) -> Result<(StatusCode, Json<WatchedAddress>), StatusCode> {
    // Only allow addresses that the indexer has already seen.
    let exists: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS(
            SELECT 1
            FROM outputs
            WHERE address = $1
        )
        "#,
    )
    .bind(&address)
    .fetch_one(&state.pool)
    .await
    .map_err(|error| {
        eprintln!("Failed to validate address: {}", error);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if !exists {
        return Err(StatusCode::NOT_FOUND);
    }

    sqlx::query(
        r#"
        INSERT INTO watched_addresses (address)
        VALUES ($1)
        ON CONFLICT (address) DO NOTHING
        "#,
    )
    .bind(&address)
    .execute(&state.pool)
    .await
    .map_err(|error| {
        eprintln!("Failed to watch address: {}", error);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((StatusCode::CREATED, Json(WatchedAddress { address })))
}

pub async fn get_watched_addresses(
    State(state): State<AppState>,
) -> Result<Json<Vec<WatchedAddress>>, StatusCode> {
    let addresses = sqlx::query_as::<_, WatchedAddress>(
        r#"
        SELECT address
        FROM watched_addresses
        ORDER BY created_at DESC
        "#,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|error| {
        eprintln!("Failed to fetch watched addresses: {}", error);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(addresses))
}
pub async fn get_watch_activity(
    State(state): State<AppState>,
    Path(address): Path<String>,
) -> Result<Json<WatchActivityResponse>, StatusCode> {
    let watching: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS(
            SELECT 1
            FROM watched_addresses
            WHERE address = $1
        )
        "#,
    )
    .bind(&address)
    .fetch_one(&state.pool)
    .await
    .map_err(|error| {
        eprintln!("Failed to check watched address: {}", error);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if !watching {
        return Err(StatusCode::NOT_FOUND);
    }

    let received: i64 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(SUM(value), 0)::BIGINT
        FROM outputs
        WHERE address = $1
        "#,
    )
    .bind(&address)
    .fetch_one(&state.pool)
    .await
    .map_err(|error| {
        eprintln!("Failed to calculate watched received value: {}", error);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let balance: i64 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(SUM(value), 0)::BIGINT
        FROM outputs
        WHERE address = $1
          AND spent = FALSE
        "#,
    )
    .bind(&address)
    .fetch_one(&state.pool)
    .await
    .map_err(|error| {
        eprintln!("Failed to calculate watched balance: {}", error);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let history = address_history(&state, &address).await.map_err(|error| {
        eprintln!("Failed to fetch watched address activity: {}", error);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let transaction_count = history.len() as i64;

    let (latest_txid, latest_block_height) = match history.into_iter().next() {
        Some(latest) => (Some(latest.txid), Some(latest.block_height)),
        None => (None, None),
    };

    Ok(Json(WatchActivityResponse {
        address,
        watching,
        received,
        balance,
        transaction_count,
        latest_txid,
        latest_block_height,
    }))
}
pub async fn unwatch_address(
    State(state): State<AppState>,
    Path(address): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let result = sqlx::query(
        r#"
        DELETE FROM watched_addresses
        WHERE address = $1
        "#,
    )
    .bind(&address)
    .execute(&state.pool)
    .await
    .map_err(|error| {
        eprintln!("Failed to remove watched address: {}", error);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
#[path = "handlers_tests.rs"]
mod tests;
