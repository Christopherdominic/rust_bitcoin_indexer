use anyhow::{Result, bail};
use sqlx::PgPool;

use crate::{
    db::{
        blocks::{get_block_hash_at, get_block_hashes_desc, get_last_indexed_height},
        reorg::{Rollback, rollback_to},
    },
    indexer::chain::ChainSource,
    ui,
};

/// How many indexed hashes are read per query while walking back.
const ANCESTOR_PAGE: i64 = 100;

/// Makes sure the indexed chain is a prefix of Bitcoin Core's canonical
/// chain, rolling back orphaned blocks if it is not.
///
/// Comparing only the indexed tip is enough: every block is indexed only
/// after its parent hash matched the block below it, so the indexed blocks
/// form a hash-linked chain. If the tip is canonical, so is everything
/// beneath it.
pub async fn reconcile<C: ChainSource + ?Sized>(
    chain: &C,
    pool: &PgPool,
) -> Result<Option<Rollback>> {
    let Some(indexed_tip) = get_last_indexed_height(pool).await? else {
        return Ok(None);
    };
    let indexed_tip = indexed_tip as u64;
    let core_tip = chain.tip_height()?;

    if indexed_tip <= core_tip {
        let indexed_hash = get_block_hash_at(pool, indexed_tip).await?;
        let canonical_hash = chain.block_hash(indexed_tip)?.to_string();

        if indexed_hash.as_deref() == Some(canonical_hash.as_str()) {
            return Ok(None);
        }
    }

    ui::warn("Chain reorganization detected");
    ui::field("indexed tip", ui::thousands(indexed_tip));
    ui::field("node tip", ui::thousands(core_tip));

    let Some(ancestor) = find_common_ancestor(chain, pool, indexed_tip, core_tip).await? else {
        bail!(
            "indexed blocks share no common ancestor with Bitcoin Core, not even genesis. \
             The database was built from a different chain (another network, or a regtest \
             node that was reset). Refusing to roll back; point DATABASE_URL at an empty \
             database or clear the indexer tables manually."
        );
    };

    ui::field("common ancestor", ui::thousands(ancestor));

    let rollback = rollback_to(pool, ancestor).await?;

    ui::success(format!(
        "Rolled back {} orphaned block(s), restored {} output(s) to unspent",
        rollback.blocks_removed, rollback.outputs_restored
    ));

    Ok(Some(rollback))
}

/// Walks back from `indexed_tip` to the highest indexed block whose hash is
/// still canonical. `None` means not even genesis matches.
pub async fn find_common_ancestor<C: ChainSource + ?Sized>(
    chain: &C,
    pool: &PgPool,
    indexed_tip: u64,
    core_tip: u64,
) -> Result<Option<u64>> {
    let mut max_height = indexed_tip;

    loop {
        let page = get_block_hashes_desc(pool, max_height, ANCESTOR_PAGE).await?;

        if let Some(ancestor) = first_canonical(chain, core_tip, &page)? {
            return Ok(Some(ancestor));
        }

        match page.last() {
            Some(&(lowest, _)) if lowest > 0 => max_height = lowest - 1,
            _ => return Ok(None),
        }
    }
}

/// First `(height, hash)` in `indexed` (highest first) that matches the
/// canonical chain. Heights above the node's tip cannot be canonical.
fn first_canonical<C: ChainSource + ?Sized>(
    chain: &C,
    core_tip: u64,
    indexed: &[(u64, String)],
) -> Result<Option<u64>> {
    for (height, hash) in indexed {
        if *height > core_tip {
            continue;
        }

        if chain.block_hash(*height)?.to_string() == *hash {
            return Ok(Some(*height));
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{MockChain, build_chain};

    fn indexed(chain: &MockChain) -> Vec<(u64, String)> {
        (0..=chain.tip_height().unwrap())
            .rev()
            .map(|height| (height, chain.block_hash(height).unwrap().to_string()))
            .collect()
    }

    #[test]
    fn identical_chains_match_at_tip() {
        let chain = MockChain::new(build_chain(&[], 5, 'a'));

        assert_eq!(
            first_canonical(&chain, 4, &indexed(&chain)).unwrap(),
            Some(4)
        );
    }

    #[test]
    fn fork_matches_at_last_shared_block() {
        let shared = build_chain(&[], 4, 'a'); // heights 0..=3
        let old = MockChain::new(build_chain(&shared, 2, 'a')); // 4A, 5A
        let new = MockChain::new(build_chain(&shared, 3, 'b')); // 4B, 5B, 6B

        assert_eq!(first_canonical(&new, 6, &indexed(&old)).unwrap(), Some(3));
    }

    #[test]
    fn heights_above_node_tip_are_skipped() {
        let long = MockChain::new(build_chain(&[], 6, 'a')); // 0..=5
        let short = MockChain::new(long.blocks[..4].to_vec()); // 0..=3

        assert_eq!(
            first_canonical(&short, 3, &indexed(&long)).unwrap(),
            Some(3)
        );
    }

    #[test]
    fn different_genesis_has_no_ancestor() {
        let ours = MockChain::new(build_chain(&[], 3, 'a'));
        let theirs = MockChain::new(build_chain(&[], 3, 'z'));

        assert_eq!(first_canonical(&theirs, 2, &indexed(&ours)).unwrap(), None);
    }
}
