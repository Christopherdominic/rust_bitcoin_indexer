"use client";

import { useCallback, useEffect, useRef, useState } from "react";

interface Settled<T> {
  key: string;
  data: T | undefined;
  error: unknown;
}

export interface ApiState<T> {
  data: T | undefined;
  error: unknown;
  /** true until the first response for the current key arrives */
  loading: boolean;
  reload: () => void;
}

/**
 * Minimal fetch-on-key hook. `key` identifies the request; when it changes the
 * fetcher runs again. Pass `null` to skip. With `keepPrevious`, data from the
 * previous key stays visible while the next one loads (used for pagination
 * and live refresh so tables don't flash to skeletons).
 */
export function useApi<T>(
  key: string | null,
  fetcher: () => Promise<T>,
  { keepPrevious = false }: { keepPrevious?: boolean } = {},
): ApiState<T> {
  const fetcherRef = useRef(fetcher);
  useEffect(() => {
    fetcherRef.current = fetcher;
  });

  const [settled, setSettled] = useState<Settled<T> | null>(null);
  const [nonce, setNonce] = useState(0);

  useEffect(() => {
    if (key === null) return;
    let cancelled = false;
    fetcherRef.current().then(
      (data) => !cancelled && setSettled({ key, data, error: null }),
      (error: unknown) => !cancelled && setSettled({ key, data: undefined, error }),
    );
    return () => {
      cancelled = true;
    };
  }, [key, nonce]);

  const reload = useCallback(() => setNonce((n) => n + 1), []);

  const current = settled?.key === key;
  const visible = current || keepPrevious ? settled : null;

  return {
    data: visible?.data,
    error: current ? settled.error : null,
    loading: key !== null && !current,
    reload,
  };
}
