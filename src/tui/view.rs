//! Rendering. Reads `App` only.

use std::time::{SystemTime, UNIX_EPOCH};

use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line as TextLine, Span},
    widgets::{Block, BorderType, Cell, Clear, List, ListItem, Paragraph, Row, Table, Wrap},
};
use serde_json::Value;

use crate::{
    tui::{
        actions::{Line, Op, ReorgCheck, btc},
        app::{AmountEdit, App, MENU, ReorgWatch, ServicesState, Stage, StepState},
        command::{Arg, CLI, Command, Plan, plan, summarize},
    },
    ui::Level,
};

const ORANGE: Color = Color::Rgb(247, 147, 26);
const GREEN: Color = Color::Rgb(95, 185, 127);
const RED: Color = Color::Rgb(226, 104, 90);
const YELLOW: Color = Color::Rgb(230, 190, 90);
const DIM: Color = Color::Rgb(125, 118, 110);
const TEXT: Color = Color::Rgb(235, 230, 223);

const SPINNER: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];

pub fn render(frame: &mut Frame, app: &App) {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(10),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    render_header(frame, header, app);

    let [left, right] =
        Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)]).areas(body);

    let [status, blocks, mempool, tracked, wallet, log] = Layout::vertical([
        Constraint::Length(4),
        Constraint::Length(8),
        Constraint::Length(5),
        Constraint::Length(5),
        Constraint::Length(4),
        Constraint::Min(5),
    ])
    .areas(left);
    render_status(frame, status, app);
    render_blocks(frame, blocks, app);
    render_mempool(frame, mempool, app);
    render_tracked(frame, tracked, app);
    render_wallet(frame, wallet, app);
    render_log(frame, log, app);

    // A running guide replaces the menu (which is inactive meanwhile);
    // otherwise the selected action's full description sits under the menu.
    let width = right.width.saturating_sub(2) as usize;
    let guide = app.guide.is_some().then(|| guide_lines(app, width));
    let (menu_height, context_height) = match &guide {
        Some(lines) => (0, lines.len() as u16 + 2),
        None => (MENU.len() as u16 + 2, 6),
    };
    let reorg_height = if app.reorg.is_some() { 10 } else { 0 };
    let [menu, context, reorg, result] = Layout::vertical([
        Constraint::Length(menu_height),
        Constraint::Length(context_height),
        Constraint::Length(reorg_height),
        Constraint::Min(5),
    ])
    .areas(right);
    match guide {
        Some(lines) => render_guide(frame, context, app, lines),
        None => {
            render_menu(frame, menu, app);
            render_about(frame, context, app);
        }
    }
    if let Some(watch) = &app.reorg {
        render_reorg(frame, reorg, app, watch);
    }
    render_result(frame, result, app);

    render_footer(frame, footer, app);

    if let Some(target) = &app.confirm {
        render_confirm(frame, app, target.height);
    }
    if let Some(edit) = &app.amount_edit {
        render_amount_edit(frame, app, edit);
    }
}

fn panel(title: impl std::fmt::Display) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(DIM))
        .title(Span::styled(
            format!(" {title} "),
            Style::new().fg(TEXT).bold(),
        ))
}

// ------------------------------------------------------------
// HEADER / FOOTER
// ------------------------------------------------------------

fn render_header(frame: &mut Frame, area: Rect, app: &App) {
    let status = app.snapshot.status.as_ref();
    let network = status
        .and_then(|s| s["network"].as_str())
        .map(str::to_uppercase);

    let mut spans = vec![
        Span::styled(
            " ₿ AMIABLE INDEXER ",
            Style::new().fg(Color::Black).bg(ORANGE).bold(),
        ),
        Span::styled("  Rust for Bitcoin · live dashboard", Style::new().fg(DIM)),
    ];
    if let Some(network) = network {
        spans.push(Span::raw("   "));
        spans.push(Span::styled(
            format!(" {network} "),
            Style::new().fg(ORANGE).bold(),
        ));
    }
    match &app.services {
        ServicesState::Running => {}
        ServicesState::Stopped(reason) => {
            spans.push(Span::raw("   "));
            spans.push(Span::styled(
                format!(" INDEXER STOPPED: {reason} "),
                Style::new().fg(Color::White).bg(RED).bold(),
            ));
        }
    }
    frame.render_widget(TextLine::from(spans), area);
}

fn render_footer(frame: &mut Frame, area: Rect, app: &App) {
    let keys: &[(&str, &str)] = if app.amount_edit.is_some() {
        &[("Enter", "save amount"), ("Esc", "cancel")]
    } else if app.confirm.is_some() {
        &[("y", "confirm"), ("any other key", "cancel")]
    } else if app.guide.is_some() {
        &[
            ("Enter", "run step"),
            ("R", "retry"),
            ("B", "back"),
            ("S", "restart"),
            ("a", "amount"),
            ("Esc", "exit guide"),
            ("q", "quit"),
        ]
    } else {
        &[
            ("↑↓/1-9", "select"),
            ("Enter", "run"),
            ("a", "send amount"),
            ("q / Ctrl+C", "quit"),
        ]
    };
    let mut spans = Vec::new();
    for (key, action) in keys {
        spans.push(Span::styled(
            format!(" {key} "),
            Style::new().fg(Color::Black).bg(DIM),
        ));
        spans.push(Span::styled(format!(" {action}   "), Style::new().fg(DIM)));
    }
    spans.push(Span::styled(
        "API: http://127.0.0.1:3000 · headless: cargo run -- --headless",
        Style::new().fg(DIM),
    ));
    frame.render_widget(TextLine::from(spans), area);
}

// ------------------------------------------------------------
// LEFT: INDEXER
// ------------------------------------------------------------

fn render_status(frame: &mut Frame, area: Rect, app: &App) {
    let block = panel("Indexer status");
    let Some(s) = &app.snapshot.status else {
        let text = app
            .snapshot
            .error
            .clone()
            .unwrap_or_else(|| "Waiting for the first status…".into());
        frame.render_widget(Paragraph::new(text).fg(DIM).block(block), area);
        return;
    };

    let sync = match s["synced"].as_bool() {
        Some(true) => Span::styled("● SYNCHRONIZED", Style::new().fg(GREEN).bold()),
        Some(false) => {
            let behind = s["blocks_behind"].as_i64().unwrap_or_default();
            if behind > 0 {
                Span::styled(format!("● {behind} BEHIND"), Style::new().fg(YELLOW).bold())
            } else {
                Span::styled(
                    "● TIP MISMATCH (reorg pending)",
                    Style::new().fg(YELLOW).bold(),
                )
            }
        }
        None => Span::styled("● UNKNOWN", Style::new().fg(DIM).bold()),
    };

    let line1 = TextLine::from(vec![
        label("Core "),
        number(&s["core_height"]),
        state(
            s["bitcoin_core"].as_str() == Some("connected"),
            "rpc ok",
            "rpc down",
        ),
        label("   Indexed "),
        number(&s["indexed_height"]),
        Span::raw("   "),
        sync,
    ]);
    let line2 = TextLine::from(vec![
        label("ZMQ blocks "),
        zmq(&s["zmq"]["blocks"]["status"]),
        label("  txs "),
        zmq(&s["zmq"]["transactions"]["status"]),
        label("   Mempool "),
        Span::styled(s["mempool_transactions"].to_string(), Style::new().fg(TEXT)),
        label("   UTXOs "),
        Span::styled(s["unspent_outputs"].to_string(), Style::new().fg(TEXT)),
    ]);

    frame.render_widget(Paragraph::new(vec![line1, line2]).block(block), area);
}

