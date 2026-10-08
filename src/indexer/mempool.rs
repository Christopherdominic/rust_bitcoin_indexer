use std::collections::{HashMap, HashSet};

use anyhow::Result;
use bitcoin::{Transaction, Txid};
use sqlx::PgPool;

use crate::{
    db::mempool::{
        MempoolEntry, delete_mempool_transactions, get_confirmed_txids, get_mempool_txids,
        save_mempool_transaction,
    },
    rpc::bitcoin::BitcoinRpc,
    ui,
};

/// Read-only view of Bitcoin Core's mempool.
pub trait MempoolSource {
    /// Every transaction currently in the mempool.
    fn mempool(&self) -> Result<HashMap<Txid, MempoolEntry>>;

    /// One mempool transaction, or `None` if it has left the mempool.
    fn mempool_transaction(&self, txid: &Txid) -> Result<Option<Transaction>>;
}

impl MempoolSource for BitcoinRpc {
    fn mempool(&self) -> Result<HashMap<Txid, MempoolEntry>> {
        Ok(self
            .get_raw_mempool_verbose()?
            .into_iter()
            .map(|(txid, entry)| {
                (
                    txid,
                    MempoolEntry {
                        fee: entry.fees.base.to_sat(),
                        vsize: entry.vsize,
                        entered_at: entry.time,
                    },
                )
            })
            .collect())
    }

