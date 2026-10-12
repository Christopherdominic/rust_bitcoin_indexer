//! Dashboard state and key handling. No I/O: operations are returned to the
//! event loop to run, and their outcomes are applied back here.

use std::collections::VecDeque;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::Value;

use crate::{
    tui::{
        actions::{BlockRef, DEFAULT_SEND_SATS, Inputs, Op, Outcome},
        command::{Plan, RpcCall, plan},
    },
    ui::{Level, LogLine},
};

pub const LOG_CAPACITY: usize = 300;

/// A send of the configured amount: `App::resolve` fills in the sats.
const SEND: Op = Op::Send { sats: 0 };

/// Below this, Bitcoin Core rejects the payment output as dust.
const MIN_SEND_SATS: u64 = 546;

pub enum Entry {
    Run(Op),
    Guide(GuideKind),
}

pub struct MenuEntry {
    pub label: &'static str,
    /// Full description, shown in the About panel when selected.
    pub detail: &'static str,
    pub entry: Entry,
}

pub const MENU: &[MenuEntry] = &[
    MenuEntry {
        label: "Blockchain info",
        detail: "Asks Bitcoin Core for its chain tip (getblockchaininfo) and compares it with the indexer's tip from /api/status: are they synchronized?",
        entry: Entry::Run(Op::ChainInfo),
    },
    MenuEntry {
        label: "New address",
        detail: "Asks Bitcoin Core's wallet for a fresh regtest address (getnewaddress). It becomes the tracked address that payments are sent to.",
        entry: Entry::Run(Op::NewAddress),
    },
    MenuEntry {
        label: "Send BTC",
        detail: "Sends the send amount (0.1 BTC by default; press a to change it) to the tracked address. A preflight checks the spendable balance and Core's fee estimate first. Watch the tracked transaction go CREATED → IN MEMPOOL.",
        entry: Entry::Run(SEND),
    },
    MenuEntry {
        label: "Mine 1 block",
        detail: "Mines one block (generatetoaddress). ZMQ rawblock tells the indexer, which indexes it; mempool transactions become CONFIRMED.",
        entry: Entry::Run(Op::Mine { blocks: 1 }),
    },
    MenuEntry {
        label: "Inspect transaction",
        detail: "Shows the indexer's view of the tracked transaction: confirmation status, fee or outputs, and its row in the address's history.",
        entry: Entry::Run(Op::Inspect),
    },
    MenuEntry {
        label: "UTXO overview",
        detail: "The indexer's UTXO set: spendable now, immature coinbase (under 100 confirmations), spent, and provably unspendable OP_RETURN outputs.",
        entry: Entry::Run(Op::Utxos),
    },
    MenuEntry {
        label: "Invalidate tip block",
        detail: "Destructive: makes Bitcoin Core disconnect its tip block (invalidateblock). Asks y/N first and starts the live reorg report.",
        entry: Entry::Run(Op::PrepareInvalidate),
    },
    MenuEntry {
        label: "Mine 2 competing blocks",
        detail: "Mines two blocks without mempool transactions (generateblock). After an invalidation this is a longer branch: the indexer must detect the reorg and roll back.",
        entry: Entry::Run(Op::MineEmpty { blocks: 2 }),
    },
    MenuEntry {
        label: "▶ Guided: transaction lifecycle",
        detail: "Step by step: create an address, send 1 BTC, watch it enter the mempool, mine a block, watch it confirm, inspect the result.",
        entry: Entry::Guide(GuideKind::Lifecycle),
    },
    MenuEntry {
        label: "▶ Guided: reorg recovery",
        detail: "Step by step: confirm a payment, orphan its block, mine a longer branch. The indexer rolls back, restores UTXOs and re-indexes.",
        entry: Entry::Guide(GuideKind::Reorg),
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuideKind {
    Lifecycle,
    Reorg,
}

pub struct Step {
    pub title: &'static str,
    pub explain: &'static str,
    pub op: Op,
}

impl GuideKind {
    pub fn title(self) -> &'static str {
        match self {
            GuideKind::Lifecycle => "Transaction lifecycle",
            GuideKind::Reorg => "Reorg recovery",
        }
    }

    pub fn steps(self) -> &'static [Step] {
        match self {
            GuideKind::Lifecycle => &[
                Step {
                    title: "Create a fresh address",
                    explain: "Bitcoin Core's wallet generates a new regtest address.",
                    op: Op::NewAddress,
                },
                Step {
                    title: "Send BTC to it",
                    explain: "The tx enters Core's mempool; rawtx over ZMQ → the indexer shows it UNCONFIRMED.",
                    op: SEND,
                },
                Step {
                    title: "Mine 1 block",
                    explain: "rawblock over ZMQ → the indexer indexes the block; the tx becomes CONFIRMED.",
                    op: Op::Mine { blocks: 1 },
                },
                Step {
                    title: "Inspect the result",
                    explain: "Confirmation status and the address's history, from the indexer's API.",
                    op: Op::Inspect,
                },
            ],
            GuideKind::Reorg => &[
                Step {
                    title: "Send the payment",
                    explain: "A payment that will end up in the block we orphan.",
                    op: SEND,
                },
                Step {
                    title: "Mine 1 block",
                    explain: "The payment is confirmed in the new tip block.",
                    op: Op::Mine { blocks: 1 },
                },
                Step {
                    title: "Invalidate that block",
                    explain: "Core disconnects its tip (asks y/N first). The payment returns to Core's mempool.",
                    op: Op::PrepareInvalidate,
                },
                Step {
                    title: "Mine 2 competing blocks",
                    explain: "A longer branch without the payment: the indexer detects the reorg, rolls back, \
                              restores spent outputs; the payment is UNCONFIRMED again.",
                    op: Op::MineEmpty { blocks: 2 },
                },
                Step {
                    title: "Mine 1 block",
                    explain: "The payment is confirmed again, on the new branch.",
                    op: Op::Mine { blocks: 1 },
                },
            ],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepState {
    Pending,
    Done,
    Failed,
}

#[derive(Clone)]
pub struct Guide {
    pub kind: GuideKind,
    /// The step the cursor is on; `steps().len()` once all are done.
    pub step: usize,
    pub states: Vec<StepState>,
    /// Each finished attempt's result, shown again when navigating back.
    pub records: Vec<Option<LastResult>>,
    /// The step whose operation is running.
    pub running: Option<usize>,
}

impl Guide {
    pub fn new(kind: GuideKind) -> Self {
        let steps = kind.steps().len();
        Self {
            kind,
            step: 0,
            states: vec![StepState::Pending; steps],
            records: vec![None; steps],
            running: None,
        }
    }

    pub fn finished(&self) -> bool {
        self.step >= self.kind.steps().len()
    }

    pub fn current(&self) -> Option<&'static Step> {
        self.kind.steps().get(self.step)
    }

    pub fn state(&self) -> Option<&StepState> {
        self.states.get(self.step)
    }
}

/// The send-amount editor.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AmountEdit {
    pub text: String,
    pub error: Option<String>,
}

/// Where the tracked transaction is, as the indexer reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Broadcast by Bitcoin Core; the indexer hasn't reported it yet.
    Created,
    Mempool,
    Confirmed {
        height: i64,
        confirmations: i64,
    },
}