fn render_blocks(frame: &mut Frame, area: Rect, app: &App) {
    let now = now();
    let rows = app.snapshot.blocks.iter().map(|b| {
        Row::new(vec![
            Cell::from(format!("#{}", b["height"])).fg(ORANGE),
            Cell::from(short(b["hash"].as_str().unwrap_or_default(), 10, 8)).fg(TEXT),
            Cell::from(b["tx_count"].to_string()).fg(TEXT),
            Cell::from(age(now, b["timestamp"].as_i64())).fg(DIM),
        ])
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(8),
            Constraint::Min(20),
            Constraint::Length(4),
            Constraint::Length(9),
        ],
    )
    .header(Row::new(["Height", "Hash", "Tx", "Age"]).fg(DIM))
    .block(panel("Latest blocks"));
    frame.render_widget(table, area);
}

fn render_mempool(frame: &mut Frame, area: Rect, app: &App) {
    let block = panel(format!(
        "Mempool · {} unconfirmed",
        app.snapshot.mempool.len()
    ));
    if app.snapshot.mempool.is_empty() {
        frame.render_widget(
            Paragraph::new("Empty: no unconfirmed transactions.")
                .fg(DIM)
                .block(block),
            area,
        );
        return;
    }
    let rows = app.snapshot.mempool.iter().map(|tx| {
        Row::new(vec![
            Cell::from(short(tx["txid"].as_str().unwrap_or_default(), 10, 8)).fg(YELLOW),
            Cell::from(format!(
                "{:.1} sat/vB",
                tx["fee_rate"].as_f64().unwrap_or_default()
            ))
            .fg(TEXT),
            Cell::from(format!(
                "{} BTC",
                btc(tx["output_value"].as_u64().unwrap_or_default())
            ))
            .fg(DIM),
        ])
    });
    let table = Table::new(
        rows,
        [
            Constraint::Min(20),
            Constraint::Length(13),
            Constraint::Length(18),
        ],
    )
    .block(block);
    frame.render_widget(table, area);
}

fn render_tracked(frame: &mut Frame, area: Rect, app: &App) {
    let block = panel("Tracked transaction");
    let (Some(txid), Some(stage)) = (&app.inputs.tracked_tx, app.stage()) else {
        frame.render_widget(
            Paragraph::new("None yet: send BTC (or start a guided demo).")
                .fg(DIM)
                .block(block),
            area,
        );
        return;
    };

    let mut header = vec![
        label("txid "),
        Span::styled(short(txid, 14, 10), Style::new().fg(TEXT)),
    ];
    if let Some(address) = &app.inputs.tracked_address {
        header.push(label("  → "));
        header.push(Span::styled(short(address, 12, 8), Style::new().fg(DIM)));
    }

    let detail = match stage {
        Stage::Created => {
            "Broadcast by Bitcoin Core · waiting for the indexer (ZMQ rawtx)".to_string()
        }
        Stage::Mempool => {
            "In the mempool · indexed as unconfirmed · waiting for a block".to_string()
        }
        Stage::Confirmed {
            height,
            confirmations,
        } => format!(
            "In block #{height} · {confirmations} confirmation(s) · indexed via ZMQ rawblock"
        ),
    };

    let lines = vec![
        TextLine::from(header),
        stepper(stage),
        TextLine::styled(detail, Style::new().fg(DIM)),
    ];
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

/// CREATED ── IN MEMPOOL ── CONFIRMED, with the current stage as a badge.
fn stepper(stage: Stage) -> TextLine<'static> {
    let reached = match stage {
        Stage::Created => 0,
        Stage::Mempool => 1,
        Stage::Confirmed { .. } => 2,
    };
    let confirmed = match stage {
        Stage::Confirmed { height, .. } => format!("CONFIRMED #{height}"),
        _ => "CONFIRMED".to_string(),
    };
    let names = ["CREATED".to_string(), "IN MEMPOOL".to_string(), confirmed];
    let badge = [ORANGE, YELLOW, GREEN];

    let mut spans = Vec::new();
    for (i, name) in names.into_iter().enumerate() {
        if i > 0 {
            let color = if i <= reached { GREEN } else { DIM };
            spans.push(Span::styled(" ──── ", Style::new().fg(color)));
        }
        spans.push(if i == reached {
            Span::styled(
                format!(" {name} "),
                Style::new().fg(Color::Black).bg(badge[i]).bold(),
            )
        } else if i < reached {
            Span::styled(format!("✔ {name}"), Style::new().fg(GREEN))
        } else {
            Span::styled(format!("○ {name}"), Style::new().fg(DIM))
        });
    }
    TextLine::from(spans)
}

fn render_log(frame: &mut Frame, area: Rect, app: &App) {
    let visible = area.height.saturating_sub(2) as usize;
    let items: Vec<ListItem> = app
        .logs
        .iter()
        .skip(app.logs.len().saturating_sub(visible))
        .map(|entry| {
            let (marker, color) = match entry.line.level {
                Level::Success => ("✔", GREEN),
                Level::Warn => ("⚠", YELLOW),
                Level::Error => ("✖", RED),
                Level::Block => ("▸", ORANGE),
                Level::Info => ("•", DIM),
            };
            ListItem::new(TextLine::from(vec![
                Span::styled(format!("{} ", entry.at), Style::new().fg(DIM)),
                Span::styled(format!("{marker} "), Style::new().fg(color)),
                Span::styled(
                    entry.line.text.clone(),
                    Style::new().fg(if color == DIM { TEXT } else { color }),
                ),
            ]))
        })
        .collect();
    frame.render_widget(List::new(items).block(panel("Event log")), area);
}

// ------------------------------------------------------------
// RIGHT: ACTIONS
// ------------------------------------------------------------

fn render_menu(frame: &mut Frame, area: Rect, app: &App) {
    let guiding = app.guide.is_some();
    let items: Vec<ListItem> = MENU
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let selected = i == app.selected && !guiding;
            let key = if i < 9 {
                (i + 1).to_string()
            } else {
                "0".into()
            };
            let style = if selected {
                Style::new().fg(Color::Black).bg(ORANGE).bold()
            } else if guiding {
                Style::new().fg(DIM)
            } else {
                Style::new().fg(TEXT)
            };
            ListItem::new(TextLine::from(vec![
                Span::styled(format!(" {key} "), style.add_modifier(Modifier::DIM)),
                Span::styled(format!("{:<34}", entry.label), style),
            ]))
        })
        .collect();
    frame.render_widget(List::new(items).block(panel("Bitcoin Core actions")), area);
}

