export function Skeleton({ className = "" }: { className?: string }) {
  return <span aria-hidden className={`block rounded-sm bg-raised animate-[pulse_2.4s_ease-in-out_infinite] ${className}`} />;
}

/** Skeleton rows matching DataTable density. */
export function TableSkeleton({ rows = 8, cols = 4 }: { rows?: number; cols?: number }) {
  return (
    <div role="status" aria-label="Loading" className="divide-y divide-line">
      {Array.from({ length: rows }, (_, r) => (
        <div key={r} className="flex items-center gap-6 px-4 py-3">
          {Array.from({ length: cols }, (_, c) => (
            <Skeleton
              key={c}
              className={`h-3.5 ${c === 0 ? "w-14" : c === 1 ? "flex-1 max-w-72" : "hidden w-20 sm:block"}`}
            />
          ))}
        </div>
      ))}
    </div>
  );
}

/** Skeleton for label/value detail lists. */
export function DetailSkeleton({ rows = 5 }: { rows?: number }) {
  return (
    <div role="status" aria-label="Loading" className="divide-y divide-line">
      {Array.from({ length: rows }, (_, i) => (
        <div key={i} className="grid gap-2 px-4 py-3 sm:grid-cols-[11rem_1fr]">
          <Skeleton className="h-3 w-24" />
          <Skeleton className={`h-3.5 ${i % 2 ? "w-40" : "w-full max-w-md"}`} />
        </div>
      ))}
    </div>
  );
}
