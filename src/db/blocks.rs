use anyhow::Result;
use bitcoin::{Block, BlockHash};
use sqlx::{PgPool, Postgres, Transaction};

pub async fn save_block(
    db: &mut Transaction<'_, Postgres>,
    height: u64,
    hash: &BlockHash,
    block: &Block,
) -> Result<i64> {
    let block_id: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO blocks (
            height,
            hash,
            previous_hash,
            timestamp,
            tx_count
        )
        VALUES ($1, $2, $3, $4, $5)

        ON CONFLICT (height)
        DO UPDATE SET
            hash = EXCLUDED.hash,
            previous_hash = EXCLUDED.previous_hash,
            timestamp = EXCLUDED.timestamp,
            tx_count = EXCLUDED.tx_count

        RETURNING id
        "#,
    )
    .bind(height as i64)
    .bind(hash.to_string())
    .bind(block.header.prev_blockhash.to_string())
    .bind(block.header.time as i64)
    .bind(block.txdata.len() as i32)
    .fetch_one(&mut **db)
    .await?;

    Ok(block_id)
}

pub async fn get_last_indexed_height(pool: &PgPool) -> Result<Option<i64>> {
    let height: Option<i64> = sqlx::query_scalar("SELECT MAX(height) FROM blocks")
        .fetch_one(pool)
        .await?;

    Ok(height)
}
