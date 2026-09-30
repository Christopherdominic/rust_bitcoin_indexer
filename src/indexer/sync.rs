use anyhow::Result;
use sqlx::PgPool;

use crate::{
    db::{
        blocks::{get_last_indexed_height, save_block},
        inputs::save_inputs,
        outputs::save_outputs,
        transactions::save_transaction,
        utxo::mark_spent,
    },
    indexer::block::BlockProcessor,
    rpc::bitcoin::BitcoinRpc,
    ui,
};

pub async fn index_block(rpc: &BitcoinRpc, pool: &PgPool, height: u64) -> Result<()> {
    ui::block_start(height);

    let hash = rpc.get_block_hash(height)?;
    let block = BlockProcessor::fetch(rpc, height)?;

    // Everything for this block is one atomic DB operation.
    let mut db = pool.begin().await?;

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

    Ok(())
}

pub async fn sync_chain(rpc: &BitcoinRpc, pool: &PgPool) -> Result<()> {
    let tip = rpc.get_block_count()?;

    let last_indexed = get_last_indexed_height(pool).await?;

    let start_height = match last_indexed {
        Some(height) => (height + 1) as u64,
        None => 0,
    };

    ui::section("Chain sync");
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
        return Ok(());
    }

    ui::info(format!(
        "Synchronizing blocks {} → {}",
        ui::thousands(start_height),
        ui::thousands(tip)
    ));

    for height in start_height..=tip {
        index_block(rpc, pool, height).await?;
    }

    println!();
    ui::success("Synchronization complete");

    Ok(())
}
