//! Database-backed tests for how a block is persisted: blocks,
//! transactions, inputs, outputs, addresses and spent state.

use std::str::FromStr;

use bitcoin::hashes::Hash;
use bitcoin::{
    Address, BlockHash, Network, OutPoint, PubkeyHash, ScriptBuf, ScriptHash, WScriptHash, Witness,
    WitnessProgram, WitnessVersion,
};
use sqlx::{FromRow, PgPool};

use crate::{
    indexer::sync::{IndexOutcome, index_block, sync_chain},
    test_support::{
        MockChain, TestDb, block_with_txdata, build_chain, coinbase, out, output_state, p2wpkh,
        push_block, tx, tx_height,
    },
};

#[derive(Debug, PartialEq, FromRow)]
struct BlockRow {
    id: i64,
    height: i64,
    hash: String,
    previous_hash: String,
    timestamp: i64,
    tx_count: i32,
}

#[derive(Debug, PartialEq, FromRow)]
struct TxRow {
    txid: String,
    position: i32,
    version: i32,
    lock_time: i64,
    is_coinbase: bool,
}

#[derive(Debug, PartialEq, FromRow)]
struct InputRow {
    vin: i32,
    prev_txid: Option<String>,
    prev_vout: Option<i64>,
}

#[derive(Debug, PartialEq, FromRow)]
struct OutputRow {
    vout: i32,
    value: i64,
    script_pubkey: String,
    address: Option<String>,
    spent: bool,
    spent_by_txid: Option<String>,
    spent_by_vin: Option<i32>,
}

