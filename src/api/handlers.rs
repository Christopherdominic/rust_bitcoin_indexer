use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Serialize;

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

pub async fn status(
    State(state): State<AppState>,
) -> Result<Json<StatusResponse>, StatusCode> {
    let indexed_height: Option<i64> =
        sqlx::query_scalar("SELECT MAX(height) FROM blocks")
            .fetch_one(&state.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let blocks: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM blocks")
            .fetch_one(&state.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let transactions: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM transactions")
            .fetch_one(&state.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let inputs: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM inputs")
            .fetch_one(&state.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let outputs: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM outputs")
            .fetch_one(&state.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let unspent_outputs: i64 =
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM outputs WHERE spent = FALSE",
        )
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
// BLOCK
// ============================================================

#[derive(Serialize, sqlx::FromRow)]
pub struct BlockResponse {
    height: i64,
    hash: String,
    previous_hash: String,
    timestamp: i64,
    tx_count: i32,
}

pub async fn get_block(
    State(state): State<AppState>,
    Path(height): Path<i64>,
) -> Result<Json<BlockResponse>, StatusCode> {
    let block = sqlx::query_as::<_, BlockResponse>(
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
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    match block {
        Some(block) => Ok(Json(block)),
        None => Err(StatusCode::NOT_FOUND),
    }
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
    let transaction =
        sqlx::query_as::<_, TransactionInfo>(
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