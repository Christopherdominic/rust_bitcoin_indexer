"use client";

import { Activity, ChevronDown, LoaderCircle, Plus, Trash2 } from "lucide-react";
import Link from "next/link";
import { useState } from "react";
import { EmptyState } from "@/components/EmptyState";
import { ErrorState } from "@/components/ErrorState";
import { HashDisplay } from "@/components/HashDisplay";
import { Skeleton, TableSkeleton } from "@/components/LoadingState";
import { PageHeader, Panel, Sats } from "@/components/ui";
import { getWatchActivity, getWatchedAddresses, isApiError, unwatchAddress, watchAddress } from "@/lib/api";
import { formatNumber } from "@/lib/format";
import { useApi } from "@/lib/use-api";

function describeError(err: unknown, notFound: string) {
  if (isApiError(err, "not_found")) return notFound;
  if (isApiError(err, "unreachable")) return "Indexer unavailable.";
  return err instanceof Error ? err.message : "Request failed.";
}

export function WatchList() {
  const { data, error, loading, reload } = useApi("watchlist", getWatchedAddresses, { keepPrevious: true });

  return (
    <div className="space-y-6">
      <PageHeader kicker="Address watch" title="Watch list">
        A stored list of addresses. Activity is queried on demand from the indexed outputs — nothing is pushed.
      </PageHeader>

      <AddForm onAdded={reload} />

      <Panel title="Watched addresses" meta={data ? `${data.length}` : undefined}>
        {error ? (
          <ErrorState error={error} onRetry={reload} compact />
        ) : !data ? (
          loading && <TableSkeleton rows={3} cols={2} />
        ) : data.length === 0 ? (
          <EmptyState
            title="No watched addresses"
            detail="Add an indexed address above, or use Watch address on any address page."
          />
        ) : (
          <ul className="divide-y divide-line">
            {data.map((w) => (
              <WatchRow key={w.address} address={w.address} onRemoved={reload} />
            ))}
          </ul>
        )}
      </Panel>
    </div>
  );
}

function AddForm({ onAdded }: { onAdded: () => void }) {
  const [value, setValue] = useState("");
  const [pending, setPending] = useState(false);
  const [message, setMessage] = useState<{ tone: "ok" | "bad"; text: string } | null>(null);

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    const address = value.trim();
    if (!address || pending) return;
    setPending(true);
    setMessage(null);
    try {
      await watchAddress(address);
      setValue("");
      setMessage({ tone: "ok", text: "Address added to watch list." });
      onAdded();
    } catch (err) {
      setMessage({
        tone: "bad",
        text: describeError(err, "The indexer has no outputs for this address. Only indexed addresses can be watched."),
      });
    } finally {
      setPending(false);
    }
  }

  return (
    <form onSubmit={submit} className="space-y-2">
      <div className="flex flex-col gap-2 sm:flex-row">
        <input
          value={value}
          onChange={(e) => {
            setValue(e.target.value);
            setMessage(null);
          }}
          placeholder="bcrt1… address to watch"
          aria-label="Address to watch"
          spellCheck={false}
          autoComplete="off"
          className="h-9 min-w-0 flex-1 rounded border border-line-strong bg-surface px-3 font-mono text-sm text-fg outline-none placeholder:font-sans placeholder:text-faint hover:border-faint focus:border-btc/70"
        />
        <button type="submit" className="btn btn-accent h-9 justify-center" disabled={pending || !value.trim()}>
          {pending ? <LoaderCircle className="size-3.5 animate-spin" /> : <Plus className="size-3.5" />}
          Watch address
        </button>
      </div>
      {message && (
        <p role="status" className={`text-xs ${message.tone === "ok" ? "text-ok" : "text-bad"}`}>
          {message.text}
        </p>
      )}
    </form>
  );
}

function WatchRow({ address, onRemoved }: { address: string; onRemoved: () => void }) {
  const [open, setOpen] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [removing, setRemoving] = useState(false);
  const [removeError, setRemoveError] = useState<string | null>(null);

  async function remove() {
    setRemoving(true);
    setRemoveError(null);
    try {
      await unwatchAddress(address);
      onRemoved();
    } catch (err) {
      setRemoveError(describeError(err, "Already removed."));
      setRemoving(false);
      setConfirming(false);
    }
  }

  return (
    <li className={removing ? "opacity-50" : ""}>
      <div className="flex flex-col gap-3 px-4 py-3 sm:flex-row sm:items-center">
        <div className="min-w-0 flex-1">
          <HashDisplay value={address} href={`/address/${address}`} head={14} tail={10} className="text-[13px]" />
        </div>
        <div className="flex items-center gap-2">
          <button type="button" className="btn" onClick={() => setOpen((o) => !o)} aria-expanded={open}>
            <Activity className="size-3.5" />
            View activity
            <ChevronDown className={`size-3.5 transition-transform ${open ? "rotate-180" : ""}`} />
          </button>
          {confirming ? (
            <>
              <button type="button" className="btn btn-danger" onClick={remove} disabled={removing}>
                {removing ? <LoaderCircle className="size-3.5 animate-spin" /> : <Trash2 className="size-3.5" />}
                Confirm
              </button>
              <button type="button" className="btn" onClick={() => setConfirming(false)} disabled={removing}>
                Cancel
              </button>
            </>
          ) : (
            <button type="button" className="btn" onClick={() => setConfirming(true)}>
              <Trash2 className="size-3.5" /> Remove
            </button>
          )}
        </div>
      </div>
      {removeError && <p className="px-4 pb-3 text-xs text-bad">{removeError}</p>}
      {open && <ActivityPanel address={address} />}
    </li>
  );
}

function ActivityPanel({ address }: { address: string }) {
  const { data, error, reload } = useApi(`watch-activity:${address}`, () => getWatchActivity(address));

  return (
    <div className="mx-4 mb-4 overflow-hidden rounded border border-line">
      {error ? (
        <ErrorState error={error} subject="Watched address" onRetry={reload} compact />
      ) : (
        <dl className="grid grid-cols-2 gap-px bg-line md:grid-cols-4">
          <Stat label="Balance">{data && <Sats value={data.balance} btc />}</Stat>
          <Stat label="Received">{data && <Sats value={data.received} btc />}</Stat>
          <Stat label="Transactions">{data && <span className="num">{formatNumber(data.transaction_count)}</span>}</Stat>
          <Stat label="Latest activity">
            {data &&
              (data.latest_txid ? (
                <span className="flex flex-col gap-0.5">
                  <HashDisplay value={data.latest_txid} href={`/tx/${data.latest_txid}`} head={8} tail={6} className="text-[13px]" />
                  {data.latest_block_height !== null && (
                    <Link href={`/block/${data.latest_block_height}`} className="num text-xs text-muted hover:text-btc">
                      block {formatNumber(data.latest_block_height)}
                    </Link>
                  )}
                </span>
              ) : (
                <span className="text-faint">—</span>
              ))}
          </Stat>
        </dl>
      )}
    </div>
  );
}

function Stat({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="min-w-0 bg-bg p-4">
      <dt className="label">{label}</dt>
      <dd className="mt-1.5 text-[13px]">{children ?? <Skeleton className="h-4 w-24" />}</dd>
    </div>
  );
}
