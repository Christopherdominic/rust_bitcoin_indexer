import type { Metadata } from "next";
import { Geist, Geist_Mono } from "next/font/google";
import { Header } from "@/components/Header";
import { StatusProvider } from "@/components/StatusProvider";
import "./globals.css";

const geistSans = Geist({
  variable: "--font-geist-sans",
  subsets: ["latin"],
});

const geistMono = Geist_Mono({
  variable: "--font-geist-mono",
  subsets: ["latin"],
});

export const metadata: Metadata = {
  title: {
    default: "Amiable Indexer",
    template: "%s · Amiable Indexer",
  },
  description: "Bitcoin indexing infrastructure — explorer for blocks, transactions, UTXOs and watched addresses indexed by Amiable.",
};

export default function RootLayout({ children }: LayoutProps<"/">) {
  return (
    <html lang="en" className={`${geistSans.variable} ${geistMono.variable} h-full antialiased`}>
      <body className="flex min-h-full flex-col">
        <StatusProvider>
          <Header />
          <main className="mx-auto w-full max-w-7xl flex-1 px-4 py-6 sm:px-6 sm:py-8">{children}</main>
          <footer className="border-t border-line">
            <div className="mx-auto flex max-w-7xl flex-wrap items-center justify-between gap-2 px-4 py-4 text-[11px] text-faint sm:px-6">
              <span>Amiable Indexer · Bitcoin Core → Rust indexer → PostgreSQL → Axum REST API</span>
              <span className="font-mono">regtest</span>
            </div>
          </footer>
        </StatusProvider>
      </body>
    </html>
  );
}
