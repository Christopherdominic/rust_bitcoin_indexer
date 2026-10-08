use anyhow::Result;
use bitcoin::{Block, BlockHash};
use sqlx::{PgExecutor, PgPool, Postgres, Transaction};

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

/// Hash of the indexed block at `height`, if any.
pub async fn get_block_hash_at<'e>(db: impl PgExecutor<'e>, height: u64) -> Result<Option<String>> {
    let hash: Option<String> = sqlx::query_scalar("SELECT hash FROM blocks WHERE height = $1")
        .bind(height as i64)
        .fetch_optional(db)
        .await?;

    Ok(hash)
}

/// Indexed `(height, hash)` pairs at or below `max_height`, highest first.
pub async fn get_block_hashes_desc(
    pool: &PgPool,
    max_height: u64,
    limit: i64,
) -> Result<Vec<(u64, String)>> {
    let rows: Vec<(i64, String)> = sqlx::query_as(
        r#"
        SELECT height, hash
        FROM blocks
        WHERE height <= $1
        ORDER BY height DESC
        LIMIT $2
        "#,
    )
    .bind(max_height as i64)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(height, hash)| (height as u64, hash))
        .collect())
}
