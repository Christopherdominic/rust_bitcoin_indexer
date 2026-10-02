import type { Metadata } from "next";
import { WatchList } from "./WatchList";

export const metadata: Metadata = { title: "Watch" };

export default function WatchPage() {
  return <WatchList />;
}
