//! Unconfirmed transactions and confirmation status.
//!
//! Mempool rows mirror Bitcoin Core's mempool and are never canonical.
//! Every query here excludes transactions already indexed as confirmed, so
//! a transaction is never reported as both confirmed and unconfirmed while
//! the mempool tables catch up with a new block.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Serialize;

use crate::api::state::AppState;

fn internal_error(context: &str, error: sqlx::Error) -> StatusCode {
    eprintln!("{context}: {error}");
    StatusCode::INTERNAL_SERVER_ERROR
}

// ============================================================
// MEMPOOL LIST
// GET /api/mempool
// ============================================================

#[derive(Serialize, sqlx::FromRow)]
pub struct MempoolTransactionSummary {
    txid: String,
    /// Fee in sats, as reported by Bitcoin Core.
    fee: i64,
    vsize: i64,
    /// sat/vB
    fee_rate: f64,
    /// Unix time the transaction entered Core's mempool.
    entered_at: i64,
    input_count: i64,
    output_count: i64,
    output_value: i64,
}

pub async fn get_mempool(
    State(state): State<AppState>,
) -> Result<Json<Vec<MempoolTransactionSummary>>, StatusCode> {
    let transactions = sqlx::query_as::<_, MempoolTransactionSummary>(
        r#"
        SELECT
            m.txid,
            m.fee,
            m.vsize,
            m.fee::FLOAT8 / GREATEST(m.vsize, 1) AS fee_rate,
            m.entered_at,
            (SELECT COUNT(*) FROM mempool_inputs i
             WHERE i.mempool_transaction_id = m.id) AS input_count,
            (SELECT COUNT(*) FROM mempool_outputs o
             WHERE o.mempool_transaction_id = m.id) AS output_count,
            (SELECT COALESCE(SUM(o.value), 0)::BIGINT FROM mempool_outputs o
             WHERE o.mempool_transaction_id = m.id) AS output_value
        FROM mempool_transactions m
        WHERE NOT EXISTS (
            SELECT 1 FROM transactions t WHERE t.txid = m.txid
        )
        ORDER BY m.entered_at DESC, m.txid
        "#,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|e| internal_error("Failed to fetch mempool", e))?;

    Ok(Json(transactions))
}

// ============================================================
// MEMPOOL TRANSACTION
// GET /api/mempool/{txid}
// ============================================================

#[derive(Serialize, sqlx::FromRow)]
pub struct MempoolTransactionInfo {
    txid: String,
    version: i32,
    lock_time: i64,
    fee: i64,
    vsize: i64,
    fee_rate: f64,
    entered_at: i64,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct MempoolInputResponse {
    vin: i32,
    prev_txid: String,
    prev_vout: i64,
    script_sig: Option<String>,
    sequence: i64,
    /// Value and address of the output being spent, when the indexer has it.
    prev_value: Option<i64>,
    prev_address: Option<String>,
    /// "confirmed" (spends an indexed block output), "mempool" (spends
    /// another unconfirmed transaction) or null (unknown to the indexer).
    prev_source: Option<String>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct MempoolOutputResponse {
    vout: i32,
    value: i64,
    script_pubkey: String,
    address: Option<String>,
    /// Another unconfirmed transaction spending this output, if any.
    spent_by_mempool_txid: Option<String>,
}

#[derive(Serialize)]
pub struct MempoolTransactionResponse {
    transaction: MempoolTransactionInfo,
    inputs: Vec<MempoolInputResponse>,
    outputs: Vec<MempoolOutputResponse>,
}

pub async fn get_mempool_transaction(
    State(state): State<AppState>,
    Path(txid): Path<String>,
) -> Result<Json<MempoolTransactionResponse>, StatusCode> {
    let transaction = sqlx::query_as::<_, MempoolTransactionInfo>(
        r#"
        SELECT
            m.txid,
            m.version,
            m.lock_time,
            m.fee,
            m.vsize,
            m.fee::FLOAT8 / GREATEST(m.vsize, 1) AS fee_rate,
            m.entered_at
        FROM mempool_transactions m
        WHERE m.txid = $1
          AND NOT EXISTS (
              SELECT 1 FROM transactions t WHERE t.txid = m.txid
          )
        "#,
    )
    .bind(&txid)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| internal_error("Failed to fetch mempool transaction", e))?
    .ok_or(StatusCode::NOT_FOUND)?;

    let inputs = sqlx::query_as::<_, MempoolInputResponse>(
        r#"
        SELECT
            i.vin,
            i.prev_txid,
            i.prev_vout,
            i.script_sig,
            i.sequence,
            COALESCE(co.value, mo.value) AS prev_value,
            COALESCE(co.address, mo.address) AS prev_address,
            CASE
                WHEN co.id IS NOT NULL THEN 'confirmed'
                WHEN mo.id IS NOT NULL THEN 'mempool'
            END AS prev_source
        FROM mempool_inputs i
        JOIN mempool_transactions m
            ON m.id = i.mempool_transaction_id
        LEFT JOIN transactions ct
            ON ct.txid = i.prev_txid
        LEFT JOIN outputs co
            ON co.transaction_id = ct.id
           AND co.vout = i.prev_vout
        LEFT JOIN mempool_transactions mt
            ON mt.txid = i.prev_txid
        LEFT JOIN mempool_outputs mo
            ON mo.mempool_transaction_id = mt.id
           AND mo.vout = i.prev_vout
        WHERE m.txid = $1
        ORDER BY i.vin
        "#,
    )
    .bind(&txid)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| internal_error("Failed to fetch mempool inputs", e))?;

    let outputs = sqlx::query_as::<_, MempoolOutputResponse>(
        r#"
        SELECT
            o.vout,
            o.value,
            o.script_pubkey,
            o.address,
            (
                SELECT st.txid
                FROM mempool_inputs si
                JOIN mempool_transactions st
                    ON st.id = si.mempool_transaction_id
                WHERE si.prev_txid = m.txid
                  AND si.prev_vout = o.vout
                LIMIT 1
            ) AS spent_by_mempool_txid
        FROM mempool_outputs o
        JOIN mempool_transactions m
            ON m.id = o.mempool_transaction_id
        WHERE m.txid = $1
        ORDER BY o.vout
        "#,
    )
    .bind(&txid)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| internal_error("Failed to fetch mempool outputs", e))?;

    Ok(Json(MempoolTransactionResponse {
        transaction,
        inputs,
        outputs,
    }))
}

