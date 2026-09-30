use anyhow::Result;
use bitcoin::Transaction as BitcoinTransaction;
use sqlx::{Postgres, Transaction};

use crate::ui;

pub async fn mark_spent(
    db: &mut Transaction<'_, Postgres>,
    spending_tx: &BitcoinTransaction,
) -> Result<()> {
    if spending_tx.is_coinbase() {
        return Ok(());
    }

    let spending_txid = spending_tx.compute_txid().to_string();

    for (vin, input) in spending_tx.input.iter().enumerate() {
        let prev_txid = input.previous_output.txid.to_string();
        let prev_vout = input.previous_output.vout as i32;

        let result = sqlx::query(
            r#"
            UPDATE outputs
            SET
                spent = TRUE,
                spent_by_txid = $1,
                spent_by_vin = $2
            WHERE transaction_id = (
                SELECT id
                FROM transactions
                WHERE txid = $3
            )
            AND vout = $4
            "#,
        )
        .bind(&spending_txid)
        .bind(vin as i32)
        .bind(&prev_txid)
        .bind(prev_vout)
        .execute(&mut **db)
        .await?;

        if result.rows_affected() == 0 {
            ui::warn(format!(
                "Previous output not indexed: {}:{}",
                prev_txid, prev_vout
            ));
        }
    }

    Ok(())
}
