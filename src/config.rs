use anyhow::Result;
use dotenvy::dotenv;
use std::env;

pub struct Config {
    pub rpc_url: String,
    pub rpc_user: String,
    pub rpc_password: String,
    pub database_url: String,
    pub zmq_block_url: String,
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
        })
    }
}
