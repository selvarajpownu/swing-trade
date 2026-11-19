import { ExternalLink } from "lucide-react";

import type { Catalyst, CatalystKind } from "@/lib/api/types";
import { cn } from "@/lib/utils";

import { Badge, type BadgeTone } from "@/components/ui/badge";

import { formatBarDate } from "./regime-banner";

const SHORT: Record<CatalystKind, string> = {
  results: "Results",
  dividend: "Dividend",
  bonus: "Bonus",
  split: "Split",
  buyback: "Buyback",
  demerger: "Demerger",
  rights: "Rights",
  fundraise: "Fund raising",
  order_win: "Order win",
  expansion: "Expansion",
  deal: "M&A",
  credit_rating: "Rating",
  management: "Resignation",
  legal: "Regulatory",
  exchange_query: "Exch. query",
  stake_change: "Stake",
  bulk_deal: "Bulk deal",
  block_deal: "Block deal",
  surveillance: "Surveillance",
  fno_ban: "F&O ban",
};

const TONE: Record<Catalyst["tone"], BadgeTone> = {
  positive: "up",
  negative: "down",
  neutral: "neutral",
  risk: "warn",
};

/** "in 3d", "today", "2d ago". */
export function relativeDays(days: number): string {
  if (days === 0) return "today";
  return days > 0 ? `in ${days}d` : `${-days}d ago`;
}

function label(c: Catalyst): string {
  const name = SHORT[c.kind] ?? c.kind;
  // Dated events read better with their distance; standing flags do not need one.
  if (c.kind === "surveillance" || c.kind === "fno_ban") return name;
  return `${name} ${relativeDays(c.days)}`;
}

function tooltip(c: Catalyst): string {
  return [`${formatBarDate(c.date)}: ${c.title}`, c.detail].filter(Boolean).join(". ");
}

interface CatalystBadgesProps {
  catalysts: readonly Catalyst[];
  /** Badges shown before "+N". */
  max?: number;
}

/** Compact badges, one per kind (the first of each kind as ordered by the API: risks, then newest). */
export function CatalystBadges({ catalysts, max = 4 }: CatalystBadgesProps) {
  const seen = new Set<CatalystKind>();
  const unique = catalysts.filter((c) => (seen.has(c.kind) ? false : (seen.add(c.kind), true)));
  if (unique.length === 0) return null;
  const rest = unique.length - max;
  return (
    <span className="flex flex-wrap gap-1">
      {unique.slice(0, max).map((c) => (
        <Badge key={c.kind} tone={TONE[c.tone]} title={tooltip(c)}>
          {label(c)}
        </Badge>
      ))}
      {rest > 0 ? (
        <Badge tone="muted" title={unique.slice(max).map(tooltip).join("\n")}>
          +{rest}
        </Badge>
      ) : null}
    </span>
  );
}

const DOT: Record<Catalyst["tone"], string> = {
  positive: "bg-up",
  negative: "bg-down",
  neutral: "bg-subtle",
  risk: "bg-warn",
};

/** Full list for the stock page: date, what happened, and a link to the filing. */
export function CatalystList({ catalysts }: { catalysts: readonly Catalyst[] }) {
  return (
    <ul className="divide-y divide-line">
      {catalysts.map((c, i) => (
        <li key={`${c.kind}-${c.date}-${i}`} className="flex gap-3 px-4 py-3 sm:px-5">
          <span
            aria-hidden="true"
            className={cn("mt-1.5 size-2 shrink-0 rounded-full", DOT[c.tone])}
          />
          <div className="min-w-0">
            <p className="text-xs text-muted">
              <time dateTime={c.date}>{formatBarDate(c.date)}</time>
              <span aria-hidden="true"> · </span>
              {relativeDays(c.days)}
            </p>
            {c.url ? (
              <a
                href={c.url}
                target="_blank"
                rel="noopener noreferrer"
                className="group mt-0.5 inline-flex items-start gap-1.5 text-sm font-medium text-fg hover:text-accent"
              >
                {c.title}
                <ExternalLink
                  aria-hidden="true"
                  className="mt-0.5 size-3.5 shrink-0 opacity-50 group-hover:opacity-100"
                />
                <span className="sr-only">(opens the filing in a new tab)</span>
              </a>
            ) : (
              <p className="mt-0.5 text-sm font-medium text-fg">{c.title}</p>
            )}
            {c.detail ? <p className="mt-0.5 text-sm leading-6 text-muted">{c.detail}</p> : null}
          </div>
        </li>
      ))}
    </ul>
  );
}
