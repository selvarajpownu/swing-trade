"use client";

import { useCallback, useMemo, useState } from "react";
import { RefreshCw, Search, X } from "lucide-react";

import { fetchStocks, stocksUrl } from "@/lib/api/client";
import { describeApiError } from "@/lib/api/errors";
import type { PlanParams } from "@/lib/api/params";
import type { StocksResponse } from "@/lib/api/types";
import { formatDateTimeIST } from "@/lib/format";
import {
  applyListView,
  countByFilter,
  defaultDirFor,
  FILTERS,
  writeListView,
  type ListView,
  type SortKey,
} from "@/lib/stock-list-view";
import { cn } from "@/lib/utils";
import { usePolling } from "@/hooks/use-polling";

import { useReportDataSource, type SymbolEntry } from "@/components/layout/data-source";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/states";

import { StockCards } from "./stock-cards";
import { StockTable } from "./stock-table";

interface StockListProps {
  initialData: StocksResponse;
  initialView: ListView;
  plan: PlanParams;
}

export function StockList({ initialData, initialView, plan }: StockListProps) {
  const url = stocksUrl(plan);
  const fetcher = useCallback((signal: AbortSignal) => fetchStocks(url, signal), [url]);
  const { data, error, isRefreshing, refresh } = usePolling(fetcher, { initialData });

  const [view, setView] = useState<ListView>(initialView);

  const symbols = useMemo<SymbolEntry[]>(
    () => data.items.map((i) => ({ symbol: i.symbol, name: i.name, exchange: i.exchange })),
    [data.items],
  );
  useReportDataSource(data.provider, symbols);

  const rows = useMemo(() => applyListView(data.items, view), [data.items, view]);
  const counts = useMemo(() => countByFilter(data.items), [data.items]);

  /** Updates state and mirrors it into the URL without a server round-trip. */
  const updateView = useCallback(
    (patch: Partial<ListView>) => {
      const next: ListView = { ...view, ...patch };
      setView(next);
      // Next.js integrates native history.replaceState with its router (no refetch).
      const query = new URLSearchParams(window.location.search);
      writeListView(next, query);
      const qs = query.toString();
      window.history.replaceState(
        null,
        "",
        qs ? `${window.location.pathname}?${qs}` : window.location.pathname,
      );
    },
    [view],
  );

  const onSort = useCallback(
    (key: SortKey) => {
      updateView(
        view.sort === key
          ? { dir: view.dir === "asc" ? "desc" : "asc" }
          : { sort: key, dir: defaultDirFor(key) },
      );
    },
    [updateView, view.sort, view.dir],
  );

  const hasActiveView = view.q.trim() !== "" || view.filter !== "all";

  return (
    <div className="space-y-4">
      {/* Toolbar: search + filter chips */}
      <div className="flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
        <div className="relative w-full lg:max-w-xs">
          <label htmlFor="stock-search" className="sr-only">
            Search stocks by name or symbol
          </label>
          <Search
            aria-hidden="true"
            className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-subtle"
          />
          <input
            id="stock-search"
            type="search"
            value={view.q}
            onChange={(e) => updateView({ q: e.target.value })}
            placeholder="Search name or symbol"
            autoComplete="off"
            spellCheck={false}
            className="h-10 w-full rounded-lg border border-line bg-surface pr-9 pl-9 text-sm text-fg placeholder:text-subtle focus:border-accent focus:outline-none"
          />
          {view.q ? (
            <button
              type="button"
              onClick={() => updateView({ q: "" })}
              aria-label="Clear search"
              className="absolute top-1/2 right-2 -translate-y-1/2 rounded p-1 text-subtle hover:text-fg"
            >
              <X aria-hidden="true" className="size-3.5" />
            </button>
          ) : null}
        </div>

        <div
          role="group"
          aria-label="Filter stocks"
          className="-mx-4 flex gap-2 overflow-x-auto px-4 pb-1 lg:mx-0 lg:px-0 lg:pb-0"
        >
          {FILTERS.map((filter) => {
            const active = view.filter === filter.id;
            return (
              <button
                key={filter.id}
                type="button"
                aria-pressed={active}
                onClick={() => updateView({ filter: filter.id })}
                className={cn(
                  "flex shrink-0 items-center gap-1.5 rounded-full border px-3 py-1.5 text-xs font-medium transition-colors",
                  active
                    ? "border-accent/50 bg-accent/15 text-fg"
                    : "border-line bg-surface text-muted hover:border-line-strong hover:text-fg",
                )}
              >
                {filter.label}
                <span className={cn("tabular-nums", active ? "text-accent" : "text-subtle")}>
                  {counts[filter.id]}
                </span>
              </button>
            );
          })}
        </div>
      </div>

      {/* Status line */}
      <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted">
        <p aria-live="polite" suppressHydrationWarning>
          {rows.length} of {data.items.length} stocks · as of {formatDateTimeIST(data.as_of)}
          <span className="hidden sm:inline"> · refreshes every 15s</span>
        </p>
        <div className="flex items-center gap-2">
          {error ? (
            <span role="status" className="text-warn">
              Refresh failed: {describeApiError(error)} Showing last data.
            </span>
          ) : null}
          <Button
            variant="ghost"
            onClick={() => void refresh()}
            disabled={isRefreshing}
            aria-label="Refresh now"
            className="px-2 py-1 text-xs"
          >
            <RefreshCw
              aria-hidden="true"
              className={cn("size-3.5", isRefreshing && "motion-safe:animate-spin")}
            />
            <span className="hidden sm:inline">Refresh</span>
          </Button>
        </div>
      </div>

      <Card className="overflow-hidden">
        {data.items.length === 0 ? (
          <EmptyState
            title="No stocks in the watchlist"
            description="The API returned an empty list. Set WATCHLIST on the API server to choose which stocks to scan."
          />
        ) : rows.length === 0 ? (
          <EmptyState
            title="No stocks match"
            description={
              view.q.trim()
                ? `Nothing matches “${view.q.trim()}” with the current filter.`
                : "No stock has this setup right now."
            }
            action={
              hasActiveView ? (
                <Button onClick={() => updateView({ q: "", filter: "all" })}>Clear filters</Button>
              ) : null
            }
          />
        ) : (
          <>
            <StockTable rows={rows} view={view} onSort={onSort} plan={plan} />
            <StockCards rows={rows} plan={plan} />
          </>
        )}
      </Card>
    </div>
  );
}
