use anyhow::Result;
use bitcoin::Transaction as BitcoinTransaction;
use sqlx::{Postgres, Transaction};

pub async fn save_outputs(
    db: &mut Transaction<'_, Postgres>,
    transaction_id: i64,
    tx: &BitcoinTransaction,
) -> Result<()> {
    for (vout, output) in tx.output.iter().enumerate() {
        sqlx::query(
            r#"
            INSERT INTO outputs (
                transaction_id,
                vout,
                value,
                script_pubkey
            )
            VALUES ($1, $2, $3, $4)

            ON CONFLICT (transaction_id, vout)
            DO UPDATE SET
                value = EXCLUDED.value,
                script_pubkey = EXCLUDED.script_pubkey
            "#,
        )
        .bind(transaction_id)
        .bind(vout as i32)
        .bind(output.value.to_sat() as i64)
        .bind(output.script_pubkey.to_string())
        .execute(&mut **db)
        .await?;
    }

    Ok(())
}
