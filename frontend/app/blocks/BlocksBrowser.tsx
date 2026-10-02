"use client";

import { BlocksTable } from "@/components/BlocksTable";
import { Pagination } from "@/components/Pagination";
import { useIndexerStatus } from "@/components/StatusProvider";
import { PageHeader, Panel } from "@/components/ui";
import { getBlocks } from "@/lib/api";
import { formatNumber } from "@/lib/format";
import { useApi } from "@/lib/use-api";

const PAGE_SIZE = 25;

export function BlocksBrowser({ page }: { page: number }) {
  const { status } = useIndexerStatus();
  const offset = (page - 1) * PAGE_SIZE;
  const { data, error, loading, reload } = useApi(`blocks:${page}`, () => getBlocks(PAGE_SIZE, offset), {
    keepPrevious: true,
  });

  const totalPages = status ? Math.max(1, Math.ceil(status.blocks / PAGE_SIZE)) : null;
  const rows = loading ? undefined : data;
  const hasNext = rows ? rows.length === PAGE_SIZE && (totalPages === null || page < totalPages) : false;
  const range =
    rows && rows.length > 0 ? `${formatNumber(rows[0].height)} → ${formatNumber(rows[rows.length - 1].height)}` : null;

  const pagination = (
    <Pagination page={page} totalPages={totalPages} hasNext={hasNext} hrefFor={(p) => (p === 1 ? "/blocks" : `/blocks?page=${p}`)} />
  );

  return (
    <div className="space-y-6">
      <PageHeader kicker="Chain" title="Blocks">
        {status ? (
          <>
            <span className="num text-fg">{formatNumber(status.blocks)}</span> blocks indexed, newest first.
          </>
        ) : (
          "Indexed blocks, newest first."
        )}
      </PageHeader>

      <Panel title={range ? <span className="num">Heights {range}</span> : "Heights"} meta={`${PAGE_SIZE} per page`} actions={pagination}>
        <BlocksTable
          blocks={data}
          loading={loading}
          error={error}
          onRetry={reload}
          showPrevious
          skeletonRows={12}
          stale={loading && data !== undefined}
        />
      </Panel>

      {rows && rows.length > 0 && <div className="flex justify-end">{pagination}</div>}
    </div>
  );
}
