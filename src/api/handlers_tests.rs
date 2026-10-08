//! Tests for the API's own SQL: what counts as a UTXO, and address totals.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use bitcoin::{Address, Network, OutPoint, ScriptBuf};

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
