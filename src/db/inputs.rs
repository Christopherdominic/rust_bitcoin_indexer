use anyhow::Result;
use bitcoin::Transaction;
use sqlx::PgPool;

pub async fn save_inputs(pool: &PgPool, transaction_id: i64, tx: &Transaction) -> Result<()> {
    for (vin, input) in tx.input.iter().enumerate() {
        let is_coinbase = tx.is_coinbase();

        let prev_txid = if is_coinbase {
            None
        } else {
            Some(input.previous_output.txid.to_string())
        };

        let prev_vout = if is_coinbase {
            None
        } else {
            Some(input.previous_output.vout as i64)
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
            DO NOTHING
            "#,
        )
        .bind(transaction_id)
        .bind(vin as i32)
        .bind(prev_txid)
        .bind(prev_vout)
        .bind(input.script_sig.to_string())
        .bind(input.sequence.to_consensus_u32() as i64)
        .execute(pool)
        .await?;
    }

    Ok(())
}
