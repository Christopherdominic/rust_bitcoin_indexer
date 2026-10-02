import type { Metadata } from "next";
import { truncateHash } from "@/lib/format";
import { AddressView } from "./AddressView";

export async function generateMetadata({ params }: PageProps<"/address/[address]">): Promise<Metadata> {
  const { address } = await params;
  return { title: `Address ${truncateHash(decodeURIComponent(address), 10, 6)}` };
}

export default async function AddressPage({ params }: PageProps<"/address/[address]">) {
  const { address } = await params;
  return <AddressView address={decodeURIComponent(address)} />;
}
