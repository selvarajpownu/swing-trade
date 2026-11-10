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
import { getStock } from "@/lib/api/server";
import type { StockDetailResponse } from "@/lib/api/types";

import { StockView } from "@/components/stock/stock-view";
import { ButtonLink } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { RetryButton } from "@/components/ui/retry-button";
import { ErrorState } from "@/components/ui/states";

export const dynamic = "force-dynamic";

interface StockPageProps {
  params: Promise<{ symbol: string }>;
  searchParams: Promise<SearchParamsRecord>;
}

export async function generateMetadata({ params }: StockPageProps): Promise<Metadata> {
  const { symbol } = await params;
  return { title: parseSymbol(symbol) ?? "Stock" };
}

export default async function StockPage({ params, searchParams }: StockPageProps) {
  const [{ symbol: rawSymbol }, query] = await Promise.all([params, searchParams]);
  const symbol = parseSymbol(rawSymbol);
  if (!symbol) notFound();

  const interval = parseInterval(firstParam(query.interval));
  const plan = parsePlanParams((key) => firstParam(query[key]));

  let data: StockDetailResponse | null = null;
  let errorMessage: string | null = null;
  try {
    data = await getStock(symbol, { interval, capital: plan.capital, riskPct: plan.riskPct });
  } catch (err) {
    if (isApiError(err) && err.isNotFound) notFound();
    errorMessage = describeApiError(err);
  }

  if (!data) {
    return (
      <Card>
        <ErrorState
          title={`Could not load ${symbol}`}
          description={errorMessage ?? undefined}
          action={
            <div className="flex gap-2">
              <RetryButton />
              <ButtonLink href="/stocks">Stock list</ButtonLink>
            </div>
          }
        />
      </Card>
    );
  }

  return (
    <StockView
      // Remount (fresh chart and polling state) when symbol, interval or plan change.
      key={`${symbol}|${interval}|${plan.capital}|${plan.riskPct}`}
      initialData={data}
      symbol={symbol}
      interval={interval}
      plan={plan}
    />
  );
}
