"use client";

import { createContext, useContext, useEffect, useState } from "react";
import { getStatus } from "@/lib/api";
import type { IndexerStatus } from "@/lib/types";

const POLL_MS = 10_000;

export type Connection = "connecting" | "live" | "offline";

interface StatusContextValue {
  status: IndexerStatus | null;
  error: unknown;
  connection: Connection;
  /** epoch ms of the last successful poll */
  updatedAt: number | null;
}

const StatusContext = createContext<StatusContextValue>({
  status: null,
  error: null,
  connection: "connecting",
  updatedAt: null,
});

/**
 * Polls GET /api/status for the whole app. The header's LIVE indicator and the
 * dashboard both read from here, so "live" always reflects a real response.
 */
export function StatusProvider({ children }: { children: React.ReactNode }) {
  const [value, setValue] = useState<StatusContextValue>({
    status: null,
    error: null,
    connection: "connecting",
    updatedAt: null,
  });

  useEffect(() => {
    let cancelled = false;
    let inFlight = false;

    const poll = () => {
      if (inFlight || document.hidden) return;
      inFlight = true;
      getStatus()
        .then(
          (status) =>
            !cancelled &&
            setValue({ status, error: null, connection: "live", updatedAt: Date.now() }),
          (error: unknown) =>
            !cancelled && setValue((prev) => ({ ...prev, error, connection: "offline" })),
        )
        .finally(() => {
          inFlight = false;
        });
    };

    poll();
    const timer = setInterval(poll, POLL_MS);
    document.addEventListener("visibilitychange", poll);
    return () => {
      cancelled = true;
      clearInterval(timer);
      document.removeEventListener("visibilitychange", poll);
    };
  }, []);

  return <StatusContext.Provider value={value}>{children}</StatusContext.Provider>;
}

export function useIndexerStatus() {
  return useContext(StatusContext);
}
