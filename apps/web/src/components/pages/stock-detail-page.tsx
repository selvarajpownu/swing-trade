import type { Metadata } from "next";
import { notFound } from "next/navigation";

import { describeApiError, isApiError } from "@/lib/api/errors";
import {
  firstParam,
  parseInterval,
  parsePlanParams,
  parseSymbol,
  type SearchParamsRecord,
} from "@/lib/api/params";
import { getStock, getStockEvents, getStockNews } from "@/lib/api/server";
import type {
  Market,
  StockDetailResponse,
  StockEventsResponse,
  StockNewsResponse,
} from "@/lib/api/types";
import { MARKETS } from "@/lib/markets";

import { StockView } from "@/components/stock/stock-view";
import { ButtonLink } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { RetryButton } from "@/components/ui/retry-button";
import { ErrorState } from "@/components/ui/states";

export interface StockRouteProps {
  params: Promise<{ symbol: string }>;
  searchParams: Promise<SearchParamsRecord>;
}

export async function stockMetadata({ params }: StockRouteProps): Promise<Metadata> {
  const { symbol } = await params;
  return { title: parseSymbol(symbol) ?? "Stock" };
}

/** The stock page of one market: chart, plan, levels and INDmoney news. */
export async function StockDetailPage({
  market,
  params,
  searchParams,
}: StockRouteProps & { market: Market }) {
  const info = MARKETS[market];
  const [{ symbol: rawSymbol }, query] = await Promise.all([params, searchParams]);
  const symbol = parseSymbol(rawSymbol);
  if (!symbol) notFound();

  // An interval this market lacks (15m for US) falls back to daily.
  const requested = parseInterval(firstParam(query.interval));
  const interval = info.intervals.includes(requested) ? requested : "1d";
  const plan = parsePlanParams((key) => firstParam(query[key]), info.defaultCapital);

  // News and NSE events are optional: a failure there never blocks the page.
  const [stock, news, nseEvents] = await Promise.allSettled([
    getStock(symbol, { market, interval, capital: plan.capital, riskPct: plan.riskPct }),
    getStockNews(symbol, market),
    market === "in" ? getStockEvents(symbol) : Promise.resolve(null),
  ]);

  let data: StockDetailResponse | null = null;
  let errorMessage: string | null = null;
  if (stock.status === "fulfilled") {
    data = stock.value;
  } else {
    if (isApiError(stock.reason) && stock.reason.isNotFound) notFound();
    errorMessage = describeApiError(stock.reason);
  }
  const newsData: StockNewsResponse | null = news.status === "fulfilled" ? news.value : null;
  const newsError = news.status === "rejected" ? describeApiError(news.reason) : null;
  const events: StockEventsResponse | null =
    nseEvents.status === "fulfilled" ? nseEvents.value : null;
  const eventsError = nseEvents.status === "rejected" ? describeApiError(nseEvents.reason) : null;

  if (!data) {
    return (
      <Card>
        <ErrorState
          title={`Could not load ${symbol}`}
          description={errorMessage ?? undefined}
          action={
            <div className="flex gap-2">
              <RetryButton />
              <ButtonLink href={info.path}>{info.label}</ButtonLink>
            </div>
          }
        />
      </Card>
    );
  }

  return (
    <StockView
      // Remount (fresh chart and polling state) when symbol, interval or plan change.
      key={`${market}|${symbol}|${interval}|${plan.capital}|${plan.riskPct}`}
      initialData={data}
      symbol={symbol}
      interval={interval}
      plan={plan}
      news={newsData}
      newsError={newsError}
      events={events}
      eventsError={eventsError}
    />
  );
}