fn render_about(frame: &mut Frame, area: Rect, app: &App) {
    let entry = &MENU[app.selected];
    frame.render_widget(
        Paragraph::new(TextLine::styled(entry.detail, Style::new().fg(TEXT)))
            .wrap(Wrap { trim: true })
            .block(panel(format!(
                "About · {}",
                entry.label.trim_start_matches("▶ ")
            ))),
        area,
    );
}

/// The live reorg report. Every line is derived from a check made against
/// Bitcoin Core or the indexer; nothing is shown as complete until seen.
fn render_reorg(frame: &mut Frame, area: Rect, app: &App, watch: &ReorgWatch) {
    let lines = reorg_lines(
        watch,
        app.snapshot.reorg.as_ref(),
        app.snapshot.status.as_ref(),
    );
    frame.render_widget(Paragraph::new(lines).block(panel("Reorg report")), area);
}

fn reorg_lines(
    watch: &ReorgWatch,
    check: Option<&ReorgCheck>,
    status: Option<&Value>,
) -> Vec<TextLine<'static>> {
    let height = watch.old.height;
    let Some(check) = check else {
        return vec![TextLine::styled(" checking…", Style::new().fg(DIM))];
    };

    let rolled_back = !check.old_indexed;
    let new_hash = check
        .core_hash
        .clone()
        .filter(|hash| *hash != watch.old.hash);
    let reindexed = new_hash.is_some() && check.indexer_hash == new_hash;

    let complete = |text: String| Span::styled(text, Style::new().fg(GREEN).bold());
    let waiting = |text: &str| Span::styled(text.to_string(), Style::new().fg(DIM));
    let row = |name: &str, value: Span<'static>| {
        TextLine::from(vec![
            Span::styled(format!(" {name:<18}"), Style::new().fg(DIM)),
            value,
        ])
    };

    let header = if rolled_back {
        TextLine::from(Span::styled(
            " REORG DETECTED ",
            Style::new().fg(Color::White).bg(RED).bold(),
        ))
    } else {
        TextLine::from(vec![
            Span::styled(
                " REORG PENDING ",
                Style::new().fg(Color::Black).bg(YELLOW).bold(),
            ),
            Span::styled("  waiting for a competing block", Style::new().fg(DIM)),
        ])
    };

    let old_state = if check.core_hash.as_deref() == Some(watch.old.hash.as_str()) {
        Span::styled("STILL CANONICAL", Style::new().fg(YELLOW).bold())
    } else {
        Span::styled("ORPHANED", Style::new().fg(RED).bold())
    };
    let old_block = TextLine::from(vec![
        Span::styled(format!(" Old block #{height}:  "), Style::new().fg(DIM)),
        Span::styled(
            format!("{}  ", short(&watch.old.hash, 8, 6)),
            Style::new().fg(TEXT),
        ),
        old_state,
    ]);

    let new_block = match &new_hash {
        Some(hash) => TextLine::from(vec![
            Span::styled(format!(" New block #{height}:  "), Style::new().fg(DIM)),
            Span::styled(format!("{}  ", short(hash, 8, 6)), Style::new().fg(TEXT)),
            if reindexed {
                Span::styled("CANONICAL", Style::new().fg(GREEN).bold())
            } else {
                Span::styled(
                    "CANONICAL in Core · indexing…",
                    Style::new().fg(YELLOW).bold(),
                )
            },
        ]),
        None => TextLine::from(vec![
            Span::styled(format!(" New block #{height}:  "), Style::new().fg(DIM)),
            waiting("none yet: Core's tip is below this height"),
        ]),
    };

    let restoration = match (&watch.rollback, rolled_back) {
        (Some(rollback), _) => complete(format!(
            "Complete · {} output(s) unspent again",
            rollback.outputs_restored
        )),
        // Restoring happens in the same SQL transaction as the rollback.
        (None, true) => complete("Complete".into()),
        (None, false) => waiting("Waiting"),
    };

    let reindexing = if reindexed {
        complete("Complete".into())
    } else if rolled_back {
        Span::styled("In progress", Style::new().fg(YELLOW))
    } else {
        waiting("Waiting")
    };

    let lag = match status.map(|s| (s["synced"].as_bool(), s["blocks_behind"].as_i64())) {
        Some((Some(true), _)) => complete("0 blocks · synchronized".into()),
        Some((Some(false), Some(behind))) if behind > 0 => {
            Span::styled(format!("{behind} block(s)"), Style::new().fg(YELLOW).bold())
        }
        Some((Some(false), _)) => Span::styled(
            "0 blocks · tip hash differs from Core",
            Style::new().fg(YELLOW).bold(),
        ),
        _ => waiting("unknown"),
    };

    vec![
        header,
        old_block,
        new_block,
        TextLine::raw(""),
        row(
            "Rollback:",
            if rolled_back {
                complete("Complete".into())
            } else {
                waiting("Waiting: indexer still has the old block")
            },
        ),
        row("UTXO restoration:", restoration),
        row("Reindexing:", reindexing),
        row("Sync lag:", lag),
    ]
}

/// The guide's steps, what the next one does, and its bitcoin-cli
/// equivalent, pre-wrapped to `width` so the panel can be sized exactly.
fn guide_lines(app: &App, width: usize) -> Vec<TextLine<'static>> {
    let Some(guide) = &app.guide else {
        return Vec::new();
    };
    let steps = guide.kind.steps();

    let mut lines: Vec<TextLine> = steps
        .iter()
        .enumerate()
        .map(|(i, step)| {
            let here = i == guide.step;
            let running = guide.running == Some(i) && app.busy.is_some();
            let (marker, color) = match (&guide.states[i], running) {
                (_, true) => (SPINNER[app.tick as usize % SPINNER.len()], ORANGE),
                (StepState::Done, _) => ("✔", GREEN),
                (StepState::Failed, _) => ("✖", RED),
                (StepState::Pending, _) if here => ("▶", ORANGE),
                (StepState::Pending, _) => ("○", DIM),
            };
            let mut style = Style::new().fg(color);
            if here {
                style = style.bold();
            }
            let title = if matches!(step.op, Op::Send { .. }) {
                format!("{} ({} BTC)", step.title, btc_trim(app.amount_sats))
            } else {
                step.title.to_string()
            };
            TextLine::from(vec![
                Span::styled(
                    format!(" {} {marker} {}. ", if here { "›" } else { " " }, i + 1),
                    style,
                ),
                Span::styled(title, style),
            ])
        })
        .collect();

    lines.push(TextLine::raw(""));
    let (next, color) = match guide.state() {
        None => ("Done. Press Enter or Esc to return to the actions.", TEXT),
        Some(StepState::Done) => (
            "Already done: Enter moves on without running it again (no rebroadcast). S restarts the guide.",
            GREEN,
        ),
        Some(StepState::Failed) => (
            "This step failed and the guide has not moved on. Fix the cause (see the error below), then press R to retry.",
            RED,
        ),
        Some(StepState::Pending) => (guide.current().map_or("", |step| step.explain), TEXT),
    };
    for line in wrap_words(next, width.saturating_sub(1)) {
        lines.push(TextLine::styled(format!(" {line}"), Style::new().fg(color)));
    }

    if guide.state() != Some(&StepState::Done)
        && let Some((_, plan)) = app.preview()
    {
        lines.push(TextLine::raw(""));
        let label = if guide.state() == Some(&StepState::Failed) {
            " R retries with:"
        } else {
            " Next step runs:"
        };
        lines.push(TextLine::styled(label, Style::new().fg(DIM)));
        lines.extend(plan_lines(&plan, width));
    }
    lines
}

