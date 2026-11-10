import type { Metadata } from "next";

import { describeApiError } from "@/lib/api/errors";
import { firstParam, parsePlanParams, type SearchParamsRecord } from "@/lib/api/params";
import { getStocks } from "@/lib/api/server";
import type { StocksResponse } from "@/lib/api/types";
import { formatINRWhole } from "@/lib/format";
import { parseListView } from "@/lib/stock-list-view";

import { StockList } from "@/components/stocks/stock-list";
import { Card } from "@/components/ui/card";
import { PageHeader } from "@/components/ui/page-header";
import { RetryButton } from "@/components/ui/retry-button";
import { ErrorState } from "@/components/ui/states";

export const metadata: Metadata = {
  title: "Stock list",
};

export const dynamic = "force-dynamic";

interface StocksPageProps {
  searchParams: Promise<SearchParamsRecord>;
}

export default async function StocksPage({ searchParams }: StocksPageProps) {
  const params = await searchParams;
  const view = parseListView(params);
  const plan = parsePlanParams((key) => firstParam(params[key]));

  let data: StocksResponse | null = null;
  let errorMessage: string | null = null;
  try {
    data = await getStocks({ capital: plan.capital, riskPct: plan.riskPct });
  } catch (err) {
    errorMessage = describeApiError(err);
  }

  return (
    <>
      <PageHeader
        title="Stock list"
        description={`Swing setups across your watchlist. Quantities use ${formatINRWhole(plan.capital)} capital and ${plan.riskPct}% risk per trade.`}
      />
      {data ? (
        <StockList
          // Remount (fresh polling state) when a navigation brings new params.
          key={[plan.capital, plan.riskPct, view.q, view.filter, view.sort, view.dir].join("|")}
          initialData={data}
          initialView={view}
          plan={plan}
        />
      ) : (
        <Card>
          <ErrorState
            title="Could not load stocks"
            description={errorMessage ?? undefined}
            action={<RetryButton />}
          />
        </Card>
      )}
    </>
  );
}
