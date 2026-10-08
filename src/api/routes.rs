use crate::api::{
    handlers::{
        get_address, get_address_transactions, get_address_utxos, get_block, get_block_by_hash,
        get_blocks, get_transaction, get_utxo_summary, get_utxos, get_watch_activity,
        get_watched_addresses, health, search, status, unwatch_address, watch_address,
    },
    mempool::{get_mempool, get_mempool_transaction, get_transaction_status},
    state::AppState,
};
use axum::{
    Router,
    routing::{delete, get, post},
};
use tower_http::cors::CorsLayer;

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/status", get(status))
        .route("/api/blocks", get(get_blocks))
        .route("/api/blocks/{height}", get(get_block))
        .route("/api/blocks/hash/{hash}", get(get_block_by_hash))
        .route("/api/transactions/{txid}", get(get_transaction))
        .route(
            "/api/transactions/{txid}/status",
            get(get_transaction_status),
        )
        .route("/api/mempool", get(get_mempool))
        .route("/api/mempool/{txid}", get(get_mempool_transaction))
        .route("/api/utxos", get(get_utxos))
        .route("/api/utxos/summary", get(get_utxo_summary))
        .route("/api/addresses/{address}", get(get_address))
        .route("/api/addresses/{address}/utxos", get(get_address_utxos))
        .route(
            "/api/addresses/{address}/transactions",
            get(get_address_transactions),
        )
        .route("/api/search/{query}", get(search))
        .route("/api/watch", get(get_watched_addresses))
        .route(
            "/api/watch/{address}",
            post(watch_address)
                .get(get_watch_activity)
                .delete(unwatch_address),
        )
        .layer(CorsLayer::permissive())
        .with_state(state)
}
