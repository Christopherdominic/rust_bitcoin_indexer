import type { Metadata } from "next";
import { truncateHash } from "@/lib/format";
import { BlockView } from "./BlockView";

export async function generateMetadata({ params }: PageProps<"/block/[height]">): Promise<Metadata> {
  const { height } = await params;
  const id = decodeURIComponent(height);
  return { title: /^\d+$/.test(id) ? `Block ${id}` : `Block ${truncateHash(id)}` };
}

export default async function BlockPage({ params }: PageProps<"/block/[height]">) {
  const { height } = await params;
  return <BlockView id={decodeURIComponent(height)} />;
}
