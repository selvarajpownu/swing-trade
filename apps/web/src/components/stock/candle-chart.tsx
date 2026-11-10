"use client";

import { useEffect, useRef, useState } from "react";
import {
  CandlestickSeries,
  ColorType,
  createChart,
  CrosshairMode,
  HistogramSeries,
  LineSeries,
  LineStyle,
  type IChartApi,
  type IPriceLine,
  type ISeriesApi,
  type MouseEventParams,
  type Time,
  type UTCTimestamp,
} from "lightweight-charts";

import type { Candle, Interval, LinePoint, Overlays, SetupDetail, Zone } from "@/lib/api/types";
import { formatNumber } from "@/lib/format";
import { cn } from "@/lib/utils";

/**
 * Canvas colours must be literal values (no CSS variables). Keep these in
 * sync with the @theme tokens in globals.css.
 */
const COLORS = {
  text: "#8f98a9",
  grid: "rgba(38, 45, 58, 0.55)",
  border: "#262d3a",
  up: "#22c37a",
  down: "#f0525f",
  upVolume: "rgba(34, 195, 122, 0.35)",
  downVolume: "rgba(240, 82, 95, 0.35)",
  entry: "#c9ced8",
  support: "rgba(34, 195, 122, 0.55)",
  resistance: "rgba(240, 82, 95, 0.55)",
  sma20: "#f2a93b",
  sma50: "#6e8dff",
  sma200: "#c084fc",
} as const;

/**
 * lightweight-charts renders times in UTC. Shift by IST (+05:30) so axis
 * labels show Indian market time; daily bars keep the same calendar date.
 */
const IST_OFFSET_SECONDS = 5.5 * 60 * 60;

/** Draw at most this many zones (nearest first) to keep the chart readable. */
const MAX_ZONES = 4;

type SmaKey = keyof Overlays;

const SMA_LINES: { key: SmaKey; label: string; color: string }[] = [
  { key: "sma20", label: "SMA 20", color: COLORS.sma20 },
  { key: "sma50", label: "SMA 50", color: COLORS.sma50 },
  { key: "sma200", label: "SMA 200", color: COLORS.sma200 },
];

const priceFormat = new Intl.NumberFormat("en-IN", {
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
});

function toTime(seconds: number): UTCTimestamp {
  return (seconds + IST_OFFSET_SECONDS) as UTCTimestamp;
}

/**
 * lightweight-charts throws on unsorted or duplicate times. Sort ascending,
 * keep the last item for a repeated time, and drop non-finite values.
 */
function sanitize<T extends { time: number }>(
  items: readonly T[],
  isValid: (item: T) => boolean,
): T[] {
  const byTime = new Map<number, T>();
  for (const item of items) {
    if (Number.isFinite(item.time) && isValid(item)) byTime.set(item.time, item);
  }
  return Array.from(byTime.values()).sort((a, b) => a.time - b.time);
}

const isValidCandle = (c: Candle) =>
  [c.open, c.high, c.low, c.close].every((v) => Number.isFinite(v));

const isValidPoint = (p: LinePoint) => Number.isFinite(p.value);

function toLineData(points: readonly LinePoint[]) {
  return sanitize(points, isValidPoint).map((p) => ({ time: toTime(p.time), value: p.value }));
}

interface HoverBar {
  open: number;
  high: number;
  low: number;
  close: number;
}

interface CandleChartProps {
  candles: Candle[];
  overlays: Overlays;
  zones: Zone[];
  setup: Pick<SetupDetail, "stop" | "entry" | "target"> | null;
  interval: Interval;
  className?: string;
}

interface ChartRefs {
  chart: IChartApi;
  candles: ISeriesApi<"Candlestick">;
  volume: ISeriesApi<"Histogram">;
  sma: Record<SmaKey, ISeriesApi<"Line">>;
}

