//! `bitcoin-cli` equivalents of the dashboard's JSON-RPC calls.
//!
//! The dashboard never runs a shell: it calls Bitcoin Core over JSON-RPC
//! through `BitcoinRpc`. These commands only show what each call would look
//! like on the command line, both before an action runs (with placeholders
//! for values not known yet) and after (with the exact parameters sent).

use serde_json::Value;

use crate::tui::actions::{Inputs, Op};

pub const CLI: &str = "bitcoin-cli -regtest";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Arg {
    Value(String),
    /// A value only known once an earlier call has returned.
    Placeholder(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub method: String,
    pub args: Vec<Arg>,
}

impl Command {
    fn new(method: &str, args: Vec<Arg>) -> Self {
        Self {
            method: method.to_string(),
            args,
        }
    }

    /// The whole command on one line.
    pub fn text(&self) -> String {
        let mut text = format!("{CLI} {}", self.method);
        for arg in &self.args {
            text.push(' ');
            text.push_str(arg.text());
        }
        text
    }
}

impl Arg {
    pub fn text(&self) -> &str {
        match self {
            Arg::Value(text) | Arg::Placeholder(text) => text,
        }
    }
}

/// One JSON-RPC call made while running an action.
#[derive(Debug, Clone, PartialEq)]
pub struct RpcCall {
    pub method: String,
    pub params: Vec<Value>,
    pub result: Result<Value, String>,
}

impl RpcCall {
    pub fn command(&self) -> Command {
        Command::new(
            &self.method,
            self.params.iter().map(|p| Arg::Value(cli_arg(p))).collect(),
        )
    }
}

/// What an action will do: the Core calls in order, then indexer endpoints.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    pub commands: Vec<Command>,
    pub endpoints: Vec<String>,
    /// Shown above the commands, e.g. that confirmation comes first.
    pub note: Option<&'static str>,
}

