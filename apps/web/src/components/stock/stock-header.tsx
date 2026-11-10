import type { Quote, SetupDetail } from "@/lib/api/types";
import { formatDateTimeIST, formatINR, formatVolume } from "@/lib/format";

import { SetupBadge } from "@/components/stocks/setup-badge";
import { PriceChange } from "@/components/ui/price-change";
import { SymbolAvatar } from "@/components/ui/symbol-avatar";

interface StockHeaderProps {
  quote: Quote;
  setup: SetupDetail | null;
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-0">
      <dt className="text-[11px] text-muted">{label}</dt>
      <dd className="mt-0.5 truncate text-sm font-medium tabular-nums">{value}</dd>
    </div>
  );
}

export function StockHeader({ quote, setup }: StockHeaderProps) {
  return (
    <div className="flex flex-col gap-4 lg:flex-row lg:items-end lg:justify-between">
      <div className="flex min-w-0 items-start gap-3">
        <SymbolAvatar symbol={quote.symbol} name={quote.name} size="lg" />
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <h1 className="truncate text-xl font-semibold tracking-tight">{quote.name}</h1>
            <SetupBadge setup={setup} />
          </div>
          <p className="mt-0.5 text-sm text-muted">
            {quote.symbol} <span aria-hidden="true">·</span> {quote.exchange}
          </p>
          <div className="mt-2 flex flex-wrap items-baseline gap-x-3 gap-y-1">
            <span className="text-2xl font-semibold tabular-nums">{formatINR(quote.price)}</span>
            <PriceChange
              change={quote.change}
              changePct={quote.change_pct}
              className="text-sm font-medium"
            />
            <span className="text-xs text-subtle" suppressHydrationWarning>
              Updated {formatDateTimeIST(quote.updated_at)}
            </span>
          </div>
        </div>
      </div>

      <dl className="grid grid-cols-2 gap-x-6 gap-y-2 rounded-2xl border border-line bg-surface px-4 py-3 sm:grid-cols-4">
        <Metric label="52W High" value={formatINR(quote.high_52w)} />
        <Metric label="52W Low" value={formatINR(quote.low_52w)} />
        <Metric
          label="Day range"
          value={`${formatINR(quote.day_low)} – ${formatINR(quote.day_high)}`}
        />
        <Metric label="Volume" value={formatVolume(quote.volume)} />
      </dl>
    </div>
  );
}
