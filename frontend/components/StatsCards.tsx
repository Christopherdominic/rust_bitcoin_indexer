"use client";

import { ChevronRight } from "lucide-react";
import Link from "next/link";
import { API_URL } from "@/lib/api";
import { formatNumber, formatRelative, percent } from "@/lib/format";
import type { BlockSummary, IndexerStatus } from "@/lib/types";
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
    <section className="overflow-hidden rounded-md border border-line bg-surface" aria-label="Indexer statistics">
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

      <Pipeline connection={connection} updatedAt={updatedAt} />
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

/** Data path from node to this UI. Only the API hop has an observable state. */
function Pipeline({ connection, updatedAt }: { connection: Connection; updatedAt: number | null }) {
  const apiHost = API_URL?.replace(/^https?:\/\//, "") ?? "unconfigured";
  const apiTone = connection === "live" ? "text-ok" : connection === "offline" ? "text-bad" : "text-muted";
  const steps: { name: string; detail: string; tone?: string }[] = [
    { name: "bitcoind", detail: "regtest" },
    { name: "rpc + zmq", detail: "" },
    { name: "amiable-indexer", detail: "rust" },
    { name: "postgres", detail: "" },
    { name: "axum api", detail: apiHost, tone: apiTone },
  ];

  return (
    <div className="flex flex-wrap items-center justify-between gap-x-6 gap-y-2 border-t border-line bg-bg/40 px-5 py-2.5 font-mono text-[11px]">
      <ol className="flex flex-wrap items-center gap-x-1.5 gap-y-1 text-muted">
        {steps.map((s, i) => (
          <li key={s.name} className="flex items-center gap-1.5">
            {i > 0 && <ChevronRight className="size-3 text-faint" aria-hidden />}
            <span className={s.tone ?? "text-muted"}>{s.name}</span>
            {s.detail && <span className="text-faint">{s.detail}</span>}
          </li>
        ))}
      </ol>
      <span className="text-faint">
        {updatedAt
          ? `status polled ${new Date(updatedAt).toLocaleTimeString("en-GB", { hour12: false })}`
          : connection === "offline"
            ? "api unreachable"
            : "polling status…"}
      </span>
    </div>
  );
}