// ============================================================
// CONFIRMATION STATUS
// GET /api/transactions/{txid}/status
// ============================================================

#[derive(Serialize)]
pub struct TransactionStatusResponse {
    txid: String,
    /// "confirmed" or "unconfirmed".
    status: &'static str,
    block_height: Option<i64>,
    block_hash: Option<String>,
    /// Counted against the indexer's tip, not necessarily Core's.
    confirmations: Option<i64>,
    /// Unix time it entered the mempool (unconfirmed only).
    entered_at: Option<i64>,
}

pub async fn get_transaction_status(
    State(state): State<AppState>,
    Path(txid): Path<String>,
) -> Result<Json<TransactionStatusResponse>, StatusCode> {
    let confirmed: Option<(i64, String, i64)> = sqlx::query_as(
        r#"
        SELECT
            b.height,
            b.hash,
            (SELECT MAX(height) FROM blocks) - b.height + 1
        FROM transactions t
        JOIN blocks b
            ON b.id = t.block_id
        WHERE t.txid = $1
        "#,
    )
    .bind(&txid)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| internal_error("Failed to fetch transaction status", e))?;

    if let Some((height, hash, confirmations)) = confirmed {
        return Ok(Json(TransactionStatusResponse {
            txid,
            status: "confirmed",
            block_height: Some(height),
            block_hash: Some(hash),
            confirmations: Some(confirmations),
            entered_at: None,
        }));
    }

    let entered_at: Option<i64> =
        sqlx::query_scalar("SELECT entered_at FROM mempool_transactions WHERE txid = $1")
            .bind(&txid)
            .fetch_optional(&state.pool)
            .await
            .map_err(|e| internal_error("Failed to fetch mempool status", e))?;

    match entered_at {
        Some(entered_at) => Ok(Json(TransactionStatusResponse {
            txid,
            status: "unconfirmed",
            block_height: None,
            block_hash: None,
            confirmations: None,
            entered_at: Some(entered_at),
        })),
        None => Err(StatusCode::NOT_FOUND),
    }
}

#[cfg(test)]
mod tests {
    use bitcoin::OutPoint;

