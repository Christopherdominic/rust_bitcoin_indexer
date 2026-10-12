mod api;
mod config;
mod db;
mod indexer;
mod rpc;
#[cfg(test)]
mod test_support;
mod tui;
mod ui;
mod zmq;

use std::io::IsTerminal;
use std::sync::Arc;

use anyhow::Result;
use sqlx::PgPool;
use tokio::net::TcpListener;

use api::{routes::create_router, state::AppState};
use config::Config;
use db::postgres::connect;
use indexer::sync::sync_chain;
use rpc::bitcoin::BitcoinRpc;
use zmq::{listener::run_zmq_listener, status::ZmqStatus};

const API_ADDR: &str = "127.0.0.1:3000";

/// `cargo run` opens the dashboard; `cargo run -- --headless` (or any run
/// whose output is not a terminal) keeps the plain log output.
#[tokio::main]
async fn main() -> Result<()> {
    let headless = std::env::args().skip(1).any(|arg| arg == "--headless");
    let dashboard = !headless && std::io::stdout().is_terminal();

    // While the dashboard owns the terminal, indexer output goes to its log
    // panel. Start capturing before connecting so nothing is lost.
    let logs = dashboard.then(ui::capture);

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

    let zmq_status = Arc::new(ZmqStatus::new(config.zmq_tx_url.is_some()));

    let state = AppState::new(pool.clone())
        .with_node(rpc.clone())
        .with_zmq(zmq_status.clone());

    let services = run_services(
        config.zmq_block_url,
        config.zmq_tx_url,
        rpc.clone(),
        pool,
        state.clone(),
        zmq_status,
    );

    match logs {
        None => services.await,
        Some(logs) => {
            let services = tokio::spawn(services);
            let outcome = tui::run(tui::Context { state, rpc, logs }, services).await;
            // Blocking RPC calls may still be in flight; don't wait for them.
            if let Err(error) = &outcome {
                eprintln!("Error: {error:#}");
            }
            std::process::exit(i32::from(outcome.is_err()));
        }
    }
}

/// Historical catch-up, then the API and live ZMQ indexing side by side.
async fn run_services(
    zmq_block_url: String,
    zmq_tx_url: Option<String>,
    rpc: Arc<BitcoinRpc>,
    pool: PgPool,
    state: AppState,
    zmq_status: Arc<ZmqStatus>,
) -> Result<()> {
    // --------------------------------------------------------
    // HISTORICAL CATCH-UP
    // --------------------------------------------------------

    sync_chain(rpc.as_ref(), &pool).await?;

    ui::success("Historical synchronization complete");

    // --------------------------------------------------------
    // API
    // --------------------------------------------------------

    let app = create_router(state);

    let listener = TcpListener::bind(API_ADDR).await?;

    ui::section("API");
    ui::success("API listening");
    ui::field("url", format!("http://{API_ADDR}"));

    // --------------------------------------------------------
    // RUN API + ZMQ CONCURRENTLY
    // --------------------------------------------------------

    let api_server = async {
        axum::serve(listener, app)
            .await
            .map_err(anyhow::Error::from)
    };

    let zmq_listener = run_zmq_listener(
        &zmq_block_url,
        zmq_tx_url.as_deref(),
        &rpc,
        &pool,
        &zmq_status,
    );

    tokio::try_join!(api_server, zmq_listener)?;

    Ok(())
}