fn render_guide(frame: &mut Frame, area: Rect, app: &App, lines: Vec<TextLine<'static>>) {
    let Some(guide) = &app.guide else { return };
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(panel(format!("Guided demo · {}", guide.kind.title()))),
        area,
    );
}

fn render_result(frame: &mut Frame, area: Rect, app: &App) {
    let width = area.width.saturating_sub(2) as usize;
    let core_height = app
        .snapshot
        .status
        .as_ref()
        .and_then(|status| status["core_height"].as_i64());

    if let Some((op, since)) = app.busy {
        let spinner = SPINNER[app.tick as usize % SPINNER.len()];
        let mut text = vec![
            TextLine::styled(
                format!(
                    " {spinner} {}… {:.1}s",
                    op.label(),
                    since.elapsed().as_secs_f32()
                ),
                Style::new().fg(ORANGE).bold(),
            ),
            TextLine::raw(""),
        ];
        text.extend(plan_lines(&plan(op, &app.inputs, core_height), width));
        frame.render_widget(Paragraph::new(text).block(panel("Running")), area);
        return;
    }

    // Browsing the menu: preview what Enter would run.
    if app.preview
        && app.guide.is_none()
        && let Some((label, plan)) = app.preview()
    {
        let mut text = plan_lines(&plan, width);
        text.push(TextLine::raw(""));
        text.push(TextLine::from(vec![
            Span::styled(" Enter ", Style::new().fg(Color::Black).bg(DIM)),
            Span::styled(" to run", Style::new().fg(DIM)),
        ]));
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(DIM))
            .title(Span::styled(
                format!(" Preview · {label} "),
                Style::new().fg(ORANGE).bold(),
            ));
        frame.render_widget(Paragraph::new(text).block(block), area);
        return;
    }

    let Some(last) = &app.last else {
        let hint = vec![
            TextLine::styled(" Press Enter to run the step above.", Style::new().fg(DIM)),
            TextLine::raw(""),
            TextLine::styled(
                " Each step shows its bitcoin-cli equivalent, Core's result",
                Style::new().fg(DIM),
            ),
            TextLine::styled(" and the indexer's response here.", Style::new().fg(DIM)),
        ];
        frame.render_widget(Paragraph::new(hint).block(panel("Result")), area);
        return;
    };

    // A failure leads with the full error and how to recover from it.
    let mut lines = match &last.result {
        Ok(_) => Vec::new(),
        Err(error) => error_lines(error, app, width),
    };
    lines.push(section("EQUIVALENT bitcoin-cli", "sent as JSON-RPC"));
    if last.calls.is_empty() {
        lines.push(TextLine::styled(
            " No Bitcoin Core call: this reads the indexer only.",
            Style::new().fg(DIM),
        ));
    }
    for call in &last.calls {
        lines.extend(command_lines(&call.command(), width));
        let (marker, text, style) = match &call.result {
            Ok(value) => ("→", summarize(value), Style::new().fg(GREEN)),
            Err(error) => ("✖", error.clone(), Style::new().fg(RED)),
        };
        for (i, chunk) in chunked(&text, width.saturating_sub(5))
            .into_iter()
            .enumerate()
        {
            let lead = if i == 0 {
                format!("   {marker} ")
            } else {
                "     ".into()
            };
            lines.push(TextLine::from(vec![
                Span::styled(lead, style),
                Span::styled(chunk, style),
            ]));
        }
    }

    lines.push(TextLine::raw(""));
    lines.push(section("INDEXER RESPONSE", "live"));
    lines.extend(indexer_lines(last.op, app, width));

    if let Ok(details) = &last.result {
        lines.push(TextLine::raw(""));
        lines.push(section("DETAILS", ""));
        lines.extend(details.iter().map(result_line));
    }

    let title = match &last.result {
        Ok(_) => Span::styled(
            format!(" ✔ {} ", last.op.label()),
            Style::new().fg(GREEN).bold(),
        ),
        Err(_) => Span::styled(
            format!(" ✖ {} failed ", last.op.label()),
            Style::new().fg(RED).bold(),
        ),
    };
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(DIM))
        .title(title);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(block),
        area,
    );
}

/// The error in full, what it means when we recognize it, and the keys
/// that recover from it.
fn error_lines(error: &str, app: &App, width: usize) -> Vec<TextLine<'static>> {
    let mut lines = vec![section("ERROR", "full message")];
    for line in wrap_words(error, width.saturating_sub(1)) {
        lines.push(TextLine::styled(format!(" {line}"), Style::new().fg(RED)));
    }
    if let Some(hint) = recovery_hint(error) {
        lines.push(TextLine::raw(""));
        for line in wrap_words(hint, width.saturating_sub(1)) {
            lines.push(TextLine::styled(
                format!(" {line}"),
                Style::new().fg(YELLOW),
            ));
        }
    }

    let keys: &[(&str, &str)] = if app.guide.is_some() {
        &[
            ("R", "retry"),
            ("a", "amount"),
            ("B", "back"),
            ("S", "restart"),
            ("Esc", "exit guide"),
            ("q", "quit"),
        ]
    } else {
        &[("Enter", "run again"), ("a", "amount"), ("q", "quit")]
    };
    lines.push(TextLine::raw(""));
    for row in keys.chunks(3) {
        let mut spans = vec![Span::raw(" ")];
        for (key, action) in row {
            spans.push(Span::styled(
                format!(" {key} "),
                Style::new().fg(Color::Black).bg(YELLOW).bold(),
            ));
            spans.push(Span::styled(
                format!(" {action:<10}"),
                Style::new().fg(TEXT),
            ));
        }
        lines.push(TextLine::from(spans));
    }
    lines.push(TextLine::raw(""));
    lines
}

/// What a recognized failure means and how to fix it. The dashboard never
/// mines, creates or loads wallets on its own; these say what you can do.
pub fn recovery_hint(error: &str) -> Option<&'static str> {
    if error.contains("Insufficient") {
        Some(
            "The wallet cannot pay this amount plus fee from its spendable balance. \
             Press a to send less, or mine blocks yourself: coinbase rewards become \
             spendable after 100 confirmations.",
        )
    } else if error.contains("RPC error -18") {
        Some("No wallet is loaded. Load one yourself (bitcoin-cli loadwallet <name>).")
    } else if error.contains("RPC error -19") {
        Some(
            "Several wallets are loaded and none was chosen: add /wallet/<name> to \
             BITCOIN_RPC_URL in .env and restart the dashboard.",
        )
    } else if error.contains("RPC error -13") {
        Some("The wallet is locked: unlock it with bitcoin-cli walletpassphrase.")
    } else if error.contains("Couldn't connect") || error.contains("Connection refused") {
        Some("Bitcoin Core is unreachable: is the Polar network running?")
    } else {
        None
    }
}

