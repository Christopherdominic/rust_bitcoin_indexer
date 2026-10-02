import type { Utxo } from "@/lib/types";
import { EmptyState } from "./EmptyState";
import { ErrorState } from "./ErrorState";
import { HashDisplay } from "./HashDisplay";
import { TableSkeleton } from "./LoadingState";
import { Sats } from "./ui";

interface UtxoTableProps {
  utxos: Utxo[] | undefined;
  loading: boolean;
  error: unknown;
  onRetry?: () => void;
  emptyTitle: string;
  emptyDetail?: string;
}

export function UtxoTable({ utxos, loading, error, onRetry, emptyTitle, emptyDetail }: UtxoTableProps) {
  if (error) return <ErrorState error={error} onRetry={onRetry} compact />;
  if (!utxos) return loading ? <TableSkeleton rows={6} cols={4} /> : null;
  if (utxos.length === 0) return <EmptyState title={emptyTitle} detail={emptyDetail} />;

  return (
    <div className="overflow-x-auto">
      <table className="data-table">
        <thead>
          <tr>
            <th>Outpoint</th>
            <th className="text-right">Value</th>
            <th className="hidden lg:table-cell">scriptPubKey</th>
          </tr>
        </thead>
        <tbody>
          {utxos.map((u) => (
            <tr key={`${u.txid}:${u.vout}`}>
              <td className="whitespace-nowrap">
                <span className="inline-flex items-center">
                  <HashDisplay value={u.txid} href={`/tx/${u.txid}`} head={10} tail={10} className="text-[13px]" />
                  <span className="num text-[13px] text-muted">
                    <span className="text-faint">:</span>
                    {u.vout}
                  </span>
                </span>
              </td>
              <td className="text-right">
                <Sats value={u.value} btc className="text-[13px]" />
              </td>
              <td className="hidden max-w-0 w-[45%] lg:table-cell">
                <code className="block truncate font-mono text-xs text-faint" title={u.script_pubkey}>
                  {u.script_pubkey}
                </code>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
