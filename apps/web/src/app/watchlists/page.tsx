import type { Metadata } from "next";
import { Bookmark } from "lucide-react";

import { ComingSoon } from "@/components/ui/coming-soon";

export const metadata: Metadata = { title: "Watchlists" };

export default function WatchlistsPage() {
  return (
    <ComingSoon
      icon={Bookmark}
      title="Watchlists"
      description="Create and switch between your own lists of stocks. For now the stock list uses the WATCHLIST configured on the API server."
    />
  );
}
