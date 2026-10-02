"use client";

import { ArrowRight } from "lucide-react";
import Link from "next/link";
import { BlocksTable } from "@/components/BlocksTable";
import { ErrorState } from "@/components/ErrorState";
import { SearchBar } from "@/components/SearchBar";
import { StatsCards } from "@/components/StatsCards";
import { useIndexerStatus } from "@/components/StatusProvider";
import { Panel } from "@/components/ui";
import { getBlocks } from "@/lib/api";
import type { BlockSummary, IndexerStatus } from "@/lib/types";
import { type ApiState, useApi } from "@/lib/use-api";

const LATEST = 10;

export default function DashboardPage() {
  const { status, error: statusError, connection, updatedAt } = useIndexerStatus();

  // Re-fetch latest blocks whenever the indexed tip moves.
  const tipKey = status?.indexed_height ?? "initial";
  const blocks = useApi(`latest:${tipKey}`, () => getBlocks(LATEST, 0), { keepPrevious: true });

  const unreachable = connection === "offline" && !status;

  return (
    <div className="space-y-6">
      <SearchBar />

      {unreachable ? (
        <Panel>
          <ErrorState error={statusError} />
        </Panel>
      ) : (
        <>
          <StatsCards status={status} connection={connection} updatedAt={updatedAt} tip={blocks.data?.[0]} />
          <LatestBlocks status={status} blocks={blocks} />
        </>
      )}
    </div>
  );
}

function LatestBlocks({
  status,
  blocks,
}: {
  status: IndexerStatus | null;
  blocks: ApiState<BlockSummary[]>;
}) {
  return (
    <Panel
      title="Latest blocks"
      meta={status?.indexed_height != null ? "refreshes when the indexed tip advances" : undefined}
      actions={
        <Link href="/blocks" className="flex items-center gap-1 text-xs text-muted hover:text-btc">
          All blocks <ArrowRight className="size-3.5" />
        </Link>
      }
    >
      <BlocksTable
        blocks={blocks.data}
        loading={blocks.loading}
        error={blocks.data ? null : blocks.error}
        onRetry={blocks.reload}
        skeletonRows={LATEST}
      />
    </Panel>
  );
}