fn render_wallet(frame: &mut Frame, area: Rect, app: &App) {
    let amount = app.amount_sats;
    let mut lines = Vec::new();
    match &app.snapshot.wallet {
        None => lines.push(TextLine::styled("Reading wallet…", Style::new().fg(DIM))),
        Some(Err(error)) => {
            lines.push(TextLine::styled(error.clone(), Style::new().fg(RED)));
            if let Some(hint) = recovery_hint(error) {
                lines.push(TextLine::styled(hint, Style::new().fg(YELLOW)));
            }
        }
        Some(Ok(wallet)) => {
            let b = wallet.balances;
            let affordable = b.trusted > amount;
            lines.push(TextLine::from(vec![
                label("wallet "),
                Span::styled(
                    if wallet.name.is_empty() {
                        "(default)".into()
                    } else {
                        wallet.name.clone()
                    },
                    Style::new().fg(TEXT).bold(),
                ),
                label("   spendable "),
                Span::styled(
                    btc_trim(b.trusted),
                    Style::new().fg(if affordable { GREEN } else { RED }).bold(),
                ),
                label("   immature "),
                Span::styled(btc_trim(b.immature), Style::new().fg(TEXT)),
                label("   unconfirmed "),
                Span::styled(btc_trim(b.untrusted_pending), Style::new().fg(TEXT)),
            ]));
            let mut second = vec![
                label("send amount "),
                Span::styled(
                    format!("{} BTC", btc_trim(amount)),
                    Style::new().fg(ORANGE).bold(),
                ),
                label("  [a] edit"),
            ];
            if !affordable {
                second.push(Span::styled(
                    "   ⚠ more than spendable: sending will be refused",
                    Style::new().fg(RED),
                ));
            }
            lines.push(TextLine::from(second));
        }
    }
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(panel("Wallet · getbalances")),
        area,
    );
}

fn render_amount_edit(frame: &mut Frame, app: &App, edit: &AmountEdit) {
    let spendable = match &app.snapshot.wallet {
        Some(Ok(wallet)) => format!("{} BTC", btc_trim(wallet.balances.trusted)),
        _ => "unknown".into(),
    };
    let mut text = vec![
        TextLine::from(vec![
            Span::styled(" Amount (BTC): ", Style::new().fg(TEXT)),
            Span::styled(format!("{}█", edit.text), Style::new().fg(ORANGE).bold()),
        ]),
        TextLine::styled(
            format!(" Spendable now: {spendable} (the fee comes on top)"),
            Style::new().fg(DIM),
        ),
    ];
    if let Some(error) = &edit.error {
        text.push(TextLine::styled(
            format!(" ✖ {error}"),
            Style::new().fg(RED),
        ));
    }
    text.push(TextLine::raw(""));
    text.push(TextLine::from(vec![
        Span::raw(" "),
        Span::styled(" Enter ", Style::new().fg(Color::Black).bg(ORANGE).bold()),
        Span::styled(" save    ", Style::new().fg(TEXT)),
        Span::styled(" Esc ", Style::new().fg(Color::Black).bg(DIM)),
        Span::styled(" cancel", Style::new().fg(TEXT)),
    ]));

    let [area] = Layout::horizontal([Constraint::Length(60)])
        .flex(Flex::Center)
        .areas(frame.area());
    let [area] = Layout::vertical([Constraint::Length(text.len() as u16 + 2)])
        .flex(Flex::Center)
        .areas(area);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(text).block(
            Block::bordered()
                .border_type(BorderType::Double)
                .border_style(Style::new().fg(ORANGE))
                .title(Span::styled(
                    " Send amount ",
                    Style::new().fg(ORANGE).bold(),
                )),
        ),
        area,
    );
}

/// BTC without trailing zeros: 0.1, 450.00021561.
fn btc_trim(sats: u64) -> String {
    let text = btc(sats);
    let text = text.trim_end_matches('0');
    text.strip_suffix('.').unwrap_or(text).to_string()
}

/// The indexer's current answer for what `op` changed, from the snapshot
/// refreshed every second.
fn indexer_lines(op: Op, app: &App, width: usize) -> Vec<TextLine<'static>> {
    let mut lines = Vec::new();
    let mut endpoint = |path: String, answer: String, style: Style| {
        for chunk in chunked(&path, width.saturating_sub(3)) {
            lines.push(TextLine::styled(format!(" {chunk}"), Style::new().fg(TEXT)));
        }
        lines.push(TextLine::styled(format!("   → {answer}"), style));
    };

    match op {
        Op::Send { .. } | Op::Inspect => match (&app.inputs.tracked_tx, app.stage()) {
            (Some(txid), Some(stage)) => {
                let (answer, color) = match stage {
                    Stage::Created => ("404: not indexed yet".to_string(), DIM),
                    Stage::Mempool => ("unconfirmed (in the mempool)".to_string(), YELLOW),
                    Stage::Confirmed {
                        height,
                        confirmations,
                    } => (
                        format!("confirmed · block #{height} · {confirmations} conf"),
                        GREEN,
                    ),
                };
                endpoint(
                    format!("GET /api/transactions/{txid}/status"),
                    answer,
                    Style::new().fg(color),
                );
            }
            _ => lines.push(TextLine::styled(
                " No tracked transaction.",
                Style::new().fg(DIM),
            )),
        },
        Op::NewAddress => lines.push(TextLine::styled(
            " Nothing indexed yet: the address appears once a transaction pays it.",
            Style::new().fg(DIM),
        )),
        Op::Utxos => lines.push(TextLine::styled(
            " GET /api/utxos/summary → see details below.",
            Style::new().fg(DIM),
        )),
        Op::ChainInfo
        | Op::Mine { .. }
        | Op::MineEmpty { .. }
        | Op::PrepareInvalidate
        | Op::Invalidate => {
            let (answer, color) = match &app.snapshot.status {
                Some(status) => {
                    let synced = status["synced"].as_bool();
                    (
                        format!(
                            "indexed #{} · Core #{} · {}",
                            status["indexed_height"],
                            status["core_height"],
                            match synced {
                                Some(true) => "synchronized".to_string(),
                                Some(false) =>
                                    format!("{} behind / tip differs", status["blocks_behind"]),
                                None => "sync unknown".to_string(),
                            }
                        ),
                        if synced == Some(true) { GREEN } else { YELLOW },
                    )
                }
                None => ("unavailable".to_string(), RED),
            };
            endpoint("GET /api/status".into(), answer, Style::new().fg(color));
        }
    }
    lines
}

