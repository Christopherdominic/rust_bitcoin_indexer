"use client";

import { CornerDownLeft, LoaderCircle, Search } from "lucide-react";
import { useRouter } from "next/navigation";
import { useEffect, useId, useRef, useState } from "react";
import { getBlockByHash, isApiError, search } from "@/lib/api";
import type { SearchResult } from "@/lib/types";

const PLACEHOLDER = "Search block height, block hash, transaction ID or address";

async function resolveRoute({ result_type, value }: SearchResult): Promise<string> {
  switch (result_type) {
    case "block":
      // Search returns the hash (not the height) when matched by block hash.
      if (/^\d+$/.test(value)) return `/block/${value}`;
      return `/block/${(await getBlockByHash(value)).height}`;
    case "transaction":
      return `/tx/${value}`;
    case "address":
      return `/address/${value}`;
  }
}

export function SearchBar({ size = "lg" }: { size?: "lg" | "sm" }) {
  const router = useRouter();
  const inputRef = useRef<HTMLInputElement>(null);
  const errorId = useId();
  const [query, setQuery] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // "/" focuses the search, as in most developer tools.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement;
      if (e.key !== "/" || target.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName)) return;
      if (!inputRef.current || inputRef.current.offsetParent === null) return;
      e.preventDefault();
      inputRef.current.focus();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  async function onSubmit(e: React.FormEvent) {
    e.preventDefault();
    const q = query.trim();
    if (!q || pending) return;
    setPending(true);
    setError(null);
    try {
      const route = await resolveRoute(await search(q));
      setQuery("");
      inputRef.current?.blur();
      router.push(route);
    } catch (err) {
      if (isApiError(err, "not_found")) {
        setError(`No indexed block, transaction or address matches “${q.length > 72 ? `${q.slice(0, 72)}…` : q}”.`);
      } else if (isApiError(err, "unreachable")) {
        setError("Indexer unavailable — search could not reach the API.");
      } else {
        setError(err instanceof Error ? err.message : "Search failed.");
      }
    } finally {
      setPending(false);
    }
  }

  const lg = size === "lg";

  return (
    <form onSubmit={onSubmit} role="search" className="w-full">
      <div
        className={`group flex items-center gap-3 rounded-md border bg-surface transition-colors focus-within:border-btc/70 ${
          error ? "border-bad/60" : "border-line-strong hover:border-faint"
        } ${lg ? "h-12 px-4" : "h-8 px-2.5"}`}
      >
        {pending ? (
          <LoaderCircle className={`shrink-0 animate-spin text-btc ${lg ? "size-4" : "size-3.5"}`} />
        ) : (
          <Search className={`shrink-0 text-faint group-focus-within:text-btc ${lg ? "size-4" : "size-3.5"}`} />
        )}
        <input
          ref={inputRef}
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            if (error) setError(null);
          }}
          placeholder={lg ? PLACEHOLDER : "Search height, hash, txid, address"}
          aria-label={PLACEHOLDER}
          aria-invalid={error ? true : undefined}
          aria-describedby={error ? errorId : undefined}
          spellCheck={false}
          autoComplete="off"
          className={`min-w-0 flex-1 bg-transparent font-mono text-fg outline-none placeholder:font-sans placeholder:text-faint ${
            lg ? "text-sm" : "text-xs"
          }`}
        />
        {lg && (
          <span className="hidden items-center gap-1 text-[11px] text-faint sm:flex">
            {query ? (
              <>
                <CornerDownLeft className="size-3" /> search
              </>
            ) : (
              <kbd className="rounded-sm border border-line-strong px-1.5 font-mono">/</kbd>
            )}
          </span>
        )}
      </div>
      {error && (
        <p
          id={errorId}
          role="alert"
          className={`text-xs text-bad ${
            lg ? "mt-2" : "absolute right-0 top-full z-20 mt-1.5 w-72 rounded border border-bad/40 bg-surface px-2.5 py-2"
          }`}
        >
          {error}
        </p>
      )}
    </form>
  );
}
