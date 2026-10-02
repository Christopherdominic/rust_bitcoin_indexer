import type { Metadata } from "next";
import { truncateHash } from "@/lib/format";
import { TransactionView } from "./TransactionView";

export async function generateMetadata({ params }: PageProps<"/tx/[txid]">): Promise<Metadata> {
  const { txid } = await params;
  return { title: `Tx ${truncateHash(decodeURIComponent(txid))}` };
}

export default async function TransactionPage({ params }: PageProps<"/tx/[txid]">) {
  const { txid } = await params;
  return <TransactionView txid={decodeURIComponent(txid)} />;
}
