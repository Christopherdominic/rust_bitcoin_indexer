//! What the dashboard does: Bitcoin Core wallet/mining RPCs and reads of
//! the indexer's own data.
//!
//! Every RPC runs on a blocking thread (`spawn_blocking`), so the event loop
//! never waits on Bitcoin Core. Indexer data comes from the same handler
//! functions the HTTP API serves, called in-process, so the dashboard shows
//! exactly what `/api/...` returns.

use std::sync::{Arc, Mutex};

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use bitcoin::Amount;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

use crate::{
    api::{
        handlers::{
            BlocksQuery, get_address, get_address_transactions, get_block, get_block_by_hash,
            get_blocks, get_transaction, get_utxo_summary, status,
        },
        mempool::{get_mempool, get_mempool_transaction, get_transaction_status},
        state::AppState,
    },
    rpc::bitcoin::BitcoinRpc,
    tui::command::{RpcCall, send_amount, send_outputs},
};

/// The guided demos' default payment: small enough for a regtest wallet whose
/// coinbase rewards are mostly still immature.
pub const DEFAULT_SEND_SATS: u64 = 10_000_000;

/// Everything an operation needs; cheap to clone into a task.
#[derive(Clone)]
pub struct Services {
    pub state: AppState,
    pub rpc: Arc<BitcoinRpc>,
    /// Collects the JSON-RPC calls of the action being executed; `None` for
    /// background refreshes, which are not shown.
    pub recorder: Option<Arc<Mutex<Vec<RpcCall>>>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    ChainInfo,
    NewAddress,
    Send {
        sats: u64,
    },
    Mine {
        blocks: u64,
    },
    /// Mine with `generateblock` and no transactions: a competing branch
    /// that leaves mempool transactions unconfirmed.
    MineEmpty {
        blocks: u64,
    },
    Inspect,
    Utxos,
    /// Look up Core's tip so the invalidation can be confirmed first.
    PrepareInvalidate,
    /// Invalidate the block chosen by `PrepareInvalidate`.
    Invalidate,
}

