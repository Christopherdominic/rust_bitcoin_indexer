use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};

use crate::api::state::AppState;

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

#[derive(Serialize)]
pub struct StatusResponse {
    indexed_height: Option<i64>,
    blocks: i64,
    transactions: i64,
    inputs: i64,
    outputs: i64,
    unspent_outputs: i64,
}

pub async fn status(State(state): State<AppState>) -> Result<Json<StatusResponse>, StatusCode> {
    let indexed_height: Option<i64> = sqlx::query_scalar("SELECT MAX(height) FROM blocks")
        .fetch_one(&state.pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let blocks: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM blocks")
        .fetch_one(&state.pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let transactions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM transactions")
        .fetch_one(&state.pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let inputs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM inputs")
        .fetch_one(&state.pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let outputs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM outputs")
        .fetch_one(&state.pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let unspent_outputs: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM outputs WHERE spent = FALSE")
            .fetch_one(&state.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(StatusResponse {
        indexed_height,
        blocks,
        transactions,
        inputs,
        outputs,
        unspent_outputs,
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

#[derive(Serialize, sqlx::FromRow)]
pub struct UtxoResponse {
    txid: String,
    vout: i32,
    value: i64,
    script_pubkey: String,
}

pub async fn get_utxos(
    State(state): State<AppState>,
) -> Result<Json<Vec<UtxoResponse>>, StatusCode> {
    let utxos = sqlx::query_as::<_, UtxoResponse>(
        r#"
        SELECT
            t.txid,
            o.vout,
            o.value,
            o.script_pubkey
        FROM outputs o
        JOIN transactions t
            ON t.id = o.transaction_id
        WHERE o.spent = FALSE
        ORDER BY o.id DESC
        LIMIT 100
        "#,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(utxos))
}
// ============================================================
// ADDRESS
// ============================================================

#[derive(Serialize)]
pub struct AddressResponse {
    address: String,
    received: i64,
    spent: i64,
    balance: i64,
    transaction_count: i64,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct AddressUtxoResponse {
    txid: String,
    vout: i32,
    value: i64,
    script_pubkey: String,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct AddressTransactionResponse {
    txid: String,
    block_height: i64,
    timestamp: i64,
    value: i64,
    spent: bool,
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
        eprintln!("Failed to calculate address balance: {}", error);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let spent = received - balance;

    let transaction_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(DISTINCT t.txid)
        FROM outputs o
        JOIN transactions t
            ON t.id = o.transaction_id
        WHERE o.address = $1
        "#,
    )
    .bind(&address)
    .fetch_one(&state.pool)
    .await
    .map_err(|error| {
        eprintln!("Failed to count address transactions: {}", error);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(AddressResponse {
        address,
        received,
        spent,
        balance,
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
) -> Result<Json<Vec<AddressUtxoResponse>>, StatusCode> {
    let utxos = sqlx::query_as::<_, AddressUtxoResponse>(
        r#"
        SELECT
            t.txid,
            o.vout,
            o.value,
            o.script_pubkey
        FROM outputs o
        JOIN transactions t
            ON t.id = o.transaction_id
        WHERE o.address = $1
          AND o.spent = FALSE
        ORDER BY o.id DESC
        "#,
    )
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
    let transactions = sqlx::query_as::<_, AddressTransactionResponse>(
        r#"
        SELECT
            t.txid,
            b.height AS block_height,
            b.timestamp,
            o.value,
            o.spent
        FROM outputs o
        JOIN transactions t
            ON t.id = o.transaction_id
        JOIN blocks b
            ON b.id = t.block_id
        WHERE o.address = $1
        ORDER BY b.height DESC, t.position DESC
        "#,
    )
    .bind(&address)
    .fetch_all(&state.pool)
    .await
    .map_err(|error| {
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

    let transaction_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(DISTINCT t.txid)
        FROM outputs o
        JOIN transactions t
            ON t.id = o.transaction_id
        WHERE o.address = $1
        "#,
    )
    .bind(&address)
    .fetch_one(&state.pool)
    .await
    .map_err(|error| {
        eprintln!("Failed to count watched transactions: {}", error);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let latest: Option<(String, i64)> = sqlx::query_as(
        r#"
        SELECT
            t.txid,
            b.height
        FROM outputs o
        JOIN transactions t
            ON t.id = o.transaction_id
        JOIN blocks b
            ON b.id = t.block_id
        WHERE o.address = $1
        ORDER BY b.height DESC, t.position DESC
        LIMIT 1
        "#,
    )
    .bind(&address)
    .fetch_optional(&state.pool)
    .await
    .map_err(|error| {
        eprintln!("Failed to fetch latest watched activity: {}", error);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let (latest_txid, latest_block_height) = match latest {
        Some((txid, height)) => (Some(txid), Some(height)),
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
