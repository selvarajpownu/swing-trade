"use client";

import Link from "next/link";
import { useCallback } from "react";
import { ChevronLeft } from "lucide-react";

import { fetchStock, stockUrl } from "@/lib/api/client";
import { MARKETS } from "@/lib/markets";
import { describeApiError } from "@/lib/api/errors";
import type { PlanParams } from "@/lib/api/params";
import type { Interval, StockDetailResponse, StockNewsResponse } from "@/lib/api/types";
import { cn } from "@/lib/utils";
import { usePolling } from "@/hooks/use-polling";

import { useReportDataSource } from "@/components/layout/data-source";
import { Card } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/states";

import { CandleChart } from "./candle-chart";
import { IntervalToggle } from "./interval-toggle";
import { KeyLevels } from "./key-levels";
import { PlanInputs } from "./plan-inputs";
import { SnapshotCard } from "./snapshot-card";
import { StockHeader } from "./stock-header";
import { StockTabs } from "./stock-tabs";
import { TradePlanCard } from "./trade-plan-card";

interface StockViewProps {
  initialData: StockDetailResponse;
  symbol: string;
  interval: Interval;
  plan: PlanParams;
  /** INDmoney headlines, loaded once on the server. */
  news?: StockNewsResponse | null;
  newsError?: string | null;
}

const INTERVAL_LABEL: Record<Interval, string> = {
  "1d": "Daily",
  "1w": "Weekly",
  "15m": "15-minute",
};

/** Stock page body. Polls /api/stocks/[symbol] every 15s while visible. */
export function StockView({
  initialData,
  symbol,
  interval,
  plan,
  news,
  newsError,
}: StockViewProps) {
  const market = initialData.market ?? "in";
  const currency = initialData.currency ?? "INR";
  const info = MARKETS[market];
  const url = stockUrl(symbol, interval, plan, market);
  const fetcher = useCallback((signal: AbortSignal) => fetchStock(url, signal), [url]);
  const { data, error, isRefreshing } = usePolling(fetcher, { initialData });

  useReportDataSource(data.provider);

  const { quote, candles, overlays, zones, snapshot, setup } = data;

  return (
    <div className="space-y-5">
      <Link
        href={info.path}
        className="inline-flex items-center gap-1 text-xs text-muted transition-colors hover:text-fg"
      >
        <ChevronLeft aria-hidden="true" className="size-3.5" />
        {info.label}
      </Link>

      <StockHeader quote={quote} setup={setup} currency={currency} />

      {error ? (
        <p
          role="status"
          className="rounded-lg border border-warn/30 bg-warn/10 px-3 py-2 text-xs text-warn"
        >
          Live refresh failed: {describeApiError(error)} Showing the last data received.
        </p>
      ) : null}

      {/* DOM order (chart, plan, tabs) is the mobile order; on xl the plan column spans both rows. */}
      <div className="grid gap-5 xl:grid-cols-[minmax(0,1fr)_360px] xl:grid-rows-[auto_1fr]">
        <div className="min-w-0 xl:col-start-1 xl:row-start-1">
          <Card aria-labelledby="chart-title" className="pb-3">
            <div className="flex flex-wrap items-center justify-between gap-3 px-4 pt-4 pb-3">
              <div>
                <h2 id="chart-title" className="text-sm font-semibold">
                  Price chart
                </h2>
                <p className="text-xs text-muted">
                  {INTERVAL_LABEL[interval]} candles · times in IST
                  <span
                    aria-hidden="true"
                    className={cn(
                      "ml-2 inline-block size-1.5 rounded-full align-middle",
                      error ? "bg-warn" : "bg-up",
                      isRefreshing && "motion-safe:animate-pulse",
                    )}
                  />
                  <span className="sr-only">
                    {error ? "Live refresh failing" : "Live, refreshes every 15 seconds"}
                  </span>
                </p>
              </div>
              <IntervalToggle symbol={symbol} interval={interval} plan={plan} market={market} />
            </div>
            {candles.length > 0 ? (
              <CandleChart
                candles={candles}
                overlays={overlays}
                zones={zones}
                setup={setup}
                interval={interval}
              />
            ) : (
              <EmptyState
                title="No price history"
                description={`The data provider returned no ${INTERVAL_LABEL[interval].toLowerCase()} candles for ${symbol}.`}
              />
            )}
          </Card>
        </div>

        <aside
          aria-label="Trade plan and levels"
          className="space-y-5 self-start xl:col-start-2 xl:row-span-2 xl:row-start-1"
        >
          <TradePlanCard symbol={symbol} setup={setup} plan={plan} currency={currency} />
          <PlanInputs plan={plan} interval={interval} market={market} />
          <KeyLevels zones={zones} price={quote.price} />
          <SnapshotCard snapshot={snapshot} volume={quote.volume} />
        </aside>

        <div className="min-w-0 self-start xl:col-start-1 xl:row-start-2">
          <StockTabs symbol={symbol} news={news} newsError={newsError} />
        </div>
      </div>
    </div>
  );
}
