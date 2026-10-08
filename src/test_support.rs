//! Test-only helpers: an in-memory chain and an isolated PostgreSQL schema.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};

use anyhow::{Result, anyhow};
use bitcoin::block::{Header, Version as BlockVersion};
use bitcoin::hashes::Hash;
use bitcoin::transaction::Version;
use bitcoin::{
    Amount, Block, BlockHash, CompactTarget, OutPoint, ScriptBuf, Sequence, Transaction, TxIn,
    TxMerkleNode, TxOut, Txid, WPubkeyHash, Witness, absolute::LockTime,
};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{AssertSqlSafe, PgPool};

use crate::indexer::chain::ChainSource;

// ------------------------------------------------------------
// IN-MEMORY CHAIN
// ------------------------------------------------------------

/// A canonical chain held in memory, standing in for Bitcoin Core.
pub struct MockChain {
    pub blocks: Vec<Block>,
    by_hash: HashMap<BlockHash, Block>,
}

impl MockChain {
    pub fn new(blocks: Vec<Block>) -> Self {
        let by_hash = blocks.iter().map(|b| (b.block_hash(), b.clone())).collect();
        Self { blocks, by_hash }
    }
}

impl ChainSource for MockChain {
    fn tip_height(&self) -> Result<u64> {
        Ok(self.blocks.len() as u64 - 1)
    }

    fn block_hash(&self, height: u64) -> Result<BlockHash> {
        self.blocks
            .get(height as usize)
            .map(Block::block_hash)
            .ok_or_else(|| anyhow!("no block at height {height}"))
    }

    fn block(&self, hash: &BlockHash) -> Result<Block> {
        self.by_hash
            .get(hash)
            .cloned()
            .ok_or_else(|| anyhow!("unknown block {hash}"))
    }
}

// ------------------------------------------------------------
// BLOCK BUILDERS
// ------------------------------------------------------------

/// A pay-to-witness-pubkey-hash script, so outputs get a regtest address.
pub fn p2wpkh(tag: u8) -> ScriptBuf {
    ScriptBuf::new_p2wpkh(&WPubkeyHash::from_byte_array([tag; 20]))
}

/// A coinbase whose txid is unique per `(height, branch)`.
pub fn coinbase(height: u64, branch: char) -> Transaction {
    let mut script_sig = height.to_le_bytes().to_vec();
    script_sig.push(branch as u8);

    Transaction {
        version: Version::TWO,
        lock_time: LockTime::ZERO,
        input: vec![TxIn {
            previous_output: OutPoint::null(),
            script_sig: ScriptBuf::from_bytes(script_sig),
            sequence: Sequence::MAX,
            witness: Witness::new(),
        }],
        output: vec![TxOut {
            value: Amount::from_sat(50_0000_0000),
            script_pubkey: p2wpkh(1),
        }],
    }
}

/// A transaction spending `prev` into one output.
pub fn spend(prev: OutPoint, value: u64) -> Transaction {
    tx(&[prev], vec![out(value, p2wpkh(2))])
}

/// An output paying `value` sats to `script_pubkey`.
pub fn out(value: u64, script_pubkey: ScriptBuf) -> TxOut {
    TxOut {
        value: Amount::from_sat(value),
        script_pubkey,
    }
}

/// A non-coinbase transaction spending `inputs` (in vin order) into `outputs`.
pub fn tx(inputs: &[OutPoint], outputs: Vec<TxOut>) -> Transaction {
    Transaction {
        version: Version::TWO,
        lock_time: LockTime::ZERO,
        input: inputs
            .iter()
            .map(|prev| TxIn {
                previous_output: *prev,
                script_sig: ScriptBuf::new(),
                sequence: Sequence::MAX,
                witness: Witness::new(),
            })
            .collect(),
        output: outputs,
    }
}

/// A block on top of `prev` holding a coinbase plus `txs`.
pub fn block(prev: BlockHash, height: u64, branch: char, txs: Vec<Transaction>) -> Block {
    let mut txdata = vec![coinbase(height, branch)];
    txdata.extend(txs);
    block_with_txdata(prev, height, txdata)
}

/// A block on top of `prev` holding exactly `txdata` (coinbase included).
pub fn block_with_txdata(prev: BlockHash, height: u64, txdata: Vec<Transaction>) -> Block {
    let mut block = Block {
        header: Header {
            version: BlockVersion::ONE,
            prev_blockhash: prev,
            merkle_root: TxMerkleNode::all_zeros(),
            time: 1_700_000_000 + height as u32,
            bits: CompactTarget::from_consensus(0x207f_ffff),
            nonce: 0,
        },
        txdata,
    };
    block.header.merkle_root = block.compute_merkle_root().expect("non-empty block");
    block
}

