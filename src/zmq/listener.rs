use anyhow::Result;
use bitcoincore_zmq::{Message, SocketMessage, subscribe_async_monitor};
use futures_util::{FutureExt, StreamExt};
use sqlx::PgPool;

use crate::{
    indexer::{mempool::sync_mempool, sync::sync_chain},
    rpc::bitcoin::BitcoinRpc,
    ui,
    zmq::status::{EndpointState, ZmqStatus},
};

pub async fn run_zmq_listener(
    block_endpoint: &str,
    tx_endpoint: Option<&str>,
    rpc: &BitcoinRpc,
    pool: &PgPool,
    status: &ZmqStatus,
) -> Result<()> {
    ui::section("Live mode");
    ui::info("Connecting to Bitcoin Core ZMQ…");
    ui::field("blocks", block_endpoint);

    let mut endpoints = vec![block_endpoint];
    match tx_endpoint {
        Some(endpoint) if endpoint != block_endpoint => {
            ui::field("transactions", endpoint);
            endpoints.push(endpoint);
        }
        Some(_) => ui::field("transactions", "same endpoint as blocks"),
        None => ui::field(
            "transactions",
            "ZMQ_TX_URL not set: mempool refreshes on new blocks only",
        ),
    }

    // The monitor variant also yields socket events (connected,
    // disconnected, retrying) per endpoint, which feed /api/status.
    let mut stream = subscribe_async_monitor(&endpoints)?;

    status.blocks.set(EndpointState::Connecting);
    if tx_endpoint.is_some() {
        status.transactions.set(EndpointState::Connecting);
    }

    ui::success("ZMQ listener subscribed");

    // Close the startup race:
    // if a block arrived between our initial sync and ZMQ setup,
    // RPC will catch it here.
    sync_chain(rpc, pool).await?;
    refresh_mempool(rpc, pool).await;

    ui::waiting();

    while let Some(message) = stream.next().await {
        // Bitcoin Core publishes one rawtx per transaction, including every
        // transaction of a newly connected block. Drain whatever else is
        // already queued so a burst costs one sync, not one per message.
        let mut new_block = false;
        let mut new_tx = false;

        let mut next = Some(message);
        while let Some(message) = next {
            match message {
                Ok(SocketMessage::Event(event)) => {
                    if event.source_url == block_endpoint {
                        status.blocks.apply(&event.event);
                    }
                    if Some(event.source_url.as_str()) == tx_endpoint {
                        status.transactions.apply(&event.event);
                    }
                }

                Ok(SocketMessage::Message(Message::Block(block, sequence))) => {
                    status.blocks.message_received();
                    ui::new_block_alert(block.block_hash(), sequence);
                    new_block = true;
                }

                Ok(SocketMessage::Message(Message::Tx(..))) => {
                    status.transactions.message_received();
                    new_tx = true;
                }

                Ok(SocketMessage::Message(other)) => {
                    ui::info(format!(
                        "Ignoring ZMQ message on topic: {}",
                        other.topic_str()
                    ));
                }

                Err(error) => {
                    ui::error(format!("ZMQ error: {}", error));
                }
            }

            next = stream.next().now_or_never().flatten();
        }

        if new_block {
            // RPC + database decide what actually needs indexing.
            sync_chain(rpc, pool).await?;
        }

        if new_block || new_tx {
            // Notifications only say "something changed"; Core's RPC
            // decides what the mempool actually contains.
            refresh_mempool(rpc, pool).await;
        }

        if new_block {
            ui::waiting();
        }
    }

    Ok(())
}

/// Unconfirmed state is not canonical, so a failed refresh is logged and
/// retried on the next notification instead of stopping the indexer.
async fn refresh_mempool(rpc: &BitcoinRpc, pool: &PgPool) {
    if let Err(error) = sync_mempool(rpc, pool).await {
        ui::error(format!("Mempool refresh failed: {error:#}"));
    }
}