impl Op {
    pub fn label(self) -> String {
        match self {
            Op::ChainInfo => "Blockchain info".into(),
            Op::NewAddress => "New address".into(),
            Op::Send { sats } => format!("Send {} BTC", btc(sats)),
            Op::Mine { blocks } => format!("Mine {blocks} block{}", plural(blocks)),
            Op::MineEmpty { blocks } => format!("Mine {blocks} competing block{}", plural(blocks)),
            Op::Inspect => "Inspect transaction".into(),
            Op::Utxos => "UTXO overview".into(),
            Op::PrepareInvalidate => "Invalidate tip block".into(),
            Op::Invalidate => "Invalidate tip block".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockRef {
    pub height: u64,
    pub hash: String,
}

/// State carried between operations (all of it comes from earlier results).
#[derive(Debug, Clone, Default)]
pub struct Inputs {
    pub tracked_tx: Option<String>,
    pub tracked_address: Option<String>,
    pub mining_address: Option<String>,
    pub invalidate_target: Option<BlockRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    Field(String, String),
    Note(String),
    Good(String),
    Warn(String),
}

#[derive(Debug, Clone)]
pub struct Outcome {
    pub op: Op,
    /// Every Bitcoin Core call made, in order, with its result.
    pub calls: Vec<RpcCall>,
    pub result: Result<Vec<Line>, String>,
    pub updates: Inputs,
}

pub async fn execute(services: Services, op: Op, inputs: Inputs) -> Outcome {
    let recorder = Arc::new(Mutex::new(Vec::new()));
    let services = Services {
        recorder: Some(recorder.clone()),
        ..services
    };
    let mut updates = Inputs::default();
    let result = run(&services, op, &inputs, &mut updates).await;
    let calls = std::mem::take(&mut *recorder.lock().expect("recorder lock"));
    Outcome {
        op,
        calls,
        result,
        updates,
    }
}

async fn run(
    services: &Services,
    op: Op,
    inputs: &Inputs,
    updates: &mut Inputs,
) -> Result<Vec<Line>, String> {
    match op {
        Op::ChainInfo => chain_info(services).await,
        Op::NewAddress => {
            let address = new_address(services).await?;
            updates.tracked_address = Some(address.clone());
            Ok(vec![
                Line::Field("address".into(), address),
                Line::Note("Fresh regtest address from Bitcoin Core's wallet.".into()),
                Line::Note("It is now the tracked address.".into()),
            ])
        }
        Op::Send { sats } => {
            // Preflight: never broadcast what the wallet cannot pay for.
            let wallet = balances(services).await?;
            if wallet.trusted < sats {
                return Err(insufficient(sats, None, &wallet));
            }
            let address = match &inputs.tracked_address {
                Some(address) => address.clone(),
                None => {
                    let address = new_address(services).await?;
                    updates.tracked_address = Some(address.clone());
                    address
                }
            };
            // Funds a PSBT without broadcasting or locking coins: Core's own
            // coin selection and fee for exactly this payment.
            let funded: Value = rpc(
                services,
                "walletcreatefundedpsbt",
                vec![json!([]), send_outputs(&address, sats)],
            )
            .await?;
            let fee = to_sats(&funded["fee"]).ok_or("walletcreatefundedpsbt returned no fee")?;
            if sats + fee > wallet.trusted {
                return Err(insufficient(sats, Some(fee), &wallet));
            }

            let txid: String = rpc(
                services,
                "sendtoaddress",
                vec![json!(address), send_amount(sats)],
            )
            .await?;
            updates.tracked_tx = Some(txid.clone());
            Ok(vec![
                Line::Field("to".into(), address),
                Line::Field("amount".into(), format!("{} BTC", btc(sats))),
                Line::Field("estimated fee".into(), format!("{} BTC", btc(fee))),
                Line::Field(
                    "spendable before".into(),
                    format!("{} BTC", btc(wallet.trusted)),
                ),
                Line::Field("txid".into(), txid),
                Line::Good("Broadcast to Bitcoin Core's mempool.".into()),
                Line::Note("Watch the tracked transaction: rawtx → UNCONFIRMED.".into()),
            ])
        }
        Op::Mine { blocks } => {
            let address = mining_address(services, inputs, updates).await?;
            let hashes: Vec<String> = rpc(
                services,
                "generatetoaddress",
                vec![json!(blocks), json!(address)],
            )
            .await?;
            mined(services, hashes.len(), hashes.last()).await
        }
        Op::MineEmpty { blocks } => {
            let address = mining_address(services, inputs, updates).await?;
            let mut last = None;
            for _ in 0..blocks {
                let block: Value =
                    rpc(services, "generateblock", vec![json!(address), json!([])]).await?;
                last = block["hash"].as_str().map(str::to_string);
            }
            let mut lines = mined(services, blocks as usize, last.as_ref()).await?;
            lines.push(Line::Note(
                "Blocks contain only a coinbase: mempool transactions stay unconfirmed.".into(),
            ));
            Ok(lines)
        }
        Op::Inspect => inspect(services, inputs).await,
        Op::Utxos => utxos(services, inputs).await,
        Op::PrepareInvalidate => {
            let info: Value = rpc(services, "getblockchaininfo", vec![]).await?;
            let target = BlockRef {
                height: info["blocks"].as_u64().unwrap_or_default(),
                hash: info["bestblockhash"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            };
            updates.invalidate_target = Some(target.clone());
            Ok(vec![
                Line::Field("tip".into(), format!("#{}", target.height)),
                Line::Field("hash".into(), target.hash),
                Line::Warn("Waiting for confirmation (y / N).".into()),
            ])
        }
        Op::Invalidate => {
            let target = inputs
                .invalidate_target
                .clone()
                .ok_or("No block selected for invalidation")?;
            let _: Value = rpc(services, "invalidateblock", vec![json!(target.hash)]).await?;
            // Informational only: the invalidation already happened, so a
            // failure here must not make the step look failed (and retried).
            let tip = rpc::<u64>(services, "getblockcount", vec![])
                .await
                .map_or_else(|_| "unknown".to_string(), |tip| tip.to_string());
            Ok(vec![
                Line::Good(format!(
                    "Bitcoin Core invalidated block #{}.",
                    target.height
                )),
                Line::Field("Core tip now".into(), format!("#{tip}")),
                Line::Note("Its transactions return to Core's mempool.".into()),
                Line::Note("ZMQ sends nothing for a disconnect, so the indexer".into()),
                Line::Note("notices at the next block: status shows the mismatch.".into()),
            ])
        }
    }
}

// ------------------------------------------------------------
// OPERATIONS
// ------------------------------------------------------------

async fn chain_info(services: &Services) -> Result<Vec<Line>, String> {
    let info: Value = rpc(services, "getblockchaininfo", vec![]).await?;
    let indexer = api(status(State(services.state.clone()))).await?;

    let synced = match indexer["synced"].as_bool() {
        Some(true) => Line::Good("Indexer is synchronized with Bitcoin Core.".into()),
        Some(false) => Line::Warn(format!(
            "Indexer is {} block(s) behind or on another branch.",
            indexer["blocks_behind"]
        )),
        None => Line::Warn("Sync state unknown.".into()),
    };

    Ok(vec![
        Line::Field("network".into(), text(&info["chain"])),
        Line::Field("Core blocks".into(), text(&info["blocks"])),
        Line::Field("Core headers".into(), text(&info["headers"])),
        Line::Field("best block".into(), text(&info["bestblockhash"])),
        Line::Field("indexed height".into(), text(&indexer["indexed_height"])),
        synced,
    ])
}

async fn inspect(services: &Services, inputs: &Inputs) -> Result<Vec<Line>, String> {
    let txid = inputs
        .tracked_tx
        .clone()
        .ok_or("No tracked transaction yet: send one first.")?;
    let state = || State(services.state.clone());

    let status = match get_transaction_status(state(), Path(txid.clone())).await {
        Ok(Json(status)) => to_value(status)?,
        Err(StatusCode::NOT_FOUND) => {
            return Ok(vec![
                Line::Field("txid".into(), txid),
                Line::Warn("Not indexed yet (neither confirmed nor in the mempool).".into()),
            ]);
        }
        Err(code) => return Err(format!("status lookup failed: HTTP {code}")),
    };

    let mut lines = vec![Line::Field("txid".into(), txid.clone())];

    if status["status"] == "confirmed" {
        let detail = api(get_transaction(state(), Path(txid.clone()))).await?;
        lines.push(Line::Good(format!(
            "CONFIRMED in block #{} · {} confirmation(s)",
            status["block_height"], status["confirmations"]
        )));
        lines.push(Line::Field(
            "inputs → outputs".into(),
            format!(
                "{} → {}",
                array_len(&detail["inputs"]),
                array_len(&detail["outputs"])
            ),
        ));
    } else {
        let detail = api(get_mempool_transaction(state(), Path(txid.clone()))).await?;
        let tx = &detail["transaction"];
        lines.push(Line::Warn("UNCONFIRMED (in the mempool)".into()));
        lines.push(Line::Field(
            "fee".into(),
            format!(
                "{} sats · {:.1} sat/vB",
                tx["fee"],
                tx["fee_rate"].as_f64().unwrap_or_default()
            ),
        ));
    }

    if let Some(address) = &inputs.tracked_address {
        // Address history and balance are confirmed-only by design.
        if let Ok(history) = api(get_address_transactions(state(), Path(address.clone()))).await
            && let Some(row) = history
                .as_array()
                .and_then(|rows| rows.iter().find(|row| row["txid"] == txid.as_str()))
        {
            lines.push(Line::Field(
                "address history".into(),
                format!(
                    "{} · net {} sats",
                    text(&row["direction"]),
                    row["net"].as_i64().unwrap_or_default()
                ),
            ));
        }
        if let Ok(summary) = api(get_address(state(), Path(address.clone()))).await {
            lines.push(Line::Field(
                "address balance".into(),
                format!(
                    "{} BTC",
                    btc(summary["balance"].as_u64().unwrap_or_default())
                ),
            ));
        }
    }

    Ok(lines)
}

async fn utxos(services: &Services, inputs: &Inputs) -> Result<Vec<Line>, String> {
    let summary = api(get_utxo_summary(State(services.state.clone()))).await?;
    let totals = |key: &str| {
        let t = &summary[key];
        format!(
            "{} · {} BTC",
            t["count"],
            btc(t["value"].as_u64().unwrap_or_default())
        )
    };

    let mut lines = vec![
        Line::Field("UTXOs".into(), totals("utxos")),
        Line::Field("spendable now".into(), totals("spendable")),
        Line::Field("immature coinbase".into(), totals("immature_coinbase")),
        Line::Field("spent".into(), totals("spent")),
        Line::Field(
            "provably unspendable".into(),
            totals("provably_unspendable"),
        ),
    ];

    if let Some(address) = &inputs.tracked_address
        && let Ok(summary) = api(get_address(
            State(services.state.clone()),
            Path(address.clone()),
        ))
        .await
    {
        lines.push(Line::Field(
            "tracked address".into(),
            format!(
                "{} BTC confirmed",
                btc(summary["balance"].as_u64().unwrap_or_default())
            ),
        ));
    }

    lines.push(Line::Note(
        "OP_RETURN outputs are never UTXOs; coinbase matures at 100 conf.".into(),
    ));
    Ok(lines)
}

async fn new_address(services: &Services) -> Result<String, String> {
    rpc(services, "getnewaddress", vec![]).await
}

/// The wallet address mined blocks pay to: reused once created.
async fn mining_address(
    services: &Services,
    inputs: &Inputs,
    updates: &mut Inputs,
) -> Result<String, String> {
    if let Some(address) = &inputs.mining_address {
        return Ok(address.clone());
    }
    let address = new_address(services).await?;
    updates.mining_address = Some(address.clone());
    Ok(address)
}

async fn mined(
    services: &Services,
    count: usize,
    last: Option<&String>,
) -> Result<Vec<Line>, String> {
    // Informational only: the blocks are mined, so a failure here must not
    // make the step look failed (a retry would mine again).
    let tip = rpc::<u64>(services, "getblockcount", vec![])
        .await
        .map_or_else(|_| "unknown".to_string(), |tip| tip.to_string());
    let mut lines = vec![
        Line::Good(format!("Mined {count} block(s); Core tip is #{tip}.")),
        Line::Note("rawblock over ZMQ → the indexer syncs it.".into()),
    ];
    if let Some(hash) = last {
        lines.insert(1, Line::Field("tip hash".into(), hash.clone()));
    }
    Ok(lines)
}

// ------------------------------------------------------------
// HELPERS
// ------------------------------------------------------------

/// A Bitcoin Core RPC on a blocking thread.
async fn rpc<T: DeserializeOwned + Send + 'static>(
    services: &Services,
    method: &'static str,
    params: Vec<Value>,
) -> Result<T, String> {
    let rpc = services.rpc.clone();
    let sent = params.clone();
    let result = tokio::task::spawn_blocking(move || rpc.call::<Value>(method, &sent))
        .await
        .map_err(|error| error.to_string())
        .and_then(|result| result.map_err(|error| describe(&error)));

    if let Some(recorder) = &services.recorder {
        recorder.lock().expect("recorder lock").push(RpcCall {
            method: method.to_string(),
            params,
            result: result.clone(),
        });
    }

    let value = result.map_err(|error| format!("{method}: {error}"))?;
    serde_json::from_value(value).map_err(|error| format!("{method}: {error}"))
}

/// Bitcoin Core's error as `RPC error <code>: <message>`, the form
/// `bitcoin-cli` prints; other failures (transport, parsing) in full.
fn describe(error: &anyhow::Error) -> String {
    use bitcoincore_rpc::{Error, jsonrpc::error::Error as JsonRpcError};

    match error.downcast_ref::<Error>() {
        Some(Error::JsonRpc(JsonRpcError::Rpc(rpc))) => {
            format!("RPC error {}: {}", rpc.code, rpc.message)
        }
        _ => format!("{error:#}"),
    }
}

/// The wallet's balances in sats, from `getbalances` (`mine`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Balances {
    /// Confirmed or own-change: what the wallet can spend now.
    pub trusted: u64,
    pub untrusted_pending: u64,
    /// Coinbase rewards with fewer than 100 confirmations.
    pub immature: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wallet {
    pub name: String,
    pub balances: Balances,
}

async fn balances(services: &Services) -> Result<Balances, String> {
    let value: Value = rpc(services, "getbalances", vec![]).await?;
    parse_balances(&value).ok_or_else(|| "getbalances: unexpected response".to_string())
}

pub fn parse_balances(value: &Value) -> Option<Balances> {
    let mine = &value["mine"];
    Some(Balances {
        trusted: to_sats(&mine["trusted"])?,
        untrusted_pending: to_sats(&mine["untrusted_pending"])?,
        immature: to_sats(&mine["immature"])?,
    })
}

/// The wallet the RPC client is talking to, for the dashboard's wallet
/// panel (refreshed in the background; not recorded as an action).
pub async fn wallet(services: Services) -> Result<Wallet, String> {
    let info: Value = rpc(&services, "getwalletinfo", vec![]).await?;
    Ok(Wallet {
        name: info["walletname"].as_str().unwrap_or("?").to_string(),
        balances: balances(&services).await?,
    })
}

/// A BTC amount from an RPC response, in sats.
fn to_sats(value: &Value) -> Option<u64> {
    Amount::from_btc(value.as_f64()?).ok().map(Amount::to_sat)
}

/// The preflight failure: nothing was broadcast.
pub fn insufficient(sats: u64, fee: Option<u64>, wallet: &Balances) -> String {
    let needed = match fee {
        Some(fee) => format!("{} BTC + {} BTC estimated fee", btc(sats), btc(fee)),
        None => format!("{} BTC plus fee", btc(sats)),
    };
    format!(
        "Insufficient spendable funds: sending needs {needed}, but only {} BTC is spendable \
         ({} BTC immature, {} BTC unconfirmed). Nothing was broadcast.",
        btc(wallet.trusted),
        btc(wallet.immature),
        btc(wallet.untrusted_pending),
    )
}

/// An API handler's response as JSON, exactly as the HTTP API would send it.
pub async fn api<T: Serialize>(
    response: impl Future<Output = Result<Json<T>, StatusCode>>,
) -> Result<Value, String> {
    match response.await {
        Ok(Json(body)) => to_value(body),
        Err(code) => Err(format!("indexer API returned HTTP {code}")),
    }
}

fn to_value(body: impl Serialize) -> Result<Value, String> {
    serde_json::to_value(body).map_err(|error| error.to_string())
}

pub fn blocks_query(limit: u32) -> Query<BlocksQuery> {
    Query(serde_json::from_value(json!({ "limit": limit })).expect("valid blocks query"))
}

/// Latest blocks via `GET /api/blocks`.
pub async fn latest_blocks(state: AppState, limit: u32) -> Result<Value, String> {
    api(get_blocks(State(state), blocks_query(limit))).await
}

/// Unconfirmed transactions via `GET /api/mempool`.
pub async fn mempool(state: AppState) -> Result<Value, String> {
    api(get_mempool(State(state))).await
}

/// `GET /api/status`.
pub async fn indexer_status(state: AppState) -> Result<Value, String> {
    api(status(State(state))).await
}

/// What Bitcoin Core and the indexer each hold at the height of a block we
/// invalidated. Every field is read, not inferred.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReorgCheck {
    pub old_hash: String,
    /// Core's canonical block at that height now (`None`: no block there yet).
    pub core_hash: Option<String>,
    /// The indexer's block at that height (`GET /api/blocks/{height}`).
    pub indexer_hash: Option<String>,
    /// The indexer still stores the invalidated block. Errors count as
    /// "still stored" so a rollback is never reported without proof.
    pub old_indexed: bool,
}

pub async fn reorg_check(services: Services, old: BlockRef) -> ReorgCheck {
    let state = || State(services.state.clone());

    let core_hash = rpc::<String>(&services, "getblockhash", vec![json!(old.height)])
        .await
        .ok();
    let indexer_hash = api(get_block(state(), Path(old.height as i64)))
        .await
        .ok()
        .and_then(|block| block["hash"].as_str().map(str::to_string));
    let old_indexed = !matches!(
        get_block_by_hash(state(), Path(old.hash.clone())).await,
        Err(StatusCode::NOT_FOUND)
    );

    ReorgCheck {
        old_hash: old.hash,
        core_hash,
        indexer_hash,
        old_indexed,
    }
}

/// `GET /api/transactions/{txid}/status`; `Ok(None)` when not indexed yet.
pub async fn tx_status(state: AppState, txid: String) -> Result<Option<Value>, String> {
    match get_transaction_status(State(state), Path(txid)).await {
        Ok(Json(body)) => to_value(body).map(Some),
        Err(StatusCode::NOT_FOUND) => Ok(None),
        Err(code) => Err(format!("indexer API returned HTTP {code}")),
    }
}

pub fn btc(sats: u64) -> String {
    format!("{:.8}", Amount::from_sat(sats).to_btc())
}

fn plural(n: u64) -> &'static str {
    if n == 1 { "" } else { "s" }
}

fn text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => "—".into(),
        other => other.to_string(),
    }
}

fn array_len(value: &Value) -> usize {
    value.as_array().map_or(0, Vec::len)
}
