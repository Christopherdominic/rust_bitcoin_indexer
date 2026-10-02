"use client";

import { Eye, EyeOff, LoaderCircle } from "lucide-react";
import Link from "next/link";
import { useState } from "react";
import { getWatchActivity, isApiError, unwatchAddress, watchAddress } from "@/lib/api";
import { useApi } from "@/lib/use-api";

/**
 * Watch-list toggle for an address. Current state comes from
 * GET /api/watch/{address} (200 = watched, 404 = not watched).
 */
export function WatchButton({ address }: { address: string }) {
  const initial = useApi(`watching:${address}`, () =>
    getWatchActivity(address).then(
      () => true,
      (err: unknown) => {
        if (isApiError(err, "not_found")) return false;
        throw err;
      },
    ),
  );

  const [override, setOverride] = useState<boolean | null>(null);
  const [pending, setPending] = useState(false);
  const [message, setMessage] = useState<{ tone: "ok" | "bad"; text: string } | null>(null);

  const watching = override ?? initial.data;

  async function toggle() {
    setPending(true);
    setMessage(null);
    try {
      if (watching) {
        await unwatchAddress(address);
        setOverride(false);
        setMessage({ tone: "ok", text: "Removed from watch list." });
      } else {
        await watchAddress(address);
        setOverride(true);
        setMessage({ tone: "ok", text: "Added to watch list." });
      }
    } catch (err) {
      setMessage({
        tone: "bad",
        text: isApiError(err, "unreachable")
          ? "Indexer unavailable."
          : isApiError(err, "not_found")
            ? watching
              ? "Address was not on the watch list."
              : "Address has no indexed outputs."
            : err instanceof Error
              ? err.message
              : "Request failed.",
      });
    } finally {
      setPending(false);
    }
  }

  if (initial.loading && override === null) {
    return (
      <button type="button" className="btn" disabled>
        <LoaderCircle className="size-3.5 animate-spin" /> Watch address
      </button>
    );
  }

  return (
    <div className="flex flex-col items-start gap-1.5 sm:items-end">
      <div className="flex items-center gap-2">
        {watching && (
          <Link href="/watch" className="flex items-center gap-1.5 font-mono text-[11px] tracking-wider text-ok">
            <span className="size-1.5 rounded-full bg-ok" /> WATCHING
          </Link>
        )}
        <button
          type="button"
          onClick={toggle}
          disabled={pending}
          className={`btn ${watching ? "" : "btn-accent"}`}
        >
          {pending ? (
            <LoaderCircle className="size-3.5 animate-spin" />
          ) : watching ? (
            <EyeOff className="size-3.5" />
          ) : (
            <Eye className="size-3.5" />
          )}
          {watching ? "Unwatch" : "Watch address"}
        </button>
      </div>
      {message ? (
        <p role="status" className={`text-xs ${message.tone === "ok" ? "text-muted" : "text-bad"}`}>
          {message.text}
        </p>
      ) : (
        initial.error != null && <p className="text-xs text-bad">Could not read watch state.</p>
      )}
    </div>
  );
}
