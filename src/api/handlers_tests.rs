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
