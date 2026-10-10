"use client";

import { RotateCw } from "lucide-react";
import { Skeleton } from "@/components/LoadingState";
import { useIndexerStatus } from "@/components/StatusProvider";
import { PageHeader, Panel, Sats, ScriptTypeTag } from "@/components/ui";
import { UtxoTable } from "@/components/UtxoTable";
import { getUtxoSummary, getUtxos } from "@/lib/api";
import { formatNumber, percent } from "@/lib/format";
import type { OutputTotals } from "@/lib/types";
import { useApi } from "@/lib/use-api";

/** GET /api/utxos returns at most this many rows, newest first. */
const API_ROW_LIMIT = 100;

export function UtxoSet() {
  const { status } = useIndexerStatus();
  const { data, error, loading, reload } = useApi("utxos", getUtxos, { keepPrevious: true });

  // Totals cover every indexed output; refetch when the indexed tip moves.
  const tipKey = status?.indexed_height ?? "initial";
  const summary = useApi(`utxo-summary:${tipKey}`, getUtxoSummary, { keepPrevious: true });
  const s = summary.data;

  const capped = data !== undefined && data.length >= API_ROW_LIMIT;

  return (
    <div className="space-y-6">
      <PageHeader kicker="State" title="UTXO set">
        Unspent outputs that can ever be spent. Provably unspendable OP_RETURN outputs are not part of the set, and
        coinbase outputs are spendable after 100 confirmations.
      </PageHeader>

      <section className="card" aria-label="UTXO totals">
        <div className="grid sm:grid-cols-3">
          <Cell label="UTXOs" totals={s?.utxos} accent />
          <Cell label="Spendable now" totals={s?.spendable} />
          <Cell label="Immature coinbase" totals={s?.immature_coinbase} />
        </div>

        {s && s.utxos.count > 0 && (
          <div className="border-t border-line px-5 py-3">
            <div className="flex h-1.5 overflow-hidden rounded-full bg-raised" aria-hidden>
              <div className="h-full bg-ok/70" style={{ width: `${(s.spendable.count / s.utxos.count) * 100}%` }} />
              <div className="h-full bg-btc/60" style={{ width: `${(s.immature_coinbase.count / s.utxos.count) * 100}%` }} />
            </div>
            <div className="mt-3 flex flex-wrap items-center gap-x-5 gap-y-2 text-[11px] text-faint">
              <span className="flex items-center gap-1.5">
                <span className="size-1.5 rounded-full bg-ok/70" /> spendable{" "}
                <span className="num text-muted">{percent(s.spendable.count, s.utxos.count)}</span>
              </span>
              <span className="flex items-center gap-1.5">
                <span className="size-1.5 rounded-full bg-btc/60" /> immature{" "}
                <span className="num text-muted">{percent(s.immature_coinbase.count, s.utxos.count)}</span>
              </span>
              <span className="flex flex-wrap items-center gap-2 sm:ml-auto">
                {s.utxos_by_script_type.map((t) => (
                  <span key={t.script_type} className="flex items-center gap-1.5">
                    <ScriptTypeTag type={t.script_type} />
                    <span className="num text-muted">{formatNumber(t.count)}</span>
                  </span>
                ))}
              </span>
            </div>
          </div>
        )}

        {s && (
          <p className="border-t border-line px-5 py-2.5 font-mono text-[11px] text-faint">
            {formatNumber(s.outputs.count)} outputs indexed · {formatNumber(s.spent.count)} spent ·{" "}
            {formatNumber(s.provably_unspendable.count)} provably unspendable · {formatNumber(s.utxos.count)} unspent
          </p>
        )}
      </section>

      <Panel
        title={capped ? `Newest ${API_ROW_LIMIT} UTXOs` : "UTXOs"}
        meta={
          data
            ? capped
              ? `the API returns at most ${API_ROW_LIMIT} rows`
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

function Cell({ label, totals, accent = false }: { label: string; totals: OutputTotals | undefined; accent?: boolean }) {
  return (
    <div className="border-b border-line p-5 last:border-b-0 sm:border-b-0 sm:border-r sm:last:border-r-0">
      <p className="label">{label}</p>
      {totals ? (
        <>
          <p className={`num mt-2 text-2xl ${accent ? "text-btc" : "text-fg"}`}>{formatNumber(totals.count)}</p>
          <Sats value={totals.value} btc className="mt-1 text-xs" />
        </>
      ) : (
        <div className="mt-2 space-y-2">
          <Skeleton className="h-7 w-20" />
          <Skeleton className="h-3.5 w-32" />
        </div>
      )}
    </div>
  );
}
