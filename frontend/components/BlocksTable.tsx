"use client";

import Link from "next/link";
import { formatNumber, formatRelative, formatTimestamp } from "@/lib/format";
import type { BlockSummary } from "@/lib/types";
import { EmptyState } from "./EmptyState";
import { ErrorState } from "./ErrorState";
import { HashDisplay } from "./HashDisplay";
import { TableSkeleton } from "./LoadingState";

interface BlocksTableProps {
  blocks: BlockSummary[] | undefined;
  loading: boolean;
  error: unknown;
  onRetry?: () => void;
  /** include the previous-hash column (wide screens only) */
  showPrevious?: boolean;
  skeletonRows?: number;
  /** dims the table while a new page loads over stale rows */
  stale?: boolean;
}

export function BlocksTable({
  blocks,
  loading,
  error,
  onRetry,
  showPrevious = false,
  skeletonRows = 10,
  stale = false,
}: BlocksTableProps) {
  if (error) return <ErrorState error={error} onRetry={onRetry} compact />;
  if (!blocks) return loading ? <TableSkeleton rows={skeletonRows} cols={4} /> : null;
  if (blocks.length === 0) return <EmptyState title="No blocks in this range" detail="The indexer has not stored any blocks for this page." />;

  return (
    <div className={`overflow-x-auto transition-opacity ${stale ? "opacity-50" : ""}`}>
      <table className="data-table">
        <thead>
          <tr>
            <th className="w-28">Height</th>
            <th>Hash</th>
            {showPrevious && <th className="hidden xl:table-cell">Previous hash</th>}
            <th className="text-right">Txs</th>
            <th className="hidden text-right sm:table-cell">Timestamp</th>
          </tr>
        </thead>
        <tbody>
          {blocks.map((b) => (
            <tr key={b.hash}>
              <td>
                <Link href={`/block/${b.height}`} className="num text-btc hover:underline underline-offset-2">
                  {formatNumber(b.height)}
                </Link>
                <span className="num block whitespace-nowrap text-[11px] text-faint sm:hidden">{formatRelative(b.timestamp)}</span>
              </td>
              <td className="max-w-0 w-full">
                <HashDisplay value={b.hash} href={`/block/${b.height}`} head={8} tail={8} className="text-[13px] [&_a]:text-fg [&_a:hover]:text-btc" />
              </td>
              {showPrevious && (
                <td className="hidden xl:table-cell">
                  <HashDisplay value={b.previous_hash} copy={false} head={8} tail={8} className="text-[13px] text-faint" />
                </td>
              )}
              <td className="num text-right text-muted">{formatNumber(b.tx_count)}</td>
              <td className="hidden whitespace-nowrap text-right sm:table-cell">
                <span className="num block text-[13px] text-muted">{formatTimestamp(b.timestamp)}</span>
                <span className="num block text-[11px] text-faint">{formatRelative(b.timestamp)}</span>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
