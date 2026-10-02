import { FileQuestion } from "lucide-react";
import Link from "next/link";

export default function NotFound() {
  return (
    <section className="flex flex-col items-center gap-2 rounded-md border border-line bg-surface px-4 py-16 text-center">
      <FileQuestion className="size-5 text-faint" strokeWidth={1.5} />
      <p className="text-sm font-medium text-fg">Page not found</p>
      <p className="text-xs text-muted">This route does not exist in the explorer.</p>
      <Link href="/" className="btn mt-2">
        Back to dashboard
      </Link>
    </section>
  );
}
