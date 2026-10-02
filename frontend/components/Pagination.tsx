import { ChevronLeft, ChevronRight } from "lucide-react";
import Link from "next/link";
import { formatNumber } from "@/lib/format";

interface PaginationProps {
  page: number;
  totalPages: number | null;
  hasNext: boolean;
  hrefFor: (page: number) => string;
}

export function Pagination({ page, totalPages, hasNext, hrefFor }: PaginationProps) {
  return (
    <nav className="flex items-center gap-2" aria-label="Pagination">
      <PageLink href={page > 1 ? hrefFor(page - 1) : null} label="Previous page">
        <ChevronLeft className="size-3.5" /> Previous
      </PageLink>
      <span className="num px-2 text-xs text-muted">
        {formatNumber(page)}
        {totalPages !== null && <span className="text-faint"> / {formatNumber(totalPages)}</span>}
      </span>
      <PageLink href={hasNext ? hrefFor(page + 1) : null} label="Next page">
        Next <ChevronRight className="size-3.5" />
      </PageLink>
    </nav>
  );
}

function PageLink({ href, label, children }: { href: string | null; label: string; children: React.ReactNode }) {
  if (!href) {
    return (
      <span className="btn pointer-events-none opacity-40" aria-disabled="true" aria-label={label}>
        {children}
      </span>
    );
  }
  return (
    <Link href={href} className="btn" aria-label={label}>
      {children}
    </Link>
  );
}