    use super::*;
    use crate::{
        api::handlers::get_utxos,
        indexer::{mempool::sync_mempool, sync::sync_chain},
        test_support::{
            MockChain, MockMempool, TestDb, build_chain, coinbase, out, p2wpkh, push_block, tx,
        },
    };

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL"]
    async fn transaction_goes_from_unconfirmed_to_confirmed() {
        let db = TestDb::new().await;
        let state = AppState::new(db.pool.clone());

        let mut blocks = build_chain(&[], 2, 'a'); // 0..=1
        sync_chain(&MockChain::new(blocks.clone()), &db.pool)
            .await
            .unwrap();

        let funding = OutPoint::new(coinbase(1, 'a').compute_txid(), 0);
        let parent = tx(&[funding], vec![out(40_000, p2wpkh(7))]);
        let child = tx(
            &[OutPoint::new(parent.compute_txid(), 0)],
            vec![out(39_000, p2wpkh(8))],
        );
        let parent_txid = parent.compute_txid().to_string();
        let child_txid = child.compute_txid().to_string();

        // `sendtoaddress`: both enter Core's mempool.
        sync_mempool(&MockMempool::with(&[parent.clone(), child]), &db.pool)
            .await
            .unwrap();

        let status = get_transaction_status(State(state.clone()), Path(parent_txid.clone()))
            .await
            .unwrap()
            .0;
        assert_eq!(status.status, "unconfirmed");
        assert_eq!(status.block_height, None);
        assert!(status.entered_at.is_some());

        let list = get_mempool(State(state.clone())).await.unwrap().0;
        assert_eq!(list.len(), 2);
        let listed = list.iter().find(|t| t.txid == parent_txid).unwrap();
        assert_eq!((listed.input_count, listed.output_count), (1, 1));
        assert_eq!(listed.output_value, 40_000);
        assert_eq!(listed.fee_rate, listed.fee as f64 / listed.vsize as f64);

        let detail = get_mempool_transaction(State(state.clone()), Path(parent_txid.clone()))
            .await
            .unwrap()
            .0;
        assert_eq!(detail.inputs[0].prev_source.as_deref(), Some("confirmed"));
        assert_eq!(detail.inputs[0].prev_value, Some(50_0000_0000));
        assert_eq!(
            detail.outputs[0].spent_by_mempool_txid.as_deref(),
            Some(child_txid.as_str())
        );

        let child_detail = get_mempool_transaction(State(state.clone()), Path(child_txid))
            .await
            .unwrap()
            .0;
        assert_eq!(
            child_detail.inputs[0].prev_source.as_deref(),
            Some("mempool")
        );
        assert_eq!(child_detail.inputs[0].prev_value, Some(40_000));

        // The confirmed UTXO set ignores unconfirmed spends.
        let utxos = get_utxos(State(state.clone())).await.unwrap().0;
        assert_eq!(
            utxos.len(),
            2,
            "both coinbases, including the one spent in the mempool"
        );

        // Mine the parent only; Core keeps the child in its mempool.
        push_block(&mut blocks, 'a', vec![parent]);
        sync_chain(&MockChain::new(blocks), &db.pool).await.unwrap();

        // Before the mempool refresh the stale row must already be hidden.
        let status = get_transaction_status(State(state.clone()), Path(parent_txid.clone()))
            .await
            .unwrap()
            .0;
        assert_eq!(status.status, "confirmed");
        assert_eq!(status.block_height, Some(2));
        assert_eq!(status.confirmations, Some(1));
        assert_eq!(
            get_mempool_transaction(State(state.clone()), Path(parent_txid.clone()))
                .await
                .err(),
            Some(StatusCode::NOT_FOUND)
        );
        let list = get_mempool(State(state.clone())).await.unwrap().0;
        assert_eq!(list.len(), 1);
        assert_ne!(list[0].txid, parent_txid);

        db.cleanup().await;
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL"]
    async fn unknown_transaction_status_is_not_found() {
        let db = TestDb::new().await;
        let state = AppState::new(db.pool.clone());

        let unknown = coinbase(42, 'x').compute_txid().to_string();
        assert_eq!(
            get_transaction_status(State(state), Path(unknown))
                .await
                .err(),
            Some(StatusCode::NOT_FOUND)
        );

        db.cleanup().await;
    }
}
