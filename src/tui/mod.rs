//! Demo Day dashboard: live indexer state on the left, Bitcoin Core actions
//! on the right, while the API and ZMQ indexing keep running underneath.
//!
//! The event loop never blocks: operations and data refreshes run as tasks
//! (RPCs on blocking threads) and report back over channels.

mod actions;
mod app;
mod command;
mod view;

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{Event, EventStream, KeyEventKind};
use futures_util::StreamExt;
use ratatui::DefaultTerminal;
use tokio::{
    sync::mpsc::{UnboundedReceiver, unbounded_channel},
    task::JoinHandle,
};

use crate::{
    api::state::AppState,
    rpc::bitcoin::BitcoinRpc,
    tui::{
        actions::{BlockRef, Services},
        app::{App, ServicesState, Snapshot},
    },
    ui::{Level, LogLine},
};

const REFRESH_EVERY: Duration = Duration::from_secs(1);
const FRAME_EVERY: Duration = Duration::from_millis(150);
const LATEST_BLOCKS: u32 = 5;

pub struct Context {
    pub state: AppState,
    pub rpc: Arc<BitcoinRpc>,
    pub logs: UnboundedReceiver<LogLine>,
}

/// Runs the dashboard until the user quits. The terminal is restored on
/// every exit path; ratatui's panic hook restores it on a panic too.
pub async fn run(context: Context, services: JoinHandle<Result<()>>) -> Result<()> {
    let mut terminal = ratatui::try_init()?;
    let outcome = event_loop(&mut terminal, context, services).await;
    ratatui::restore();
    outcome
}

async fn event_loop(
    terminal: &mut DefaultTerminal,
    context: Context,
    mut services: JoinHandle<Result<()>>,
) -> Result<()> {
    let Context {
        state,
        rpc,
        mut logs,
    } = context;
    let ops = Services {
        state: state.clone(),
        rpc,
        recorder: None,
    };

    let mut app = App::default();
    let mut events = EventStream::new();
    let (outcome_sender, mut outcomes) = unbounded_channel();
    let (snapshot_sender, mut snapshots) = unbounded_channel();
    let mut frames = tokio::time::interval(FRAME_EVERY);

    let mut refresh_due = Instant::now();
    let mut refreshing = false;
    let mut services_running = true;

    loop {
        terminal.draw(|frame| view::render(frame, &app))?;
        if app.quit {
            return Ok(());
        }

        if !refreshing && Instant::now() >= refresh_due {
            refreshing = true;
            refresh_due = Instant::now() + REFRESH_EVERY;
            let (ops, tracked, watched, sender) = (
                ops.clone(),
                app.inputs.tracked_tx.clone(),
                app.reorg.as_ref().map(|watch| watch.old.clone()),
                snapshot_sender.clone(),
            );
            tokio::spawn(async move {
                let _ = sender.send(refresh(ops, tracked, watched).await);
            });
        }

        tokio::select! {
            event = events.next() => match event {
                Some(Ok(Event::Key(key))) if key.kind == KeyEventKind::Press => {
                    if let Some(op) = app.on_key(key) {
                        let (ops, inputs, sender) =
                            (ops.clone(), app.inputs.clone(), outcome_sender.clone());
                        tokio::spawn(async move {
                            let _ = sender.send(actions::execute(ops, op, inputs).await);
                        });
                    }
                }
                // Resize and other events: redraw on the next iteration.
                Some(Ok(_)) => {}
                Some(Err(error)) => return Err(error.into()),
                None => return Ok(()),
            },

            Some(line) = logs.recv() => {
                app.push_log(line);
                while let Ok(line) = logs.try_recv() {
                    app.push_log(line);
                }
            }

            Some(outcome) = outcomes.recv() => {
                app.on_outcome(outcome);
                // Show the effect of the action without waiting a full second.
                refresh_due = Instant::now();
            }

            Some(snapshot) = snapshots.recv() => {
                refreshing = false;
                app.apply_snapshot(snapshot);
            }

            joined = &mut services, if services_running => {
                services_running = false;
                let reason = match joined {
                    Ok(Ok(())) => "services exited".to_string(),
                    Ok(Err(error)) => format!("{error:#}"),
                    Err(error) => error.to_string(),
                };
                app.push_log(LogLine { level: Level::Error, text: format!("Indexer stopped: {reason}") });
                app.services = ServicesState::Stopped(reason);
            }

            _ = frames.tick() => app.tick += 1,

            // SIGINT from outside (in raw mode, Ctrl+C arrives as a key).
            _ = tokio::signal::ctrl_c() => app.quit = true,
        }
    }
}

/// One read of the indexer through its API handlers, plus the live reorg
/// checks while a reorg is being watched.
async fn refresh(ops: Services, tracked: Option<String>, watched: Option<BlockRef>) -> Snapshot {
    let state = ops.state.clone();

    let tracked_status = async {
        match &tracked {
            Some(txid) => Some(actions::tx_status(state.clone(), txid.clone()).await),
            None => None,
        }
    };
    let reorg = async {
        match watched {
            Some(old) => Some(actions::reorg_check(ops.clone(), old).await),
            None => None,
        }
    };

    let (status, blocks, mempool, tracked_status, reorg, wallet) = tokio::join!(
        actions::indexer_status(state.clone()),
        actions::latest_blocks(state.clone(), LATEST_BLOCKS),
        actions::mempool(state.clone()),
        tracked_status,
        reorg,
        actions::wallet(ops.clone()),
    );

    let list = |value: Result<serde_json::Value, String>| {
        value
            .ok()
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
    };

    Snapshot {
        error: status.as_ref().err().cloned(),
        status: status.ok(),
        blocks: list(blocks),
        mempool: list(mempool),
        tracked: tracked_status.and_then(Result::ok),
        tracked_txid: tracked,
        reorg,
        wallet: Some(wallet),
    }
}
