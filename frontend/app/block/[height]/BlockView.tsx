"use client";

import { ChevronLeft, ChevronRight } from "lucide-react";
import Link from "next/link";
import { EmptyState } from "@/components/EmptyState";
import { ErrorState } from "@/components/ErrorState";
import { HashDisplay } from "@/components/HashDisplay";
import { DetailSkeleton, TableSkeleton } from "@/components/LoadingState";
import { useIndexerStatus } from "@/components/StatusProvider";
import { DetailList, DetailRow, PageHeader, Panel, Tag } from "@/components/ui";
import { getBlock, getBlockByHash } from "@/lib/api";
import { formatNumber, formatRelative, formatTimestamp, truncateHash } from "@/lib/format";
import { useApi } from "@/lib/use-api";

const GENESIS_PREV = "0".repeat(64);

/** `id` is a block height, or a block hash (search returns hashes for hash matches). */
export function BlockView({ id }: { id: string }) {
  const isHeight = /^\d+$/.test(id);
  const { status } = useIndexerStatus();
  const { data: block, error, loading, reload } = useApi(`block:${id}`, () =>
    isHeight ? getBlock(Number(id)) : getBlockByHash(id),
  );

  const subject = isHeight ? `Block ${id}` : `Block ${truncateHash(id)}`;

  if (error) {
    return (
      <Panel>
        <ErrorState error={error} subject={subject} onRetry={reload} />
      </Panel>
    );
  }

  const tip = status?.indexed_height ?? null;
  const height = block?.height;

  return (
    <div className="space-y-6">
      <PageHeader
        kicker="Block"
        title={
          height !== undefined ? (
            <span className="num">#{formatNumber(height)}</span>
          ) : (
            <span className="num text-muted">{isHeight ? `#${formatNumber(Number(id))}` : "…"}</span>
          )
        }
        actions={
          height !== undefined && (
            <>
              <StepLink href={height > 0 ? `/block/${height - 1}` : null} label="Previous block">
                <ChevronLeft className="size-3.5" /> {height > 0 ? formatNumber(height - 1) : "—"}
              </StepLink>
              <StepLink href={tip !== null && height < tip ? `/block/${height + 1}` : null} label="Next block">
                {tip !== null && height < tip ? formatNumber(height + 1) : "tip"} <ChevronRight className="size-3.5" />
              </StepLink>
            </>
          )
        }
      >
        {block && tip !== null && (
          <span>
            {tip - block.height === 0 ? (
              <Tag tone="btc">Chain tip</Tag>
            ) : (
              <>
                <span className="num text-fg">{formatNumber(tip - block.height)}</span> blocks below the indexed tip
              </>
            )}
          </span>
        )}
      </PageHeader>

      <Panel title="Header">
        {!block || loading ? (
          <DetailSkeleton rows={5} />
        ) : (
          <DetailList>
            <DetailRow label="Height">
              <span className="num">{formatNumber(block.height)}</span>
            </DetailRow>
            <DetailRow label="Hash">
              <HashDisplay value={block.hash} mode="full" className="text-[13px]" />
            </DetailRow>
            <DetailRow label="Previous block">
              {block.previous_hash === GENESIS_PREV ? (
                <span className="font-mono text-[13px] text-faint">none (genesis)</span>
              ) : (
                <HashDisplay
                  value={block.previous_hash}
                  href={block.height > 0 ? `/block/${block.height - 1}` : undefined}
                  mode="full"
                  className="text-[13px]"
                />
              )}
            </DetailRow>
            <DetailRow label="Timestamp">
              <span className="num">{formatTimestamp(block.timestamp)}</span>
              <span className="num ml-2 text-xs text-faint">
                {formatRelative(block.timestamp)} · unix {block.timestamp}
              </span>
            </DetailRow>
            <DetailRow label="Transactions">
              <span className="num">{formatNumber(block.tx_count)}</span>
            </DetailRow>
          </DetailList>
        )}
      </Panel>

      <Panel title="Transactions" meta={block ? `${formatNumber(block.transactions.length)} indexed, by position` : undefined}>
        {!block ? (
          <TableSkeleton rows={3} cols={2} />
        ) : block.transactions.length === 0 ? (
          <EmptyState title="No transactions indexed for this block" />
        ) : (
          <div className="overflow-x-auto">
            <table className="data-table">
              <thead>
                <tr>
                  <th className="w-20">Pos</th>
                  <th>Transaction ID</th>
                  <th className="w-24 text-right">Type</th>
                </tr>
              </thead>
              <tbody>
                {block.transactions.map((tx) => (
                  <tr key={tx.txid}>
                    <td className="num text-muted">{tx.position}</td>
                    <td className="max-w-0 w-full">
                      <HashDisplay value={tx.txid} href={`/tx/${tx.txid}`} head={12} tail={12} className="text-[13px]" />
                    </td>
                    <td className="text-right">{tx.position === 0 && <Tag tone="btc">Coinbase</Tag>}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </Panel>
    </div>
  );
}

function StepLink({ href, label, children }: { href: string | null; label: string; children: React.ReactNode }) {
  if (!href) {
    return (
      <span className="btn num pointer-events-none opacity-40" aria-disabled="true">
        {children}
      </span>
    );
  }
  return (
    <Link href={href} className="btn num" aria-label={label}>
      {children}
    </Link>
  );
}
