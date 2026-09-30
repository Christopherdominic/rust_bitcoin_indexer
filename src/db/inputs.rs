use anyhow::Result;
use bitcoin::Transaction as BitcoinTransaction;
use sqlx::{Postgres, Transaction};

pub async fn save_inputs(
    db: &mut Transaction<'_, Postgres>,
    transaction_id: i64,
    tx: &BitcoinTransaction,
) -> Result<()> {
    for (vin, input) in tx.input.iter().enumerate() {
        let (prev_txid, prev_vout) = if tx.is_coinbase() {
            (None, None)
        } else {
            (
                Some(input.previous_output.txid.to_string()),
                Some(input.previous_output.vout as i64),
            )
        };

        sqlx::query(
            r#"
            INSERT INTO inputs (
                transaction_id,
                vin,
                prev_txid,
                prev_vout,
                script_sig,
                sequence
            )
            VALUES ($1, $2, $3, $4, $5, $6)

            ON CONFLICT (transaction_id, vin)
            DO UPDATE SET
                prev_txid = EXCLUDED.prev_txid,
                prev_vout = EXCLUDED.prev_vout,
                script_sig = EXCLUDED.script_sig,
                sequence = EXCLUDED.sequence
            "#,
        )
        .bind(transaction_id)
        .bind(vin as i32)
        .bind(prev_txid)
        .bind(prev_vout)
        .bind(input.script_sig.to_string())
        .bind(input.sequence.to_consensus_u32() as i64)
        .execute(&mut **db)
        .await?;
    }

    Ok(())
}
