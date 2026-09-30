mod config;
mod db;
mod indexer;
mod rpc;
mod ui;
mod zmq;

use anyhow::Result;

use config::Config;
use db::postgres::connect;
use indexer::sync::sync_chain;
use rpc::bitcoin::BitcoinRpc;
use zmq::listener::run_zmq_listener;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::from_env()?;

    ui::banner();
    ui::section("Connecting");

    let pool = connect(&config.database_url).await?;

    ui::success("PostgreSQL connected");

    let rpc = BitcoinRpc::new(&config)?;

    ui::success("Bitcoin Core RPC ready");
    ui::field("rpc", &config.rpc_url);

    // Catch up blocks missed while the indexer was offline.
    sync_chain(&rpc, &pool).await?;

    ui::success("Historical synchronization complete");

    // Stay running and react to new Bitcoin blocks.
    run_zmq_listener(&config.zmq_block_url, &rpc, &pool).await?;

    Ok(())
}
