use axum::{Router, routing::get};

use crate::api::{
    handlers::{get_block, get_transaction, get_utxos, health, status},
    state::AppState,
};

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/status", get(status))
        .route("/api/blocks/{height}", get(get_block))
        .route("/api/transactions/{txid}", get(get_transaction))
        .route("/api/utxos", get(get_utxos))
        .with_state(state)
}
