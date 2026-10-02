import { Inbox } from "lucide-react";

export function EmptyState({ title, detail }: { title: string; detail?: React.ReactNode }) {
  return (
    <div className="flex flex-col items-center gap-2 px-4 py-12 text-center">
      <Inbox className="size-5 text-faint" strokeWidth={1.5} />
      <p className="text-sm text-fg">{title}</p>
      {detail && <p className="max-w-sm text-xs text-muted">{detail}</p>}
    </div>
  );
}