/// An action's plan as labelled bitcoin-cli commands and indexer endpoints.
fn plan_lines(plan: &Plan, width: usize) -> Vec<TextLine<'static>> {
    let mut lines = vec![section("EQUIVALENT bitcoin-cli", "sent as JSON-RPC")];
    if let Some(note) = plan.note {
        for (i, line) in wrap_words(note, width.saturating_sub(3))
            .into_iter()
            .enumerate()
        {
            let lead = if i == 0 { " ⚠ " } else { "   " };
            lines.push(TextLine::styled(
                format!("{lead}{line}"),
                Style::new().fg(YELLOW),
            ));
        }
    }
    if plan.commands.is_empty() {
        lines.push(TextLine::styled(
            " No Bitcoin Core call: this reads the indexer only.",
            Style::new().fg(DIM),
        ));
    }
    for command in &plan.commands {
        lines.extend(command_lines(command, width));
    }
    if !plan.endpoints.is_empty() {
        lines.push(section("INDEXER API", "in-process"));
        for path in &plan.endpoints {
            for chunk in chunked(path, width.saturating_sub(3)) {
                lines.push(TextLine::styled(format!(" {chunk}"), Style::new().fg(TEXT)));
            }
        }
    }
    lines
}

fn section(title: &str, note: &str) -> TextLine<'static> {
    let mut spans = vec![Span::styled(
        format!(" {title}"),
        Style::new().fg(TEXT).bold(),
    )];
    if !note.is_empty() {
        spans.push(Span::styled(format!(" · {note}"), Style::new().fg(DIM)));
    }
    TextLine::from(spans)
}

/// One command, on one line when it fits; otherwise one argument per line
/// with shell-style `\` continuations, and over-long arguments (hashes,
/// addresses) split across lines. Nothing is ever cut off.
fn command_lines(command: &Command, width: usize) -> Vec<TextLine<'static>> {
    const INDENT: &str = "     ";
    let dim = Style::new().fg(DIM);
    let method = Style::new().fg(ORANGE).bold();
    let style = |arg: &Arg| match arg {
        Arg::Value(_) => Style::new().fg(TEXT),
        Arg::Placeholder(_) => Style::new().fg(YELLOW).italic(),
    };
    let head = || {
        vec![
            Span::styled(format!(" $ {CLI} "), dim),
            Span::styled(command.method.clone(), method),
        ]
    };

    if command.text().chars().count() + 3 <= width {
        let mut spans = head();
        for arg in &command.args {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(arg.text().to_string(), style(arg)));
        }
        return vec![TextLine::from(spans)];
    }

    let continued = !command.args.is_empty();
    let mut lines = if CLI.len() + command.method.len() + 6 <= width {
        let mut first = head();
        if continued {
            first.push(Span::styled(" \\", dim));
        }
        vec![TextLine::from(first)]
    } else {
        // Too narrow even for the method name: give it its own line.
        let mut method_line = vec![
            Span::raw(INDENT),
            Span::styled(command.method.clone(), method),
        ];
        if continued {
            method_line.push(Span::styled(" \\", dim));
        }
        vec![
            TextLine::from(vec![Span::styled(format!(" $ {CLI} \\"), dim)]),
            TextLine::from(method_line),
        ]
    };
    let room = width.saturating_sub(INDENT.len() + 2).max(8);
    for (i, arg) in command.args.iter().enumerate() {
        let chunks = chunked(arg.text(), room);
        let last_chunk = chunks.len() - 1;
        for (j, chunk) in chunks.into_iter().enumerate() {
            let mut spans = vec![Span::raw(INDENT), Span::styled(chunk, style(arg))];
            if j == last_chunk && i + 1 < command.args.len() {
                spans.push(Span::styled(" \\", dim));
            }
            lines.push(TextLine::from(spans));
        }
    }
    lines
}

/// `text` split into pieces of at most `width` characters (at least one).
fn chunked(text: &str, width: usize) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return vec![String::new()];
    }
    chars
        .chunks(width.max(1))
        .map(|chunk| chunk.iter().collect())
        .collect()
}

/// Word-wraps `text` to `width` columns.
fn wrap_words(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

fn result_line(line: &Line) -> TextLine<'static> {
    match line {
        Line::Field(label, value) => TextLine::from(vec![
            Span::styled(format!(" {label:<18}"), Style::new().fg(DIM)),
            Span::styled(value.clone(), Style::new().fg(TEXT)),
        ]),
        Line::Note(text) => TextLine::styled(format!(" {text}"), Style::new().fg(DIM)),
        Line::Good(text) => TextLine::styled(format!(" ✔ {text}"), Style::new().fg(GREEN)),
        Line::Warn(text) => TextLine::styled(format!(" ⚠ {text}"), Style::new().fg(YELLOW)),
    }
}

fn render_confirm(frame: &mut Frame, app: &App, height: u64) {
    const WIDTH: u16 = 76;
    let command = plan(Op::Invalidate, &app.inputs, None)
        .commands
        .into_iter()
        .next()
        .expect("invalidation has a command");

    let mut text = vec![
        TextLine::from(vec![
            Span::styled(" Invalidate block ", Style::new().fg(TEXT)),
            Span::styled(format!("#{height}"), Style::new().fg(ORANGE).bold()),
            Span::styled(" in Bitcoin Core?", Style::new().fg(TEXT)),
        ]),
        TextLine::raw(""),
        section("EQUIVALENT bitcoin-cli", "sent as JSON-RPC after y"),
    ];
    text.extend(command_lines(&command, WIDTH as usize - 2));
    text.extend([
        TextLine::raw(""),
        TextLine::styled(
            " This rewrites the regtest chain: the block is disconnected and",
            Style::new().fg(TEXT),
        ),
        TextLine::styled(
            " its transactions return to the mempool.",
            Style::new().fg(TEXT),
        ),
        TextLine::raw(""),
        TextLine::from(vec![
            Span::raw(" "),
            Span::styled(" y ", Style::new().fg(Color::Black).bg(RED).bold()),
            Span::styled(" invalidate    ", Style::new().fg(TEXT)),
            Span::styled(" any other key ", Style::new().fg(Color::Black).bg(DIM)),
            Span::styled(" cancel", Style::new().fg(TEXT)),
        ]),
    ]);

    let [area] = Layout::horizontal([Constraint::Length(WIDTH)])
        .flex(Flex::Center)
        .areas(frame.area());
    let [area] = Layout::vertical([Constraint::Length(text.len() as u16 + 2)])
        .flex(Flex::Center)
        .areas(area);

    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(text).block(
            Block::bordered()
                .border_type(BorderType::Double)
                .border_style(Style::new().fg(RED))
                .title(Span::styled(
                    " Confirm destructive action ",
                    Style::new().fg(RED).bold(),
                )),
        ),
        area,
    );
}

// ------------------------------------------------------------
// HELPERS
// ------------------------------------------------------------

fn label(text: &str) -> Span<'static> {
    Span::styled(text.to_string(), Style::new().fg(DIM))
}

fn number(value: &Value) -> Span<'static> {
    match value.as_i64() {
        Some(n) => Span::styled(format!("#{n}"), Style::new().fg(ORANGE).bold()),
        None => Span::styled("—", Style::new().fg(DIM)),
    }
}

