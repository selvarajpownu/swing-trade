import type { Metadata } from "next";

import { describeApiError } from "@/lib/api/errors";
import { firstParam, parsePlanParams, type SearchParamsRecord } from "@/lib/api/params";
import { getScan } from "@/lib/api/server";
import type { ScanResponse } from "@/lib/api/types";
import { formatINRWhole } from "@/lib/format";
import { parseScanFilters, SCAN_PAGE_LIMIT, scanApiParams } from "@/lib/scan-view";

import { ScanFilters } from "@/components/scan/scan-filters";
import { ScanResults } from "@/components/scan/scan-results";
import { Card } from "@/components/ui/card";
import { PageHeader } from "@/components/ui/page-header";
import { RetryButton } from "@/components/ui/retry-button";
import { ErrorState } from "@/components/ui/states";

export const metadata: Metadata = {
  title: "Scanner",
};

export const dynamic = "force-dynamic";

interface ScanPageProps {
  searchParams: Promise<SearchParamsRecord>;
}

export default async function ScanPage({ searchParams }: ScanPageProps) {
  const params = await searchParams;
  const get = (key: string) => firstParam(params[key]);
  const filters = parseScanFilters(get);
  const plan = parsePlanParams(get);

  let data: ScanResponse | null = null;
  let errorMessage: string | null = null;
  try {
    data = await getScan({
      ...scanApiParams(filters),
      limit: SCAN_PAGE_LIMIT,
      capital: plan.capital,
      riskPct: plan.riskPct,
    });
  } catch (err) {
    errorMessage = describeApiError(err);
  }

  return (
    <>
      <PageHeader
        title="Scanner"
        description={`Swing setups across the scan universe, best score first. Quantities use ${formatINRWhole(plan.capital)} capital and ${plan.riskPct}% risk per trade.`}
      />
      <div className="space-y-4">
        <ScanFilters filters={filters} />
        {data ? (
          <ScanResults
            // Remount (fresh polling state) when the filters or sizing change.
            key={[
              filters.preset,
              filters.minScore,
              filters.minRr,
              filters.watchingOnly,
              plan.capital,
              plan.riskPct,
            ].join("|")}
            initialData={data}
            filters={filters}
            plan={plan}
          />
        ) : (
          <Card>
            <ErrorState
              title="Could not load the scan"
              description={errorMessage ?? undefined}
              action={<RetryButton />}
            />
          </Card>
        )}
      </div>
    </>
  );
}
