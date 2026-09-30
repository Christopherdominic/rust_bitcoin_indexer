use anyhow::Result;
use bitcoin::Transaction;
use sqlx::PgPool;

pub async fn save_outputs(pool: &PgPool, transaction_id: i64, tx: &Transaction) -> Result<()> {
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
            DO NOTHING
            "#,
        )
        .bind(transaction_id)
        .bind(vout as i32)
        .bind(output.value.to_sat() as i64)
        .bind(output.script_pubkey.to_string())
        .execute(pool)
        .await?;
    }

    Ok(())
}
