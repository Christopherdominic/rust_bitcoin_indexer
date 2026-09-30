use anyhow::Result;
use bitcoin::consensus::deserialize;
use bitcoin::{Block, BlockHash};

use crate::rpc::bitcoin::BitcoinRpc;

pub struct BlockProcessor;

impl BlockProcessor {
    pub fn fetch(rpc: &BitcoinRpc, height: u64) -> Result<Block> {
        let hash = rpc.get_block_hash(height)?;

        println!("Fetching block {}...", height);
        println!("Hash: {}", hash);

        // Get the raw serialized block as hexadecimal
        let block_hex = rpc.get_block_hex(&hash)?;

        println!("Raw block size: {} hex characters", block_hex.len());

        // Convert hex → bytes
        let raw_block = hex::decode(&block_hex)?;

        println!("Raw block size: {} bytes", raw_block.len());

        // Decode Bitcoin's binary block format
        let block: Block = deserialize(&raw_block)?;

        Ok(block)
    }

    pub fn print_info(height: u64, hash: &BlockHash, block: &Block) {
        println!();
        println!("=== Block ===");
        println!("Height: {}", height);
        println!("Hash: {}", hash);
        println!("Previous block: {}", block.header.prev_blockhash);
        println!("Merkle root: {}", block.header.merkle_root);
        println!("Timestamp: {}", block.header.time);
        println!("Nonce: {}", block.header.nonce);
        println!("Transactions: {}", block.txdata.len());
    }
}
