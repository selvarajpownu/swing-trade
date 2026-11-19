import { ShieldAlert, ShieldCheck, TriangleAlert } from "lucide-react";

import type { RegimeState, SwingRegime, SwingResponse } from "@/lib/api/types";
import { formatNumber } from "@/lib/format";
import { cn } from "@/lib/utils";

import { Card } from "@/components/ui/card";

const STATES: Record<
  RegimeState,
  { title: string; tone: string; icon: typeof ShieldCheck; text: (index: string) => string }
> = {
  normal: {
    title: "Market: normal",
    tone: "border-up/30 bg-up/10 text-up",
    icon: ShieldCheck,
    text: (index) => `${index} is above its 50- and 200-day averages and breadth is healthy.`,
  },
  caution: {
    title: "Market: caution",
    tone: "border-warn/30 bg-warn/10 text-warn",
    icon: TriangleAlert,
    text: (index) =>
      `${index} is below its 50-day average, or fewer than 40% of stocks are above theirs. The playbook takes fewer new positions in this state.`,
  },
  defensive: {
    title: "Market: defensive",
    tone: "border-down/30 bg-down/10 text-down",
    icon: ShieldAlert,
    text: (index) =>
      `${index} is below its 200-day average. The playbook avoids fresh long setups in this state.`,
  },
};

function Stat({ label, value, hint }: { label: string; value: string; hint?: string }) {
  return (
    <div className="min-w-0">
      <dt className="text-[11px] text-muted">{label}</dt>
      <dd className="mt-0.5 text-sm font-medium tabular-nums">{value}</dd>
      {hint ? <dd className="text-[11px] text-subtle">{hint}</dd> : null}
    </div>
  );
}

function vs(close: number | null, average: number | null): string | undefined {
  if (close === null || average === null || average === 0) return undefined;
  const pct = (close / average - 1) * 100;
  return `price ${pct >= 0 ? "+" : "−"}${Math.abs(pct).toFixed(1)}%`;
}

/** "19 Nov 2026" from YYYY-MM-DD. */
export function formatBarDate(iso: string | null | undefined): string {
  if (!iso) return "—";
  const date = new Date(`${iso}T00:00:00Z`);
  if (Number.isNaN(date.getTime())) return iso;
  return new Intl.DateTimeFormat("en-IN", {
    day: "numeric",
    month: "short",
    year: "numeric",
    timeZone: "UTC",
  }).format(date);
}

interface RegimeBannerProps {
  regime: SwingRegime;
  data: Pick<SwingResponse, "as_of" | "source" | "status">;
}

/** Market regime from the benchmark index and the universe's breadth. */
export function RegimeBanner({ regime, data }: RegimeBannerProps) {
  const state = STATES[regime.state];
  const Icon = state.icon;
  const index = regime.index ?? "The universe";
  const source =
    data.source === "nse"
      ? "NSE end-of-day files"
      : data.source === "demo"
        ? "demo data"
        : data.source;
  return (
    <Card className="p-4 sm:p-5">
      <div className="flex flex-col gap-4 lg:flex-row lg:items-center lg:justify-between">
        <div className="min-w-0">
          <span
            className={cn(
              "inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-xs font-semibold",
              state.tone,
            )}
          >
            <Icon aria-hidden="true" className="size-3.5" />
            {state.title}
          </span>
          <p className="mt-2 max-w-xl text-sm text-muted">{state.text(index)}</p>
          <p className="mt-1 text-xs text-subtle">
            Daily bars to {formatBarDate(data.as_of)} · {regime.universe} stocks · {source}
            {data.status.failed > 0 ? ` · ${data.status.failed} without data` : ""}
          </p>
        </div>
        <dl className="grid shrink-0 grid-cols-2 gap-x-6 gap-y-3 sm:grid-cols-4">
          {regime.index && regime.index_close !== null ? (
            <>
              <Stat label={regime.index} value={formatNumber(regime.index_close)} />
              <Stat
                label="50-day avg"
                value={formatNumber(regime.index_sma50)}
                hint={vs(regime.index_close, regime.index_sma50)}
              />
              <Stat
                label="200-day avg"
                value={formatNumber(regime.index_sma200)}
                hint={vs(regime.index_close, regime.index_sma200)}
              />
            </>
          ) : null}
          <Stat
            label="Breadth"
            value={`${regime.breadth_50_pct.toFixed(0)}% / ${regime.breadth_200_pct.toFixed(0)}%`}
            hint="above 50 / 200-day"
          />
        </dl>
      </div>
    </Card>
  );
}
