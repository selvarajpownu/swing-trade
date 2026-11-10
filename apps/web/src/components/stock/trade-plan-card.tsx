import { CircleAlert } from "lucide-react";

import type { PlanParams } from "@/lib/api/params";
import type { SetupDetail } from "@/lib/api/types";
import { formatINR, formatINRWhole, formatInteger, formatRiskReward } from "@/lib/format";
import { cn } from "@/lib/utils";

import { SetupBadge } from "@/components/stocks/setup-badge";
import { Card, CardBody, CardHeader } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/states";

interface TradePlanCardProps {
  symbol: string;
  setup: SetupDetail | null;
  plan: PlanParams;
}

interface StepProps {
  badge: string;
  label: string;
  price: number;
  tone: "down" | "neutral" | "up";
  last?: boolean;
}

const BADGE_TONE = {
  down: "border-down/30 bg-down/15 text-down",
  neutral: "border-line-strong bg-surface-3 text-fg",
  up: "border-up/30 bg-up/15 text-up",
} as const;

function Step({ badge, label, price, tone, last = false }: StepProps) {
  return (
    <li className={cn("relative flex items-center gap-3", !last && "pb-5")}>
      {/* Connector to the next step */}
      {!last ? (
        <span
          aria-hidden="true"
          className="absolute top-9 bottom-0 left-[17px] border-l border-dashed border-line-strong"
        />
      ) : null}
      <span
        aria-hidden="true"
        className={cn(
          "relative z-10 flex size-9 shrink-0 items-center justify-center rounded-xl border text-xs font-bold",
          BADGE_TONE[tone],
        )}
      >
        {badge}
      </span>
      <span className="flex-1 text-sm text-muted">{label}</span>
      <span className={cn("text-base font-semibold tabular-nums", tone === "up" && "text-up")}>
        {formatINR(price)}
      </span>
    </li>
  );
}

function Stat({ label, value, hint }: { label: string; value: string; hint?: string }) {
  return (
    <div className="rounded-xl bg-surface-2 px-3 py-2.5">
      <dt className="text-[11px] text-muted">{label}</dt>
      <dd className="mt-0.5 text-sm font-semibold tabular-nums">{value}</dd>
      {hint ? <dd className="mt-0.5 text-[11px] text-subtle">{hint}</dd> : null}
    </div>
  );
}

export function TradePlanCard({ symbol, setup, plan }: TradePlanCardProps) {
  if (!setup) {
    return (
      <Card aria-labelledby="plan-title">
        <CardHeader title="Trade plan" titleId="plan-title" />
        <EmptyState
          className="py-8"
          title="No setup right now"
          description={`None of the presets (pullback, breakout, support bounce, oversold) fire for ${symbol} on this data.`}
        />
      </Card>
    );
  }

  const riskBudget = (plan.capital * plan.riskPct) / 100;
  const positionValue = setup.quantity * setup.entry;
  const maxLoss = setup.quantity * setup.risk_per_share;
  const lowRR = setup.state === "low_risk_reward";

  return (
    <Card aria-labelledby="plan-title">
      <CardHeader
        title="Trade plan"
        titleId="plan-title"
        description={`Score ${Math.round(setup.score)}/100`}
        action={<SetupBadge setup={setup} />}
      />
      <CardBody>
        {lowRR ? (
          <p className="mb-4 flex gap-2 rounded-lg border border-warn/30 bg-warn/10 px-3 py-2 text-xs text-warn">
            <CircleAlert aria-hidden="true" className="mt-px size-3.5 shrink-0" />
            Risk-reward is below the minimum. Shown for reference only.
          </p>
        ) : null}

        <ol aria-label="Price levels" className={cn(lowRR && "opacity-80")}>
          <Step badge="SL" label="Stop Loss" price={setup.stop} tone="down" />
          <Step badge="B" label="Entry at" price={setup.entry} tone="neutral" />
          <Step badge="TP" label="Target Price" price={setup.target} tone="up" last />
        </ol>

        <dl className="mt-5 grid grid-cols-2 gap-2">
          <Stat label="Risk-reward" value={formatRiskReward(setup.risk_reward)} />
          <Stat
            label="Quantity"
            value={`${formatInteger(setup.quantity)} shares`}
            hint={`≈ ${formatINRWhole(positionValue)} position`}
          />
          <Stat label="Risk / share" value={formatINR(setup.risk_per_share)} />
          <Stat label="Reward / share" value={formatINR(setup.reward_per_share)} />
        </dl>

        <p className="mt-3 text-xs leading-5 text-muted">
          Sized for {formatINRWhole(plan.capital)} capital at {plan.riskPct}% risk (
          {formatINRWhole(riskBudget)} per trade). Loss at stop ≈{" "}
          <span className="text-down tabular-nums">{formatINRWhole(maxLoss)}</span>.{" "}
          {setup.target_from_zone
            ? "Target is the next resistance zone."
            : "Target uses a fixed reward multiple (no resistance zone above)."}
        </p>

        {setup.reasons.length > 0 ? (
          <div className="mt-4 border-t border-line pt-4">
            <h3 className="text-xs font-semibold tracking-wide text-muted uppercase">
              Why this setup
            </h3>
            <ul className="mt-2 space-y-1.5 text-sm">
              {setup.reasons.map((reason, i) => (
                <li key={`${i}-${reason}`} className="flex gap-2">
                  <span
                    aria-hidden="true"
                    className="mt-2 size-1 shrink-0 rounded-full bg-accent"
                  />
                  <span>{reason}</span>
                </li>
              ))}
            </ul>
          </div>
        ) : null}
      </CardBody>
    </Card>
  );
}
