import { AlertTriangle, FileQuestion, PlugZap, RotateCw } from "lucide-react";
import Link from "next/link";
import { API_URL, isApiError } from "@/lib/api";

interface ErrorStateProps {
  error: unknown;
  /** what was being looked up, used for 404 copy, e.g. "Block 900" */
  subject?: string;
  onRetry?: () => void;
  compact?: boolean;
}

export function ErrorState({ error, subject, onRetry, compact = false }: ErrorStateProps) {
  let Icon = AlertTriangle;
  let title = "Request failed";
  let detail: React.ReactNode = error instanceof Error ? error.message : "Unexpected error.";

  if (isApiError(error, "unreachable")) {
    Icon = PlugZap;
    title = "Indexer unavailable";
    detail = (
      <>
        Unable to reach the Amiable Indexer API at <code className="font-mono text-fg">{API_URL}</code>.
      </>
    );
  } else if (isApiError(error, "not_found")) {
    Icon = FileQuestion;
    title = subject ? `${subject} not found` : "Not found";
    detail = "Nothing matching this was found in the indexed chain data.";
  } else if (isApiError(error, "config")) {
    title = "API not configured";
    detail = (
      <>
        Set <code className="font-mono text-fg">NEXT_PUBLIC_API_URL</code> in <code className="font-mono text-fg">.env.local</code> and restart.
      </>
    );
  } else if (isApiError(error, "malformed")) {
    title = "Unexpected API response";
  }

  return (
    <div className={`flex flex-col items-center gap-2 px-4 text-center ${compact ? "py-8" : "py-14"}`} role="alert">
      <Icon className={`size-5 ${isApiError(error, "not_found") ? "text-faint" : "text-bad"}`} strokeWidth={1.5} />
      <p className="text-sm font-medium text-fg">{title}</p>
      <p className="max-w-md text-xs leading-relaxed text-muted">{detail}</p>
      <div className="mt-2 flex items-center gap-2">
        {onRetry && !isApiError(error, "not_found") && (
          <button type="button" onClick={onRetry} className="btn">
            <RotateCw className="size-3.5" /> Retry
          </button>
        )}
        {isApiError(error, "not_found") && (
          <Link href="/" className="btn">
            Back to dashboard
          </Link>
        )}
      </div>
    </div>
  );
}
