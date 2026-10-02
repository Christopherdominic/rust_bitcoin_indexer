use anyhow::Result;
use bitcoin::{Address, Network, Transaction as BitcoinTransaction};
use sqlx::{Postgres, Transaction};

pub async fn save_outputs(
    db: &mut Transaction<'_, Postgres>,
    transaction_id: i64,
    tx: &BitcoinTransaction,
) -> Result<()> {
    for (vout, output) in tx.output.iter().enumerate() {
        // Try to convert the scriptPubKey into a standard
        // Bitcoin regtest address.
        //
        // Some outputs, such as OP_RETURN outputs, do not
        // correspond to an address. In that case we store NULL.
        let address = Address::from_script(&output.script_pubkey, Network::Regtest)
            .ok()
            .map(|address| address.to_string());

        sqlx::query(
            r#"
            INSERT INTO outputs (
                transaction_id,
                vout,
                value,
                script_pubkey,
                address
            )
            VALUES ($1, $2, $3, $4, $5)

            ON CONFLICT (transaction_id, vout)
            DO UPDATE SET
                value = EXCLUDED.value,
                script_pubkey = EXCLUDED.script_pubkey,
                address = EXCLUDED.address
            "#,
        )
        .bind(transaction_id)
        .bind(vout as i32)
        .bind(output.value.to_sat() as i64)
        .bind(output.script_pubkey.to_string())
        .bind(address)
        .execute(&mut **db)
        .await?;
    }

    Ok(())
}
