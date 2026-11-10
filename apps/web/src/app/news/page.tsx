import type { Metadata } from "next";

import { describeApiError } from "@/lib/api/errors";
import { firstParam, type SearchParamsRecord } from "@/lib/api/params";
import { getHeadlines } from "@/lib/api/server";
import type { HeadlinesResponse } from "@/lib/api/types";
import { formatDateTimeIST } from "@/lib/format";
import { stockHref } from "@/lib/links";
import { MARKETS, parseMarket } from "@/lib/markets";

import { MarketTabs } from "@/components/layout/market-tabs";
import { NewsList } from "@/components/news/news-list";
import { Card } from "@/components/ui/card";
import { PageHeader } from "@/components/ui/page-header";
import { RetryButton } from "@/components/ui/retry-button";
import { EmptyState, ErrorState } from "@/components/ui/states";

export const metadata: Metadata = {
  title: "Stock news",
};

export const dynamic = "force-dynamic";

export default async function NewsPage({
  searchParams,
}: {
  searchParams: Promise<SearchParamsRecord>;
}) {
  const params = await searchParams;
  const market = parseMarket(firstParam(params.market));
  const info = MARKETS[market];

  let data: HeadlinesResponse | null = null;
  let errorMessage: string | null = null;
  try {
    data = await getHeadlines(market);
  } catch (err) {
    errorMessage = describeApiError(err);
  }

  return (
    <div className="space-y-4">
      <MarketTabs
        active={market}
        label="News by market"
        hrefFor={(m) => (m === "in" ? "/news" : `/news?market=${m}`)}
      />
      <PageHeader
        title="Stock news"
        description={`Latest headlines for the ${info.noun} in your ${info.label} watchlist, from Google News.`}
      />
      <Card className="overflow-hidden">
        {data && data.items.length > 0 ? (
          <>
            <NewsList
              items={data.items}
              showSymbol
              symbolHref={(symbol) => stockHref(symbol, undefined, undefined, market)}
            />
            <p
              className="border-t border-line px-4 py-3 text-xs text-subtle sm:px-5"
              suppressHydrationWarning
            >
              Headlines via Google News, matched by company name, so some may be about the wider
              market. Links open the publisher&apos;s article. Updated{" "}
              {formatDateTimeIST(data.as_of)}; feeds refresh every 30 minutes.
            </p>
          </>
        ) : data ? (
          <EmptyState
            title="No headlines"
            description={`Google News returned nothing for the ${info.label} watchlist right now.`}
          />
        ) : (
          <ErrorState
            title="Could not load news"
            description={errorMessage ?? undefined}
            action={<RetryButton />}
          />
        )}
      </Card>
    </div>
  );
}
