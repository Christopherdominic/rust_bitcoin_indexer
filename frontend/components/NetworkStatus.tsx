"use client";

import { formatNumber } from "@/lib/format";
import { useIndexerStatus } from "./StatusProvider";

const STATES = {
  connecting: { label: "CONNECTING", dot: "bg-faint", text: "text-muted" },
  live: { label: "LIVE", dot: "bg-ok", text: "text-ok" },
  offline: { label: "OFFLINE", dot: "bg-bad", text: "text-bad" },
} as const;

/** Network badge + API connection state, both from real /api/status polling. */
export function NetworkStatus() {
  const { connection, status } = useIndexerStatus();
  const s = STATES[connection];
  const tip = status?.indexed_height;
  // Only what Bitcoin Core reports; hidden while it is unknown or unreachable.
  const network = status?.network;

  return (
    <div className="flex items-center gap-3 font-mono text-[11px] tracking-wider">
      {network && (
        <span className="rounded-sm border border-btc/40 px-1.5 py-0.5 text-btc" title={`Bitcoin network reported by Bitcoin Core: ${network}`}>
          {network.toUpperCase()}
        </span>
      )}
      <span
        className={`flex items-center gap-1.5 ${s.text}`}
        title={connection === "live" ? "Indexer API responding" : connection === "offline" ? "Indexer API unreachable" : "Contacting indexer API"}
      >
        <span className="relative flex size-1.5">
          {connection === "live" && (
            <span className="absolute inline-flex size-full animate-ping rounded-full bg-ok opacity-50 [animation-duration:2.5s]" />
          )}
          <span className={`relative inline-flex size-1.5 rounded-full ${s.dot}`} />
        </span>
        {s.label}
      </span>
      {connection === "live" && tip != null && (
        <span className="hidden text-muted sm:inline" title="Indexed chain tip">
          #{formatNumber(tip)}
        </span>
      )}
    </div>
  );
}
