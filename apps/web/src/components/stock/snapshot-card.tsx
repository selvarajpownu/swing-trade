import type { ReactNode } from "react";

import type { Snapshot } from "@/lib/api/types";
import { formatDistancePct, formatNumber, formatPercent, formatRatio } from "@/lib/format";
import { cn } from "@/lib/utils";

import { Card, CardBody, CardHeader } from "@/components/ui/card";

interface SnapshotCardProps {
  snapshot: Snapshot;
  /** Latest volume (from the quote), compared with the 20-day average. */
  volume: number;
}

function rsiLabel(rsi: number | null): { text: string; className: string } | null {
  if (rsi === null) return null;
  if (rsi >= 70) return { text: "Overbought", className: "text-down" };
  if (rsi <= 30) return { text: "Oversold", className: "text-up" };
  return { text: "Neutral", className: "text-subtle" };
}

function Item({ label, value, note }: { label: string; value: ReactNode; note?: ReactNode }) {
  return (
    <div className="flex items-baseline justify-between gap-3 py-2">
      <dt className="text-sm text-muted">{label}</dt>
      <dd className="text-right">
        <span className="text-sm font-medium tabular-nums">{value}</span>
        {note ? <span className="ml-2 text-[11px] tabular-nums">{note}</span> : null}
      </dd>
    </div>
  );
}

function SmaNote({ close, sma }: { close: number; sma: number | null }) {
  if (sma === null) return null;
  const above = close >= sma;
  return (
    <span className={above ? "text-up" : "text-down"}>
      {above ? "▲" : "▼"} {formatDistancePct(close, sma)}
    </span>
  );
}

export function SnapshotCard({ snapshot, volume }: SnapshotCardProps) {
  const rsi = rsiLabel(snapshot.rsi14);
  const atrPct =
    snapshot.atr14 !== null && snapshot.close > 0 ? (snapshot.atr14 / snapshot.close) * 100 : null;
  const volRatio =
    snapshot.volume_avg20 !== null && snapshot.volume_avg20 > 0
      ? volume / snapshot.volume_avg20
      : null;

  return (
    <Card aria-labelledby="snapshot-title">
      <CardHeader
        title="Snapshot"
        titleId="snapshot-title"
        description="Daily indicators. Price vs each average in brackets."
      />
      <CardBody>
        <dl className="divide-y divide-line">
          <Item
            label="RSI (14)"
            value={snapshot.rsi14 !== null ? snapshot.rsi14.toFixed(1) : "—"}
            note={rsi ? <span className={rsi.className}>{rsi.text}</span> : null}
          />
          <Item
            label="ATR (14)"
            value={formatNumber(snapshot.atr14)}
            note={
              atrPct !== null ? (
                <span className="text-subtle">{formatPercent(atrPct)} of price</span>
              ) : null
            }
          />
          <Item
            label="SMA 20"
            value={formatNumber(snapshot.sma20)}
            note={<SmaNote close={snapshot.close} sma={snapshot.sma20} />}
          />
          <Item
            label="SMA 50"
            value={formatNumber(snapshot.sma50)}
            note={<SmaNote close={snapshot.close} sma={snapshot.sma50} />}
          />
          <Item
            label="SMA 200"
            value={formatNumber(snapshot.sma200)}
            note={<SmaNote close={snapshot.close} sma={snapshot.sma200} />}
          />
          <Item
            label="Volume vs 20d avg"
            value={formatRatio(volume, snapshot.volume_avg20)}
            note={
              volRatio !== null ? (
                <span className={cn(volRatio >= 1.5 ? "text-up" : "text-subtle")}>
                  {volRatio >= 1.5 ? "Heavy" : volRatio < 0.7 ? "Light" : "Normal"}
                </span>
              ) : null
            }
          />
        </dl>
      </CardBody>
    </Card>
  );
}
