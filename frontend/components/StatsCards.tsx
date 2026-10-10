"use client";

import Link from "next/link";
import { formatNumber, formatRelative, percent } from "@/lib/format";
import type { BlockSummary, IndexerStatus, ZmqEndpointState } from "@/lib/types";
import { HashDisplay } from "./HashDisplay";
import { Skeleton } from "./LoadingState";
import type { Connection } from "./StatusProvider";

interface StatsCardsProps {
  status: IndexerStatus | null;
  connection: Connection;
  updatedAt: number | null;
  tip?: BlockSummary;
}

/**
 * Indexer state at a glance. Indexed height leads; the remaining counts read
 * as a ledger rather than a grid of identical cards.
 */
export function StatsCards({ status, connection, updatedAt, tip }: StatsCardsProps) {
  const ready = status !== null;

  return (
    <section className="card" aria-label="Indexer statistics">
      <div className="grid lg:grid-cols-[minmax(0,20rem)_1fr]">
        {/* Primary metric */}
        <div className="border-b border-line p-5 lg:border-b-0 lg:border-r">
          <p className="label">Indexed height</p>
          <div className="mt-2 font-mono text-4xl font-medium tabular-nums tracking-tight text-btc">
            {ready ? (
              status.indexed_height !== null ? formatNumber(status.indexed_height) : <span className="text-faint">—</span>
            ) : (
              <Skeleton className="h-9 w-28" />
            )}
          </div>
          <div className="mt-3 space-y-1.5 text-xs text-muted">
            {tip ? (
              <>
                <div className="flex items-center gap-2">
                  <span className="w-10 text-faint">tip</span>
                  <HashDisplay value={tip.hash} href={`/block/${tip.height}`} head={10} tail={6} className="text-xs" />
                </div>
                <div className="flex items-center gap-2">
                  <span className="w-10 text-faint">mined</span>
                  <span className="num">{formatRelative(tip.timestamp)}</span>
                </div>
              </>
            ) : (
              ready &&
              status.indexed_height === null && <p>No blocks indexed yet.</p>
            )}
          </div>
        </div>

        {/* Ledger of counts */}
        <div className="grid grid-cols-2 gap-px bg-line sm:grid-cols-4">
          <Metric label="Blocks" value={status?.blocks} href="/blocks" />
          <Metric label="Transactions" value={status?.transactions} />
          <Metric label="Inputs" value={status?.inputs} />
          <Metric label="Outputs" value={status?.outputs} />
          <div className="col-span-2 bg-surface p-5 sm:col-span-4">
            <div className="flex flex-wrap items-baseline justify-between gap-x-6 gap-y-1">
              <Link href="/utxos" className="label hover:text-muted">
                Unspent outputs
              </Link>
              {ready && (
                <span className="text-xs text-faint">
                  <span className="num text-muted">{percent(status.unspent_outputs, status.outputs)}</span> of indexed outputs
                </span>
              )}
            </div>
            <div className="mt-2 font-mono text-2xl tabular-nums text-fg">
              {ready ? formatNumber(status.unspent_outputs) : <Skeleton className="h-7 w-20" />}
            </div>
            <div className="mt-3 h-1 overflow-hidden rounded-full bg-raised" aria-hidden>
              {ready && status.outputs > 0 && (
                <div
                  className="h-full bg-btc/70"
                  style={{ width: `${(status.unspent_outputs / status.outputs) * 100}%` }}
                />
              )}
            </div>
          </div>
        </div>
      </div>

      <Pipeline status={status} connection={connection} updatedAt={updatedAt} />
    </section>
  );
}

function Metric({ label, value, href }: { label: string; value: number | undefined; href?: string }) {
  const body = (
    <>
      <p className="label">{label}</p>
      <div className="mt-2 font-mono text-xl tabular-nums text-fg">
        {value === undefined ? <Skeleton className="h-6 w-14" /> : formatNumber(value)}
      </div>
    </>
  );
  const cls = "block bg-surface p-5";
  return href ? (
    <Link href={href} className={`${cls} transition-colors hover:bg-raised`}>
      {body}
    </Link>
  ) : (
    <div className={cls}>{body}</div>
  );
}

type Health = "ok" | "warn" | "bad" | "idle";

const healthDot: Record<Health, string> = {
  ok: "bg-ok",
  warn: "bg-btc",
  bad: "bg-bad",
  idle: "bg-faint",
};

const zmqLabel: Record<ZmqEndpointState, [string, Health]> = {
  connected: ["connected", "ok"],
  connecting: ["connecting", "idle"],
  disconnected: ["disconnected", "bad"],
  not_started: ["not started", "idle"],
  not_configured: ["not configured", "idle"],
};

/**
 * Live state of each hop from Bitcoin Core to this UI, exactly as /api/status
 * reports it. Nothing here is inferred from the page having loaded.
 */
function Pipeline({
  status,
  connection,
  updatedAt,
}: {
  status: IndexerStatus | null;
  connection: Connection;
  updatedAt: number | null;
}) {
  const items: { name: string; value: string; health: Health; title?: string }[] = [];

  if (connection === "offline") {
    items.push({ name: "indexer api", value: "unreachable", health: "bad" });
  } else if (status) {
    const core = status.bitcoin_core === "connected";
    items.push({ name: "bitcoin core", value: core ? "connected" : "unreachable", health: core ? "ok" : "bad" });

    const [blocks, blocksHealth] = zmqLabel[status.zmq.blocks.status];
    items.push({ name: "zmq blocks", value: blocks, health: blocksHealth });
    const [txs, txsHealth] = zmqLabel[status.zmq.transactions.status];
    items.push({ name: "zmq txs", value: txs, health: txsHealth });

    if (status.synced === null) {
      items.push({ name: "sync", value: "unknown", health: "idle", title: "Bitcoin Core is unreachable" });
    } else if (status.synced) {
      items.push({ name: "sync", value: "synchronized", health: "ok" });
    } else {
      const behind = status.blocks_behind ?? 0;
      items.push({
        name: "sync",
        value: behind > 0 ? `${formatNumber(behind)} behind` : "reorg pending",
        health: "warn",
        title: behind > 0 ? undefined : "Same height as Bitcoin Core, different tip hash",
      });
    }

    items.push({
      name: "mempool",
      value: `${formatNumber(status.mempool_transactions)} tx`,
      health: "idle",
    });
  }

  return (
    <div className="flex flex-wrap items-center justify-between gap-x-6 gap-y-2 border-t border-line bg-raised/40 px-5 py-2.5 font-mono text-[11px]">
      <ul className="flex flex-wrap items-center gap-x-5 gap-y-1.5" aria-label="System status">
        {items.map((item) => (
          <li key={item.name} className="flex items-center gap-1.5" title={item.title}>
            <span className={`size-1.5 rounded-full ${healthDot[item.health]}`} aria-hidden />
            <span className="text-faint">{item.name}</span>
            <span className={item.health === "bad" ? "text-bad" : item.health === "warn" ? "text-btc" : "text-muted"}>
              {item.value}
            </span>
          </li>
        ))}
      </ul>
      <span className="text-faint">
        {updatedAt
          ? `polled ${new Date(updatedAt).toLocaleTimeString("en-GB", { hour12: false })}`
          : connection === "offline"
            ? "api unreachable"
            : "polling status…"}
      </span>
    </div>
  );
}
