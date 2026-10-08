use anyhow::Result;
use bitcoin::Transaction as BitcoinTransaction;
use sqlx::{PgPool, Postgres, Transaction};

use crate::db::outputs::output_address;

/// Fee, size and arrival time Bitcoin Core reports for a mempool entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MempoolEntry {
    pub fee: u64,
    pub vsize: u64,
    pub entered_at: u64,
}

pub async fn get_mempool_txids(pool: &PgPool) -> Result<Vec<String>> {
    let txids = sqlx::query_scalar("SELECT txid FROM mempool_transactions")
        .fetch_all(pool)
        .await?;

    Ok(txids)
}

/// Which of `txids` are already in a confirmed block.
pub async fn get_confirmed_txids(pool: &PgPool, txids: &[String]) -> Result<Vec<String>> {
    let confirmed = sqlx::query_scalar("SELECT txid FROM transactions WHERE txid = ANY($1)")
        .bind(txids)
        .fetch_all(pool)
        .await?;

    Ok(confirmed)
}

/// Removes mempool transactions (and, by cascade, their inputs and outputs).
pub async fn delete_mempool_transactions(
    db: &mut Transaction<'_, Postgres>,
    txids: &[String],
) -> Result<u64> {
    let removed = sqlx::query("DELETE FROM mempool_transactions WHERE txid = ANY($1)")
        .bind(txids)
        .execute(&mut **db)
        .await?
        .rows_affected();

    Ok(removed)
}

/// Stores an unconfirmed transaction. Returns `false` if it was already stored.
pub async fn save_mempool_transaction(
    db: &mut Transaction<'_, Postgres>,
    tx: &BitcoinTransaction,
    entry: &MempoolEntry,
) -> Result<bool> {
    let id: Option<i64> = sqlx::query_scalar(
        r#"
        INSERT INTO mempool_transactions (
            txid,
            version,
            lock_time,
            fee,
            vsize,
            entered_at
        )
        VALUES ($1, $2, $3, $4, $5, $6)

        ON CONFLICT (txid) DO NOTHING

        RETURNING id
        "#,
    )
    .bind(tx.compute_txid().to_string())
    .bind(tx.version.0)
    .bind(tx.lock_time.to_consensus_u32() as i64)
    .bind(entry.fee as i64)
    .bind(entry.vsize as i64)
    .bind(entry.entered_at as i64)
    .fetch_optional(&mut **db)
    .await?;

    let Some(id) = id else {
        return Ok(false);
    };

    for (vin, input) in tx.input.iter().enumerate() {
        sqlx::query(
            r#"
            INSERT INTO mempool_inputs (
                mempool_transaction_id,
                vin,
                prev_txid,
                prev_vout,
                script_sig,
                sequence
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(id)
        .bind(vin as i32)
        .bind(input.previous_output.txid.to_string())
        .bind(input.previous_output.vout as i64)
        .bind(input.script_sig.to_string())
        .bind(input.sequence.to_consensus_u32() as i64)
        .execute(&mut **db)
        .await?;
    }

    for (vout, output) in tx.output.iter().enumerate() {
        sqlx::query(
            r#"
            INSERT INTO mempool_outputs (
                mempool_transaction_id,
                vout,
                value,
                script_pubkey,
                address
            )
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(id)
        .bind(vout as i32)
        .bind(output.value.to_sat() as i64)
        .bind(output.script_pubkey.to_string())
        .bind(output_address(&output.script_pubkey))
        .execute(&mut **db)
        .await?;
    }

    Ok(true)
}
