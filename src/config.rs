use anyhow::Result;
use dotenvy::dotenv;
use std::env;

pub struct Config {
    pub rpc_url: String,
    pub rpc_user: String,
    pub rpc_password: String,
    pub database_url: String,
    pub zmq_block_url: String,
    /// Optional `zmqpubrawtx` endpoint. Without it the mempool is only
    /// refreshed at startup and on each new block.
    pub zmq_tx_url: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        dotenv().ok();

        Ok(Self {
            rpc_url: env::var("BITCOIN_RPC_URL")?,
            rpc_user: env::var("BITCOIN_RPC_USER")?,
            rpc_password: env::var("BITCOIN_RPC_PASSWORD")?,
            database_url: env::var("DATABASE_URL")?,
            zmq_block_url: env::var("ZMQ_BLOCK_URL")?,
            zmq_tx_url: env::var("ZMQ_TX_URL").ok().filter(|url| !url.is_empty()),
        })
    }
}
