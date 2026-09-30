use anyhow::{Context, Result};
use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub rpc_url: String,
    pub rpc_user: String,
    pub rpc_password: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        dotenvy::dotenv().ok();

        let database_url = env::var("DATABASE_URL")
            .context("DATABASE_URL is not set; check the project .env file")?;

        let rpc_url = env::var("BITCOIN_RPC_URL")
            .or_else(|_| env::var("RPC_URL"))
            .context("BITCOIN_RPC_URL or RPC_URL is not set")?;

        let rpc_user = env::var("BITCOIN_RPC_USER")
            .or_else(|_| env::var("RPC_USER"))
            .context("BITCOIN_RPC_USER or RPC_USER is not set")?;

        let rpc_password = env::var("BITCOIN_RPC_PASSWORD")
            .or_else(|_| env::var("RPC_PASSWORD"))
            .context("BITCOIN_RPC_PASSWORD or RPC_PASSWORD is not set")?;

        Ok(Self {
            database_url,
            rpc_url,
            rpc_user,
            rpc_password,
        })
    }
}
