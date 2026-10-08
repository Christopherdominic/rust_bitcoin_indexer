"use client";

import Link from "next/link";
import { EmptyState } from "@/components/EmptyState";
import { ErrorState } from "@/components/ErrorState";
import { HashDisplay } from "@/components/HashDisplay";
import { Skeleton, TableSkeleton } from "@/components/LoadingState";
import { PageHeader, Panel, Sats, Tag } from "@/components/ui";
import { UtxoTable } from "@/components/UtxoTable";
import { WatchButton } from "@/components/WatchButton";
import { getAddress, getAddressTransactions, getAddressUtxos } from "@/lib/api";
import { formatBTC, formatNumber, formatRelative, formatSats, formatTimestamp, percent, truncateHash } from "@/lib/format";
import type { AddressDirection, AddressSummary, AddressTransaction } from "@/lib/types";
import { useApi } from "@/lib/use-api";

export function AddressView({ address }: { address: string }) {
  const summary = useApi(`addr:${address}`, () => getAddress(address));
  const utxos = useApi(`addr-utxos:${address}`, () => getAddressUtxos(address));
  const history = useApi(`addr-txs:${address}`, () => getAddressTransactions(address));

  if (summary.error) {
    return (
      <Panel>
        <ErrorState error={summary.error} subject={`Address ${truncateHash(address, 10, 6)}`} onRetry={summary.reload} />
      </Panel>
    );
  }

  return (
    <div className="space-y-6">
      <PageHeader
        kicker="Address"
        title={<HashDisplay value={address} mode="full" className="text-base font-normal sm:text-lg" />}
        actions={summary.data && <WatchButton address={address} />}
      >
        <span className="flex items-center gap-2">
          <Tag tone="btc">Regtest</Tag>
          <span className="text-xs">Totals derived from outputs the indexer has stored for this address.</span>
        </span>
      </PageHeader>

      <Balance summary={summary.data} />

      <Panel
        title="Unspent outputs"
        meta={utxos.data ? `${utxos.data.length} · ${formatSats(utxos.data.reduce((s, u) => s + u.value, 0))}` : undefined}
      >
        <UtxoTable
          utxos={utxos.data}
          loading={utxos.loading}
          error={utxos.error}
          onRetry={utxos.reload}
          emptyTitle="No unspent outputs"
          emptyDetail="Every indexed output paying to this address has been spent."
        />
      </Panel>

      <Panel title="Transactions" meta="Confirmed transactions paying to or spending from this address, with their effect on its balance">
        <HistoryTable rows={history.data} loading={history.loading} error={history.error} onRetry={history.reload} />
      </Panel>
    </div>
  );
}

function Balance({ summary }: { summary: AddressSummary | undefined }) {
  const s = summary;
  return (
    <section className="overflow-hidden rounded-md border border-line bg-surface" aria-label="Address totals">
      <div className="grid md:grid-cols-[minmax(0,1.3fr)_minmax(0,2fr)]">
        <div className="border-b border-line p-5 md:border-b-0 md:border-r">
          <p className="label">Balance</p>
          {s ? (
            <>
              <p className="mt-2 font-mono text-3xl tabular-nums tracking-tight text-btc">
                {formatBTC(s.balance, false)} <span className="text-base text-muted">BTC</span>
              </p>
              <p className="num mt-1 text-sm text-muted">{formatSats(s.balance)}</p>
            </>
          ) : (
            <div className="mt-2 space-y-2">
              <Skeleton className="h-8 w-48" />
              <Skeleton className="h-4 w-32" />
            </div>
          )}
        </div>

        <div className="grid grid-cols-1 divide-y divide-line sm:grid-cols-3 sm:divide-x sm:divide-y-0">
          <Figure label="Received">{s ? <Sats value={s.received} btc /> : null}</Figure>
          <Figure label="Spent">{s ? <Sats value={s.spent} btc /> : null}</Figure>
          <Figure label="Transactions">
            {s ? <span className="num text-fg">{formatNumber(s.transaction_count)}</span> : null}
          </Figure>
        </div>
      </div>

      {s && s.received > 0 && (
        <div className="border-t border-line px-5 py-3">
          <div className="flex h-1.5 overflow-hidden rounded-full bg-raised" aria-hidden>
            <div className="h-full bg-ok/70" style={{ width: `${(s.balance / s.received) * 100}%` }} />
            <div className="h-full bg-bad/60" style={{ width: `${(s.spent / s.received) * 100}%` }} />
          </div>
          <div className="mt-2 flex flex-wrap gap-x-5 gap-y-1 text-[11px] text-faint">
            <span className="flex items-center gap-1.5">
              <span className="size-1.5 rounded-full bg-ok/70" /> unspent{" "}
              <span className="num text-muted">{percent(s.balance, s.received)}</span>
            </span>
            <span className="flex items-center gap-1.5">
              <span className="size-1.5 rounded-full bg-bad/60" /> spent{" "}
              <span className="num text-muted">{percent(s.spent, s.received)}</span>
            </span>
            <span>of total received</span>
          </div>
        </div>
      )}
    </section>
  );
}

function Figure({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="p-5">
      <p className="label">{label}</p>
      <div className="mt-2 text-sm">{children ?? <Skeleton className="h-4 w-28" />}</div>
    </div>
  );
}

const directionTone: Record<AddressDirection, "ok" | "bad" | "neutral"> = {
  received: "ok",
  sent: "bad",
  self: "neutral",
};

function HistoryTable({
  rows,
  loading,
  error,
  onRetry,
}: {
  rows: AddressTransaction[] | undefined;
  loading: boolean;
  error: unknown;
  onRetry: () => void;
}) {
  if (error) return <ErrorState error={error} onRetry={onRetry} compact />;
  if (!rows) return loading ? <TableSkeleton rows={4} cols={4} /> : null;
  if (rows.length === 0) return <EmptyState title="No indexed transactions for this address" />;

  return (
    <div className="overflow-x-auto">
      <table className="data-table">
        <thead>
          <tr>
            <th>Transaction</th>
            <th className="w-24">Block</th>
            <th className="w-24">Direction</th>
            <th className="text-right">Balance change</th>
            <th className="hidden text-right md:table-cell">Timestamp</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={r.txid}>
              <td className="max-w-0 w-full">
                <HashDisplay value={r.txid} href={`/tx/${r.txid}`} head={10} tail={10} className="text-[13px]" />
              </td>
              <td>
                <Link href={`/block/${r.block_height}`} className="num text-btc hover:underline underline-offset-2">
                  {formatNumber(r.block_height)}
                </Link>
              </td>
              <td>
                <Tag tone={directionTone[r.direction]}>{r.direction}</Tag>
              </td>
              <td className="whitespace-nowrap text-right">
                <span className={`num block text-[13px] ${r.net > 0 ? "text-ok" : r.net < 0 ? "text-bad" : "text-muted"}`}>
                  {r.net > 0 ? "+" : r.net < 0 ? "−" : ""}
                  {formatSats(Math.abs(r.net))}
                </span>
                {r.received > 0 && r.sent > 0 && (
                  <span className="num block text-[11px] text-faint">
                    {formatSats(r.sent, false)} out · {formatSats(r.received, false)} back
                  </span>
                )}
              </td>
              <td className="hidden whitespace-nowrap text-right md:table-cell">
                <span className="num block text-[13px] text-muted">{formatTimestamp(r.timestamp)}</span>
                <span className="num block text-[11px] text-faint">{formatRelative(r.timestamp)}</span>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
