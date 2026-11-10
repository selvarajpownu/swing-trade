import { describeApiError } from "@/lib/api/errors";
import { firstParam, parsePlanParams, type SearchParamsRecord } from "@/lib/api/params";
import { getStocks } from "@/lib/api/server";
import type { Market, StocksResponse } from "@/lib/api/types";
import { formatMoneyWhole } from "@/lib/format";
import { MARKETS } from "@/lib/markets";
import { parseListView } from "@/lib/stock-list-view";

import { MarketTabs } from "@/components/layout/market-tabs";
import { StockList } from "@/components/stocks/stock-list";
import { Card } from "@/components/ui/card";
import { PageHeader } from "@/components/ui/page-header";
import { RetryButton } from "@/components/ui/retry-button";
import { ErrorState } from "@/components/ui/states";

interface MarketListPageProps {
  market: Market;
  searchParams: Promise<SearchParamsRecord>;
}

/** The list page of one market (IN Stock, US Stock or Commodity). */
export async function MarketListPage({ market, searchParams }: MarketListPageProps) {
  const info = MARKETS[market];
  const params = await searchParams;
  const view = parseListView(params);
  const plan = parsePlanParams((key) => firstParam(params[key]), info.defaultCapital);

  let data: StocksResponse | null = null;
  let errorMessage: string | null = null;
  try {
    data = await getStocks({ market, capital: plan.capital, riskPct: plan.riskPct });
  } catch (err) {
    errorMessage = describeApiError(err);
  }

  const sizing =
    market === "mcx"
      ? `Plans show entry, stop and target; futures are sized in lots, so no quantity is given.`
      : `Quantities use ${formatMoneyWhole(plan.capital, info.currency)} capital and ${plan.riskPct}% risk per trade.`;

  return (
    <div className="space-y-4">
      <MarketTabs active={market} />
      <PageHeader
        title={info.label}
        description={`Swing setups across ${info.description}. ${sizing}`}
      />
      {data ? (
        <StockList
          // Remount (fresh polling state) when a navigation brings new params.
          key={[market, plan.capital, plan.riskPct, view.q, view.filter, view.sort, view.dir].join(
            "|",
          )}
          initialData={data}
          initialView={view}
          plan={plan}
        />
      ) : (
        <Card>
          <ErrorState
            title={`Could not load ${info.noun}`}
            description={errorMessage ?? undefined}
            action={<RetryButton />}
          />
        </Card>
      )}
    </div>
  );
}