export function CandleChart({
  candles,
  overlays,
  zones,
  setup,
  interval,
  className,
}: CandleChartProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const refs = useRef<ChartRefs | null>(null);
  const priceLinesRef = useRef<IPriceLine[]>([]);
  const fittedRef = useRef(false);

  const [visible, setVisible] = useState<Record<SmaKey, boolean>>({
    sma20: true,
    sma50: true,
    sma200: true,
  });
  const [hover, setHover] = useState<HoverBar | null>(null);

  // Create the chart once.
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const chart = createChart(container, {
      autoSize: true,
      layout: {
        background: { type: ColorType.Solid, color: "transparent" },
        textColor: COLORS.text,
        fontSize: 11,
        fontFamily: "ui-sans-serif, system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif",
      },
      grid: {
        vertLines: { color: COLORS.grid },
        horzLines: { color: COLORS.grid },
      },
      rightPriceScale: {
        borderColor: COLORS.border,
        scaleMargins: { top: 0.08, bottom: 0.26 },
      },
      timeScale: {
        borderColor: COLORS.border,
        secondsVisible: false,
        rightOffset: 4,
      },
      crosshair: { mode: CrosshairMode.Normal },
      localization: {
        locale: "en-IN",
        priceFormatter: (price: number) => priceFormat.format(price),
      },
    });

    const candleSeries = chart.addSeries(CandlestickSeries, {
      upColor: COLORS.up,
      downColor: COLORS.down,
      wickUpColor: COLORS.up,
      wickDownColor: COLORS.down,
      borderVisible: false,
      priceLineVisible: true,
    });

    // Volume on its own overlay scale, squeezed into the bottom of the pane.
    const volumeSeries = chart.addSeries(HistogramSeries, {
      priceScaleId: "",
      priceFormat: { type: "volume" },
      lastValueVisible: false,
      priceLineVisible: false,
    });
    volumeSeries.priceScale().applyOptions({ scaleMargins: { top: 0.8, bottom: 0 } });

    const sma = {} as Record<SmaKey, ISeriesApi<"Line">>;
    for (const line of SMA_LINES) {
      sma[line.key] = chart.addSeries(LineSeries, {
        color: line.color,
        lineWidth: 1,
        priceLineVisible: false,
        lastValueVisible: false,
        crosshairMarkerVisible: false,
      });
    }

    const onCrosshairMove = (param: MouseEventParams<Time>) => {
      const bar = param.seriesData.get(candleSeries);
      if (bar && "open" in bar) {
        setHover({ open: bar.open, high: bar.high, low: bar.low, close: bar.close });
      } else {
        setHover(null);
      }
    };
    chart.subscribeCrosshairMove(onCrosshairMove);

    refs.current = { chart, candles: candleSeries, volume: volumeSeries, sma };

    return () => {
      chart.unsubscribeCrosshairMove(onCrosshairMove);
      chart.remove();
      refs.current = null;
      priceLinesRef.current = [];
      fittedRef.current = false;
    };
  }, []);

  // Intraday bars need the time of day on the axis.
  useEffect(() => {
    refs.current?.chart.applyOptions({ timeScale: { timeVisible: interval === "15m" } });
  }, [interval]);

  // Push data (initial render and every poll).
  useEffect(() => {
    const r = refs.current;
    if (!r) return;

    const bars = sanitize(candles, isValidCandle);
    r.candles.setData(
      bars.map((c) => ({
        time: toTime(c.time),
        open: c.open,
        high: c.high,
        low: c.low,
        close: c.close,
      })),
    );
    r.volume.setData(
      bars.map((c) => ({
        time: toTime(c.time),
        value: Number.isFinite(c.volume) ? c.volume : 0,
        color: c.close >= c.open ? COLORS.upVolume : COLORS.downVolume,
      })),
    );
    for (const line of SMA_LINES) {
      r.sma[line.key].setData(toLineData(overlays[line.key]));
    }

    // Replace price lines: zones first, then the trade plan on top.
    for (const pl of priceLinesRef.current) r.candles.removePriceLine(pl);
    const lines: IPriceLine[] = [];

    const support = zones.filter((z) => z.kind === "support");
    const resistance = zones.filter((z) => z.kind === "resistance");
    zones.slice(0, MAX_ZONES).forEach((zone) => {
      const color = zone.kind === "support" ? COLORS.support : COLORS.resistance;
      const index = (zone.kind === "support" ? support : resistance).indexOf(zone) + 1;
      const tag = `${zone.kind === "support" ? "S" : "R"}${index}`;
      lines.push(
        r.candles.createPriceLine({
          price: zone.high,
          color,
          lineWidth: 1,
          lineStyle: LineStyle.Dotted,
          axisLabelVisible: false,
          title: zone.kind === "resistance" ? tag : "",
        }),
        r.candles.createPriceLine({
          price: zone.low,
          color,
          lineWidth: 1,
          lineStyle: LineStyle.Dotted,
          axisLabelVisible: false,
          title: zone.kind === "support" ? tag : "",
        }),
      );
    });

    if (setup) {
      lines.push(
        r.candles.createPriceLine({
          price: setup.stop,
          color: COLORS.down,
          lineWidth: 1,
          lineStyle: LineStyle.Dashed,
          axisLabelVisible: true,
          title: "SL",
        }),
        r.candles.createPriceLine({
          price: setup.entry,
          color: COLORS.entry,
          lineWidth: 1,
          lineStyle: LineStyle.Dashed,
          axisLabelVisible: true,
          title: "Entry",
        }),
        r.candles.createPriceLine({
          price: setup.target,
          color: COLORS.up,
          lineWidth: 1,
          lineStyle: LineStyle.Dashed,
          axisLabelVisible: true,
          title: "TP",
        }),
      );
    }
    priceLinesRef.current = lines;

    if (!fittedRef.current && bars.length > 0) {
      r.chart.timeScale().fitContent();
      fittedRef.current = true;
    }
  }, [candles, overlays, zones, setup]);

  // SMA legend toggles.
  useEffect(() => {
    const r = refs.current;
    if (!r) return;
    for (const line of SMA_LINES) r.sma[line.key].applyOptions({ visible: visible[line.key] });
  }, [visible]);

  const last = candles[candles.length - 1];
  const shown: HoverBar | null =
    hover ?? (last ? { open: last.open, high: last.high, low: last.low, close: last.close } : null);

  return (
    <div className={cn("flex flex-col", className)}>
      <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2 px-4 pb-2">
        <div role="group" aria-label="Moving averages" className="flex flex-wrap gap-1.5">
          {SMA_LINES.map((line) => (
            <button
              key={line.key}
              type="button"
              aria-pressed={visible[line.key]}
              onClick={() => setVisible((v) => ({ ...v, [line.key]: !v[line.key] }))}
              className={cn(
                "inline-flex items-center gap-1.5 rounded-md border px-2 py-1 text-[11px] font-medium transition-colors",
                visible[line.key]
                  ? "border-line-strong bg-surface-2 text-fg"
                  : "border-line text-subtle line-through",
              )}
            >
              <span
                aria-hidden="true"
                className="h-0.5 w-3 rounded-full"
                style={{ backgroundColor: line.color, opacity: visible[line.key] ? 1 : 0.35 }}
              />
              {line.label}
            </button>
          ))}
        </div>
        {shown ? (
          <p className="text-[11px] text-muted tabular-nums" aria-hidden="true">
            O <span className="text-fg">{formatNumber(shown.open)}</span> H{" "}
            <span className="text-fg">{formatNumber(shown.high)}</span> L{" "}
            <span className="text-fg">{formatNumber(shown.low)}</span> C{" "}
            <span className={shown.close >= shown.open ? "text-up" : "text-down"}>
              {formatNumber(shown.close)}
            </span>
          </p>
        ) : null}
      </div>
      <div
        ref={containerRef}
        role="img"
        aria-label={`Candlestick chart, ${candles.length} bars, with volume, moving averages, key levels and trade plan lines.`}
        className="h-[360px] w-full sm:h-[440px] lg:h-[500px]"
      />
      <ChartKey hasSetup={setup !== null} hasZones={zones.length > 0} />
    </div>
  );
}

function ChartKey({ hasSetup, hasZones }: { hasSetup: boolean; hasZones: boolean }) {
  if (!hasSetup && !hasZones) return null;
  return (
    <ul className="flex flex-wrap gap-x-4 gap-y-1 px-4 pt-2 text-[11px] text-muted">
      {hasSetup ? (
        <>
          <li className="flex items-center gap-1.5">
            <span aria-hidden="true" className="w-3 border-t border-dashed border-down" /> Stop loss
          </li>
          <li className="flex items-center gap-1.5">
            <span aria-hidden="true" className="w-3 border-t border-dashed border-fg/80" /> Entry
          </li>
          <li className="flex items-center gap-1.5">
            <span aria-hidden="true" className="w-3 border-t border-dashed border-up" /> Target
          </li>
        </>
      ) : null}
      {hasZones ? (
        <li className="flex items-center gap-1.5">
          <span aria-hidden="true" className="w-3 border-t border-dotted border-muted" /> Support
          (S) / resistance (R) zones
        </li>
      ) : null}
    </ul>
  );
}
