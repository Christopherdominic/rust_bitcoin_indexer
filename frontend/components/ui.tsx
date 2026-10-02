import { formatBTC, formatSats } from "@/lib/format";

/** Bordered section with a header bar. The primary structural surface. */
export function Panel({
  title,
  meta,
  actions,
  children,
  className = "",
}: {
  title?: React.ReactNode;
  meta?: React.ReactNode;
  actions?: React.ReactNode;
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <section className={`overflow-hidden rounded-md border border-line bg-surface ${className}`}>
      {(title || actions) && (
        <header className="flex min-h-11 flex-wrap items-center justify-between gap-x-4 gap-y-2 border-b border-line px-4 py-2">
          <div className="flex min-w-0 items-baseline gap-3">
            {title && <h2 className="text-[13px] font-semibold text-fg">{title}</h2>}
            {meta && <span className="truncate text-xs text-faint">{meta}</span>}
          </div>
          {actions && <div className="flex items-center gap-2">{actions}</div>}
        </header>
      )}
      {children}
    </section>
  );
}

/** Label/value rows for detail pages. */
export function DetailList({ children }: { children: React.ReactNode }) {
  return <dl className="divide-y divide-line">{children}</dl>;
}

export function DetailRow({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="grid gap-1 px-4 py-3 sm:grid-cols-[11rem_1fr] sm:gap-4">
      <dt className="label pt-0.5">{label}</dt>
      <dd className="min-w-0 text-sm text-fg">{children}</dd>
    </div>
  );
}

/** Page title block: small kicker + title, optional right-aligned actions. */
export function PageHeader({
  kicker,
  title,
  children,
  actions,
}: {
  kicker: string;
  title: React.ReactNode;
  children?: React.ReactNode;
  actions?: React.ReactNode;
}) {
  return (
    <div className="flex flex-wrap items-end justify-between gap-4">
      <div className="min-w-0">
        <p className="label">{kicker}</p>
        <h1 className="mt-1 text-xl font-semibold tracking-tight text-fg">{title}</h1>
        {children && <div className="mt-1.5 text-sm text-muted">{children}</div>}
      </div>
      {actions && <div className="flex flex-wrap items-center gap-2">{actions}</div>}
    </div>
  );
}

/** Satoshi amount with optional BTC equivalent underneath. */
export function Sats({ value, btc = false, className = "" }: { value: number; btc?: boolean; className?: string }) {
  return (
    <span className={`inline-flex flex-col leading-tight ${className}`}>
      <span className="num whitespace-nowrap text-fg">
        {formatSats(value, false)} <span className="text-faint">sats</span>
      </span>
      {btc && <span className="num whitespace-nowrap text-xs text-faint">{formatBTC(value)}</span>}
    </span>
  );
}

type Tone = "ok" | "bad" | "btc" | "neutral";

const toneClass: Record<Tone, string> = {
  ok: "border-ok/30 bg-ok-soft text-ok",
  bad: "border-bad/30 bg-bad-soft text-bad",
  btc: "border-btc/30 bg-btc-soft text-btc",
  neutral: "border-line-strong text-muted",
};

/** Small square-cornered status tag (UNSPENT / SPENT / COINBASE …). */
export function Tag({ tone = "neutral", children }: { tone?: Tone; children: React.ReactNode }) {
  return (
    <span
      className={`inline-flex h-5 items-center whitespace-nowrap rounded-sm border px-1.5 font-mono text-[10px] font-medium uppercase tracking-wider ${toneClass[tone]}`}
    >
      {children}
    </span>
  );
}

export function SpendTag({ spent }: { spent: boolean }) {
  return spent ? <Tag tone="bad">Spent</Tag> : <Tag tone="ok">Unspent</Tag>;
}

/** Monospace script (ASM) block that wraps instead of overflowing. */
export function Script({ value }: { value: string | null }) {
  if (!value) return <span className="text-faint">—</span>;
  return (
    <code className="block break-all font-mono text-xs leading-relaxed text-muted [overflow-wrap:anywhere]">
      {value}
    </code>
  );
}
