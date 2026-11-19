import type { Metadata } from "next";

import { describeApiError } from "@/lib/api/errors";
import { firstParam, parsePlanParams, type SearchParamsRecord } from "@/lib/api/params";
import { getSwing } from "@/lib/api/server";
import type { SwingResponse } from "@/lib/api/types";
import { formatINRWhole } from "@/lib/format";
import { gatesParam, parseSwingView, SWING_TABS } from "@/lib/swing-view";
import { cn } from "@/lib/utils";

import { ReportProvider } from "@/components/layout/report-provider";
import { RegimeBanner } from "@/components/swing/regime-banner";
import { RefreshUntilReady, SwingControls } from "@/components/swing/swing-controls";
import { SwingTable } from "@/components/swing/swing-table";
import { Card } from "@/components/ui/card";
import { PageHeader } from "@/components/ui/page-header";
import { RetryButton } from "@/components/ui/retry-button";
import { EmptyState, ErrorState } from "@/components/ui/states";

export const metadata: Metadata = {
  title: "Swing Stocks IN",
};

export const dynamic = "force-dynamic";

const PAGE_LIMIT = 200;

export default async function SwingPage({
  searchParams,
}: {
  searchParams: Promise<SearchParamsRecord>;
}) {
  const params = await searchParams;
  const get = (key: string) => firstParam(params[key]);
  const view = parseSwingView(get);
  const plan = parsePlanParams(get);
  const tab = SWING_TABS.find((t) => t.id === view.tab) ?? SWING_TABS[0]!;

  let data: SwingResponse | null = null;
  let errorMessage: string | null = null;
  try {
    data = await getSwing({
      setup: view.tab,
      gates: gatesParam(view.gates),
      limit: PAGE_LIMIT,
      capital: plan.capital,
      riskPct: plan.riskPct,
    });
  } catch (err) {
    errorMessage = describeApiError(err);
  }

  const status = data?.status;
  return (
    <div className="space-y-4">
      <PageHeader
        title="Swing Stocks IN"
        description={`NSE stocks screened for 2–6 week swing setups on daily bars. Quantities use ${formatINRWhole(plan.capital)} capital and ${plan.riskPct}% risk per trade.`}
      />
      <ReportProvider provider={data?.source ?? null} />
      {status ? <RefreshUntilReady state={status.state} /> : null}

      {data?.regime ? <RegimeBanner regime={data.regime} data={data} /> : null}
      {status?.error ? (
        <p
          role="status"
          className="rounded-lg border border-warn/30 bg-warn/10 px-3 py-2 text-xs text-warn"
        >
          NSE download problem: {status.error}. Showing the sessions already saved.
        </p>
      ) : null}

      <SwingControls view={view} counts={data?.regime ? data.counts : null} />
      <p className="text-xs text-muted">{tab.blurb}</p>

      <Card className="overflow-hidden">
        {!data ? (
          <ErrorState
            title="Could not load the swing screen"
            description={errorMessage ?? undefined}
            action={<RetryButton />}
          />
        ) : data.status.state !== "ready" ? (
          <EmptyState
            title={
              data.status.state === "syncing" ? "Downloading NSE daily files" : "First scan pending"
            }
            description={
              data.status.days_target
                ? `${data.status.days_cached ?? 0} of ${data.status.days_target} sessions saved. The first download takes a few minutes; this page refreshes by itself.`
                : "The first scan is running. This page refreshes by itself."
            }
          />
        ) : data.items.length === 0 ? (
          <EmptyState
            title={`No ${tab.label.toLowerCase()} right now`}
            description="No stock passes the selected gates with this setup on the latest daily bar. Untick a gate to see what was filtered out, or check the Leaders list."
          />
        ) : (
          <SwingTable rows={data.items} plan={plan} />
        )}
      </Card>

      {data && data.sectors.length > 0 ? (
        <details className="rounded-2xl border border-line bg-surface px-4 py-3 text-sm">
          <summary className="cursor-pointer font-medium">
            Sector strength ({data.sectors.length} industries, median 3-month return)
          </summary>
          <ol className="mt-3 grid gap-x-6 gap-y-1.5 sm:grid-cols-2 xl:grid-cols-3">
            {data.sectors.map((s) => (
              <li key={s.industry} className="flex items-baseline justify-between gap-3">
                <span className={cn("truncate", s.strong ? "text-fg" : "text-muted")}>
                  {s.rank}. {s.industry} <span className="text-xs text-subtle">({s.members})</span>
                </span>
                <span className={cn("tabular-nums", s.ret_3m_pct >= 0 ? "text-up" : "text-down")}>
                  {s.ret_3m_pct > 0 ? "+" : ""}
                  {s.ret_3m_pct.toFixed(1)}%
                </span>
              </li>
            ))}
          </ol>
          <p className="mt-3 text-xs text-subtle">
            The stronger half (brighter names) passes the Strong sector gate.
          </p>
        </details>
      ) : null}

      <p className="text-xs leading-5 text-subtle">
        Plans are entry just above the setup bar&apos;s high, a stop below the structure (at least 1
        ATR), and targets at 2R and 3R; review any open plan by the date shown (six weeks). Every
        threshold here is a starting value that has not been back-tested yet. Business-quality
        (fundamentals) and results-date checks are not applied yet. For personal analysis only, not
        investment advice.
      </p>
    </div>
  );
}
