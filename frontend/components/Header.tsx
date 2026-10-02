"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { NetworkStatus } from "./NetworkStatus";
import { SearchBar } from "./SearchBar";

const NAV = [
  { href: "/", label: "Dashboard", match: (p: string) => p === "/" },
  { href: "/blocks", label: "Blocks", match: (p: string) => p.startsWith("/block") },
  { href: "/utxos", label: "UTXOs", match: (p: string) => p.startsWith("/utxos") },
  { href: "/watch", label: "Watch", match: (p: string) => p.startsWith("/watch") },
];

export function Header() {
  const pathname = usePathname();
  const onDashboard = pathname === "/";

  return (
    <header className="sticky top-0 z-30 border-b border-line bg-bg/95 backdrop-blur-sm">
      <div className="mx-auto flex h-14 max-w-7xl items-center gap-6 px-4 sm:px-6">
        <Link href="/" className="flex shrink-0 items-center gap-2.5" aria-label="Amiable Indexer — dashboard">
          <span className="flex size-7 items-center justify-center rounded border border-btc/50 font-mono text-sm font-semibold text-btc">
            ₿
          </span>
          <span className="leading-none">
            <span className="block text-[13px] font-semibold tracking-[0.12em] text-fg">AMIABLE INDEXER</span>
            <span className="mt-1 hidden text-[11px] text-faint sm:block">Bitcoin Indexing Infrastructure</span>
          </span>
        </Link>

        <nav className="hidden h-full items-stretch md:flex" aria-label="Primary">
          {NAV.map((item) => (
            <NavLink key={item.href} href={item.href} active={item.match(pathname)}>
              {item.label}
            </NavLink>
          ))}
        </nav>

        <div className="ml-auto flex items-center gap-5">
          {!onDashboard && (
            <div className="relative hidden w-64 lg:block">
              <SearchBar size="sm" />
            </div>
          )}
          <NetworkStatus />
        </div>
      </div>

      {/* Mobile navigation */}
      <nav className="flex h-10 items-stretch overflow-x-auto border-t border-line px-2 md:hidden" aria-label="Primary">
        {NAV.map((item) => (
          <NavLink key={item.href} href={item.href} active={item.match(pathname)}>
            {item.label}
          </NavLink>
        ))}
      </nav>
    </header>
  );
}

function NavLink({ href, active, children }: { href: string; active: boolean; children: React.ReactNode }) {
  return (
    <Link
      href={href}
      aria-current={active ? "page" : undefined}
      className={`relative flex items-center px-3 text-[13px] transition-colors ${
        active ? "text-fg" : "text-muted hover:text-fg"
      }`}
    >
      {children}
      {active && <span className="absolute inset-x-3 -bottom-px h-px bg-btc" />}
    </Link>
  );
}
