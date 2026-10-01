use anyhow::Result;
use bitcoincore_zmq::{Message, subscribe_async};
use futures_util::StreamExt;
use sqlx::PgPool;

use crate::{indexer::sync::sync_chain, rpc::bitcoin::BitcoinRpc, ui};

pub async fn run_zmq_listener(endpoint: &str, rpc: &BitcoinRpc, pool: &PgPool) -> Result<()> {
    ui::section("Live mode");
    ui::info("Connecting to Bitcoin Core ZMQ…");
    ui::field("endpoint", endpoint);

    let mut stream = subscribe_async(&[endpoint])?;

    ui::success("ZMQ listener connected");
    ui::waiting();

    // Close the startup race:
    // if a block arrived between our initial sync and ZMQ setup,
    // RPC will catch it here.
    sync_chain(rpc, pool).await?;

    while let Some(message) = stream.next().await {
        match message {
            Ok(Message::Block(block, sequence)) => {
                ui::new_block_alert(block.block_hash(), sequence);

                // RPC + database decide what actually needs indexing.
                sync_chain(rpc, pool).await?;

                ui::waiting();
            }

            Ok(other) => {
                ui::info(format!(
                    "Ignoring ZMQ message on topic: {}",
                    other.topic_str()
                ));
            }

            Err(error) => {
                ui::error(format!("ZMQ error: {}", error));
            }
        }
    }

    Ok(())
}
