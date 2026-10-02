"use client";

import { CornerDownRight } from "lucide-react";
import Link from "next/link";
import { EmptyState } from "@/components/EmptyState";
import { ErrorState } from "@/components/ErrorState";
import { HashDisplay } from "@/components/HashDisplay";
import { DetailSkeleton, Skeleton } from "@/components/LoadingState";
import { DetailList, DetailRow, PageHeader, Panel, Sats, Script, SpendTag, Tag } from "@/components/ui";
import { getTransaction } from "@/lib/api";
import { formatNumber, formatSequence, formatTimestamp, truncateHash } from "@/lib/format";
import type { TransactionInput, TransactionOutput } from "@/lib/types";
import { useApi } from "@/lib/use-api";

const LOCKTIME_THRESHOLD = 500_000_000;

function describeLockTime(lockTime: number) {
  if (lockTime === 0) return "none";
  if (lockTime < LOCKTIME_THRESHOLD) return `block height ${formatNumber(lockTime)}`;
  return formatTimestamp(lockTime);
}

export function TransactionView({ txid }: { txid: string }) {
  const { data, error, reload } = useApi(`tx:${txid}`, () => getTransaction(txid));

  if (error) {
    return (
      <Panel>
        <ErrorState error={error} subject={`Transaction ${truncateHash(txid)}`} onRetry={reload} />
      </Panel>
    );
  }

  const tx = data?.transaction;
  const totalOut = data?.outputs.reduce((sum, o) => sum + o.value, 0) ?? 0;
  const unspentCount = data?.outputs.filter((o) => !o.spent).length ?? 0;

  return (
    <div className="space-y-6">
      <PageHeader
        kicker="Transaction"
        title={<HashDisplay value={txid} mode="full" className="text-base font-normal sm:text-lg" />}
      >
        {tx && (
          <span className="flex flex-wrap items-center gap-2">
            {tx.is_coinbase && <Tag tone="btc">Coinbase</Tag>}
            <span>
              Confirmed in block{" "}
              <Link href={`/block/${tx.block_height}`} className="num text-btc hover:underline underline-offset-2">
                #{formatNumber(tx.block_height)}
              </Link>{" "}
              at position <span className="num text-fg">{tx.position}</span>
            </span>
          </span>
        )}
      </PageHeader>

      <Panel title="Summary">
        {!data || !tx ? (
          <DetailSkeleton rows={6} />
        ) : (
          <div className="grid divide-y divide-line lg:grid-cols-2 lg:divide-x lg:divide-y-0">
            <DetailList>
              <DetailRow label="Block">
                <Link href={`/block/${tx.block_height}`} className="num text-btc hover:underline underline-offset-2">
                  {formatNumber(tx.block_height)}
                </Link>
              </DetailRow>
              <DetailRow label="Position">
                <span className="num">{tx.position}</span>
              </DetailRow>
              <DetailRow label="Version">
                <span className="num">{tx.version}</span>
              </DetailRow>
              <DetailRow label="Lock time">
                <span className="num">{tx.lock_time}</span>
                <span className="ml-2 text-xs text-faint">{describeLockTime(tx.lock_time)}</span>
              </DetailRow>
            </DetailList>
            <DetailList>
              <DetailRow label="Type">{tx.is_coinbase ? "Coinbase" : "Standard"}</DetailRow>
              <DetailRow label="Inputs / outputs">
                <span className="num">
                  {data.inputs.length} <span className="text-faint">→</span> {data.outputs.length}
                </span>
              </DetailRow>
              <DetailRow label="Total output">
                <Sats value={totalOut} btc />
              </DetailRow>
              <DetailRow label="Unspent outputs">
                <span className="num">
                  {unspentCount} <span className="text-faint">of {data.outputs.length}</span>
                </span>
              </DetailRow>
            </DetailList>
          </div>
        )}
      </Panel>

      <div className="grid items-start gap-6 lg:grid-cols-2">
        <Panel title="Inputs" meta={data ? `${data.inputs.length}` : undefined}>
          {!data ? (
            <IoSkeleton />
          ) : data.inputs.length === 0 ? (
            <EmptyState title="No inputs indexed" />
          ) : (
            <ol className="divide-y divide-line">
              {data.inputs.map((input) => (
                <InputRow key={input.vin} input={input} />
              ))}
            </ol>
          )}
        </Panel>

        <Panel title="Outputs" meta={data ? `${data.outputs.length}` : undefined}>
          {!data ? (
            <IoSkeleton />
          ) : data.outputs.length === 0 ? (
            <EmptyState title="No outputs indexed" />
          ) : (
            <ol className="divide-y divide-line">
              {data.outputs.map((output) => (
                <OutputRow key={output.vout} output={output} />
              ))}
            </ol>
          )}
        </Panel>
      </div>
    </div>
  );
}

function Index({ n }: { n: number }) {
  return <span className="num w-8 shrink-0 pt-px text-xs text-faint">#{n}</span>;
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="grid gap-1 sm:grid-cols-[6.5rem_1fr] sm:gap-3">
      <span className="label pt-px">{label}</span>
      <div className="min-w-0 text-[13px]">{children}</div>
    </div>
  );
}

function InputRow({ input }: { input: TransactionInput }) {
  const coinbase = input.prev_txid === null;
  return (
    <li className="flex gap-3 px-4 py-3.5">
      <Index n={input.vin} />
      <div className="min-w-0 flex-1 space-y-2">
        <Field label="Outpoint">
          {coinbase ? (
            <span className="text-btc">Coinbase — newly generated coins</span>
          ) : (
            <span className="flex min-w-0 items-center">
              <HashDisplay value={input.prev_txid!} href={`/tx/${input.prev_txid}`} head={10} tail={10} />
              <span className="num shrink-0 text-muted">:{input.prev_vout}</span>
            </span>
          )}
        </Field>
        <Field label="Sequence">
          <span className="num text-muted">{formatSequence(input.sequence)}</span>
        </Field>
        <Field label={coinbase ? "Coinbase" : "scriptSig"}>
          <Script value={input.script_sig} />
        </Field>
      </div>
    </li>
  );
}

function OutputRow({ output }: { output: TransactionOutput }) {
  return (
    <li className="flex gap-3 px-4 py-3.5">
      <Index n={output.vout} />
      <div className="min-w-0 flex-1 space-y-2">
        <div className="flex items-start justify-between gap-3">
          <Sats value={output.value} btc className="text-[13px]" />
          <SpendTag spent={output.spent} />
        </div>
        <Field label="scriptPubKey">
          <Script value={output.script_pubkey} />
        </Field>
        {output.spent && output.spent_by_txid && (
          <Field label="Spent by">
            <span className="flex min-w-0 items-center gap-1">
              <CornerDownRight className="size-3.5 shrink-0 text-faint" />
              <HashDisplay value={output.spent_by_txid} href={`/tx/${output.spent_by_txid}`} head={10} tail={10} />
              {output.spent_by_vin !== null && <span className="num shrink-0 text-muted">vin {output.spent_by_vin}</span>}
            </span>
          </Field>
        )}
      </div>
    </li>
  );
}

function IoSkeleton() {
  return (
    <div className="divide-y divide-line" role="status" aria-label="Loading">
      {[0, 1].map((i) => (
        <div key={i} className="flex gap-3 px-4 py-4">
          <Skeleton className="h-3 w-6" />
          <div className="flex-1 space-y-2.5">
            <Skeleton className="h-3.5 w-48" />
            <Skeleton className="h-3 w-full" />
            <Skeleton className="h-3 w-2/3" />
          </div>
        </div>
      ))}
    </div>
  );
}
