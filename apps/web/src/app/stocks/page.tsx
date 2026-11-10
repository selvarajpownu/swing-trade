import type { Metadata } from "next";

import type { SearchParamsRecord } from "@/lib/api/params";

import { MarketListPage } from "@/components/pages/market-list-page";

export const metadata: Metadata = {
  title: "IN Stock",
};

export const dynamic = "force-dynamic";

export default function StocksPage({
  searchParams,
}: {
  searchParams: Promise<SearchParamsRecord>;
}) {
  return <MarketListPage market="in" searchParams={searchParams} />;
}
