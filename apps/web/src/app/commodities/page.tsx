import type { Metadata } from "next";

import type { SearchParamsRecord } from "@/lib/api/params";

import { MarketListPage } from "@/components/pages/market-list-page";

export const metadata: Metadata = {
  title: "Commodity",
};

export const dynamic = "force-dynamic";

export default function CommoditiesPage({
  searchParams,
}: {
  searchParams: Promise<SearchParamsRecord>;
}) {
  return <MarketListPage market="mcx" searchParams={searchParams} />;
}
