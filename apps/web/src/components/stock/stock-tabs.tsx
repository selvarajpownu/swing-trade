"use client";

import { useId, useRef, useState, type KeyboardEvent } from "react";
import { Building2, FileText, Newspaper, Sparkles, type LucideIcon } from "lucide-react";

import { cn } from "@/lib/utils";

import { Card } from "@/components/ui/card";

interface TabDef {
  id: "news" | "filings" | "ai" | "fundamentals";
  label: string;
  icon: LucideIcon;
  blurb: string;
}

const TABS: readonly TabDef[] = [
  {
    id: "news",
    label: "News",
    icon: Newspaper,
    blurb: "Recent headlines for this stock from a real news source.",
  },
  {
    id: "filings",
    label: "Filings",
    icon: FileText,
    blurb: "Exchange filings and corporate announcements (results, board meetings, actions).",
  },
  {
    id: "ai",
    label: "AI summary",
    icon: Sparkles,
    blurb: "A short AI-written summary of the news and filings, with links to the sources.",
  },
  {
    id: "fundamentals",
    label: "Fundamentals",
    icon: Building2,
    blurb: "Key ratios and financials: valuation, growth, debt and shareholding.",
  },
];

/** Tabs below the chart. Content arrives in phase 4; no placeholder data is shown. */
export function StockTabs({ symbol }: { symbol: string }) {
  const [active, setActive] = useState(0);
  const baseId = useId();
  const tabRefs = useRef<(HTMLButtonElement | null)[]>([]);

  const focusTab = (index: number) => {
    const next = (index + TABS.length) % TABS.length;
    setActive(next);
    tabRefs.current[next]?.focus();
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "ArrowRight") {
      event.preventDefault();
      focusTab(active + 1);
    } else if (event.key === "ArrowLeft") {
      event.preventDefault();
      focusTab(active - 1);
    } else if (event.key === "Home") {
      event.preventDefault();
      focusTab(0);
    } else if (event.key === "End") {
      event.preventDefault();
      focusTab(TABS.length - 1);
    }
  };

  const tab = TABS[active] ?? TABS[0]!;
  const Icon = tab.icon;

  return (
    <Card>
      <div
        role="tablist"
        aria-label={`More about ${symbol}`}
        onKeyDown={onKeyDown}
        className="flex gap-1 overflow-x-auto border-b border-line px-2"
      >
        {TABS.map((t, i) => {
          const selected = i === active;
          return (
            <button
              key={t.id}
              ref={(el: HTMLButtonElement | null) => {
                tabRefs.current[i] = el;
              }}
              type="button"
              role="tab"
              id={`${baseId}-tab-${t.id}`}
              aria-selected={selected}
              aria-controls={`${baseId}-panel-${t.id}`}
              tabIndex={selected ? 0 : -1}
              onClick={() => setActive(i)}
              className={cn(
                "relative shrink-0 px-3 py-3 text-sm font-medium transition-colors",
                selected ? "text-fg" : "text-muted hover:text-fg",
              )}
            >
              {t.label}
              {selected ? (
                <span
                  aria-hidden="true"
                  className="absolute inset-x-2 bottom-0 h-0.5 rounded-full bg-accent"
                />
              ) : null}
            </button>
          );
        })}
      </div>
      <div
        role="tabpanel"
        id={`${baseId}-panel-${tab.id}`}
        aria-labelledby={`${baseId}-tab-${tab.id}`}
        tabIndex={0}
        className="flex flex-col items-center px-6 py-10 text-center focus-visible:outline-offset-[-2px]"
      >
        <span className="flex size-10 items-center justify-center rounded-xl bg-surface-3">
          <Icon aria-hidden="true" className="size-5 text-muted" />
        </span>
        <p className="mt-3 text-sm font-semibold">Coming in phase 4</p>
        <p className="mt-1 max-w-md text-sm text-muted">{tab.blurb}</p>
        <p className="mt-2 text-xs text-subtle">
          Nothing is shown here until a real source is connected.
        </p>
      </div>
    </Card>
  );
}