fn state(ok: bool, good: &str, bad: &str) -> Span<'static> {
    if ok {
        Span::styled(format!(" {good}"), Style::new().fg(GREEN))
    } else {
        Span::styled(format!(" {bad}"), Style::new().fg(RED))
    }
}

fn zmq(status: &Value) -> Span<'static> {
    let text = status.as_str().unwrap_or("unknown").replace('_', " ");
    let color = match status.as_str() {
        Some("connected") => GREEN,
        Some("disconnected") => RED,
        _ => DIM,
    };
    Span::styled(text, Style::new().fg(color))
}

fn short(hash: &str, head: usize, tail: usize) -> String {
    if hash.len() <= head + tail + 1 {
        hash.to_string()
    } else {
        format!("{}…{}", &hash[..head], &hash[hash.len() - tail..])
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}

fn age(now: i64, timestamp: Option<i64>) -> String {
    let Some(ts) = timestamp else {
        return "—".into();
    };
    let secs = (now - ts).max(0);
    match secs {
        0..60 => format!("{secs}s ago"),
        60..3_600 => format!("{}m ago", secs / 60),
        3_600..86_400 => format!("{}h ago", secs / 3_600),
        _ => format!("{}d ago", secs / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};
    use serde_json::json;

    use super::*;
    use crate::tui::{
        actions::{BlockRef, Op},
        app::{Guide, GuideKind, LastResult},
    };

    fn screen(app: &App) -> String {
        let mut terminal = Terminal::new(TestBackend::new(160, 45)).unwrap();
        terminal.draw(|frame| render(frame, app)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        buffer
            .content
            .chunks(buffer.area.width as usize)
            .map(|row| row.iter().map(|c| c.symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn live_app() -> App {
        let mut app = App::default();
        app.snapshot.status = Some(json!({
            "network": "regtest", "core_height": 343, "indexed_height": 343,
            "blocks_behind": 0, "synced": true, "bitcoin_core": "connected",
            "zmq": { "blocks": { "status": "connected" }, "transactions": { "status": "not_configured" } },
            "mempool_transactions": 1, "unspent_outputs": 354
        }));
        app.snapshot.blocks = vec![json!({
            "height": 343, "hash": "715320e2645fce918554df6facbfc2812aca4b0b8855c87e4cd52208284ccca6",
            "tx_count": 1, "timestamp": 0
        })];
        app
    }

    #[test]
    fn renders_live_status_blocks_and_menu() {
        let text = screen(&live_app());

        for expected in [
            "AMIABLE INDEXER",
            "REGTEST",
            "SYNCHRONIZED",
            "#343",
            "715320e264…08284ccca6".split('…').next().unwrap(),
            "not configured",
            "Bitcoin Core actions",
            "Guided: reorg recovery",
            "Event log",
        ] {
            assert!(text.contains(expected), "missing {expected:?}\n{text}");
        }
    }

    #[test]
    fn renders_guide_progress_and_confirmation_popup() {
        let mut app = live_app();
        app.guide = Some(guide_at(GuideKind::Reorg, 2));
        app.confirm = Some(BlockRef {
            height: 344,
            hash: "00ff".into(),
        });

        let text = screen(&app);
        assert!(text.contains("Guided demo · Reorg recovery"));
        assert!(text.contains("✔ 1. Send the payment (0.1 BTC)"), "{text}");
        assert!(text.contains("▶ 3. Invalidate that block"));
        assert!(text.contains("Invalidate block #344 in Bitcoin Core?"));
        assert!(text.contains("y  invalidate"));
    }

    #[test]
    fn renders_failures_in_the_result_panel() {
        let mut app = live_app();
        app.preview = false;
        app.last = Some(LastResult {
            calls: Vec::new(),
            op: Op::Send { sats: 100_000_000 },
            result: Err("sendtoaddress: Insufficient funds".into()),
        });

        let text = screen(&app);
        assert!(text.contains("✖ Send 1.00000000 BTC failed"));
        assert!(text.contains("Insufficient funds"));
    }

    #[test]
    fn shortens_hashes_and_formats_ages() {
        assert_eq!(short("abcdefghij", 3, 3), "abc…hij");
        assert_eq!(short("abc", 3, 3), "abc");
        assert_eq!(age(1_000, Some(990)), "10s ago");
        assert_eq!(age(10_000, Some(10_000 - 7_200)), "2h ago");
    }

    #[test]
    fn about_panel_shows_the_full_description() {
        let mut app = live_app();
        app.selected = MENU.iter().position(|m| m.label == "Send BTC").unwrap();
        let text = screen(&app);
        assert!(text.contains("About · Send BTC"));
        assert!(text.contains("CREATED → IN MEMPOOL"), "{text}");
    }

    #[test]
    fn stepper_highlights_the_current_stage() {
        let mut app = live_app();
        app.inputs.tracked_tx = Some("ab".repeat(32));
        app.snapshot.tracked_txid = app.inputs.tracked_tx.clone();
        app.snapshot.tracked = Some(Some(json!({
            "status": "confirmed", "block_height": 345, "confirmations": 1
        })));

        let text = screen(&app);
        assert!(
            text.contains("✔ CREATED ──── ✔ IN MEMPOOL ────  CONFIRMED #345"),
            "{text}"
        );
        assert!(text.contains("In block #345 · 1 confirmation(s)"));
    }

    fn watched(rollback: Option<crate::db::reorg::Rollback>) -> App {
        let mut app = live_app();
        app.reorg = Some(crate::tui::app::ReorgWatch {
            old: BlockRef {
                height: 357,
                hash: "aaaaaaaa1111111111111111bbbbbb".into(),
            },
            rollback,
        });
        app
    }

    #[test]
    fn reorg_report_is_pending_until_the_indexer_rolls_back() {
        let mut app = watched(None);
        // Core disconnected #357 and has nothing at that height yet; the
        // indexer still stores the old block.
        app.snapshot.reorg = Some(ReorgCheck {
            old_hash: "aaaaaaaa1111111111111111bbbbbb".into(),
            core_hash: None,
            indexer_hash: Some("aaaaaaaa1111111111111111bbbbbb".into()),
            old_indexed: true,
        });

        let text = screen(&app);
        assert!(text.contains("REORG PENDING"));
        assert!(text.contains("Old block #357:  aaaaaaaa…bbbbbb  ORPHANED"));
        assert!(text.contains("none yet"));
        assert!(text.contains("Waiting: indexer still has the old block"));
        assert!(!text.contains("Complete"), "{text}");
    }

    #[test]
    fn reorg_report_completes_from_real_checks() {
        let mut app = watched(Some(crate::db::reorg::Rollback {
            blocks_removed: 1,
            outputs_restored: 2,
        }));
        app.snapshot.reorg = Some(ReorgCheck {
            old_hash: "aaaaaaaa1111111111111111bbbbbb".into(),
            core_hash: Some("dddddddd2222222222222222eeeeee".into()),
            indexer_hash: Some("dddddddd2222222222222222eeeeee".into()),
            old_indexed: false,
        });

        let text = screen(&app);
        for expected in [
            "REORG DETECTED",
            "Old block #357:  aaaaaaaa…bbbbbb  ORPHANED",
            "New block #357:  dddddddd…eeeeee  CANONICAL",
            "Rollback:         Complete",
            "UTXO restoration: Complete · 2 output(s) unspent again",
            "Reindexing:       Complete",
            "Sync lag:         0 blocks · synchronized",
        ] {
            assert!(text.contains(expected), "missing {expected:?}\n{text}");
        }
    }

    #[test]
    fn selecting_an_action_previews_its_bitcoin_cli_equivalent() {
        let mut app = live_app();
        app.on_key(crossterm::event::KeyEvent::from(
            crossterm::event::KeyCode::Char('3'),
        ));

        let text = screen(&app);
        assert!(text.contains("Preview · Send 0.10000000 BTC"), "{text}");
        assert!(text.contains("EQUIVALENT bitcoin-cli · sent as JSON-RPC"));
        assert!(text.contains("$ bitcoin-cli -regtest getnewaddress"));
        assert!(text.contains("$ bitcoin-cli -regtest getbalances"));
        assert!(text.contains("<new address> 0.1"), "{text}");
    }

    #[test]
    fn long_commands_wrap_without_losing_characters() {
        let hash = "0123456789abcdef".repeat(4);
        let command = crate::tui::command::RpcCall {
            method: "invalidateblock".into(),
            params: vec![json!(hash)],
            result: Ok(Value::Null),
        }
        .command();

        let lines = command_lines(&command, 30);
        assert!(lines.len() > 2);
        let joined: String = lines
            .iter()
            .flat_map(|line| line.spans.iter().map(|span| span.content.to_string()))
            .collect::<String>()
            .replace([' ', '\\'], "");
        assert!(joined.contains(&hash), "{joined}");
        assert!(lines.iter().all(|line| line.width() <= 30));
    }

    #[test]
    fn result_shows_commands_core_results_and_the_indexer_response() {
        let mut app = live_app();
        let txid = "9f".repeat(32);
        app.inputs.tracked_tx = Some(txid.clone());
        app.snapshot.tracked_txid = Some(txid.clone());
        app.snapshot.tracked = Some(Some(json!({ "status": "unconfirmed" })));
        app.preview = false;
        app.last = Some(LastResult {
            op: Op::Send { sats: 100_000_000 },
            calls: vec![crate::tui::command::RpcCall {
                method: "sendtoaddress".into(),
                params: vec![json!("bcrt1qdest"), json!(1.0)],
                result: Ok(json!(txid)),
            }],
            result: Ok(vec![]),
        });

        let text = screen(&app);
        assert!(
            text.contains("$ bitcoin-cli -regtest sendtoaddress bcrt1qdest 1.0"),
            "{text}"
        );
        assert!(text.contains(&format!("→ {}", &txid[..40])));
        assert!(text.contains("INDEXER RESPONSE · live"));
        assert!(text.contains("→ unconfirmed (in the mempool)"));
    }

    #[test]
    fn guide_shows_the_next_steps_command() {
        let mut app = live_app();
        app.inputs.mining_address = Some("bcrt1qminer".into());
        app.guide = Some(guide_at(GuideKind::Reorg, 3));

        let text = screen(&app);
        assert!(text.contains("Next step runs:"));
        assert!(
            text.contains("$ bitcoin-cli -regtest generateblock bcrt1qminer '[]'"),
            "{text}"
        );
        assert!(
            !text.contains("Bitcoin Core actions"),
            "menu hidden while guiding"
        );
    }

    #[test]
    fn confirmation_popup_shows_the_full_invalidateblock_command() {
        let mut app = live_app();
        let hash = "ab".repeat(32);
        let target = BlockRef {
            height: 344,
            hash: hash.clone(),
        };
        app.inputs.invalidate_target = Some(target.clone());
        app.confirm = Some(target);

        let text = screen(&app);
        assert!(text.contains("bitcoin-cli -regtest invalidateblock"));
        assert!(text.contains(&hash), "{text}");
        assert!(text.contains("y  invalidate"));
    }

    fn guide_at(kind: GuideKind, step: usize) -> Guide {
        let mut guide = Guide::new(kind);
        guide.step = step;
        for state in &mut guide.states[..step] {
            *state = crate::tui::app::StepState::Done;
        }
        guide
    }

    #[test]
    fn failed_guide_step_shows_full_rpc_error_and_recovery_keys() {
        let mut app = live_app();
        let mut guide = guide_at(GuideKind::Lifecycle, 1);
        guide.states[1] = crate::tui::app::StepState::Failed;
        app.guide = Some(guide);
        app.preview = false;
        app.last = Some(LastResult {
            op: Op::Send { sats: 100_000_000 },
            calls: vec![crate::tui::command::RpcCall {
                method: "sendtoaddress".into(),
                params: vec![json!("bcrt1qdest"), json!(1.0)],
                result: Err("RPC error -6: Insufficient funds".into()),
            }],
            result: Err("sendtoaddress: RPC error -6: Insufficient funds".into()),
        });

        let text = screen(&app);
        for expected in [
            "ERROR · full message",
            "sendtoaddress: RPC error -6: Insufficient funds",
            "Press a to send less",
            " R  retry",
            " B  back",
            " S  restart",
            "✖ 2. Send BTC to it (0.1 BTC)",
            "R retries with:",
        ] {
            assert!(text.contains(expected), "missing {expected:?}\n{text}");
        }
    }

    #[test]
    fn wallet_panel_separates_spendable_and_immature_and_warns() {
        let mut app = live_app();
        app.snapshot.wallet = Some(Ok(crate::tui::actions::Wallet {
            name: "polar".into(),
            balances: crate::tui::actions::Balances {
                trusted: 50_000_000,
                untrusted_pending: 0,
                immature: 45_000_021_561,
            },
        }));
        let text = screen(&app);
        assert!(
            text.contains("wallet polar   spendable 0.5   immature 450.00021561   unconfirmed 0"),
            "{text}"
        );
        assert!(text.contains("send amount 0.1 BTC"));
        assert!(!text.contains("more than spendable"));

        app.amount_sats = 100_000_000;
        assert!(screen(&app).contains("more than spendable"));
    }

    #[test]
    fn send_preview_matches_the_selected_amount() {
        let mut app = live_app();
        app.inputs.tracked_address = Some("bcrt1qdest".into());
        app.amount_sats = 5_000_000;
        app.selected = MENU.iter().position(|m| m.label == "Send BTC").unwrap();
        let text = screen(&app);
        assert!(
            text.contains("$ bitcoin-cli -regtest getbalances"),
            "{text}"
        );
        assert!(
            text.contains("$ bitcoin-cli -regtest walletcreatefundedpsbt \\"),
            "{text}"
        );
        assert!(text.contains(r#"'[{"bcrt1qdest":0.05}]'"#), "{text}");
        assert!(text.contains("spendable."), "note wraps, not cut\n{text}");
        assert!(text.contains("sendtoaddress bcrt1qdest 0.05"));
    }
}
