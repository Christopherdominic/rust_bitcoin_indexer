import type { Metadata } from "next";
import { BlocksBrowser } from "./BlocksBrowser";

export const metadata: Metadata = { title: "Blocks" };

export default async function BlocksPage({ searchParams }: PageProps<"/blocks">) {
  const raw = (await searchParams).page;
  const page = Math.max(1, Number.parseInt(Array.isArray(raw) ? raw[0] : (raw ?? "1"), 10) || 1);
  return <BlocksBrowser page={page} />;
}
