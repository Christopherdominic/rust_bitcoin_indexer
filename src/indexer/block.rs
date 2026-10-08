use anyhow::Result;
use bitcoin::consensus::deserialize;
use bitcoin::{Block, BlockHash};

use crate::{rpc::bitcoin::BitcoinRpc, ui};

pub struct BlockProcessor;

impl BlockProcessor {
    /// Fetches a block by hash rather than height, so the block we store
    /// is exactly the one whose hash we looked up — even if the node
    /// reorganizes between the two RPC calls.
    pub fn fetch(rpc: &BitcoinRpc, hash: &BlockHash) -> Result<Block> {
        let block_hex = rpc.get_block_hex(hash)?;
        let raw_block = hex::decode(&block_hex)?;

        ui::block_fetched(hash, raw_block.len());

        let block: Block = deserialize(&raw_block)?;

        Ok(block)
    }
}
