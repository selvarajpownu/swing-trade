import Link from "next/link";

import type { PlanParams } from "@/lib/api/params";
import type { Interval } from "@/lib/api/types";
import { stockHref } from "@/lib/links";
import { cn } from "@/lib/utils";

const OPTIONS: { value: Interval; label: string; title: string }[] = [
  { value: "1d", label: "1D", title: "Daily candles" },
  { value: "1w", label: "1W", title: "Weekly candles" },
  { value: "15m", label: "15m", title: "15-minute candles" },
];

interface IntervalToggleProps {
  symbol: string;
  interval: Interval;
  plan: PlanParams;
}

/** Segmented control backed by the `interval` search param. */
export function IntervalToggle({ symbol, interval, plan }: IntervalToggleProps) {
  return (
    <nav
      aria-label="Chart interval"
      className="inline-flex rounded-lg border border-line bg-surface-2 p-0.5"
    >
      {OPTIONS.map((option) => {
        const active = option.value === interval;
        return (
          <Link
            key={option.value}
            href={stockHref(symbol, plan, option.value)}
            scroll={false}
            title={option.title}
            aria-current={active ? "true" : undefined}
            className={cn(
              "rounded-md px-3 py-1 text-xs font-medium transition-colors",
              active ? "bg-surface-3 text-fg shadow-sm" : "text-muted hover:text-fg",
            )}
          >
            {option.label}
          </Link>
        );
      })}
    </nav>
  );
}
