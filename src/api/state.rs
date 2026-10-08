use std::sync::Arc;

use anyhow::Result;
use sqlx::PgPool;

use crate::{
    rpc::bitcoin::{BitcoinRpc, BlockchainInfo},
    zmq::status::ZmqStatus,
};

/// What the status endpoint asks Bitcoin Core on each request.
pub trait NodeInfo: Send + Sync {
    fn blockchain_info(&self) -> Result<BlockchainInfo>;
}

impl NodeInfo for BitcoinRpc {
    fn blockchain_info(&self) -> Result<BlockchainInfo> {
        self.get_blockchain_info()
    }
}

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    /// `None` only when no RPC client was attached (tests).
    pub node: Option<Arc<dyn NodeInfo>>,
    pub zmq: Arc<ZmqStatus>,
}

impl AppState {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            node: None,
            zmq: Arc::new(ZmqStatus::new(false)),
        }
    }

    pub fn with_node(mut self, node: Arc<dyn NodeInfo>) -> Self {
        self.node = Some(node);
        self
    }

    pub fn with_zmq(mut self, zmq: Arc<ZmqStatus>) -> Self {
        self.zmq = zmq;
        self
    }
}