/// Tracks a reorg from the moment we invalidate a block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReorgWatch {
    pub old: BlockRef,
    /// The indexer's own rollback report, once it logs one.
    pub rollback: Option<crate::db::reorg::Rollback>,
}

/// The indexer's data, refreshed every second from its API handlers.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub status: Option<Value>,
    pub blocks: Vec<Value>,
    pub mempool: Vec<Value>,
    /// Status of the tracked transaction: `Some(None)` = not indexed yet.
    pub tracked: Option<Option<Value>>,
    /// The txid `tracked` was fetched for.
    pub tracked_txid: Option<String>,
    /// Live checks for the watched reorg.
    pub reorg: Option<crate::tui::actions::ReorgCheck>,
    /// The RPC client's wallet: name and balances, or why they're unknown.
    pub wallet: Option<Result<crate::tui::actions::Wallet, String>>,
    pub error: Option<String>,
}

pub enum ServicesState {
    Running,
    Stopped(String),
}

#[derive(Clone)]
pub struct LastResult {
    pub op: Op,
    pub calls: Vec<RpcCall>,
    pub result: Result<Vec<crate::tui::actions::Line>, String>,
}

pub struct TimedLog {
    pub at: String,
    pub line: LogLine,
}

pub struct App {
    pub selected: usize,
    pub guide: Option<Guide>,
    /// Waiting for y/N before invalidating this block.
    pub confirm: Option<BlockRef>,
    pub busy: Option<(Op, Instant)>,
    pub last: Option<LastResult>,
    pub inputs: Inputs,
    pub snapshot: Snapshot,
    pub logs: VecDeque<TimedLog>,
    pub services: ServicesState,
    pub quit: bool,
    pub tick: u64,
    pub reorg: Option<ReorgWatch>,
    /// The result panel previews the selected action (or next guide step)
    /// instead of showing the last result.
    pub preview: bool,
    /// What a send pays, in sats.
    pub amount_sats: u64,
    pub amount_edit: Option<AmountEdit>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            selected: 0,
            guide: None,
            confirm: None,
            busy: None,
            last: None,
            inputs: Inputs::default(),
            snapshot: Snapshot::default(),
            logs: VecDeque::new(),
            services: ServicesState::Running,
            quit: false,
            tick: 0,
            reorg: None,
            preview: true,
            amount_sats: DEFAULT_SEND_SATS,
            amount_edit: None,
        }
    }
}

