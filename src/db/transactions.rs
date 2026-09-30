use anyhow::Result;
use bitcoin::Transaction as BitcoinTransaction;
use sqlx::{Postgres, Transaction};

pub async fn save_transaction(
    db: &mut Transaction<'_, Postgres>,
    block_id: i64,
    position: usize,
    tx: &BitcoinTransaction,
) -> Result<i64> {
    let txid = tx.compute_txid().to_string();

    let transaction_id: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO transactions (
            txid,
            block_id,
            position,
            version,
            lock_time,
            is_coinbase
        )
        VALUES ($1, $2, $3, $4, $5, $6)

        ON CONFLICT (txid)
        DO UPDATE SET
            block_id = EXCLUDED.block_id,
            position = EXCLUDED.position,
            version = EXCLUDED.version,
            lock_time = EXCLUDED.lock_time,
            is_coinbase = EXCLUDED.is_coinbase

        RETURNING id
        "#,
    )
    .bind(&txid)
    .bind(block_id)
    .bind(position as i32)
    .bind(tx.version.0)
    .bind(tx.lock_time.to_consensus_u32() as i64)
    .bind(tx.is_coinbase())
    .fetch_one(&mut **db)
    .await?;

    Ok(transaction_id)
}
