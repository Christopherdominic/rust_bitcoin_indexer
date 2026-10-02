"use client";

import { RotateCw } from "lucide-react";
import { Skeleton } from "@/components/LoadingState";
import { useIndexerStatus } from "@/components/StatusProvider";
import { PageHeader, Panel, Sats } from "@/components/ui";
import { UtxoTable } from "@/components/UtxoTable";
import { getUtxos } from "@/lib/api";
import { formatNumber, percent } from "@/lib/format";
import { useApi } from "@/lib/use-api";

/** GET /api/utxos returns at most this many rows, newest first. */
const API_ROW_LIMIT = 100;

export function UtxoSet() {
  const { status } = useIndexerStatus();
  const { data, error, loading, reload } = useApi("utxos", getUtxos, { keepPrevious: true });

  const shownValue = data?.reduce((sum, u) => sum + u.value, 0);
  const capped = data !== undefined && data.length >= API_ROW_LIMIT;

  return (
    <div className="space-y-6">
      <PageHeader kicker="State" title="UTXO set">
        Unspent outputs tracked by the indexer. Outputs are marked spent as inputs referencing them are indexed.
      </PageHeader>

      <section className="grid overflow-hidden rounded-md border border-line bg-surface sm:grid-cols-3" aria-label="UTXO totals">
        <Cell label="Unspent outputs">
          {status ? <span className="num text-2xl text-btc">{formatNumber(status.unspent_outputs)}</span> : <Skeleton className="h-7 w-20" />}
        </Cell>
        <Cell label="Spent outputs">
          {status ? (
            <span className="num text-2xl text-fg">
              {formatNumber(status.outputs - status.unspent_outputs)}
              <span className="ml-2 text-xs text-faint">{percent(status.outputs - status.unspent_outputs, status.outputs)} of outputs</span>
            </span>
          ) : (
            <Skeleton className="h-7 w-20" />
          )}
        </Cell>
        <Cell label={capped ? `Value of ${API_ROW_LIMIT} newest` : "Value shown"}>
          {shownValue !== undefined ? <Sats value={shownValue} btc /> : <Skeleton className="h-7 w-32" />}
        </Cell>
      </section>

      <Panel
        title="Unspent outputs"
        meta={
          data
            ? capped
              ? `Newest ${API_ROW_LIMIT} by creation — the API caps this list at ${API_ROW_LIMIT} rows`
              : `${formatNumber(data.length)} rows, newest first`
            : undefined
        }
        actions={
          <button type="button" className="btn" onClick={reload} disabled={loading}>
            <RotateCw className={`size-3.5 ${loading && data ? "animate-spin" : ""}`} /> Refresh
          </button>
        }
      >
        <UtxoTable
          utxos={data}
          loading={loading}
          error={error}
          onRetry={reload}
          emptyTitle="No unspent outputs"
          emptyDetail="The indexer has not recorded any unspent outputs."
        />
      </Panel>
    </div>
  );
}

function Cell({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="border-b border-line p-5 last:border-b-0 sm:border-b-0 sm:border-r sm:last:border-r-0">
      <p className="label">{label}</p>
      <div className="mt-2">{children}</div>
    </div>
  );
}
