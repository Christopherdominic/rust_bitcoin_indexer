use anyhow::Result;
use bitcoin::{Block, BlockHash};

use crate::{indexer::block::BlockProcessor, rpc::bitcoin::BitcoinRpc};

/// Read-only view of the canonical chain.
///
/// Bitcoin Core is the source of truth; this trait only exists so the
/// synchronization and reorg logic can also run against an in-memory
/// chain in tests.
pub trait ChainSource {
    /// Height of the best block on the canonical chain.
    fn tip_height(&self) -> Result<u64>;

    /// Hash of the canonical block at `height`.
    fn block_hash(&self, height: u64) -> Result<BlockHash>;

    /// Full block for `hash`.
    fn block(&self, hash: &BlockHash) -> Result<Block>;
}

impl ChainSource for BitcoinRpc {
    fn tip_height(&self) -> Result<u64> {
        self.get_block_count()
    }

    fn block_hash(&self, height: u64) -> Result<BlockHash> {
        self.get_block_hash(height)
    }

    fn block(&self, hash: &BlockHash) -> Result<Block> {
        BlockProcessor::fetch(self, hash)
    }
}
