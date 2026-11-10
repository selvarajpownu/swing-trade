import Link from "next/link";
import { ArrowRight } from "lucide-react";

import type { PlanParams } from "@/lib/api/params";
import type { ScanResponse } from "@/lib/api/types";
import { formatINR, formatRiskReward } from "@/lib/format";
import { stockHref } from "@/lib/links";

import { ScanStatusLine } from "@/components/scan/scan-status";
import { ScoreBar } from "@/components/stocks/score-bar";
import { SetupBadge } from "@/components/stocks/setup-badge";
import { ButtonLink } from "@/components/ui/button";
import { Card, CardHeader } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/states";
import { SymbolAvatar } from "@/components/ui/symbol-avatar";

interface TopSetupsProps {
  data: ScanResponse;
  plan: PlanParams;
}

/**
 * The best few setups from the latest scan: watching state, risk-reward ≥ 2.
 * A ranking of chart setups for the owner's own analysis, not advice.
 */
export function TopSetups({ data, plan }: TopSetupsProps) {
  const { items, status } = data;
  return (
    <Card>
      <CardHeader
        title="Top setups"
        titleId="top-setups"
        description={
          <span className="text-xs">
            <ScanStatusLine data={data} />
          </span>
        }
        action={
          <ButtonLink href="/scan?min_rr=2&state=watching" variant="ghost" className="text-xs">
            Open scanner
            <ArrowRight aria-hidden="true" className="size-3.5" />
          </ButtonLink>
        }
      />
      {items.length === 0 ? (
        <EmptyState
          title={status.finished_at ? "No setup passes right now" : "First scan in progress"}
          description={
            status.finished_at
              ? "No stock in the scan universe has a watching setup with risk-reward of 2 or more."
              : `Top setups appear when the first scan finishes (${status.scanned} of ${status.universe_size} stocks so far).`
          }
        />
      ) : (
        <ol aria-labelledby="top-setups" className="divide-y divide-line">
          {items.map((item, i) => (
            <li key={item.symbol}>
              <Link
                href={stockHref(item.symbol, plan)}
                className="grid grid-cols-[1.5rem_auto_1fr_auto] items-center gap-3 px-4 py-3 transition-colors hover:bg-surface-2 sm:grid-cols-[1.5rem_auto_1fr_auto_auto_auto] sm:px-5"
              >
                <span className="text-sm text-subtle tabular-nums">{i + 1}</span>
                <SymbolAvatar symbol={item.symbol} name={item.name} size="sm" />
                <span className="min-w-0">
                  <span className="block truncate text-sm font-medium text-fg">{item.name}</span>
                  <span className="flex items-center gap-2 text-xs text-muted">
                    {item.symbol}
                    <SetupBadge setup={item.setup} showState={false} />
                  </span>
                </span>
                <span className="hidden text-right text-xs text-muted tabular-nums sm:block">
                  <span className="block">Entry {formatINR(item.setup.entry)}</span>
                  <span className="block">SL {formatINR(item.setup.stop)}</span>
                </span>
                <span className="hidden text-right text-xs text-muted tabular-nums sm:block">
                  <span className="block">R:R {formatRiskReward(item.setup.risk_reward)}</span>
                  <span className="block">{item.setup.quantity} shares</span>
                </span>
                <ScoreBar score={item.setup.score} className="w-20" />
              </Link>
            </li>
          ))}
        </ol>
      )}
      <p className="border-t border-line px-4 py-3 text-xs text-subtle sm:px-5">
        Ranked by setup score from the latest scan (watching setups, R:R ≥ 2). Holdings such as
        INDRAMEDCO are never listed. For personal analysis only, not investment advice.
      </p>
    </Card>
  );
}