impl App {
    /// Handles a key press; returns an operation for the event loop to run.
    pub fn on_key(&mut self, key: KeyEvent) -> Option<Op> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return None;
        }

        // The amount editor captures every key until saved or cancelled.
        if self.amount_edit.is_some() {
            self.edit_amount(key.code);
            return None;
        }

        // A pending confirmation captures every key: only 'y' proceeds.
        if let Some(target) = self.confirm.take() {
            if matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y')) {
                // In the reorg guide this completes the invalidation step.
                let step = self.guide.as_ref().map(|guide| guide.step);
                let op = self.start(Op::Invalidate);
                if let (Some(guide), Some(_)) = (&mut self.guide, op) {
                    guide.running = step;
                }
                return op;
            }
            self.log(
                Level::Warn,
                format!("Invalidation of block #{} cancelled", target.height),
            );
            return None;
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') => self.quit = true,
            KeyCode::Char('a') | KeyCode::Char('A') => {
                self.amount_edit = Some(AmountEdit {
                    text: crate::tui::command::cli_arg(&crate::tui::command::send_amount(
                        self.amount_sats,
                    )),
                    error: None,
                });
            }
            KeyCode::Esc => {
                if self.guide.take().is_some() {
                    self.preview = true;
                    self.log(Level::Info, "Left the guided demo");
                }
            }
            _ if self.guide.is_some() => return self.guide_key(key.code),
            KeyCode::Up | KeyCode::Char('k') => {
                self.select((self.selected + MENU.len() - 1) % MENU.len());
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.select((self.selected + 1) % MENU.len());
            }
            KeyCode::Char(c @ '1'..='9') => {
                let index = c as usize - '1' as usize;
                if index < MENU.len() {
                    self.select(index);
                }
            }
            KeyCode::Char('0') if MENU.len() >= 10 => self.select(9),
            KeyCode::Enter => return self.activate(),
            _ => {}
        }
        None
    }

    /// Enter runs the current step, R retries a failed one, B steps back,
    /// S restarts. A completed step is never run again from here, so no
    /// transaction is rebroadcast by navigating.
    fn guide_key(&mut self, code: KeyCode) -> Option<Op> {
        let guide = self.guide.as_ref()?;
        let state = guide.state().cloned();
        let busy = self.busy.is_some();

        match code {
            KeyCode::Enter => match state {
                None => {
                    self.guide = None;
                    self.preview = true;
                    None
                }
                Some(StepState::Pending) => self.run_step(),
                Some(StepState::Failed) => {
                    self.log(Level::Warn, "This step failed: press R to retry it");
                    None
                }
                Some(StepState::Done) => {
                    // Revisited via B: move on without re-running it.
                    let guide = self.guide.as_mut()?;
                    guide.step += 1;
                    self.show_step();
                    None
                }
            },
            KeyCode::Char('r') | KeyCode::Char('R') => {
                if state == Some(StepState::Failed) {
                    self.run_step()
                } else {
                    self.log(Level::Info, "Nothing to retry: no failed step here");
                    None
                }
            }
            KeyCode::Char('b') | KeyCode::Char('B') => {
                if busy {
                    self.log(Level::Warn, "Wait for the running step to finish");
                } else if guide.step == 0 {
                    self.log(Level::Info, "Already at the first step");
                } else {
                    let guide = self.guide.as_mut()?;
                    guide.step -= 1;
                    self.show_step();
                }
                None
            }
            KeyCode::Char('s') | KeyCode::Char('S') => {
                if busy {
                    self.log(Level::Warn, "Wait for the running step to finish");
                } else {
                    let kind = guide.kind;
                    self.guide = Some(Guide::new(kind));
                    self.preview = true;
                    self.log(
                        Level::Info,
                        format!(
                            "Restarted: {} · nothing runs until Enter; earlier txids are kept",
                            kind.title()
                        ),
                    );
                }
                None
            }
            _ => None,
        }
    }

    /// Starts the guide's current step.
    fn run_step(&mut self) -> Option<Op> {
        let step = self.guide.as_ref()?.current()?;
        let op = self.start(self.resolve(step.op))?;
        let guide = self.guide.as_mut()?;
        guide.running = Some(guide.step);
        Some(op)
    }

    /// Shows the cursor step's last result if it has one, else its preview.
    fn show_step(&mut self) {
        let record = self
            .guide
            .as_ref()
            .and_then(|guide| guide.records.get(guide.step).cloned().flatten());
        self.preview = record.is_none();
        if record.is_some() {
            self.last = record;
        }
    }

    fn edit_amount(&mut self, code: KeyCode) {
        let Some(edit) = &mut self.amount_edit else {
            return;
        };
        match code {
            KeyCode::Esc => self.amount_edit = None,
            KeyCode::Backspace => {
                edit.text.pop();
                edit.error = None;
            }
            KeyCode::Char(c) if c.is_ascii_digit() || c == '.' => {
                if edit.text.len() < 16 {
                    edit.text.push(c);
                }
                edit.error = None;
            }
            KeyCode::Enter => match parse_amount(&edit.text) {
                Ok(sats) => {
                    self.amount_sats = sats;
                    self.amount_edit = None;
                    self.log(
                        Level::Info,
                        format!("Send amount set to {} BTC", crate::tui::actions::btc(sats)),
                    );
                }
                Err(error) => edit.error = Some(error),
            },
            _ => {}
        }
    }

    /// Fills the configured amount into a send.
    pub fn resolve(&self, op: Op) -> Op {
        match op {
            Op::Send { .. } => Op::Send {
                sats: self.amount_sats,
            },
            other => other,
        }
    }

    fn select(&mut self, index: usize) {
        self.selected = index;
        self.preview = true;
    }

    /// What the result panel previews: the next guide step, or the selected
    /// action (a guide entry previews its first step).
    pub fn preview(&self) -> Option<(String, Plan)> {
        let core_height = self
            .snapshot
            .status
            .as_ref()
            .and_then(|status| status["core_height"].as_i64());
        let op = match &self.guide {
            Some(guide) => guide.current()?.op,
            None => match MENU[self.selected].entry {
                Entry::Run(op) => op,
                Entry::Guide(kind) => kind.steps()[0].op,
            },
        };
        let op = self.resolve(op);
        Some((op.label(), plan(op, &self.inputs, core_height)))
    }

    fn activate(&mut self) -> Option<Op> {
        match MENU[self.selected].entry {
            Entry::Run(op) => self.start(self.resolve(op)),
            Entry::Guide(kind) => {
                self.guide = Some(Guide::new(kind));
                self.preview = true;
                self.log(Level::Info, format!("Guided demo: {}", kind.title()));
                None
            }
        }
    }

    /// Marks `op` as running, unless another operation still is.
    fn start(&mut self, op: Op) -> Option<Op> {
        if let Some((running, _)) = self.busy {
            self.log(Level::Warn, format!("Still running: {}", running.label()));
            return None;
        }
        self.busy = Some((op, Instant::now()));
        Some(op)
    }

    pub fn on_outcome(&mut self, outcome: Outcome) {
        self.busy = None;
        self.preview = false;

        let Outcome {
            op,
            calls,
            result,
            updates,
        } = outcome;

        if updates.tracked_tx.is_some() {
            self.inputs.tracked_tx = updates.tracked_tx;
            self.snapshot.tracked = None;
        }
        if updates.tracked_address.is_some() {
            self.inputs.tracked_address = updates.tracked_address;
        }
        if updates.mining_address.is_some() {
            self.inputs.mining_address = updates.mining_address;
        }
        if updates.invalidate_target.is_some() {
            self.inputs.invalidate_target = updates.invalidate_target;
        }

        match &result {
            Ok(_) => {
                if op == Op::Invalidate
                    && let Some(old) = self.inputs.invalidate_target.clone()
                {
                    self.reorg = Some(ReorgWatch {
                        old,
                        rollback: None,
                    });
                }
                if matches!(op, Op::Send { .. }) {
                    self.log(
                        Level::Info,
                        "Tracked tx → CREATED: broadcast by Bitcoin Core",
                    );
                }
                if op == Op::PrepareInvalidate {
                    self.confirm = self.inputs.invalidate_target.clone();
                } else {
                    self.log(Level::Success, format!("{} ✔", op.label()));
                }
            }
            Err(error) => self.log(Level::Error, format!("{} failed: {error}", op.label())),
        }

        let last = LastResult { op, calls, result };
        self.record_step(&last);
        self.last = Some(last);
    }

    /// Settles the guide step that was running: done (cursor moves on) or
    /// failed (cursor stays; R retries). The invalidation preview only
    /// settles a failure; its success waits for the confirmed invalidation.
    fn record_step(&mut self, last: &LastResult) {
        let Some(guide) = &mut self.guide else {
            return;
        };
        let Some(index) = guide.running.take() else {
            return;
        };
        let state = match (&last.result, last.op) {
            (Err(_), _) => StepState::Failed,
            (Ok(_), Op::PrepareInvalidate) => return,
            (Ok(_), _) => StepState::Done,
        };
        guide.records[index] = Some(last.clone());
        if state == StepState::Done && guide.step == index {
            guide.step += 1;
        }
        guide.states[index] = state;

        if guide.finished() {
            let kind = guide.kind;
            self.log(
                Level::Success,
                format!("Guided demo complete: {}", kind.title()),
            );
        }
    }

    /// Where the tracked transaction is, per the latest snapshot.
    pub fn stage(&self) -> Option<Stage> {
        self.inputs.tracked_tx.as_ref()?;
        Some(match &self.snapshot.tracked {
            Some(Some(status)) if status["status"] == "confirmed" => Stage::Confirmed {
                height: status["block_height"].as_i64().unwrap_or_default(),
                confirmations: status["confirmations"].as_i64().unwrap_or_default(),
            },
            Some(Some(_)) => Stage::Mempool,
            _ => Stage::Created,
        })
    }

    /// Installs a fresh snapshot and narrates stage changes of the tracked
    /// transaction in the event log.
    pub fn apply_snapshot(&mut self, mut snapshot: Snapshot) {
        // Ignore a tracked status fetched for a previous transaction.
        if snapshot.tracked_txid != self.inputs.tracked_tx {
            snapshot.tracked = None;
        }
        // Ignore a reorg check made for a previous invalidation.
        if snapshot.reorg.as_ref().map(|c| &c.old_hash) != self.reorg.as_ref().map(|w| &w.old.hash)
        {
            snapshot.reorg = None;
        }

        let before = self.stage();
        let had_status = self.snapshot.tracked.is_some();
        self.snapshot = snapshot;
        let after = self.stage();

        // Only narrate real changes, not the first reading after a send.
        if !had_status || before == after {
            return;
        }
        match (before, after) {
            (_, Some(Stage::Mempool)) if matches!(before, Some(Stage::Confirmed { .. })) => self
                .log(
                    Level::Warn,
                    "Tracked tx → back IN MEMPOOL: its block was orphaned (reorg)",
                ),
            (_, Some(Stage::Mempool)) => self.log(
                Level::Success,
                "Tracked tx → IN MEMPOOL: the indexer saw it via ZMQ rawtx",
            ),
            (_, Some(Stage::Confirmed { height, .. })) if !matches!(before, Some(Stage::Confirmed { height: h, .. }) if h == height) => {
                self.log(
                    Level::Success,
                    format!("Tracked tx → CONFIRMED in block #{height}"),
                )
            }
            _ => {}
        }
    }

    pub fn push_log(&mut self, line: LogLine) {
        if let Some(watch) = &mut self.reorg
            && let Some(rollback) = crate::indexer::reorg::parse_rollback_message(&line.text)
        {
            watch.rollback = Some(rollback);
        }

        if self.logs.len() == LOG_CAPACITY {
            self.logs.pop_front();
        }
        self.logs.push_back(TimedLog { at: clock(), line });
    }

    fn log(&mut self, level: Level, text: impl Into<String>) {
        self.push_log(LogLine {
            level,
            text: text.into(),
        });
    }
}

