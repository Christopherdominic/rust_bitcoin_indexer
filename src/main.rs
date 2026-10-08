mod api;
mod config;
mod db;
mod indexer;
mod rpc;
#[cfg(test)]
mod test_support;
mod ui;
mod zmq;

use std::sync::Arc;

use anyhow::Result;
use tokio::net::TcpListener;

use api::{routes::create_router, state::AppState};
use config::Config;
use db::postgres::connect;
use indexer::sync::sync_chain;
use rpc::bitcoin::BitcoinRpc;
use zmq::{listener::run_zmq_listener, status::ZmqStatus};

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::from_env()?;

    ui::banner();
    ui::section("Connecting");

    // --------------------------------------------------------
    // DATABASE
    // --------------------------------------------------------

    let pool = connect(&config.database_url).await?;

    ui::success("PostgreSQL connected");

    // --------------------------------------------------------
    // BITCOIN RPC
    // --------------------------------------------------------

    // Shared with the API, which asks Core for its tip on each status request.
    let rpc = Arc::new(BitcoinRpc::new(&config)?);

    ui::success("Bitcoin Core RPC ready");
    ui::field("rpc", &config.rpc_url);

    // --------------------------------------------------------
    // HISTORICAL CATCH-UP
    // --------------------------------------------------------

    sync_chain(rpc.as_ref(), &pool).await?;

    ui::success("Historical synchronization complete");

    // --------------------------------------------------------
    // API
    // --------------------------------------------------------

    let zmq_status = Arc::new(ZmqStatus::new(config.zmq_tx_url.is_some()));

    let state = AppState::new(pool.clone())
        .with_node(rpc.clone())
        .with_zmq(zmq_status.clone());

    let app = create_router(state);

    let listener = TcpListener::bind("127.0.0.1:3000").await?;

    ui::section("API");
    ui::success("API listening");
    ui::field("url", "http://127.0.0.1:3000");

    // --------------------------------------------------------
    // RUN API + ZMQ CONCURRENTLY
    // --------------------------------------------------------

    let api_server = async {
        axum::serve(listener, app)
            .await
            .map_err(anyhow::Error::from)
    };

    let zmq_listener = run_zmq_listener(
        &config.zmq_block_url,
        config.zmq_tx_url.as_deref(),
        &rpc,
        &pool,
        &zmq_status,
    );

    tokio::try_join!(api_server, zmq_listener)?;

    Ok(())
}
