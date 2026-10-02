import type { Metadata } from "next";
import { UtxoSet } from "./UtxoSet";

export const metadata: Metadata = { title: "UTXOs" };

export default function UtxosPage() {
  return <UtxoSet />;
}
