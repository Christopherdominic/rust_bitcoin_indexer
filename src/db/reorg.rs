use anyhow::Result;
use sqlx::PgPool;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rollback {
    pub blocks_removed: u64,
    pub outputs_restored: u64,
}

/// Removes every indexed block above `ancestor_height` and undoes the
/// spends those blocks made, as one atomic database transaction.
///
/// Deleting a block cascades to its transactions, inputs and outputs, but
/// outputs created *before* the ancestor that were spent by an orphaned
/// transaction live in surviving rows. Their `spent` flag has to be reset
/// explicitly, before the spending transactions are deleted.
pub async fn rollback_to(pool: &PgPool, ancestor_height: u64) -> Result<Rollback> {
    let mut db = pool.begin().await?;

    let outputs_restored = sqlx::query(
        r#"
        UPDATE outputs o
        SET
            spent = FALSE,
            spent_by_txid = NULL,
            spent_by_vin = NULL
        FROM transactions ot
        JOIN blocks ob
            ON ob.id = ot.block_id
        WHERE ot.id = o.transaction_id
          AND ob.height <= $1
          AND o.spent_by_txid IN (
            SELECT t.txid
            FROM transactions t
            JOIN blocks b
                ON b.id = t.block_id
            WHERE b.height > $1
        )
        "#,
    )
    .bind(ancestor_height as i64)
    .execute(&mut *db)
    .await?
    .rows_affected();

    // Cascades to transactions → inputs and outputs.
    let blocks_removed = sqlx::query("DELETE FROM blocks WHERE height > $1")
        .bind(ancestor_height as i64)
        .execute(&mut *db)
        .await?
        .rows_affected();

    db.commit().await?;

    Ok(Rollback {
        blocks_removed,
        outputs_restored,
    })
}
