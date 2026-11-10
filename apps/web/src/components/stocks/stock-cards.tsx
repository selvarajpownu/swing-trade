import Link from "next/link";
import { ChevronRight } from "lucide-react";

import type { PlanParams } from "@/lib/api/params";
import type { StockListItem } from "@/lib/api/types";
import { formatINR } from "@/lib/format";
import { stockHref } from "@/lib/links";

import { PriceChange } from "@/components/ui/price-change";
import { SymbolAvatar } from "@/components/ui/symbol-avatar";

import { SetupBadge } from "./setup-badge";
import { Sparkline } from "./sparkline";

interface StockCardsProps {
  rows: StockListItem[];
  plan: PlanParams;
}

/** Compact list for small screens (below md). */
export function StockCards({ rows, plan }: StockCardsProps) {
  return (
    <ul className="divide-y divide-line md:hidden" aria-label="Stocks">
      {rows.map((item) => (
        <li key={item.symbol}>
          <Link
            href={stockHref(item.symbol, plan)}
            className="flex items-center gap-3 px-4 py-3 transition-colors hover:bg-surface-2 focus-visible:bg-surface-2"
          >
            <SymbolAvatar symbol={item.symbol} name={item.name} />
            <div className="min-w-0 flex-1">
              <div className="flex items-baseline justify-between gap-2">
                <span className="truncate font-semibold text-fg">{item.name}</span>
                <span className="shrink-0 font-semibold tabular-nums">{formatINR(item.price)}</span>
              </div>
              <div className="mt-0.5 flex items-center justify-between gap-2">
                <span className="text-xs text-muted">{item.symbol}</span>
                <PriceChange change={item.change} changePct={item.change_pct} className="text-xs" />
              </div>
              <div className="mt-2 flex items-center justify-between gap-2">
                <SetupBadge setup={item.setup} />
                <Sparkline values={item.sparkline} width={56} height={18} />
              </div>
            </div>
            <ChevronRight aria-hidden="true" className="size-4 shrink-0 text-subtle" />
          </Link>
        </li>
      ))}
    </ul>
  );
}
