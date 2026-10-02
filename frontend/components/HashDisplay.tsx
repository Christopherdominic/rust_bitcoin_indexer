import Link from "next/link";
import { truncateHash } from "@/lib/format";
import { CopyButton } from "./CopyButton";

interface HashDisplayProps {
  value: string;
  href?: string;
  /** "short": middle-truncated; "full": wraps across lines, never overflows */
  mode?: "short" | "full";
  head?: number;
  tail?: number;
  copy?: boolean;
  className?: string;
}

/**
 * Renders hashes, txids and addresses in monospace with safe truncation,
 * the full value in a tooltip, and an optional copy button.
 */
export function HashDisplay({
  value,
  href,
  mode = "short",
  head = 8,
  tail = 8,
  copy = true,
  className = "",
}: HashDisplayProps) {
  const short = mode === "short";
  const textClass = short ? "whitespace-nowrap" : "break-all [overflow-wrap:anywhere]";
  // Short mode tightens to 6…6 below the sm breakpoint so rows fit on phones.
  const text =
    short && (head > 6 || tail > 6) ? (
      <>
        <span className="sm:hidden">{truncateHash(value, Math.min(head, 6), Math.min(tail, 6))}</span>
        <span className="hidden sm:inline">{truncateHash(value, head, tail)}</span>
      </>
    ) : short ? (
      truncateHash(value, head, tail)
    ) : (
      value
    );

  return (
    <span className={`inline-flex min-w-0 max-w-full items-center gap-1 font-mono ${className}`}>
      {href ? (
        <Link href={href} title={value} className={`${textClass} min-w-0 text-btc hover:underline underline-offset-2`}>
          {text}
        </Link>
      ) : (
        <span title={value} className={`${textClass} min-w-0`}>
          {text}
        </span>
      )}
      {copy && <CopyButton value={value} />}
    </span>
  );
}
