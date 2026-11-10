"use client";

import { useCallback, useMemo, useState } from "react";
import { RefreshCw } from "lucide-react";

import { fetchScan } from "@/lib/api/client";
import { describeApiError } from "@/lib/api/errors";
import type { PlanParams } from "@/lib/api/params";
import type { ScanResponse } from "@/lib/api/types";
import { SCAN_PAGE_LIMIT, scanUrl, type ScanFilters } from "@/lib/scan-view";
import {
  applyListView,
  defaultDirFor,
  DEFAULT_VIEW,
  type ListView,
  type SortKey,
} from "@/lib/stock-list-view";
import { cn } from "@/lib/utils";
import { usePolling } from "@/hooks/use-polling";

import { useReportDataSource, type SymbolEntry } from "@/components/layout/data-source";
import { StockCards } from "@/components/stocks/stock-cards";
import { StockTable } from "@/components/stocks/stock-table";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/states";

import { ScanStatusLine } from "./scan-status";

const POLL_RUNNING_MS = 10_000;
const POLL_IDLE_MS = 60_000;

interface ScanResultsProps {
  initialData: ScanResponse;
  filters: ScanFilters;
  plan: PlanParams;
}

export function ScanResults({ initialData, filters, plan }: ScanResultsProps) {
  const url = scanUrl(filters, plan, SCAN_PAGE_LIMIT);
  const fetcher = useCallback((signal: AbortSignal) => fetchScan(url, signal), [url]);
  const [running, setRunning] = useState(initialData.status.state !== "idle");
  const { data, error, isRefreshing, refresh } = usePolling(fetcher, {
    initialData,
    intervalMs: running ? POLL_RUNNING_MS : POLL_IDLE_MS,
  });
  const nowRunning = data.status.state !== "idle";
  if (nowRunning !== running) setRunning(nowRunning);

  const [view, setView] = useState<ListView>(DEFAULT_VIEW);
  const onSort = useCallback(
    (key: SortKey) =>
      setView((v) =>
        v.sort === key
          ? { ...v, dir: v.dir === "asc" ? "desc" : "asc" }
          : { ...v, sort: key, dir: defaultDirFor(key) },
      ),
    [],
  );

  const symbols = useMemo<SymbolEntry[]>(
    () => data.items.map((i) => ({ symbol: i.symbol, name: i.name, exchange: i.exchange })),
    [data.items],
  );
  useReportDataSource(data.provider, symbols);

  const rows = useMemo(() => applyListView(data.items, view), [data.items, view]);
  const s = data.status;

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted">
        <p aria-live="polite">
          {data.total > data.items.length
            ? `Top ${data.items.length} of ${data.total} matches · `
            : `${data.total} ${data.total === 1 ? "match" : "matches"} · `}
          <ScanStatusLine data={data} />
        </p>
        <div className="flex items-center gap-2">
          {error ? (
            <span role="status" className="text-warn">
              Refresh failed: {describeApiError(error)}
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
        {rows.length > 0 ? (
          <>
            <StockTable rows={rows} view={view} onSort={onSort} plan={plan} />
            <StockCards rows={rows} plan={plan} />
          </>
        ) : s.state !== "idle" && !s.finished_at ? (
          <EmptyState
            title="First scan in progress"
            description={`Results appear when it finishes (${s.scanned} of ${s.universe_size} stocks so far). Daily candles are saved, so later scans are much faster.`}
          />
        ) : (
          <EmptyState
            title="No setups match"
            description="No stock in the scan universe has a setup that passes these filters right now. Loosen the filters or wait for the next scan."
          />
        )}
      </Card>
      {s.excluded.length > 0 ? (
        <p className="text-xs text-subtle">
          Never scanned: {s.excluded.join(", ")} (holdings and SCAN_EXCLUDE). Prices are from the
          latest daily close.
        </p>
      ) : null}
    </div>
  );
}
