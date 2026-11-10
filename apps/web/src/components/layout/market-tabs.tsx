import Link from "next/link";

import type { Market } from "@/lib/api/types";
import { MARKET_ORDER, MARKETS } from "@/lib/markets";
import { cn } from "@/lib/utils";

interface MarketTabsProps {
  active: Market;
  /** Link for each tab; defaults to the market's list page. */
  hrefFor?: (market: Market) => string;
  label?: string;
}

/** A strip of market tabs (India · US · Commodities) above market pages. */
export function MarketTabs({ active, hrefFor, label = "Markets" }: MarketTabsProps) {
  return (
    <nav aria-label={label} className="-mx-4 overflow-x-auto px-4 lg:mx-0 lg:px-0">
      <ul className="flex gap-1 border-b border-line">
        {MARKET_ORDER.map((m) => {
          const selected = m === active;
          return (
            <li key={m}>
              <Link
                href={hrefFor ? hrefFor(m) : MARKETS[m].path}
                aria-current={selected ? "page" : undefined}
                className={cn(
                  "relative block px-3 py-2.5 text-sm font-medium whitespace-nowrap transition-colors",
                  selected ? "text-fg" : "text-muted hover:text-fg",
                )}
              >
                {MARKETS[m].tab}
                {selected ? (
                  <span
                    aria-hidden="true"
                    className="absolute inset-x-2 bottom-0 h-0.5 rounded-full bg-accent"
                  />
                ) : null}
              </Link>
            </li>
          );
        })}
      </ul>
    </nav>
  );
}