/// The calls `actions::execute(op, inputs)` will make. `core_height` is the
/// latest known Core tip, used to label the invalidation placeholder.
pub fn plan(op: Op, inputs: &Inputs, core_height: Option<i64>) -> Plan {
    let value = |s: &str| Arg::Value(s.to_string());
    let hole = |s: &str| Arg::Placeholder(format!("<{s}>"));
    let cmd = Command::new;

    let mining = || match &inputs.mining_address {
        Some(address) => (None, value(address)),
        None => (
            Some(cmd("getnewaddress", vec![])),
            hole("new mining address"),
        ),
    };
    let tx = || {
        inputs
            .tracked_tx
            .as_deref()
            .map_or_else(|| hole("tracked txid"), value)
    };

    let mut plan = Plan::default();
    match op {
        Op::ChainInfo => {
            plan.commands.push(cmd("getblockchaininfo", vec![]));
            plan.endpoints.push("GET /api/status".into());
        }
        Op::NewAddress => plan.commands.push(cmd("getnewaddress", vec![])),
        Op::Send { sats } => {
            plan.note = Some("Preflight: stops before sendtoaddress if amount + fee > spendable.");
            plan.commands.push(cmd("getbalances", vec![]));
            let (to, outputs) = match &inputs.tracked_address {
                Some(address) => (
                    value(address),
                    value(&cli_arg(&send_outputs(address, sats))),
                ),
                None => {
                    plan.commands.push(cmd("getnewaddress", vec![]));
                    let placeholder = "<new address>";
                    (
                        Arg::Placeholder(placeholder.into()),
                        Arg::Placeholder(cli_arg(&send_outputs(placeholder, sats))),
                    )
                }
            };
            plan.commands
                .push(cmd("walletcreatefundedpsbt", vec![value("'[]'"), outputs]));
            plan.commands.push(cmd(
                "sendtoaddress",
                vec![to, value(&cli_arg(&send_amount(sats)))],
            ));
            plan.endpoints
                .push("GET /api/transactions/<txid from sendtoaddress>/status".to_string());
        }
        Op::Mine { blocks } => {
            let (first, address) = mining();
            plan.commands.extend(first);
            plan.commands.push(cmd(
                "generatetoaddress",
                vec![value(&blocks.to_string()), address],
            ));
            plan.commands.push(cmd("getblockcount", vec![]));
            plan.endpoints.push("GET /api/status".into());
        }
        Op::MineEmpty { blocks } => {
            let (first, address) = mining();
            plan.commands.extend(first);
            for _ in 0..blocks {
                plan.commands
                    .push(cmd("generateblock", vec![address.clone(), value("'[]'")]));
            }
            plan.commands.push(cmd("getblockcount", vec![]));
            plan.endpoints.push("GET /api/status".into());
        }
        Op::Inspect => {
            let tx = tx();
            plan.endpoints
                .push(format!("GET /api/transactions/{}/status", tx.text()));
            plan.endpoints.push(format!(
                "GET /api/transactions/{0}  or  GET /api/mempool/{0}",
                tx.text()
            ));
            if let Some(address) = &inputs.tracked_address {
                plan.endpoints
                    .push(format!("GET /api/addresses/{address}/transactions"));
                plan.endpoints.push(format!("GET /api/addresses/{address}"));
            }
        }
        Op::Utxos => {
            plan.endpoints.push("GET /api/utxos/summary".into());
            if let Some(address) = &inputs.tracked_address {
                plan.endpoints.push(format!("GET /api/addresses/{address}"));
            }
        }
        Op::PrepareInvalidate => {
            let tip = core_height.map_or_else(
                || "Core tip hash".to_string(),
                |h| format!("hash of Core tip #{h}"),
            );
            plan.note = Some("Destructive: invalidateblock only runs after you press y.");
            plan.commands.push(cmd("getblockchaininfo", vec![]));
            plan.commands.push(cmd("invalidateblock", vec![hole(&tip)]));
            plan.commands.push(cmd("getblockcount", vec![]));
        }
        Op::Invalidate => {
            let hash = inputs
                .invalidate_target
                .as_ref()
                .map_or_else(|| hole("block hash"), |t| value(&t.hash));
            plan.commands.push(cmd("invalidateblock", vec![hash]));
            plan.commands.push(cmd("getblockcount", vec![]));
        }
    }
    plan
}

/// The amount parameter of `sendtoaddress`: a JSON number in BTC. Shared by
/// the call and its preview so the two cannot differ.
pub fn send_amount(sats: u64) -> Value {
    serde_json::json!(bitcoin::Amount::from_sat(sats).to_btc())
}

/// The outputs parameter of `walletcreatefundedpsbt` for a payment.
pub fn send_outputs(address: &str, sats: u64) -> Value {
    serde_json::json!([{ address: send_amount(sats) }])
}

/// A JSON-RPC parameter as `bitcoin-cli` takes it on the command line:
/// strings and numbers bare, arrays and objects as quoted JSON.
pub fn cli_arg(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(_) | Value::Object(_) => format!("'{value}'"),
        other => other.to_string(),
    }
}

