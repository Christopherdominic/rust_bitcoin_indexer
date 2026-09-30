use anyhow::Result;
use bitcoin::Block;
use bitcoin::consensus::deserialize;

use crate::{rpc::bitcoin::BitcoinRpc, ui};

pub struct BlockProcessor;

impl BlockProcessor {
    pub fn fetch(rpc: &BitcoinRpc, height: u64) -> Result<Block> {
        let hash = rpc.get_block_hash(height)?;

        let block_hex = rpc.get_block_hex(&hash)?;
        let raw_block = hex::decode(&block_hex)?;

        ui::block_fetched(&hash, raw_block.len());

        let block: Block = deserialize(&raw_block)?;

        Ok(block)
    }
}