async fn block_rows(pool: &PgPool) -> Vec<BlockRow> {
    sqlx::query_as(
        "SELECT id, height, hash, previous_hash, timestamp, tx_count FROM blocks ORDER BY height",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn tx_rows_at(pool: &PgPool, height: i64) -> Vec<TxRow> {
    sqlx::query_as(
        r#"
        SELECT t.txid, t.position, t.version, t.lock_time, t.is_coinbase
        FROM transactions t
        JOIN blocks b ON b.id = t.block_id
        WHERE b.height = $1
        ORDER BY t.position
        "#,
    )
    .bind(height)
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn input_rows(pool: &PgPool, txid: &str) -> Vec<InputRow> {
    sqlx::query_as(
        r#"
        SELECT i.vin, i.prev_txid, i.prev_vout
        FROM inputs i
        JOIN transactions t ON t.id = i.transaction_id
        WHERE t.txid = $1
        ORDER BY i.vin
        "#,
    )
    .bind(txid)
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn output_rows(pool: &PgPool, txid: &str) -> Vec<OutputRow> {
    sqlx::query_as(
        r#"
        SELECT o.vout, o.value, o.script_pubkey, o.address,
               o.spent, o.spent_by_txid, o.spent_by_vin
        FROM outputs o
        JOIN transactions t ON t.id = o.transaction_id
        WHERE t.txid = $1
        ORDER BY o.vout
        "#,
    )
    .bind(txid)
    .fetch_all(pool)
    .await
    .unwrap()
}

/// Row counts of `(blocks, transactions, inputs, outputs, spent outputs)`.
async fn counts(pool: &PgPool) -> (i64, i64, i64, i64, i64) {
    sqlx::query_as(
        r#"
        SELECT
            (SELECT COUNT(*) FROM blocks),
            (SELECT COUNT(*) FROM transactions),
            (SELECT COUNT(*) FROM inputs),
            (SELECT COUNT(*) FROM outputs),
            (SELECT COUNT(*) FROM outputs WHERE spent)
        "#,
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

fn coinbase_out(height: u64) -> OutPoint {
    OutPoint::new(coinbase(height, 'a').compute_txid(), 0)
}

// ------------------------------------------------------------
// BLOCKS AND TRANSACTIONS
// ------------------------------------------------------------

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn block_and_transaction_rows_match_the_block() {
    let db = TestDb::new().await;

    let mut blocks = build_chain(&[], 2, 'a'); // 0..=1
    let payment = tx(&[coinbase_out(1)], vec![out(1_000, p2wpkh(7))]);
    push_block(&mut blocks, 'a', vec![payment.clone()]); // 2
    sync_chain(&MockChain::new(blocks.clone()), &db.pool)
        .await
        .unwrap();

    let rows = block_rows(&db.pool).await;
    assert_eq!(rows.len(), 3);
    for (row, block) in rows.iter().zip(&blocks) {
        assert_eq!(row.hash, block.block_hash().to_string());
        assert_eq!(row.previous_hash, block.header.prev_blockhash.to_string());
        assert_eq!(row.timestamp, block.header.time as i64);
        assert_eq!(row.tx_count, block.txdata.len() as i32);
    }
    assert_eq!(rows[0].previous_hash, BlockHash::all_zeros().to_string());

    assert_eq!(
        tx_rows_at(&db.pool, 2).await,
        vec![
            TxRow {
                txid: coinbase(2, 'a').compute_txid().to_string(),
                position: 0,
                version: 2,
                lock_time: 0,
                is_coinbase: true,
            },
            TxRow {
                txid: payment.compute_txid().to_string(),
                position: 1,
                version: 2,
                lock_time: 0,
                is_coinbase: false,
            },
        ]
    );

    db.cleanup().await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn reindexing_a_block_is_idempotent() {
    let db = TestDb::new().await;

    // Block 2 spends block 1's coinbase; block 3 spends block 2's output,
    // so re-indexing block 2 must not reset the output block 3 spent.
    let first = tx(&[coinbase_out(1)], vec![out(5_000, p2wpkh(7))]);
    let second = tx(
        &[OutPoint::new(first.compute_txid(), 0)],
        vec![out(4_000, p2wpkh(8))],
    );
    let mut blocks = build_chain(&[], 2, 'a');
    push_block(&mut blocks, 'a', vec![first.clone()]); // 2
    push_block(&mut blocks, 'a', vec![second.clone()]); // 3
    let chain = MockChain::new(blocks);

    sync_chain(&chain, &db.pool).await.unwrap();
    let blocks_before = block_rows(&db.pool).await;
    let counts_before = counts(&db.pool).await;
    let first_outputs_before = output_rows(&db.pool, &first.compute_txid().to_string()).await;

    for height in 0..=3 {
        assert_eq!(
            index_block(&chain, &db.pool, height).await.unwrap(),
            IndexOutcome::Indexed
        );
    }

    assert_eq!(block_rows(&db.pool).await, blocks_before);
    assert_eq!(counts(&db.pool).await, counts_before);
    assert_eq!(
        output_rows(&db.pool, &first.compute_txid().to_string()).await,
        first_outputs_before
    );
    assert_eq!(
        output_state(&db.pool, OutPoint::new(first.compute_txid(), 0)).await,
        (true, Some(second.compute_txid().to_string()), Some(0))
    );

    db.cleanup().await;
}

/// Bitcoin identifies transactions by txid, which excludes witness data.
#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn transactions_are_keyed_by_txid_not_wtxid() {
    let db = TestDb::new().await;

    let mut segwit = tx(&[coinbase_out(1)], vec![out(9_000, p2wpkh(7))]);
    segwit.input[0].witness = Witness::from_slice(&[vec![0x30; 72], vec![0x02; 33]]);
    assert_ne!(
        segwit.compute_txid().to_string(),
        segwit.compute_wtxid().to_string()
    );
    let child = tx(
        &[OutPoint::new(segwit.compute_txid(), 0)],
        vec![out(8_000, p2wpkh(8))],
    );

    let mut blocks = build_chain(&[], 2, 'a');
    push_block(&mut blocks, 'a', vec![segwit.clone()]);
    push_block(&mut blocks, 'a', vec![child.clone()]);
    sync_chain(&MockChain::new(blocks), &db.pool).await.unwrap();

    assert_eq!(tx_height(&db.pool, segwit.compute_txid()).await, Some(2));
    assert_eq!(
        output_state(&db.pool, OutPoint::new(segwit.compute_txid(), 0)).await,
        (true, Some(child.compute_txid().to_string()), Some(0))
    );

    db.cleanup().await;
}

// ------------------------------------------------------------
// INPUTS, OUTPUTS AND SPENT STATE
// ------------------------------------------------------------

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn inputs_reference_outpoints_and_spend_exactly_those_outputs() {
    let db = TestDb::new().await;

    // A: two outputs. B: vin 0 spends A:1, vin 1 spends coinbase 2:0.
    let a = tx(
        &[coinbase_out(1)],
        vec![out(10_000, p2wpkh(7)), out(20_000, p2wpkh(8))],
    );
    let a1 = OutPoint::new(a.compute_txid(), 1);
    let b = tx(&[a1, coinbase_out(2)], vec![out(25_000, p2wpkh(9))]);

    let mut blocks = build_chain(&[], 3, 'a'); // 0..=2
    push_block(&mut blocks, 'a', vec![a.clone()]); // 3
    push_block(&mut blocks, 'a', vec![b.clone()]); // 4
    sync_chain(&MockChain::new(blocks), &db.pool).await.unwrap();

    let b_txid = b.compute_txid().to_string();
    assert_eq!(
        input_rows(&db.pool, &b_txid).await,
        vec![
            InputRow {
                vin: 0,
                prev_txid: Some(a.compute_txid().to_string()),
                prev_vout: Some(1),
            },
            InputRow {
                vin: 1,
                prev_txid: Some(coinbase_out(2).txid.to_string()),
                prev_vout: Some(0),
            },
        ]
    );

    let a_outputs = output_rows(&db.pool, &a.compute_txid().to_string()).await;
    assert_eq!(a_outputs.len(), 2);
    assert_eq!((a_outputs[0].vout, a_outputs[0].value), (0, 10_000));
    assert_eq!((a_outputs[1].vout, a_outputs[1].value), (1, 20_000));

    // Each input marks exactly its own outpoint, recording which vin spent it.
    assert_eq!(
        output_state(&db.pool, OutPoint::new(a.compute_txid(), 0)).await,
        (false, None, None)
    );
    assert_eq!(
        output_state(&db.pool, a1).await,
        (true, Some(b_txid.clone()), Some(0))
    );
    assert_eq!(
        output_state(&db.pool, coinbase_out(2)).await,
        (true, Some(b_txid.clone()), Some(1))
    );
    assert_eq!(
        output_state(&db.pool, OutPoint::new(b.compute_txid(), 0)).await,
        (false, None, None)
    );

    db.cleanup().await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn output_spent_later_in_the_same_block_is_marked_spent() {
    let db = TestDb::new().await;

    let parent = tx(&[coinbase_out(1)], vec![out(7_000, p2wpkh(7))]);
    let child = tx(
        &[OutPoint::new(parent.compute_txid(), 0)],
        vec![out(6_000, p2wpkh(8))],
    );

    let mut blocks = build_chain(&[], 2, 'a');
    push_block(&mut blocks, 'a', vec![parent.clone(), child.clone()]);
    sync_chain(&MockChain::new(blocks), &db.pool).await.unwrap();

    assert_eq!(
        output_state(&db.pool, OutPoint::new(parent.compute_txid(), 0)).await,
        (true, Some(child.compute_txid().to_string()), Some(0))
    );

    db.cleanup().await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn spending_an_unindexed_outpoint_changes_no_outputs() {
    let db = TestDb::new().await;

    let mut blocks = build_chain(&[], 2, 'a');
    sync_chain(&MockChain::new(blocks.clone()), &db.pool)
        .await
        .unwrap();
    let spent_before = counts(&db.pool).await.4;

    let unknown = OutPoint::new(coinbase(99, 'x').compute_txid(), 0);
    let orphan_spend = tx(&[unknown], vec![out(1_000, p2wpkh(7))]);
    push_block(&mut blocks, 'a', vec![orphan_spend.clone()]);
    sync_chain(&MockChain::new(blocks), &db.pool).await.unwrap();

    assert_eq!(counts(&db.pool).await.4, spent_before);
    assert_eq!(
        input_rows(&db.pool, &orphan_spend.compute_txid().to_string()).await,
        vec![InputRow {
            vin: 0,
            prev_txid: Some(unknown.txid.to_string()),
            prev_vout: Some(0),
        }]
    );

    db.cleanup().await;
}

// ------------------------------------------------------------
// COINBASE
// ------------------------------------------------------------

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn coinbase_has_no_previous_outpoint_and_spends_nothing() {
    let db = TestDb::new().await;

    // Like real regtest coinbases: a reward output plus a witness
    // commitment, which is an OP_RETURN output.
    let mut reward = coinbase(1, 'a');
    let mut commitment = vec![0x6a, 0x24, 0xaa, 0x21, 0xa9, 0xed];
    commitment.extend([0x11; 32]);
    reward
        .output
        .push(out(0, ScriptBuf::from_bytes(commitment)));

    let genesis = build_chain(&[], 1, 'a');
    let mut blocks = genesis.clone();
    blocks.push(block_with_txdata(
        genesis[0].block_hash(),
        1,
        vec![reward.clone()],
    ));
    sync_chain(&MockChain::new(blocks), &db.pool).await.unwrap();

    let txid = reward.compute_txid().to_string();
    assert!(tx_rows_at(&db.pool, 1).await[0].is_coinbase);
    assert_eq!(
        input_rows(&db.pool, &txid).await,
        vec![InputRow {
            vin: 0,
            prev_txid: None,
            prev_vout: None,
        }]
    );

    let outputs = output_rows(&db.pool, &txid).await;
    assert_eq!(outputs.len(), 2, "both coinbase outputs are stored");
    assert!(outputs.iter().all(|o| !o.spent));
    assert!(outputs[1].script_pubkey.starts_with("OP_RETURN"));
    assert_eq!(counts(&db.pool).await.4, 0, "a coinbase spends nothing");

    db.cleanup().await;
}

// ------------------------------------------------------------
// ADDRESSES
// ------------------------------------------------------------

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn addresses_are_extracted_for_standard_scripts_only() {
    let db = TestDb::new().await;

    let scripts = [
        (
            "p2pkh",
            ScriptBuf::new_p2pkh(&PubkeyHash::from_byte_array([3; 20])),
        ),
        (
            "p2sh",
            ScriptBuf::new_p2sh(&ScriptHash::from_byte_array([4; 20])),
        ),
        ("p2wpkh", p2wpkh(5)),
        (
            "p2wsh",
            ScriptBuf::new_p2wsh(&WScriptHash::from_byte_array([6; 32])),
        ),
        (
            "p2tr",
            ScriptBuf::new_witness_program(
                &WitnessProgram::new(WitnessVersion::V1, &[7; 32]).unwrap(),
            ),
        ),
        (
            "op_return",
            ScriptBuf::from_bytes(vec![0x6a, 0x04, 0xde, 0xad, 0xbe, 0xef]),
        ),
        ("bare OP_TRUE", ScriptBuf::from_bytes(vec![0x51])),
    ];
    let payment = tx(
        &[coinbase_out(1)],
        scripts.iter().map(|(_, s)| out(1_000, s.clone())).collect(),
    );

    let mut blocks = build_chain(&[], 2, 'a');
    push_block(&mut blocks, 'a', vec![payment.clone()]);
    sync_chain(&MockChain::new(blocks), &db.pool).await.unwrap();

    let rows = output_rows(&db.pool, &payment.compute_txid().to_string()).await;
    let expected_prefix = ["m|n", "2", "bcrt1q", "bcrt1q", "bcrt1p"];

    for ((name, script), row) in scripts.iter().zip(&rows) {
        match (*name, &row.address) {
            ("op_return" | "bare OP_TRUE", address) => {
                assert_eq!(address, &None, "{name} has no address");
            }
            (_, Some(address)) => {
                // The stored address is valid for regtest and pays to exactly
                // this script.
                let parsed = Address::from_str(address)
                    .unwrap()
                    .require_network(Network::Regtest)
                    .unwrap_or_else(|e| panic!("{name}: {address} is not regtest: {e}"));
                assert_eq!(&parsed.script_pubkey(), script, "{name}");
            }
            (_, None) => panic!("{name} should have an address"),
        }
    }
    for (prefix, row) in expected_prefix.iter().zip(&rows) {
        let address = row.address.as_deref().unwrap();
        assert!(
            prefix.split('|').any(|p| address.starts_with(p)),
            "{address} should start with {prefix}"
        );
    }

    db.cleanup().await;
}

// ------------------------------------------------------------
// ATOMICITY
// ------------------------------------------------------------

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn failed_block_leaves_no_partial_state() {
    let db = TestDb::new().await;

    let mut blocks = build_chain(&[], 2, 'a'); // 0..=1
    sync_chain(&MockChain::new(blocks.clone()), &db.pool)
        .await
        .unwrap();
    let counts_before = counts(&db.pool).await;

    // Inject a database failure partway through the block: the first
    // transaction (which spends block 1's coinbase) is written, then the
    // second one's output insert fails.
    sqlx::raw_sql(
        r#"
        CREATE FUNCTION fail_on_666() RETURNS trigger AS $$
        BEGIN
            IF NEW.value = 666 THEN
                RAISE EXCEPTION 'injected failure';
            END IF;
            RETURN NEW;
        END
        $$ LANGUAGE plpgsql;

        CREATE TRIGGER fail_on_666
        BEFORE INSERT ON outputs
        FOR EACH ROW EXECUTE FUNCTION fail_on_666();
        "#,
    )
    .execute(&db.pool)
    .await
    .unwrap();

    let spender = tx(&[coinbase_out(1)], vec![out(1_000, p2wpkh(7))]);
    let poisoned = tx(
        &[OutPoint::new(spender.compute_txid(), 0)],
        vec![out(666, p2wpkh(8))],
    );
    push_block(&mut blocks, 'a', vec![spender.clone(), poisoned]);
    let chain = MockChain::new(blocks.clone());

    let error = sync_chain(&chain, &db.pool).await.unwrap_err();
    assert!(
        format!("{error:#}").contains("injected failure"),
        "{error:#}"
    );

    // Nothing from block 2 survived, including the spend it made.
    assert_eq!(counts(&db.pool).await, counts_before);
    assert_eq!(tx_height(&db.pool, spender.compute_txid()).await, None);
    assert_eq!(
        output_state(&db.pool, coinbase_out(1)).await,
        (false, None, None)
    );

    // Once the fault is gone, the next sync indexes the block normally.
    sqlx::raw_sql("DROP TRIGGER fail_on_666 ON outputs")
        .execute(&db.pool)
        .await
        .unwrap();
    sync_chain(&chain, &db.pool).await.unwrap();
    assert_eq!(
        block_rows(&db.pool).await.last().unwrap().hash,
        blocks[2].block_hash().to_string()
    );
    assert_eq!(
        output_state(&db.pool, coinbase_out(1)).await,
        (true, Some(spender.compute_txid().to_string()), Some(0))
    );

    db.cleanup().await;
}
