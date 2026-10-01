mod api;
mod config;
mod db;
mod indexer;
mod rpc;
mod ui;
mod zmq;

use anyhow::Result;
use tokio::net::TcpListener;

use api::{routes::create_router, state::AppState};
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

    // --------------------------------------------------------
    // DATABASE
    // --------------------------------------------------------

    let pool = connect(&config.database_url).await?;

    ui::success("PostgreSQL connected");

    // --------------------------------------------------------
    // BITCOIN RPC
    // --------------------------------------------------------

    let rpc = BitcoinRpc::new(&config)?;

    ui::success("Bitcoin Core RPC ready");
    ui::field("rpc", &config.rpc_url);

    // --------------------------------------------------------
    // HISTORICAL CATCH-UP
    // --------------------------------------------------------

    sync_chain(&rpc, &pool).await?;

    ui::success("Historical synchronization complete");

    // --------------------------------------------------------
    // API
    // --------------------------------------------------------

    let state = AppState::new(pool.clone());

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

    let zmq_listener = run_zmq_listener(&config.zmq_block_url, &rpc, &pool);

    tokio::try_join!(api_server, zmq_listener)?;

    Ok(())
}
