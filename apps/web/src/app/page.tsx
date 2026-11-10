import type { Metadata } from "next";

import { describeApiError } from "@/lib/api/errors";
import { firstParam, parsePlanParams, type SearchParamsRecord } from "@/lib/api/params";
import { getScan } from "@/lib/api/server";
import type { ScanResponse } from "@/lib/api/types";
import { scanApiParams, TOP_SETUP_FILTERS } from "@/lib/scan-view";

import { TopSetups } from "@/components/home/top-setups";
import { ReportProvider } from "@/components/layout/report-provider";
import { Card } from "@/components/ui/card";
import { PageHeader } from "@/components/ui/page-header";
import { RetryButton } from "@/components/ui/retry-button";
import { ErrorState } from "@/components/ui/states";

export const metadata: Metadata = {
  title: "Home",
};

export const dynamic = "force-dynamic";

const TOP_SETUPS = 5;

interface HomePageProps {
  searchParams: Promise<SearchParamsRecord>;
}

export default async function HomePage({ searchParams }: HomePageProps) {
  const params = await searchParams;
  const plan = parsePlanParams((key) => firstParam(params[key]));

  let scan: ScanResponse | null = null;
  let errorMessage: string | null = null;
  try {
    scan = await getScan({
      ...scanApiParams(TOP_SETUP_FILTERS),
      limit: TOP_SETUPS,
      capital: plan.capital,
      riskPct: plan.riskPct,
    });
  } catch (err) {
    errorMessage = describeApiError(err);
  }

  return (
    <>
      <PageHeader
        title="Home"
        description="Today's strongest swing setups from your scan universe."
      />
      <ReportProvider provider={scan?.provider ?? null} />
      {scan ? (
        <TopSetups data={scan} plan={plan} />
      ) : (
        <Card>
          <ErrorState
            title="Could not load top setups"
            description={errorMessage ?? undefined}
            action={<RetryButton />}
          />
        </Card>
      )}
    </>
  );
}