/// A short, readable form of an RPC result.
pub fn summarize(value: &Value) -> String {
    const KEYS: &[&str] = &["chain", "blocks", "headers", "bestblockhash", "hash"];
    match value {
        Value::Null => "null (success)".into(),
        Value::String(text) => text.clone(),
        Value::Object(map) => {
            let picked: Vec<String> = KEYS
                .iter()
                .filter_map(|key| map.get(*key).map(|v| format!("{key}: {}", cli_arg(v))))
                .collect();
            if picked.is_empty() {
                value.to_string()
            } else if picked.len() < map.len() {
                format!("{{ {}, … }}", picked.join(", "))
            } else {
                format!("{{ {} }}", picked.join(", "))
            }
        }
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::tui::actions::BlockRef;

    fn texts(plan: &Plan) -> Vec<String> {
        plan.commands.iter().map(Command::text).collect()
    }

    #[test]
    fn send_without_an_address_previews_its_creation() {
        let plan = plan(Op::Send { sats: 100_000_000 }, &Inputs::default(), None);
        assert_eq!(
            texts(&plan),
            [
                "bitcoin-cli -regtest getbalances",
                "bitcoin-cli -regtest getnewaddress",
                r#"bitcoin-cli -regtest walletcreatefundedpsbt '[]' '[{"<new address>":1.0}]'"#,
                "bitcoin-cli -regtest sendtoaddress <new address> 1.0",
            ]
        );
    }

    #[test]
    fn known_values_replace_placeholders() {
        let inputs = Inputs {
            tracked_address: Some("bcrt1qxyz".into()),
            mining_address: Some("bcrt1qminer".into()),
            invalidate_target: Some(BlockRef {
                height: 7,
                hash: "00ab".into(),
            }),
            ..Inputs::default()
        };
        assert_eq!(
            texts(&plan(Op::Send { sats: 50_000_000 }, &inputs, None)),
            [
                "bitcoin-cli -regtest getbalances",
                r#"bitcoin-cli -regtest walletcreatefundedpsbt '[]' '[{"bcrt1qxyz":0.5}]'"#,
                "bitcoin-cli -regtest sendtoaddress bcrt1qxyz 0.5",
            ]
        );
        assert_eq!(
            texts(&plan(Op::Mine { blocks: 1 }, &inputs, None))[0],
            "bitcoin-cli -regtest generatetoaddress 1 bcrt1qminer"
        );
        assert_eq!(
            texts(&plan(Op::MineEmpty { blocks: 2 }, &inputs, None))[..2],
            [
                "bitcoin-cli -regtest generateblock bcrt1qminer '[]'",
                "bitcoin-cli -regtest generateblock bcrt1qminer '[]'",
            ]
        );
        assert_eq!(
            texts(&plan(Op::Invalidate, &inputs, None))[0],
            "bitcoin-cli -regtest invalidateblock 00ab"
        );
    }

    #[test]
    fn invalidation_preview_names_the_tip_and_the_confirmation() {
        let plan = plan(Op::PrepareInvalidate, &Inputs::default(), Some(357));
        assert!(plan.note.unwrap().contains("press y"));
        assert_eq!(
            texts(&plan)[1],
            "bitcoin-cli -regtest invalidateblock <hash of Core tip #357>"
        );
    }

    #[test]
    fn indexer_only_actions_have_no_core_commands() {
        let plan = plan(Op::Inspect, &Inputs::default(), None);
        assert!(plan.commands.is_empty());
        assert_eq!(
            plan.endpoints[0],
            "GET /api/transactions/<tracked txid>/status"
        );
    }

    #[test]
    fn recorded_calls_render_as_cli_arguments() {
        let call = RpcCall {
            method: "generateblock".into(),
            params: vec![json!("bcrt1q"), json!([])],
            result: Ok(json!({ "hash": "00ff" })),
        };
        assert_eq!(
            call.command().text(),
            "bitcoin-cli -regtest generateblock bcrt1q '[]'"
        );
        assert_eq!(cli_arg(&json!(0.5)), "0.5");
        assert_eq!(summarize(&json!({ "hash": "00ff" })), "{ hash: 00ff }");
        assert_eq!(summarize(&Value::Null), "null (success)");
    }

    #[test]
    fn preview_and_real_call_share_send_parameters() {
        // actions::run sends exactly send_amount / send_outputs; the preview
        // must render the same values.
        let inputs = Inputs {
            tracked_address: Some("bcrt1qa".into()),
            ..Inputs::default()
        };
        let preview = plan(Op::Send { sats: 12_345_678 }, &inputs, None);
        let sent = RpcCall {
            method: "sendtoaddress".into(),
            params: vec![serde_json::json!("bcrt1qa"), send_amount(12_345_678)],
            result: Ok(Value::Null),
        };
        assert_eq!(
            preview.commands.last().unwrap().text(),
            sent.command().text()
        );
        assert_eq!(cli_arg(&send_amount(12_345_678)), "0.12345678");
    }
}
