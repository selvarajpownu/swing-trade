import { ExternalLink } from "lucide-react";

import type { NewsItem } from "@/lib/api/types";
import { formatTimeAgo } from "@/lib/format";
import { cn } from "@/lib/utils";

const SENTIMENT_TONE: Record<string, string> = {
  positive: "border-up/30 bg-up/10 text-up",
  negative: "border-down/30 bg-down/10 text-down",
  neutral: "border-line-strong bg-surface-3 text-muted",
};

function SentimentChip({ sentiment }: { sentiment: string }) {
  const tone = SENTIMENT_TONE[sentiment] ?? SENTIMENT_TONE.neutral;
  return (
    <span className={cn("rounded-full border px-2 py-px text-[11px] capitalize", tone)}>
      {sentiment}
    </span>
  );
}

export interface NewsRow extends NewsItem {
  /** Set on the Stock news page, where headlines span a whole watchlist. */
  symbol?: string;
  name?: string;
}

interface NewsListProps {
  items: readonly NewsRow[];
  /** Show the stock each headline was found for. */
  showSymbol?: boolean;
  /** Called to build the link for a symbol chip. */
  symbolHref?: (symbol: string) => string;
}

/** Compact headline rows: "Publisher · 3h ago", then the headline and summary. */
export function NewsList({ items, showSymbol = false, symbolHref }: NewsListProps) {
  return (
    <ul className="divide-y divide-line">
      {items.map((n, i) => (
        <li key={`${n.url ?? n.title}-${i}`} className="px-4 py-3 sm:px-5">
          <div className="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-muted">
            {showSymbol && n.symbol ? (
              symbolHref ? (
                <a
                  href={symbolHref(n.symbol)}
                  className="rounded bg-surface-3 px-1.5 py-0.5 font-medium text-fg hover:text-accent"
                  title={n.name}
                >
                  {n.symbol}
                </a>
              ) : (
                <span className="rounded bg-surface-3 px-1.5 py-0.5 font-medium text-fg">
                  {n.symbol}
                </span>
              )
            ) : null}
            {n.source ? <span className="font-medium text-fg/80">{n.source}</span> : null}
            {n.source && n.published_at ? <span aria-hidden="true">·</span> : null}
            {n.published_at ? (
              <time dateTime={n.published_at} suppressHydrationWarning>
                {formatTimeAgo(n.published_at)}
              </time>
            ) : null}
            {n.sentiment ? <SentimentChip sentiment={n.sentiment} /> : null}
          </div>
          {n.url ? (
            <a
              href={n.url}
              target="_blank"
              rel="noopener noreferrer"
              className="group mt-1 inline-flex items-start gap-1.5 text-sm leading-snug font-medium text-fg hover:text-accent"
            >
              {n.title}
              <ExternalLink
                aria-hidden="true"
                className="mt-0.5 size-3.5 shrink-0 opacity-50 group-hover:opacity-100"
              />
              <span className="sr-only">(opens in a new tab)</span>
            </a>
          ) : (
            <p className="mt-1 text-sm leading-snug font-medium text-fg">{n.title}</p>
          )}
          {n.summary ? (
            <p className="mt-1 line-clamp-3 text-sm leading-6 text-muted">{n.summary}</p>
          ) : null}
        </li>
      ))}
    </ul>
  );
}
