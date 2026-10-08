use anyhow::{Result, bail};
use sqlx::PgPool;

use crate::{
    db::{
        blocks::{get_block_hash_at, get_last_indexed_height, save_block},
        inputs::save_inputs,
        outputs::save_outputs,
        transactions::save_transaction,
        utxo::mark_spent,
    },
    indexer::{chain::ChainSource, reorg::reconcile},
    ui,
};

/// How many times one synchronization restarts when Bitcoin Core's chain
/// changes underneath it before giving up.
const MAX_SYNC_ATTEMPTS: usize = 5;

#[derive(Debug, PartialEq, Eq)]
pub enum IndexOutcome {
    Indexed,
    /// The block does not extend the indexed tip; nothing was written.
    ParentMismatch,
}

enum SyncOutcome {
    Done,
    ChainChanged,
}

pub async fn index_block<C: ChainSource + ?Sized>(
    chain: &C,
    pool: &PgPool,
    height: u64,
) -> Result<IndexOutcome> {
    ui::block_start(height);

    let hash = chain.block_hash(height)?;
    let block = chain.block(&hash)?;

    // Everything for this block is one atomic DB operation.
    let mut db = pool.begin().await?;

    // The block must build on the block we indexed below it. If it doesn't,
    // the node reorganized since we last checked; dropping `db` rolls back.
    if height > 0 {
        let indexed_parent = get_block_hash_at(&mut *db, height - 1).await?;
        let expected_parent = block.header.prev_blockhash.to_string();

        if indexed_parent.as_deref() != Some(expected_parent.as_str()) {
            ui::warn(format!(
                "Block #{} does not extend the indexed chain",
                ui::thousands(height)
            ));
            return Ok(IndexOutcome::ParentMismatch);
        }
    }

    let block_id = save_block(&mut db, height, &hash, &block).await?;

    for (position, tx) in block.txdata.iter().enumerate() {
        let transaction_id = save_transaction(&mut db, block_id, position, tx).await?;

        // Inputs spend outputs that were created earlier.
        mark_spent(&mut db, tx).await?;

        save_inputs(&mut db, transaction_id, tx).await?;

        // Current transaction creates new unspent outputs.
        save_outputs(&mut db, transaction_id, tx).await?;
    }

    db.commit().await?;

    ui::block_indexed(height, block.txdata.len());

    Ok(IndexOutcome::Indexed)
}

pub async fn sync_chain<C: ChainSource + ?Sized>(chain: &C, pool: &PgPool) -> Result<()> {
    ui::section("Chain sync");

    for attempt in 1..=MAX_SYNC_ATTEMPTS {
        match sync_once(chain, pool).await? {
            SyncOutcome::Done => return Ok(()),
            SyncOutcome::ChainChanged => ui::warn(format!(
                "Chain changed during synchronization, retrying ({attempt}/{MAX_SYNC_ATTEMPTS})"
            )),
        }
    }

    bail!("Bitcoin Core's chain kept changing; gave up after {MAX_SYNC_ATTEMPTS} attempts")
}

