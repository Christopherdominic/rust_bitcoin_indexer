//! Tests for the API's own SQL: what counts as a UTXO, and address totals.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use bitcoin::{Address, Network, OutPoint, ScriptBuf};

use crate::rpc::bitcoin::BlockchainInfo;

use super::*;
use crate::{
    indexer::sync::sync_chain,
    test_support::{MockChain, TestDb, build_chain, coinbase, out, p2wpkh, push_block, tx},
};

fn address_of(script: &ScriptBuf) -> String {
    Address::from_script(script, Network::Regtest)
        .unwrap()
        .to_string()
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn op_return_is_in_transaction_history_but_never_a_utxo() {
    let db = TestDb::new().await;
    let state = AppState::new(db.pool.clone());

    let funding = OutPoint::new(coinbase(1, 'a').compute_txid(), 0);
    let op_return = ScriptBuf::from_bytes(vec![0x6a, 0x04, 0xde, 0xad, 0xbe, 0xef]);
    let payment = tx(
        &[funding],
        vec![out(40_000, p2wpkh(9)), out(0, op_return.clone())],
    );
    let txid = payment.compute_txid().to_string();

    let mut blocks = build_chain(&[], 2, 'a'); // 0..=1
    push_block(&mut blocks, 'a', vec![payment]); // 2
    sync_chain(&MockChain::new(blocks), &db.pool).await.unwrap();

    // Stored and returned by the transaction endpoint.
    let detail = get_transaction(State(state.clone()), Path(txid.clone()))
        .await
        .unwrap()
        .0;
    assert_eq!(detail.outputs.len(), 2);
    assert_eq!(detail.outputs[1].vout, 1);
    assert_eq!(detail.outputs[1].script_pubkey, op_return.to_string());
    assert!(detail.outputs[1].script_pubkey.starts_with("OP_RETURN"));
    assert!(!detail.outputs[1].spent);

    // Not returned as a UTXO; the spent coinbase output is not either.
    let utxos = get_utxos(State(state.clone())).await.unwrap().0;
    assert!(!utxos.iter().any(|u| u.txid == txid && u.vout == 1));
    assert!(
        !utxos
            .iter()
            .any(|u| u.txid == funding.txid.to_string() && u.vout == 0)
    );
    assert!(utxos.iter().any(|u| u.txid == txid && u.vout == 0));
    // Remaining UTXOs: coinbases of blocks 0 and 2, plus the payment.
    assert_eq!(utxos.len(), 3);

    // Status: every output is counted, but only spendable ones are unspent.
    let status = status(State(state)).await.unwrap().0;
    assert_eq!(status.indexed_height, Some(2));
    assert_eq!(status.blocks, 3);
    assert_eq!(status.transactions, 4);
    assert_eq!(status.inputs, 4);
    assert_eq!(status.outputs, 5);
    assert_eq!(status.unspent_outputs, utxos.len() as i64);

    db.cleanup().await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn address_totals_follow_spent_state() {
    let db = TestDb::new().await;
    let state = AppState::new(db.pool.clone());

    // A pays the address twice; B later spends only A:0.
    let watched = p2wpkh(9);
    let address = address_of(&watched);
    let a = tx(
        &[OutPoint::new(coinbase(1, 'a').compute_txid(), 0)],
        vec![out(30_000, watched.clone()), out(20_000, watched.clone())],
    );
    let b = tx(
        &[OutPoint::new(a.compute_txid(), 0)],
        vec![out(25_000, p2wpkh(10))],
    );

    let mut blocks = build_chain(&[], 2, 'a');
    push_block(&mut blocks, 'a', vec![a.clone()]);
    let before_spend = blocks.clone();
    push_block(&mut blocks, 'a', vec![b]);

    // Before the spend: everything received is still balance.
    sync_chain(&MockChain::new(before_spend), &db.pool)
        .await
        .unwrap();
    let summary = get_address(State(state.clone()), Path(address.clone()))
        .await
        .unwrap()
        .0;
    assert_eq!(
        (summary.received, summary.spent, summary.balance),
        (50_000, 0, 50_000)
    );

    // After the spend.
    sync_chain(&MockChain::new(blocks), &db.pool).await.unwrap();
    let summary = get_address(State(state.clone()), Path(address.clone()))
        .await
        .unwrap()
        .0;
    assert_eq!(summary.address, address);
    assert_eq!(
        (summary.received, summary.spent, summary.balance),
        (50_000, 30_000, 20_000)
    );

    let utxos = get_address_utxos(State(state.clone()), Path(address))
        .await
        .unwrap()
        .0;
    assert_eq!(utxos.len(), 1);
    assert_eq!(
        (utxos[0].txid.clone(), utxos[0].vout, utxos[0].value),
        (a.compute_txid().to_string(), 1, 20_000)
    );

    // An address the indexer has never seen.
    let unknown = address_of(&p2wpkh(200));
    assert_eq!(
        get_address(State(state), Path(unknown)).await.err(),
        Some(StatusCode::NOT_FOUND)
    );

    db.cleanup().await;
}

/// `(txid, direction, received, sent, net, value, spent)` per history row.
fn history_rows(
    rows: &[AddressTransactionResponse],
) -> Vec<(String, &'static str, i64, i64, i64, i64, bool)> {
    rows.iter()
        .map(|r| {
            (
                r.txid.clone(),
                r.direction,
                r.received,
                r.sent,
                r.net,
                r.value,
                r.spent,
            )
        })
        .collect()
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn address_history_nets_receives_spends_and_change_per_transaction() {
    let db = TestDb::new().await;
    let state = AppState::new(db.pool.clone());

    let watched = p2wpkh(9);
    let address = address_of(&watched);
    let elsewhere = p2wpkh(10);

    // A: pays the address twice in one transaction.
    let a = tx(
        &[OutPoint::new(coinbase(1, 'a').compute_txid(), 0)],
        vec![out(30_000, watched.clone()), out(20_000, watched.clone())],
    );
    // B: spends A:0 (30k), pays 25k away and 4k change back to the address.
    let b = tx(
        &[OutPoint::new(a.compute_txid(), 0)],
        vec![out(25_000, elsewhere.clone()), out(4_000, watched.clone())],
    );
    // C: spends A:1 (20k) and B:1 (4k) together, nothing comes back.
    let c = tx(
        &[
            OutPoint::new(a.compute_txid(), 1),
            OutPoint::new(b.compute_txid(), 1),
        ],
        vec![out(23_000, elsewhere)],
    );

    let mut blocks = build_chain(&[], 2, 'a'); // 0..=1
    push_block(&mut blocks, 'a', vec![a.clone()]); // 2
    push_block(&mut blocks, 'a', vec![b.clone()]); // 3
    push_block(&mut blocks, 'a', vec![c.clone()]); // 4
    sync_chain(&MockChain::new(blocks), &db.pool).await.unwrap();

    let history = get_address_transactions(State(state.clone()), Path(address.clone()))
        .await
        .unwrap()
        .0;

    let txid = |t: &bitcoin::Transaction| t.compute_txid().to_string();
    assert_eq!(
        history_rows(&history),
        vec![
            // Spending two of the address's outputs is one row, not two.
            (txid(&c), "sent", 0, 24_000, -24_000, 24_000, false),
            // Change is netted: 30k out, 4k back, so the balance fell 26k.
            (txid(&b), "sent", 4_000, 30_000, -26_000, 26_000, true),
            // Two outputs to the address in one transaction: one row.
            (txid(&a), "received", 50_000, 0, 50_000, 50_000, true),
        ]
    );
    assert_eq!(
        history.iter().map(|r| r.block_height).collect::<Vec<_>>(),
        vec![4, 3, 2]
    );

    // The history agrees with the summary totals.
    let summary = get_address(State(state.clone()), Path(address.clone()))
        .await
        .unwrap()
        .0;
    assert_eq!(summary.transaction_count, 3);
    assert_eq!(
        summary.received,
        history.iter().map(|r| r.received).sum::<i64>()
    );
    assert_eq!(summary.spent, history.iter().map(|r| r.sent).sum::<i64>());
    assert_eq!(summary.balance, history.iter().map(|r| r.net).sum::<i64>());
    assert_eq!(summary.balance, 0);

    // Watch activity counts spends too, and its latest activity is a spend.
    let (created, _) = watch_address(State(state.clone()), Path(address.clone()))
        .await
        .unwrap();
    assert_eq!(created, StatusCode::CREATED);
    let activity = get_watch_activity(State(state), Path(address))
        .await
        .unwrap()
        .0;
    assert_eq!(activity.transaction_count, 3);
    assert_eq!(activity.latest_txid, Some(txid(&c)));
    assert_eq!(activity.latest_block_height, Some(4));

    db.cleanup().await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn plain_receive_keeps_the_previous_row_shape() {
    let db = TestDb::new().await;
    let state = AppState::new(db.pool.clone());

    let watched = p2wpkh(9);
    let payment = tx(
        &[OutPoint::new(coinbase(1, 'a').compute_txid(), 0)],
        vec![out(12_345, watched.clone())],
    );
    let mut blocks = build_chain(&[], 2, 'a');
    push_block(&mut blocks, 'a', vec![payment.clone()]);
    sync_chain(&MockChain::new(blocks), &db.pool).await.unwrap();

    let history = get_address_transactions(State(state), Path(address_of(&watched)))
        .await
        .unwrap()
        .0;

    // Exactly what the per-output endpoint returned for this case before.
    assert_eq!(history.len(), 1);
    let row = &history[0];
    assert_eq!(row.txid, payment.compute_txid().to_string());
    assert_eq!(row.block_height, 2);
    assert_eq!(row.value, 12_345);
    assert!(!row.spent);
    assert_eq!(row.direction, "received");

    db.cleanup().await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn self_transfer_with_external_fee_input_is_self() {
    let db = TestDb::new().await;
    let state = AppState::new(db.pool.clone());

    // The address's 10k output is moved back to the address unchanged; a
    // second, unrelated input pays the fee.
    let watched = p2wpkh(9);
    let fund = tx(
        &[OutPoint::new(coinbase(1, 'a').compute_txid(), 0)],
        vec![out(10_000, watched.clone())],
    );
    let consolidate = tx(
        &[
            OutPoint::new(fund.compute_txid(), 0),
            OutPoint::new(coinbase(2, 'a').compute_txid(), 0),
        ],
        vec![out(10_000, watched.clone())],
    );
    let mut blocks = build_chain(&[], 3, 'a');
    push_block(&mut blocks, 'a', vec![fund]);
    push_block(&mut blocks, 'a', vec![consolidate.clone()]);
    sync_chain(&MockChain::new(blocks), &db.pool).await.unwrap();

    let history = get_address_transactions(State(state), Path(address_of(&watched)))
        .await
        .unwrap()
        .0;

    assert_eq!(history[0].txid, consolidate.compute_txid().to_string());
    assert_eq!(
        (
            history[0].direction,
            history[0].received,
            history[0].sent,
            history[0].net
        ),
        ("self", 10_000, 10_000, 0)
    );

    db.cleanup().await;
}

// ------------------------------------------------------------
// STATUS
// ------------------------------------------------------------

/// Bitcoin Core as the status endpoint sees it: a tip, or unreachable.
struct MockNode(Option<BlockchainInfo>);

impl crate::api::state::NodeInfo for MockNode {
    fn blockchain_info(&self) -> anyhow::Result<BlockchainInfo> {
        self.0
            .clone()
            .ok_or_else(|| anyhow::anyhow!("connection refused"))
    }
}

fn core_at(height: u64, hash: &bitcoin::BlockHash) -> MockNode {
    MockNode(Some(BlockchainInfo {
        chain: "regtest".to_string(),
        blocks: height,
        best_block_hash: hash.to_string(),
    }))
}

async fn status_with(db: &TestDb, node: MockNode) -> StatusResponse {
    let state = AppState::new(db.pool.clone()).with_node(std::sync::Arc::new(node));
    status(State(state)).await.unwrap().0
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn status_compares_indexed_tip_with_core() {
    let db = TestDb::new().await;

    let shared = build_chain(&[], 2, 'a'); // 0..=1
    let indexed = build_chain(&shared, 1, 'a'); // 2A
    sync_chain(&MockChain::new(indexed.clone()), &db.pool)
        .await
        .unwrap();

    // Core's best block is our tip.
    let s = status_with(&db, core_at(2, &indexed[2].block_hash())).await;
    assert_eq!(s.bitcoin_core, ServiceState::Connected);
    assert_eq!(s.network.as_deref(), Some("regtest"));
    assert_eq!(
        (s.core_height, s.indexed_height, s.blocks_behind, s.synced),
        (Some(2), Some(2), Some(0), Some(true))
    );

    // Core is three blocks ahead.
    let ahead = build_chain(&indexed, 3, 'a');
    let s = status_with(&db, core_at(5, &ahead[5].block_hash())).await;
    assert_eq!((s.blocks_behind, s.synced), (Some(3), Some(false)));

    // Same height, different block: a reorg the indexer has not handled yet.
    let fork = build_chain(&shared, 1, 'b'); // 2B
    let s = status_with(&db, core_at(2, &fork[2].block_hash())).await;
    assert_eq!((s.blocks_behind, s.synced), (Some(0), Some(false)));

    db.cleanup().await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn status_reports_unknown_rather_than_guessing_when_core_is_down() {
    let db = TestDb::new().await;
    sync_chain(&MockChain::new(build_chain(&[], 3, 'a')), &db.pool)
        .await
        .unwrap();

    let s = status_with(&db, MockNode(None)).await;

    assert_eq!(s.bitcoin_core, ServiceState::Unreachable);
    assert_eq!(s.database, ServiceState::Connected);
    assert_eq!(
        (s.network, s.core_height, s.blocks_behind, s.synced),
        (None, None, None, None)
    );
    // What the database knows is still reported.
    assert_eq!((s.indexed_height, s.blocks), (Some(2), 3));

    db.cleanup().await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn status_json_exposes_recorded_zmq_state() {
    use crate::zmq::status::{EndpointState, ZmqStatus};
    use bitcoincore_rpc::jsonrpc::serde_json::{self, json};

    let db = TestDb::new().await;
    let blocks = build_chain(&[], 1, 'a');
    sync_chain(&MockChain::new(blocks.clone()), &db.pool)
        .await
        .unwrap();

    let zmq = std::sync::Arc::new(ZmqStatus::new(false));
    zmq.blocks.set(EndpointState::Connected);
    zmq.blocks.message_received();

    let state = AppState::new(db.pool.clone())
        .with_node(std::sync::Arc::new(core_at(0, &blocks[0].block_hash())))
        .with_zmq(zmq);
    let body = serde_json::to_value(status(State(state)).await.unwrap().0).unwrap();

    assert_eq!(body["bitcoin_core"], json!("connected"));
    assert_eq!(body["database"], json!("connected"));
    assert_eq!(body["synced"], json!(true));
    assert_eq!(body["zmq"]["blocks"]["status"], json!("connected"));
    assert!(body["zmq"]["blocks"]["last_message_at"].is_i64());
    assert_eq!(
        body["zmq"]["transactions"]["status"],
        json!("not_configured")
    );
    assert_eq!(body["zmq"]["transactions"]["last_message_at"], json!(null));
    assert_eq!(body["mempool_transactions"], json!(0));
    // Fields the frontend already reads are still there.
    for field in [
        "indexed_height",
        "blocks",
        "transactions",
        "inputs",
        "outputs",
        "unspent_outputs",
    ] {
        assert!(body.get(field).is_some(), "{field} missing");
    }

    db.cleanup().await;
}

// ------------------------------------------------------------
// UTXO SEMANTICS
// ------------------------------------------------------------

/// One output of every kind the classifier distinguishes, plus edge cases.
fn assorted_scripts() -> Vec<ScriptBuf> {
    use bitcoin::hashes::Hash;
    vec![
        ScriptBuf::new_p2pkh(&bitcoin::PubkeyHash::from_byte_array([1; 20])),
        ScriptBuf::new_p2sh(&bitcoin::ScriptHash::from_byte_array([2; 20])),
        p2wpkh(3),
        ScriptBuf::new_p2wsh(&bitcoin::WScriptHash::from_byte_array([4; 32])),
        ScriptBuf::from_bytes([vec![0x51, 0x20], vec![5; 32]].concat()), // p2tr
        ScriptBuf::from_bytes(vec![0x6a, 0x02, 0xbe, 0xef]),             // OP_RETURN data
        ScriptBuf::from_bytes(vec![0x6a]),                               // bare OP_RETURN
        ScriptBuf::from_bytes([vec![0x21], vec![2; 33], vec![0xac]].concat()), // p2pk
        ScriptBuf::from_bytes(vec![0x51, 0x02, 0x4e, 0x73]),             // pay-to-anchor
        ScriptBuf::from_bytes(vec![0xbb, 0x51]),                         // prints as OP_RETURN_187
        ScriptBuf::from_bytes(vec![0x7e]),                               // OP_CAT
        ScriptBuf::new(),
    ]
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn sql_backfill_classifier_agrees_with_the_indexer() {
    let db = TestDb::new().await;

    let payment = tx(
        &[OutPoint::new(coinbase(1, 'a').compute_txid(), 0)],
        assorted_scripts()
            .into_iter()
            .map(|s| out(1_000, s))
            .collect(),
    );
    let mut blocks = build_chain(&[], 2, 'a');
    push_block(&mut blocks, 'a', vec![payment]);
    sync_chain(&MockChain::new(blocks), &db.pool).await.unwrap();

    // Migration 007 backfills existing rows with classify_script_pubkey().
    let rows: Vec<(String, String, String, bool)> = sqlx::query_as(
        r#"
        SELECT script_pubkey, script_type,
               classify_script_pubkey(script_pubkey),
               provably_unspendable
        FROM outputs
        "#,
    )
    .fetch_all(&db.pool)
    .await
    .unwrap();

    assert!(rows.len() > assorted_scripts().len());
    for (asm, indexed, backfilled, unspendable) in rows {
        assert_eq!(indexed, backfilled, "script_type for {asm:?}");
        assert_eq!(
            unspendable,
            backfilled == "op_return",
            "provably_unspendable for {asm:?}"
        );
    }

    db.cleanup().await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn only_op_return_outputs_are_left_out_of_the_utxo_set() {
    let db = TestDb::new().await;
    let state = AppState::new(db.pool.clone());

    let payment = tx(
        &[OutPoint::new(coinbase(1, 'a').compute_txid(), 0)],
        assorted_scripts()
            .into_iter()
            .map(|s| out(1_000, s))
            .collect(),
    );
    let txid = payment.compute_txid().to_string();
    let mut blocks = build_chain(&[], 2, 'a');
    push_block(&mut blocks, 'a', vec![payment]);
    sync_chain(&MockChain::new(blocks), &db.pool).await.unwrap();

    let utxos = get_utxos(State(state.clone())).await.unwrap().0;
    let listed: Vec<i32> = utxos
        .iter()
        .filter(|u| u.txid == txid)
        .map(|u| u.vout)
        .collect();

    // vouts 5 and 6 are OP_RETURN. 9 (0xbb, printed "OP_RETURN_187") used to
    // be dropped by the old `NOT LIKE 'OP_RETURN%'` filter.
    let expected: Vec<i32> = (0..12).filter(|v| *v != 5 && *v != 6).rev().collect();
    assert_eq!(listed, expected);
    let undefined_opcode = utxos
        .iter()
        .find(|u| u.txid == txid && u.vout == 9)
        .unwrap();
    assert_eq!(undefined_opcode.script_type, "unknown");

    let s = status(State(state)).await.unwrap().0;
    assert_eq!(s.unspent_outputs, utxos.len() as i64);

    db.cleanup().await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn coinbase_outputs_mature_after_100_confirmations() {
    let db = TestDb::new().await;
    let state = AppState::new(db.pool.clone());
    let miner = address_of(&p2wpkh(1)); // every test coinbase pays here
    let reward = 50_0000_0000;

    // Heights 0..=99: genesis has 100 confirmations, height 1 has 99.
    let mut blocks = build_chain(&[], 100, 'a');
    sync_chain(&MockChain::new(blocks.clone()), &db.pool)
        .await
        .unwrap();

    let summary = get_utxo_summary(State(state.clone())).await.unwrap().0;
    assert_eq!(summary.coinbase_maturity, 100);
    assert_eq!(summary.utxos.count, 100);
    assert_eq!(summary.immature_coinbase.count, 99);
    assert_eq!(summary.spendable.count, 1);

    let utxos = get_address_utxos(State(state.clone()), Path(miner.clone()))
        .await
        .unwrap()
        .0;
    let at = |h: i64| utxos.iter().find(|u| u.block_height == h).unwrap();
    assert_eq!((at(0).confirmations, at(0).mature), (100, true));
    assert_eq!((at(1).confirmations, at(1).mature), (99, false));
    assert!(at(1).is_coinbase);

    let address = get_address(State(state.clone()), Path(miner.clone()))
        .await
        .unwrap()
        .0;
    assert_eq!(address.balance, 100 * reward);
    assert_eq!(address.immature_balance, 99 * reward);
    assert_eq!(address.spendable_balance, reward);

    // One more block: height 1 reaches 100 confirmations.
    blocks = build_chain(&blocks, 1, 'a');
    sync_chain(&MockChain::new(blocks), &db.pool).await.unwrap();

    let summary = get_utxo_summary(State(state)).await.unwrap().0;
    assert_eq!(summary.immature_coinbase.count, 99); // heights 2..=100
    assert_eq!(summary.spendable.count, 2); // heights 0 and 1

    db.cleanup().await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn utxo_summary_categories_add_up() {
    let db = TestDb::new().await;
    let state = AppState::new(db.pool.clone());

    // A non-coinbase payment spending block 1's coinbase into a mix of
    // outputs, one of which is spent again later.
    let payment = tx(
        &[OutPoint::new(coinbase(1, 'a').compute_txid(), 0)],
        assorted_scripts()
            .into_iter()
            .map(|s| out(1_000, s))
            .collect(),
    );
    let respend = tx(
        &[OutPoint::new(payment.compute_txid(), 2)],
        vec![out(900, p2wpkh(9))],
    );
    let mut blocks = build_chain(&[], 2, 'a');
    push_block(&mut blocks, 'a', vec![payment]);
    push_block(&mut blocks, 'a', vec![respend]);
    sync_chain(&MockChain::new(blocks), &db.pool).await.unwrap();

    let s = get_utxo_summary(State(state)).await.unwrap().0;

    let sum = |a: OutputTotals, b: OutputTotals| OutputTotals {
        count: a.count + b.count,
        value: a.value + b.value,
    };
    assert_eq!(
        s.outputs,
        sum(sum(s.provably_unspendable, s.spent), s.utxos),
        "outputs = unspendable + spent + utxos"
    );
    assert_eq!(s.utxos, sum(s.immature_coinbase, s.spendable));
    assert_eq!(s.provably_unspendable.count, 2);
    assert_eq!(s.spent.count, 2); // block 1's coinbase and payment:2
    assert_eq!(
        s.utxos_by_script_type.iter().map(|t| t.count).sum::<i64>(),
        s.utxos.count
    );
    // Only non-coinbase outputs are spendable at this low height.
    assert_eq!(s.spendable.count, 10);

    db.cleanup().await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn migration_007_backfills_previously_indexed_outputs() {
    let db = TestDb::migrated_until("007").await;

    // Rows as the indexer stored them before 007: no script semantics.
    sqlx::raw_sql(
        r#"
        INSERT INTO blocks (height, hash, previous_hash, timestamp, tx_count)
        VALUES (0, 'h0', 'none', 0, 1);
        INSERT INTO transactions (txid, block_id, position, version, lock_time, is_coinbase)
        SELECT 't0', id, 0, 2, 0, TRUE FROM blocks;
        "#,
    )
    .execute(&db.pool)
    .await
    .unwrap();
    let scripts = assorted_scripts();
    for (vout, script) in scripts.iter().enumerate() {
        sqlx::query(
            r#"
            INSERT INTO outputs (transaction_id, vout, value, script_pubkey)
            SELECT id, $1, 1000, $2 FROM transactions
            "#,
        )
        .bind(vout as i32)
        .bind(script.to_string())
        .execute(&db.pool)
        .await
        .unwrap();
    }

    db.apply_migration("007_output_script_semantics.sql").await;

    let rows: Vec<(i32, String, bool)> =
        sqlx::query_as("SELECT vout, script_type, provably_unspendable FROM outputs ORDER BY vout")
            .fetch_all(&db.pool)
            .await
            .unwrap();

    for ((vout, script_type, unspendable), script) in rows.iter().zip(&scripts) {
        assert_eq!(
            script_type.as_str(),
            crate::db::outputs::script_type(script),
            "vout {vout}: {script}"
        );
        assert_eq!(
            *unspendable,
            crate::db::outputs::is_provably_unspendable(script),
            "vout {vout}: {script}"
        );
    }
    assert_eq!(rows.len(), scripts.len());

    db.cleanup().await;
}