    fn mempool_transaction(&self, txid: &Txid) -> Result<Option<Transaction>> {
        self.get_raw_transaction(txid)
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct MempoolSync {
    pub added: u64,
    pub removed: u64,
}

/// Makes the mempool tables mirror Bitcoin Core's mempool.
///
/// Core is the source of truth: whatever it no longer holds (mined,
/// evicted, expired or replaced) is removed, and whatever it holds that we
/// don't is fetched and stored. Transactions already indexed as confirmed
/// are never stored as unconfirmed. Confirmed tables are not touched.
pub async fn sync_mempool<M: MempoolSource + ?Sized>(
    source: &M,
    pool: &PgPool,
) -> Result<MempoolSync> {
    let entries = source.mempool()?;

    let core_txids: Vec<String> = entries.keys().map(Txid::to_string).collect();
    let confirmed: HashSet<String> = get_confirmed_txids(pool, &core_txids)
        .await?
        .into_iter()
        .collect();
    let stored: HashSet<String> = get_mempool_txids(pool).await?.into_iter().collect();

    let wanted: HashSet<&String> = core_txids
        .iter()
        .filter(|txid| !confirmed.contains(*txid))
        .collect();

    let stale: Vec<String> = stored
        .iter()
        .filter(|txid| !wanted.contains(txid))
        .cloned()
        .collect();

    // Fetch everything over RPC before opening the database transaction.
    let mut new = Vec::new();
    for (txid, entry) in &entries {
        let key = txid.to_string();
        if !wanted.contains(&key) || stored.contains(&key) {
            continue;
        }
        if let Some(tx) = source.mempool_transaction(txid)? {
            new.push((tx, *entry));
        }
    }

    if stale.is_empty() && new.is_empty() {
        return Ok(MempoolSync::default());
    }

    let mut db = pool.begin().await?;

    let removed = delete_mempool_transactions(&mut db, &stale).await?;

    let mut added = 0;
    for (tx, entry) in &new {
        if save_mempool_transaction(&mut db, tx, entry).await? {
            added += 1;
        }
    }

    db.commit().await?;

    ui::info(format!(
        "Mempool updated: +{added} unconfirmed, −{removed} removed ({} in mempool)",
        wanted.len()
    ));

    Ok(MempoolSync { added, removed })
}

#[cfg(test)]
mod tests {
    use bitcoin::OutPoint;

    use super::*;
    use crate::{
        indexer::sync::sync_chain,
        test_support::{
            MockChain, MockMempool, TestDb, build_chain, coinbase, out, output_state, p2wpkh,
            push_block, tx, tx_height,
        },
    };

    async fn mempool_txids(pool: &PgPool) -> Vec<String> {
        let mut txids = get_mempool_txids(pool).await.unwrap();
        txids.sort();
        txids
    }

    /// `(mempool_inputs, mempool_outputs)` row counts.
    async fn mempool_row_counts(pool: &PgPool) -> (i64, i64) {
        sqlx::query_as(
            "SELECT (SELECT COUNT(*) FROM mempool_inputs), (SELECT COUNT(*) FROM mempool_outputs)",
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    fn coinbase_out(height: u64) -> OutPoint {
        OutPoint::new(coinbase(height, 'a').compute_txid(), 0)
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL"]
    async fn unconfirmed_transactions_are_mirrored_without_touching_confirmed_utxos() {
        let db = TestDb::new().await;

        sync_chain(&MockChain::new(build_chain(&[], 2, 'a')), &db.pool)
            .await
            .unwrap();

        let payment = tx(
            &[coinbase_out(1)],
            vec![out(30_000, p2wpkh(7)), out(19_000, p2wpkh(8))],
        );
        let core = MockMempool::with(std::slice::from_ref(&payment));

        assert_eq!(
            sync_mempool(&core, &db.pool).await.unwrap(),
            MempoolSync {
                added: 1,
                removed: 0
            }
        );
        assert_eq!(
            mempool_txids(&db.pool).await,
            vec![payment.compute_txid().to_string()]
        );
        assert_eq!(mempool_row_counts(&db.pool).await, (1, 2));

        // Not confirmed, and the confirmed output it spends stays unspent.
        assert_eq!(tx_height(&db.pool, payment.compute_txid()).await, None);
        assert_eq!(
            output_state(&db.pool, coinbase_out(1)).await,
            (false, None, None)
        );

        // Nothing changed in Core: nothing changes here.
        assert_eq!(
            sync_mempool(&core, &db.pool).await.unwrap(),
            MempoolSync::default()
        );
        assert_eq!(mempool_row_counts(&db.pool).await, (1, 2));

        db.cleanup().await;
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL"]
    async fn transactions_core_drops_are_removed() {
        let db = TestDb::new().await;

        sync_chain(&MockChain::new(build_chain(&[], 3, 'a')), &db.pool)
            .await
            .unwrap();
        let kept = tx(&[coinbase_out(1)], vec![out(1_000, p2wpkh(7))]);
        let evicted = tx(&[coinbase_out(2)], vec![out(2_000, p2wpkh(8))]);

        sync_mempool(&MockMempool::with(&[kept.clone(), evicted]), &db.pool)
            .await
            .unwrap();

        // Evicted, expired or replaced: Core no longer lists it.
        let result = sync_mempool(&MockMempool::with(std::slice::from_ref(&kept)), &db.pool)
            .await
            .unwrap();

        assert_eq!(
            result,
            MempoolSync {
                added: 0,
                removed: 1
            }
        );
        assert_eq!(
            mempool_txids(&db.pool).await,
            vec![kept.compute_txid().to_string()]
        );
        assert_eq!(mempool_row_counts(&db.pool).await, (1, 1), "cascade");

        db.cleanup().await;
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL"]
    async fn mined_transaction_moves_from_mempool_to_confirmed_exactly_once() {
        let db = TestDb::new().await;

        let mut blocks = build_chain(&[], 2, 'a');
        sync_chain(&MockChain::new(blocks.clone()), &db.pool)
            .await
            .unwrap();

        let payment = tx(&[coinbase_out(1)], vec![out(5_000, p2wpkh(7))]);
        let core = MockMempool::with(std::slice::from_ref(&payment));
        sync_mempool(&core, &db.pool).await.unwrap();

        push_block(&mut blocks, 'a', vec![payment.clone()]);
        sync_chain(&MockChain::new(blocks), &db.pool).await.unwrap();

        // Race: Core still lists it (the block's rawtx arrived first). It is
        // confirmed in the database, so it must not stay unconfirmed.
        let result = sync_mempool(&core, &db.pool).await.unwrap();

        assert_eq!(
            result,
            MempoolSync {
                added: 0,
                removed: 1
            }
        );
        assert!(mempool_txids(&db.pool).await.is_empty());
        assert_eq!(tx_height(&db.pool, payment.compute_txid()).await, Some(2));
        assert_eq!(
            output_state(&db.pool, coinbase_out(1)).await,
            (true, Some(payment.compute_txid().to_string()), Some(0))
        );

        db.cleanup().await;
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL"]
    async fn transaction_gone_between_rpc_calls_is_skipped() {
        let db = TestDb::new().await;

        sync_chain(&MockChain::new(build_chain(&[], 2, 'a')), &db.pool)
            .await
            .unwrap();
        let payment = tx(&[coinbase_out(1)], vec![out(5_000, p2wpkh(7))]);
        let mut core = MockMempool::with(std::slice::from_ref(&payment));
        core.vanishing.push(payment.compute_txid());

        assert_eq!(
            sync_mempool(&core, &db.pool).await.unwrap(),
            MempoolSync::default()
        );
        assert!(mempool_txids(&db.pool).await.is_empty());

        db.cleanup().await;
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL"]
    async fn reorged_out_transaction_returns_to_mempool() {
        let db = TestDb::new().await;

        let shared = build_chain(&[], 2, 'a'); // 0..=1
        let payment = tx(&[coinbase_out(1)], vec![out(5_000, p2wpkh(7))]);
        let mut mined = shared.clone();
        push_block(&mut mined, 'a', vec![payment.clone()]); // 2A
        sync_chain(&MockChain::new(mined), &db.pool).await.unwrap();

        // Core switches to a longer branch without the payment and puts it
        // back in its mempool.
        let other = build_chain(&shared, 2, 'b'); // 2B, 3B
        sync_chain(&MockChain::new(other), &db.pool).await.unwrap();
        sync_mempool(&MockMempool::with(std::slice::from_ref(&payment)), &db.pool)
            .await
            .unwrap();

        assert_eq!(tx_height(&db.pool, payment.compute_txid()).await, None);
        assert_eq!(
            mempool_txids(&db.pool).await,
            vec![payment.compute_txid().to_string()]
        );
        assert_eq!(
            output_state(&db.pool, coinbase_out(1)).await,
            (false, None, None)
        );

        db.cleanup().await;
    }
}