async fn sync_once<C: ChainSource + ?Sized>(chain: &C, pool: &PgPool) -> Result<SyncOutcome> {
    // Roll back any blocks that are no longer on the canonical chain
    // before extending it.
    reconcile(chain, pool).await?;

    let tip = chain.tip_height()?;

    let last_indexed = get_last_indexed_height(pool).await?;

    let start_height = match last_indexed {
        Some(height) => (height + 1) as u64,
        None => 0,
    };

    ui::field("node tip", ui::thousands(tip));

    match last_indexed {
        Some(height) => {
            ui::field("last indexed", ui::thousands(height as u64));
        }
        None => {
            ui::field("last indexed", "none (database is empty)");
        }
    }

    if start_height > tip {
        ui::success("Indexer is already synchronized");
        return Ok(SyncOutcome::Done);
    }

    ui::info(format!(
        "Synchronizing blocks {} → {}",
        ui::thousands(start_height),
        ui::thousands(tip)
    ));

    for height in start_height..=tip {
        if index_block(chain, pool, height).await? == IndexOutcome::ParentMismatch {
            return Ok(SyncOutcome::ChainChanged);
        }
    }

    println!();
    ui::success("Synchronization complete");

    Ok(SyncOutcome::Done)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use bitcoin::hashes::Hash;
    use bitcoin::{Block, BlockHash, OutPoint, Txid};

    use super::*;
    use crate::{
        db::reorg::{Rollback, rollback_to},
        test_support::{MockChain, TestDb, build_chain, coinbase, push_block, spend},
    };

    async fn indexed_hashes(pool: &PgPool) -> Vec<String> {
        sqlx::query_scalar("SELECT hash FROM blocks ORDER BY height")
            .fetch_all(pool)
            .await
            .unwrap()
    }

    fn hashes(blocks: &[Block]) -> Vec<String> {
        blocks.iter().map(|b| b.block_hash().to_string()).collect()
    }

    /// `(spent, spent_by_txid, spent_by_vin)` for an outpoint.
    async fn output_state(
        pool: &PgPool,
        outpoint: OutPoint,
    ) -> (bool, Option<String>, Option<i32>) {
        sqlx::query_as(
            r#"
            SELECT o.spent, o.spent_by_txid, o.spent_by_vin
            FROM outputs o
            JOIN transactions t ON t.id = o.transaction_id
            WHERE t.txid = $1 AND o.vout = $2
            "#,
        )
        .bind(outpoint.txid.to_string())
        .bind(outpoint.vout as i32)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    /// Height of the block holding `txid`, if it is indexed.
    async fn tx_height(pool: &PgPool, txid: Txid) -> Option<i64> {
        sqlx::query_scalar(
            "SELECT b.height FROM transactions t JOIN blocks b ON b.id = t.block_id WHERE t.txid = $1",
        )
        .bind(txid.to_string())
        .fetch_optional(pool)
        .await
        .unwrap()
    }

    fn coinbase_out(height: u64, branch: char) -> OutPoint {
        OutPoint::new(coinbase(height, branch).compute_txid(), 0)
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL"]
    async fn no_reorg_keeps_indexed_blocks() {
        let db = TestDb::new().await;

        let first = build_chain(&[], 4, 'a');
        sync_chain(&MockChain::new(first.clone()), &db.pool)
            .await
            .unwrap();
        let ids_before: Vec<i64> = sqlx::query_scalar("SELECT id FROM blocks ORDER BY height")
            .fetch_all(&db.pool)
            .await
            .unwrap();

        let extended = MockChain::new(build_chain(&first, 2, 'a'));
        assert_eq!(reconcile(&extended, &db.pool).await.unwrap(), None);
        sync_chain(&extended, &db.pool).await.unwrap();

        assert_eq!(indexed_hashes(&db.pool).await, hashes(&extended.blocks));
        let ids_after: Vec<i64> =
            sqlx::query_scalar("SELECT id FROM blocks ORDER BY height LIMIT 4")
                .fetch_all(&db.pool)
                .await
                .unwrap();
        assert_eq!(
            ids_before, ids_after,
            "existing blocks must not be rewritten"
        );

        db.cleanup().await;
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL"]
    async fn same_height_reorg_replaces_tip() {
        let db = TestDb::new().await;

        let shared = build_chain(&[], 3, 'a'); // 0..=2
        let old = MockChain::new(build_chain(&shared, 1, 'a')); // 3A
        let new = MockChain::new(build_chain(&shared, 1, 'b')); // 3B

        sync_chain(&old, &db.pool).await.unwrap();

        let rollback = reconcile(&new, &db.pool).await.unwrap();
        assert_eq!(
            rollback,
            Some(Rollback {
                blocks_removed: 1,
                outputs_restored: 0
            })
        );

        sync_chain(&new, &db.pool).await.unwrap();

        assert_eq!(indexed_hashes(&db.pool).await, hashes(&new.blocks));
        assert_eq!(
            tx_height(&db.pool, coinbase(3, 'a').compute_txid()).await,
            None
        );
        assert_eq!(
            tx_height(&db.pool, coinbase(3, 'b').compute_txid()).await,
            Some(3)
        );

        db.cleanup().await;
    }

    /// Mirrors 336 → 337 → 338A → 339A becoming 336 → 337 → 338B → 339B → 340B,
    /// with heights 0..=3 shared and the fork starting at 4.
    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL"]
    async fn longer_reorg_restores_spent_outputs_and_indexes_new_branch() {
        let db = TestDb::new().await;

        let shared = build_chain(&[], 4, 'a'); // 0..=3, common ancestor = 3
        let only_in_a = spend(coinbase_out(1, 'a'), 10_000);
        let in_both = spend(coinbase_out(2, 'a'), 20_000);

        let mut a = shared.clone();
        push_block(&mut a, 'a', vec![only_in_a.clone(), in_both.clone()]); // 4A
        push_block(&mut a, 'a', vec![]); // 5A

        let mut b = shared.clone();
        push_block(&mut b, 'b', vec![in_both.clone()]); // 4B re-mines one tx
        push_block(&mut b, 'b', vec![]); // 5B
        push_block(&mut b, 'b', vec![]); // 6B

        sync_chain(&MockChain::new(a), &db.pool).await.unwrap();
        assert!(output_state(&db.pool, coinbase_out(1, 'a')).await.0);
        assert!(output_state(&db.pool, coinbase_out(2, 'a')).await.0);

        let b = MockChain::new(b);
        let rollback = reconcile(&b, &db.pool).await.unwrap();
        assert_eq!(
            rollback,
            Some(Rollback {
                blocks_removed: 2,
                outputs_restored: 2
            })
        );

        sync_chain(&b, &db.pool).await.unwrap();

        assert_eq!(indexed_hashes(&db.pool).await, hashes(&b.blocks));

        // Spent only on the orphaned branch: unspent again.
        assert_eq!(
            output_state(&db.pool, coinbase_out(1, 'a')).await,
            (false, None, None)
        );
        assert_eq!(tx_height(&db.pool, only_in_a.compute_txid()).await, None);

        // Spent on both branches: spent again, now by the canonical block.
        assert_eq!(
            output_state(&db.pool, coinbase_out(2, 'a')).await,
            (true, Some(in_both.compute_txid().to_string()), Some(0))
        );
        assert_eq!(tx_height(&db.pool, in_both.compute_txid()).await, Some(4));

        db.cleanup().await;
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL"]
    async fn output_spent_only_by_orphaned_tx_becomes_unspent() {
        let db = TestDb::new().await;

        let funding = coinbase_out(1, 'a');
        let spender = spend(funding, 30_000);

        let mut blocks = build_chain(&[], 3, 'a'); // 0..=2
        push_block(&mut blocks, 'a', vec![spender.clone()]); // 3
        sync_chain(&MockChain::new(blocks), &db.pool).await.unwrap();

        assert_eq!(
            output_state(&db.pool, funding).await,
            (true, Some(spender.compute_txid().to_string()), Some(0))
        );

        let rollback = rollback_to(&db.pool, 2).await.unwrap();
        assert_eq!(
            rollback,
            Rollback {
                blocks_removed: 1,
                outputs_restored: 1
            }
        );

        assert_eq!(output_state(&db.pool, funding).await, (false, None, None));
        assert_eq!(tx_height(&db.pool, spender.compute_txid()).await, None);
        let spender_outputs: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM outputs o JOIN transactions t ON t.id = o.transaction_id WHERE t.txid = $1",
        )
        .bind(spender.compute_txid().to_string())
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert_eq!(spender_outputs, 0);

        db.cleanup().await;
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL"]
    async fn shorter_chain_rolls_back_to_node_tip() {
        let db = TestDb::new().await;

        let long = build_chain(&[], 6, 'a'); // 0..=5
        sync_chain(&MockChain::new(long.clone()), &db.pool)
            .await
            .unwrap();

        let short = MockChain::new(long[..4].to_vec()); // 0..=3
        sync_chain(&short, &db.pool).await.unwrap();

        assert_eq!(indexed_hashes(&db.pool).await, hashes(&short.blocks));

        db.cleanup().await;
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL"]
    async fn different_genesis_refuses_to_roll_back() {
        let db = TestDb::new().await;

        let ours = build_chain(&[], 3, 'a');
        sync_chain(&MockChain::new(ours.clone()), &db.pool)
            .await
            .unwrap();

        let theirs = MockChain::new(build_chain(&[], 5, 'z'));
        let error = sync_chain(&theirs, &db.pool).await.unwrap_err();

        assert!(error.to_string().contains("no common ancestor"), "{error}");
        assert_eq!(indexed_hashes(&db.pool).await, hashes(&ours));

        db.cleanup().await;
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL"]
    async fn block_not_extending_indexed_tip_is_not_written() {
        let db = TestDb::new().await;

        let shared = build_chain(&[], 2, 'a'); // 0..=1
        let indexed = build_chain(&shared, 1, 'a'); // 2A
        sync_chain(&MockChain::new(indexed.clone()), &db.pool)
            .await
            .unwrap();

        let other = MockChain::new(build_chain(&shared, 2, 'b')); // 2B, 3B
        let outcome = index_block(&other, &db.pool, 3).await.unwrap();

        assert_eq!(outcome, IndexOutcome::ParentMismatch);
        assert_eq!(indexed_hashes(&db.pool).await, hashes(&indexed));

        db.cleanup().await;
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL"]
    async fn retries_are_bounded_when_chain_never_links() {
        let db = TestDb::new().await;

        // Block 2 claims a parent that is not block 1, on every attempt.
        let mut blocks = build_chain(&[], 2, 'a');
        blocks.push(crate::test_support::block(
            BlockHash::from_byte_array([7; 32]),
            2,
            'a',
            vec![],
        ));
        let broken = MockChain::new(blocks.clone());

        let error = sync_chain(&broken, &db.pool).await.unwrap_err();

        assert!(error.to_string().contains("kept changing"), "{error}");
        assert_eq!(indexed_hashes(&db.pool).await, hashes(&blocks[..2]));

        db.cleanup().await;
    }

    /// Serves branch `before` until block `trigger` is fetched, then serves
    /// branch `after` — Bitcoin Core switching branches in the middle of
    /// `index_block`, after the hash lookup but before the commit.
    struct SwitchingChain {
        before: MockChain,
        after: MockChain,
        trigger: BlockHash,
        switched: Cell<bool>,
    }

    impl SwitchingChain {
        fn current(&self) -> &MockChain {
            if self.switched.get() {
                &self.after
            } else {
                &self.before
            }
        }
    }

    impl ChainSource for SwitchingChain {
        fn tip_height(&self) -> Result<u64> {
            self.current().tip_height()
        }

        fn block_hash(&self, height: u64) -> Result<BlockHash> {
            self.current().block_hash(height)
        }

        fn block(&self, hash: &BlockHash) -> Result<Block> {
            // Old-branch blocks stay retrievable by hash, as in Core.
            let block = self
                .before
                .block(hash)
                .or_else(|_| self.after.block(hash))?;
            if *hash == self.trigger {
                self.switched.set(true);
            }
            Ok(block)
        }
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL"]
    async fn block_from_branch_abandoned_mid_index_is_corrected_by_next_sync() {
        let db = TestDb::new().await;

        let shared = build_chain(&[], 2, 'a'); // 0..=1
        sync_chain(&MockChain::new(shared.clone()), &db.pool)
            .await
            .unwrap();

        let a = build_chain(&shared, 2, 'a'); // 2A, 3A
        let b = build_chain(&shared, 3, 'b'); // 2B, 3B, 4B
        let chain = SwitchingChain {
            trigger: a[2].block_hash(),
            before: MockChain::new(a.clone()),
            after: MockChain::new(b.clone()),
            switched: Cell::new(false),
        };

        // The race: 2A's parent matches the database, so it is persisted
        // even though Core has already moved to branch B.
        assert_eq!(
            index_block(&chain, &db.pool, 2).await.unwrap(),
            IndexOutcome::Indexed
        );
        assert_eq!(indexed_hashes(&db.pool).await, hashes(&a[..3]));
        assert_ne!(
            chain.block_hash(2).unwrap(),
            a[2].block_hash(),
            "Core no longer considers 2A canonical"
        );

        // The next synchronization sees the stale tip and repairs it.
        sync_chain(&chain, &db.pool).await.unwrap();

        assert_eq!(indexed_hashes(&db.pool).await, hashes(&b));
        assert_eq!(
            tx_height(&db.pool, coinbase(2, 'a').compute_txid()).await,
            None
        );

        db.cleanup().await;
    }
}