/// `prefix` followed by `count` empty blocks on branch `branch`.
pub fn build_chain(prefix: &[Block], count: u64, branch: char) -> Vec<Block> {
    let mut blocks = prefix.to_vec();
    for _ in 0..count {
        let height = blocks.len() as u64;
        let prev = blocks
            .last()
            .map(Block::block_hash)
            .unwrap_or_else(BlockHash::all_zeros);
        blocks.push(block(prev, height, branch, vec![]));
    }
    blocks
}

/// Appends a block holding `txs` on top of `blocks`.
pub fn push_block(blocks: &mut Vec<Block>, branch: char, txs: Vec<Transaction>) {
    let height = blocks.len() as u64;
    let prev = blocks.last().expect("chain has genesis").block_hash();
    blocks.push(block(prev, height, branch, txs));
}

// ------------------------------------------------------------
// DATABASE INSPECTION
// ------------------------------------------------------------

/// `(spent, spent_by_txid, spent_by_vin)` for an outpoint.
pub async fn output_state(
    pool: &PgPool,
    outpoint: OutPoint,
) -> (bool, Option<String>, Option<i32>) {
    sqlx::query_as(
        r#"
        SELECT o.spent, o.spent_by_txid, o.spent_by_vin
        FROM outputs o
        JOIN transactions t ON t.id = o.transaction_id
        WHERE t.txid = $1 AND o.vout = $2
        "#,
    )
    .bind(outpoint.txid.to_string())
    .bind(outpoint.vout as i32)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Height of the block holding `txid`, if it is indexed.
pub async fn tx_height(pool: &PgPool, txid: Txid) -> Option<i64> {
    sqlx::query_scalar(
        "SELECT b.height FROM transactions t JOIN blocks b ON b.id = t.block_id WHERE t.txid = $1",
    )
    .bind(txid.to_string())
    .fetch_optional(pool)
    .await
    .unwrap()
}

// ------------------------------------------------------------
// ISOLATED DATABASE
// ------------------------------------------------------------

/// A pool whose connections only see one freshly migrated schema.
pub struct TestDb {
    pub pool: PgPool,
    admin: PgPool,
    schema: String,
}

impl TestDb {
    /// Connects using `TEST_DATABASE_URL` only — never `DATABASE_URL`.
    pub async fn new() -> Self {
        let url = std::env::var("TEST_DATABASE_URL")
            .expect("TEST_DATABASE_URL must be set to run database tests");

        if std::env::var("DATABASE_URL").is_ok_and(|main| main == url) {
            panic!("TEST_DATABASE_URL must not point at the indexer's DATABASE_URL");
        }

        static NEXT: AtomicU32 = AtomicU32::new(0);
        let schema = format!(
            "amiable_test_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );

        let options: PgConnectOptions = url.parse().expect("valid TEST_DATABASE_URL");

        let admin = PgPoolOptions::new()
            .max_connections(1)
            .connect_with(options.clone())
            .await
            .expect("connect to TEST_DATABASE_URL");

        sqlx::raw_sql(AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
            .execute(&admin)
            .await
            .expect("create test schema");

        // Every pooled connection starts with only the test schema on its
        // search_path, so unqualified table names resolve there.
        let pool = PgPoolOptions::new()
            .max_connections(3)
            .connect_with(options.options([("search_path", schema.as_str())]))
            .await
            .expect("connect test pool");

        let mut migrations: Vec<_> = std::fs::read_dir("src/migrations")
            .expect("read src/migrations")
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "sql"))
            .collect();
        migrations.sort();

        for path in migrations {
            let sql = std::fs::read_to_string(&path).unwrap();
            sqlx::raw_sql(AssertSqlSafe(sql))
                .execute(&pool)
                .await
                .unwrap_or_else(|e| panic!("migration {}: {e}", path.display()));
        }

        Self {
            pool,
            admin,
            schema,
        }
    }

    pub async fn cleanup(self) {
        self.pool.close().await;
        sqlx::raw_sql(AssertSqlSafe(format!(
            "DROP SCHEMA {} CASCADE",
            self.schema
        )))
        .execute(&self.admin)
        .await
        .expect("drop test schema");
    }
}
