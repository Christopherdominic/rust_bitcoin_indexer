mod config;
mod db;
mod indexer;
mod rpc;

use anyhow::Result;

use config::Config;
use db::postgres::connect;
use db::{
    blocks::save_block, inputs::save_inputs, outputs::save_outputs, transactions::save_transaction,
    utxo::mark_spent,
};
use indexer::block::BlockProcessor;
use indexer::transaction::TransactionProcessor;
use rpc::bitcoin::BitcoinRpc;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::from_env()?;

    let pool = connect(&config.database_url).await?;

    println!("Database connection successful!");

    let rpc = BitcoinRpc::new(&config)?;

    println!("=== Bitcoin Indexer ===");

    let height = rpc.get_block_count()?;

    println!("Current height: {}", height);

    let hash = rpc.get_block_hash(height)?;

    let block = BlockProcessor::fetch(&rpc, height)?;

    let block_id = save_block(&pool, height, &hash, &block).await?;

    println!("Block saved to database with ID: {}", block_id);

    for (position, tx) in block.txdata.iter().enumerate() {
        let transaction_id = save_transaction(&pool, block_id, position, tx).await?;

        mark_spent(&pool, tx).await?;

        save_inputs(&pool, transaction_id, tx).await?;

        save_outputs(&pool, transaction_id, tx).await?;

        println!(
            "Saved transaction {} with {} input(s) and {} output(s)",
            tx.compute_txid(),
            tx.input.len(),
            tx.output.len()
        );
    }
    println!("Transactions saved to database!");

    BlockProcessor::print_info(height, &hash, &block);

    TransactionProcessor::print_transactions(&block);

    Ok(())
}
