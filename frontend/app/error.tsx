"use client";

import { AlertTriangle, RotateCw } from "lucide-react";

export default function Error({ error, retry }: { error: Error & { digest?: string }; retry: () => void }) {
  return (
    <section
      role="alert"
      className="flex flex-col items-center gap-2 rounded-md border border-line bg-surface px-4 py-16 text-center"
    >
      <AlertTriangle className="size-5 text-bad" strokeWidth={1.5} />
      <p className="text-sm font-medium text-fg">Something went wrong rendering this view</p>
      <p className="max-w-md font-mono text-xs text-muted">{error.digest ?? error.message}</p>
      <button type="button" onClick={() => retry()} className="btn mt-2">
        <RotateCw className="size-3.5" /> Try again
      </button>
    </section>
  );
}
