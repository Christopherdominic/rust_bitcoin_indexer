use anyhow::Result;
use bitcoin::BlockHash;
use bitcoincore_rpc::{Auth, Client, RpcApi};

use crate::config::Config;

pub struct BitcoinRpc {
    client: Client,
}

impl BitcoinRpc {
    pub fn new(config: &Config) -> Result<Self> {
        let auth = Auth::UserPass(config.rpc_user.clone(), config.rpc_password.clone());

        let client = Client::new(&config.rpc_url, auth)?;

        Ok(Self { client })
    }

    pub fn get_block_count(&self) -> Result<u64> {
        Ok(self.client.get_block_count()?)
    }

    pub fn get_block_hash(&self, height: u64) -> Result<BlockHash> {
        Ok(self.client.get_block_hash(height)?)
    }

    pub fn get_block_hex(&self, hash: &BlockHash) -> Result<String> {
        Ok(self.client.get_block_hex(hash)?)
    }
}
