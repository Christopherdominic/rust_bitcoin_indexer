use std::collections::HashMap;

use anyhow::Result;
use bitcoin::{Amount, BlockHash, Transaction, Txid};
use bitcoincore_rpc::{Auth, Client, Error as RpcError, RpcApi, jsonrpc};
use serde::Deserialize;

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

    /// Bitcoin Core's current mempool, keyed by txid.
    ///
    /// Deserialized into our own minimal struct rather than
    /// `bitcoincore_rpc`'s, which requires fields newer Core versions drop.
    pub fn get_raw_mempool_verbose(&self) -> Result<HashMap<Txid, RawMempoolEntry>> {
        Ok(self.client.call("getrawmempool", &[true.into()])?)
    }

    /// A transaction by txid, or `None` if Core no longer knows it (for
    /// example it left the mempool between two calls).
    pub fn get_raw_transaction(&self, txid: &Txid) -> Result<Option<Transaction>> {
        match self.client.get_raw_transaction(txid, None) {
            Ok(tx) => Ok(Some(tx)),
            // RPC_INVALID_ADDRESS_OR_KEY: "No such mempool or blockchain transaction".
            Err(RpcError::JsonRpc(jsonrpc::error::Error::Rpc(e))) if e.code == -5 => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawMempoolEntry {
    pub vsize: u64,
    /// Unix time the transaction entered the mempool.
    pub time: u64,
    pub fees: RawMempoolFees,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawMempoolFees {
    #[serde(with = "bitcoin::amount::serde::as_btc")]
    pub base: Amount,
}

#[cfg(test)]
mod tests {
    use bitcoincore_rpc::jsonrpc::serde_json;

    use super::*;

    #[test]
    fn mempool_entries_parse_with_or_without_optional_fields() {
        // Shape of `getrawmempool true` on recent Bitcoin Core, plus an entry
        // missing fields some versions drop. Only vsize, time and fees.base
        // are required.
        let json = r#"{
            "1111111111111111111111111111111111111111111111111111111111111111": {
                "vsize": 141, "weight": 561, "time": 1790900000, "height": 340,
                "descendantcount": 1, "descendantsize": 141,
                "ancestorcount": 1, "ancestorsize": 141,
                "wtxid": "2222222222222222222222222222222222222222222222222222222222222222",
                "fees": { "base": 0.00002820, "modified": 0.00002820,
                          "ancestor": 0.00002820, "descendant": 0.00002820 },
                "depends": [], "spentby": [], "bip125-replaceable": true, "unbroadcast": false
            },
            "3333333333333333333333333333333333333333333333333333333333333333": {
                "vsize": 110, "time": 1790900001, "fees": { "base": 0.00000110 }
            }
        }"#;

        let entries: HashMap<Txid, RawMempoolEntry> = serde_json::from_str(json).unwrap();

        let full = &entries[&"1111111111111111111111111111111111111111111111111111111111111111"
            .parse::<Txid>()
            .unwrap()];
        assert_eq!((full.vsize, full.time), (141, 1790900000));
        assert_eq!(full.fees.base, Amount::from_sat(2_820));

        let minimal = &entries[&"3333333333333333333333333333333333333333333333333333333333333333"
            .parse::<Txid>()
            .unwrap()];
        assert_eq!(minimal.fees.base, Amount::from_sat(110));
    }
}
