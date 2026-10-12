//! Terminal presentation helpers.
//!
//! Output only — nothing here affects indexing logic. Colors are disabled
//! automatically when stdout is not a terminal or `NO_COLOR` is set.
//!
//! When the dashboard (`src/tui`) owns the terminal, [`capture`] redirects
//! every line to a channel instead, so indexer output never corrupts it.

use std::fmt::Display;
use std::io::IsTerminal;
use std::sync::OnceLock;

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Success,
    Warn,
    Error,
    /// A block was announced or indexed.
    Block,
}

#[derive(Debug, Clone)]
pub struct LogLine {
    pub level: Level,
    pub text: String,
}

static SINK: OnceLock<UnboundedSender<LogLine>> = OnceLock::new();

/// Sends all further output to the returned channel instead of the terminal.
pub fn capture() -> UnboundedReceiver<LogLine> {
    let (sender, receiver) = unbounded_channel();
    let _ = SINK.set(sender);
    receiver
}

fn capturing() -> bool {
    SINK.get().is_some()
}

/// Forwards the line when capturing; returns whether it was captured.
fn captured(level: Level, text: impl Display) -> bool {
    match SINK.get() {
        Some(sink) => {
            let _ = sink.send(LogLine {
                level,
                text: text.to_string(),
            });
            true
        }
        None => false,
    }
}

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const BLUE: &str = "\x1b[34m";
const MAGENTA: &str = "\x1b[35m";
const CYAN: &str = "\x1b[36m";
const ORANGE: &str = "\x1b[38;5;208m";

fn color_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED
        .get_or_init(|| std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal())
}

fn paint(style: &str, text: impl Display) -> String {
    if color_enabled() {
        format!("{style}{text}{RESET}")
    } else {
        text.to_string()
    }
}

/// Formats an integer with thousands separators: 812345 -> "812,345".
pub fn thousands(n: impl Into<u64>) -> String {
    let digits = n.into().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

fn human_bytes(bytes: usize) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

pub fn banner() {
    if capturing() {
        return;
    }
    let lines = [
        "  ₿  Amiable Bitcoin Indexer",
        "     Bitcoin Core  →  PostgreSQL",
    ];
    let width = 40;
    println!();
    println!("{}", paint(ORANGE, format!("╭{}╮", "─".repeat(width))));
    for line in lines {
        let pad = width.saturating_sub(line.chars().count());
        println!(
            "{}{}{}{}",
            paint(ORANGE, "│"),
            paint(BOLD, line),
            " ".repeat(pad),
            paint(ORANGE, "│")
        );
    }
    println!("{}", paint(ORANGE, format!("╰{}╯", "─".repeat(width))));
    println!();
}

pub fn section(title: &str) {
    // Section headings are terminal layout; the dashboard has its own.
    if capturing() {
        return;
    }
    println!();
    println!("{} {}", paint(MAGENTA, "━━"), paint(BOLD, title));
}

pub fn success(msg: impl Display) {
    if captured(Level::Success, &msg) {
        return;
    }
    println!("  {} {}", paint(GREEN, "✔"), msg);
}

pub fn info(msg: impl Display) {
    if captured(Level::Info, &msg) {
        return;
    }
    println!("  {} {}", paint(BLUE, "•"), msg);
}

pub fn field(label: &str, value: impl Display) {
    if captured(Level::Info, format!("  {label}: {value}")) {
        return;
    }
    println!(
        "    {} {}",
        paint(DIM, format!("{label:<14}")),
        paint(CYAN, value)
    );
}

pub fn warn(msg: impl Display) {
    if captured(Level::Warn, &msg) {
        return;
    }
    println!("  {} {}", paint(YELLOW, "⚠"), paint(YELLOW, msg));
}

pub fn error(msg: impl Display) {
    if captured(Level::Error, &msg) {
        return;
    }
    eprintln!("  {} {}", paint(RED, "✖"), paint(RED, msg));
}

pub fn waiting() {
    if capturing() {
        return;
    }
    println!(
        "  {} {}",
        paint(DIM, "◌"),
        paint(DIM, "Listening for new blocks…")
    );
}

pub fn block_start(height: u64) {
    // block_indexed reports the outcome; the dashboard skips the preamble.
    if capturing() {
        return;
    }
    println!();
    println!(
        "  {} {}",
        paint(ORANGE, "▸"),
        paint(BOLD, format!("Block #{}", thousands(height)))
    );
}

pub fn block_fetched(hash: impl Display, size: usize) {
    if capturing() {
        return;
    }
    println!("    {} {}", paint(DIM, "hash"), paint(DIM, hash));
    println!("    {} {}", paint(DIM, "size"), human_bytes(size));
}

pub fn block_indexed(height: u64, tx_count: usize) {
    if captured(
        Level::Block,
        format!("Indexed block #{} · {} tx", thousands(height), tx_count),
    ) {
        return;
    }
    println!(
        "  {} Indexed block {} {}",
        paint(GREEN, "✔"),
        paint(BOLD, format!("#{}", thousands(height))),
        paint(DIM, format!("· {} tx", thousands(tx_count as u64)))
    );
}

pub fn new_block_alert(hash: impl Display, sequence: impl Display) {
    if captured(Level::Block, format!("ZMQ: new block {hash}")) {
        return;
    }
    println!();
    println!(
        "{} {}",
        paint(ORANGE, "⚡"),
        paint(BOLD, "New block announced via ZMQ")
    );
    field("hash", hash);
    field("sequence", sequence);
}

/// An empty line between output groups (terminal mode only).
pub fn blank() {
    if !capturing() {
        println!();
    }
}