/// A BTC amount typed by the user, in sats.
pub fn parse_amount(text: &str) -> Result<u64, String> {
    let amount = bitcoin::Amount::from_str_in(text.trim(), bitcoin::Denomination::Bitcoin)
        .map_err(|_| format!("'{text}' is not a BTC amount (up to 8 decimals)"))?;
    if amount.to_sat() < MIN_SEND_SATS {
        return Err(format!(
            "At least {MIN_SEND_SATS} sats: smaller outputs are dust"
        ));
    }
    if amount > bitcoin::Amount::MAX_MONEY {
        return Err("More than 21 million BTC".into());
    }
    Ok(amount.to_sat())
}

/// Wall-clock time of day (UTC) for log lines.
fn clock() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
        % 86_400;
    format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyEventKind, KeyEventState};

    use super::*;

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn ok(op: Op) -> Outcome {
        Outcome {
            calls: Vec::new(),
            op,
            result: Ok(vec![]),
            updates: Inputs::default(),
        }
    }

    fn select(app: &mut App, label: &str) {
        app.selected = MENU.iter().position(|m| m.label.contains(label)).unwrap();
    }

    #[test]
    fn enter_runs_the_selected_action_and_blocks_a_second_one() {
        let mut app = App::default();
        select(&mut app, "Mine 1 block");

        assert_eq!(
            app.on_key(press(KeyCode::Enter)),
            Some(Op::Mine { blocks: 1 })
        );
        // Busy: a second Enter starts nothing.
        assert_eq!(app.on_key(press(KeyCode::Enter)), None);

        app.on_outcome(ok(Op::Mine { blocks: 1 }));
        assert!(app.busy.is_none());
        assert_eq!(
            app.on_key(press(KeyCode::Enter)),
            Some(Op::Mine { blocks: 1 })
        );
    }

    #[test]
    fn invalidation_needs_an_explicit_yes() {
        let mut app = App::default();
        select(&mut app, "Invalidate");
        assert_eq!(
            app.on_key(press(KeyCode::Enter)),
            Some(Op::PrepareInvalidate)
        );

        let target = BlockRef {
            height: 7,
            hash: "abc".into(),
        };
        app.on_outcome(Outcome {
            calls: Vec::new(),
            op: Op::PrepareInvalidate,
            result: Ok(vec![]),
            updates: Inputs {
                invalidate_target: Some(target.clone()),
                ..Inputs::default()
            },
        });
        assert_eq!(app.confirm, Some(target.clone()));

        // Anything but 'y' cancels, including Enter.
        assert_eq!(app.on_key(press(KeyCode::Enter)), None);
        assert_eq!(app.confirm, None);

        app.confirm = Some(target);
        assert_eq!(app.on_key(press(KeyCode::Char('y'))), Some(Op::Invalidate));
    }

    fn failed(op: Op, error: &str) -> Outcome {
        Outcome {
            result: Err(error.into()),
            ..ok(op)
        }
    }

    /// A lifecycle guide whose first step (new address) is done.
    fn lifecycle_at_send() -> App {
        let mut app = App::default();
        select(&mut app, "transaction lifecycle");
        app.on_key(press(KeyCode::Enter));
        assert_eq!(app.on_key(press(KeyCode::Enter)), Some(Op::NewAddress));
        app.on_outcome(ok(Op::NewAddress));
        assert_eq!(app.guide.as_ref().unwrap().step, 1);
        app
    }

    fn step(app: &App) -> usize {
        app.guide.as_ref().unwrap().step
    }

    #[test]
    fn guided_lifecycle_steps_through_in_order() {
        let mut app = App::default();
        select(&mut app, "transaction lifecycle");
        assert_eq!(
            app.on_key(press(KeyCode::Enter)),
            None,
            "opening the guide runs nothing"
        );

        for guide_step in GuideKind::Lifecycle.steps() {
            let op = app.resolve(guide_step.op);
            assert_eq!(app.on_key(press(KeyCode::Enter)), Some(op));
            app.on_outcome(ok(op));
        }
        assert!(app.guide.as_ref().unwrap().finished());

        // Enter on a finished guide closes it.
        assert_eq!(app.on_key(press(KeyCode::Enter)), None);
        assert!(app.guide.is_none());
    }

    #[test]
    fn guided_send_uses_the_default_point_one_btc() {
        let mut app = lifecycle_at_send();
        assert_eq!(
            app.on_key(press(KeyCode::Enter)),
            Some(Op::Send { sats: 10_000_000 })
        );
    }

    #[test]
    fn failed_step_does_not_advance_and_enter_does_not_rerun_it() {
        let mut app = lifecycle_at_send();
        let op = app.on_key(press(KeyCode::Enter)).unwrap();
        app.on_outcome(failed(
            op,
            "sendtoaddress: RPC error -6: Insufficient funds",
        ));

        let guide = app.guide.as_ref().unwrap();
        assert_eq!(guide.step, 1, "a failed step must not advance");
        assert_eq!(guide.states[1], StepState::Failed);
        assert!(app.last.as_ref().unwrap().result.is_err());

        // Enter on a failed step only explains; nothing is re-sent.
        assert_eq!(app.on_key(press(KeyCode::Enter)), None);
        assert!(app.busy.is_none());
    }

    #[test]
    fn r_retries_a_failed_step_with_the_edited_amount() {
        let mut app = lifecycle_at_send();
        let op = app.on_key(press(KeyCode::Enter)).unwrap();
        app.on_outcome(failed(op, "Insufficient spendable funds"));

        // Lower the amount, then retry.
        app.on_key(press(KeyCode::Char('a')));
        for _ in 0..8 {
            app.on_key(press(KeyCode::Backspace));
        }
        for c in "0.05".chars() {
            app.on_key(press(KeyCode::Char(c)));
        }
        app.on_key(press(KeyCode::Enter));
        assert_eq!(app.amount_sats, 5_000_000);

        let retry = app.on_key(press(KeyCode::Char('r')));
        assert_eq!(retry, Some(Op::Send { sats: 5_000_000 }));
        app.on_outcome(ok(retry.unwrap()));
        assert_eq!(step(&app), 2);
        assert_eq!(app.guide.as_ref().unwrap().states[1], StepState::Done);
    }

    #[test]
    fn r_does_nothing_unless_the_step_failed() {
        let mut app = lifecycle_at_send();
        assert_eq!(app.on_key(press(KeyCode::Char('r'))), None);
        assert!(app.busy.is_none());
    }

    #[test]
    fn controls_work_while_an_error_is_displayed() {
        let mut app = lifecycle_at_send();
        let op = app.on_key(press(KeyCode::Enter)).unwrap();
        app.on_outcome(failed(op, "boom"));

        app.on_key(press(KeyCode::Char('b')));
        assert_eq!(step(&app), 0);
        app.on_key(press(KeyCode::Char('s')));
        assert_eq!(step(&app), 0);
        app.on_key(press(KeyCode::Esc));
        assert!(app.guide.is_none());
        app.on_key(press(KeyCode::Char('q')));
        assert!(app.quit);
    }

    #[test]
    fn back_never_rebroadcasts_a_completed_send() {
        let mut app = lifecycle_at_send();
        let op = app.on_key(press(KeyCode::Enter)).unwrap();
        app.on_outcome(Outcome {
            updates: Inputs {
                tracked_tx: Some("txid-1".into()),
                ..Inputs::default()
            },
            ..ok(op)
        });
        assert_eq!(step(&app), 2);

        // Back onto the completed send: its result is shown again...
        app.on_key(press(KeyCode::Char('b')));
        assert_eq!(step(&app), 1);
        assert!(!app.preview);
        assert!(matches!(app.last.as_ref().unwrap().op, Op::Send { .. }));

        // ...and Enter moves on instead of sending again.
        assert_eq!(app.on_key(press(KeyCode::Enter)), None);
        assert_eq!(step(&app), 2);
        assert!(app.busy.is_none());
        assert_eq!(app.inputs.tracked_tx.as_deref(), Some("txid-1"));
    }

    #[test]
    fn back_at_the_first_step_and_while_busy_does_nothing() {
        let mut app = App::default();
        select(&mut app, "transaction lifecycle");
        app.on_key(press(KeyCode::Enter));
        app.on_key(press(KeyCode::Char('b')));
        assert_eq!(step(&app), 0);

        let mut app = lifecycle_at_send();
        app.on_key(press(KeyCode::Enter)); // send running
        app.on_key(press(KeyCode::Char('b')));
        app.on_key(press(KeyCode::Char('s')));
        assert_eq!(step(&app), 1, "no navigation while a step runs");
    }

    #[test]
    fn restart_resets_steps_but_keeps_txids_and_runs_nothing() {
        let mut app = lifecycle_at_send();
        app.inputs.tracked_tx = Some("txid-1".into());
        app.on_key(press(KeyCode::Char('s')));

        let guide = app.guide.as_ref().unwrap();
        assert_eq!(guide.step, 0);
        assert!(guide.states.iter().all(|s| *s == StepState::Pending));
        assert!(app.busy.is_none());
        assert_eq!(app.inputs.tracked_tx.as_deref(), Some("txid-1"));
    }

    #[test]
    fn exiting_the_guide_mid_step_leaves_the_outcome_unattributed() {
        let mut app = lifecycle_at_send();
        let op = app.on_key(press(KeyCode::Enter)).unwrap();
        app.on_key(press(KeyCode::Esc));
        assert!(app.guide.is_none());

        app.on_outcome(ok(op));
        assert!(app.guide.is_none());
        assert!(app.busy.is_none());
        // The menu works again.
        select(&mut app, "Mine 1 block");
        assert_eq!(
            app.on_key(press(KeyCode::Enter)),
            Some(Op::Mine { blocks: 1 })
        );
    }

    #[test]
    fn duplicate_enter_while_a_step_runs_starts_nothing() {
        let mut app = lifecycle_at_send();
        assert!(app.on_key(press(KeyCode::Enter)).is_some());
        assert_eq!(app.on_key(press(KeyCode::Enter)), None);
        assert_eq!(app.on_key(press(KeyCode::Char('r'))), None);
    }

    #[test]
    fn reorg_guide_only_advances_past_invalidation_once_confirmed() {
        let mut app = App::default();
        let mut guide = Guide::new(GuideKind::Reorg);
        guide.step = 2;
        guide.states[0] = StepState::Done;
        guide.states[1] = StepState::Done;
        app.guide = Some(guide);

        assert_eq!(
            app.on_key(press(KeyCode::Enter)),
            Some(Op::PrepareInvalidate)
        );
        app.on_outcome(Outcome {
            updates: Inputs {
                invalidate_target: Some(BlockRef {
                    height: 9,
                    hash: "h".into(),
                }),
                ..Inputs::default()
            },
            ..ok(Op::PrepareInvalidate)
        });
        assert_eq!(step(&app), 2, "not advanced before 'y'");

        assert_eq!(app.on_key(press(KeyCode::Char('y'))), Some(Op::Invalidate));
        app.on_outcome(ok(Op::Invalidate));
        assert_eq!(step(&app), 3);
    }

    #[test]
    fn amount_editor_validates_input() {
        assert_eq!(parse_amount("0.1"), Ok(10_000_000));
        assert_eq!(parse_amount("1"), Ok(100_000_000));
        assert!(parse_amount("0.000001").is_err(), "dust");
        assert!(parse_amount("0.123456789").is_err(), "9 decimals");
        assert!(parse_amount("").is_err());

        let mut app = App::default();
        app.on_key(press(KeyCode::Char('a')));
        assert_eq!(app.amount_edit.as_ref().unwrap().text, "0.1");
        app.on_key(press(KeyCode::Char('.'))); // "0.1." is invalid
        app.on_key(press(KeyCode::Enter));
        assert!(app.amount_edit.as_ref().unwrap().error.is_some());
        app.on_key(press(KeyCode::Esc));
        assert!(app.amount_edit.is_none());
        assert_eq!(app.amount_sats, DEFAULT_SEND_SATS);
    }

    #[test]
    fn q_and_ctrl_c_quit() {
        let mut app = App::default();
        app.on_key(press(KeyCode::Char('q')));
        assert!(app.quit);

        let mut app = App::default();
        app.on_key(KeyEvent {
            modifiers: KeyModifiers::CONTROL,
            ..press(KeyCode::Char('c'))
        });
        assert!(app.quit);
    }

    #[test]
    fn updates_from_outcomes_are_remembered() {
        let mut app = App::default();
        app.on_outcome(Outcome {
            calls: Vec::new(),
            op: Op::Send { sats: 1 },
            result: Ok(vec![]),
            updates: Inputs {
                tracked_tx: Some("t".into()),
                tracked_address: Some("a".into()),
                ..Inputs::default()
            },
        });
        // A later outcome without updates keeps them.
        app.on_outcome(ok(Op::Mine { blocks: 1 }));

        assert_eq!(app.inputs.tracked_tx.as_deref(), Some("t"));
        assert_eq!(app.inputs.tracked_address.as_deref(), Some("a"));
    }

    fn snapshot_for(txid: &str, status: Option<Value>) -> Snapshot {
        Snapshot {
            tracked: Some(status),
            tracked_txid: Some(txid.into()),
            ..Snapshot::default()
        }
    }

    fn last_log(app: &App) -> String {
        app.logs.back().unwrap().line.text.clone()
    }

    #[test]
    fn stage_changes_are_narrated_in_the_log() {
        use serde_json::json;

        let mut app = App::default();
        app.inputs.tracked_tx = Some("t".into());

        app.apply_snapshot(snapshot_for("t", None));
        assert_eq!(app.stage(), Some(Stage::Created));

        app.apply_snapshot(snapshot_for("t", Some(json!({ "status": "unconfirmed" }))));
        assert_eq!(app.stage(), Some(Stage::Mempool));
        assert!(last_log(&app).contains("IN MEMPOOL"), "{}", last_log(&app));

        let confirmed = json!({ "status": "confirmed", "block_height": 345, "confirmations": 1 });
        app.apply_snapshot(snapshot_for("t", Some(confirmed.clone())));
        assert!(last_log(&app).contains("CONFIRMED in block #345"));

        // More confirmations in the same block: no new log line.
        let logged = app.logs.len();
        let deeper = json!({ "status": "confirmed", "block_height": 345, "confirmations": 2 });
        app.apply_snapshot(snapshot_for("t", Some(deeper)));
        assert_eq!(app.logs.len(), logged);

        // Reorged out: back to the mempool.
        app.apply_snapshot(snapshot_for("t", Some(json!({ "status": "unconfirmed" }))));
        assert!(last_log(&app).contains("back IN MEMPOOL"));
    }

    #[test]
    fn status_for_another_transaction_is_ignored() {
        let mut app = App::default();
        app.inputs.tracked_tx = Some("new".into());
        app.apply_snapshot(snapshot_for(
            "old",
            Some(serde_json::json!({ "status": "confirmed" })),
        ));
        assert_eq!(app.stage(), Some(Stage::Created));
    }

    #[test]
    fn confirmed_invalidation_starts_a_reorg_watch_that_records_the_rollback() {
        let target = BlockRef {
            height: 357,
            hash: "abc".into(),
        };
        let mut app = App::default();
        app.inputs.invalidate_target = Some(target.clone());

        app.on_outcome(ok(Op::Invalidate));
        assert_eq!(
            app.reorg,
            Some(ReorgWatch {
                old: target,
                rollback: None,
            })
        );

        let rollback = crate::db::reorg::Rollback {
            blocks_removed: 1,
            outputs_restored: 2,
        };
        app.push_log(LogLine {
            level: Level::Success,
            text: crate::indexer::reorg::rollback_message(&rollback),
        });
        assert_eq!(app.reorg.unwrap().rollback, Some(rollback));
    }
}
