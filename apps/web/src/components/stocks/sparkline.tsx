import { cn } from "@/lib/utils";

interface SparklineProps {
  values: readonly number[];
  width?: number;
  height?: number;
  className?: string;
}

/** Tiny SVG line of recent closes. Green if the last close is above the first. */
export function Sparkline({ values, width = 72, height = 24, className }: SparklineProps) {
  const points = values.filter((v) => Number.isFinite(v));
  if (points.length < 2) {
    return (
      <span
        aria-hidden="true"
        className={cn("inline-block", className)}
        style={{ width, height }}
      />
    );
  }

  const min = Math.min(...points);
  const max = Math.max(...points);
  const range = max - min || 1;
  const pad = 2;
  const stepX = (width - pad * 2) / (points.length - 1);
  const coords = points.map((v, i) => {
    const x = pad + i * stepX;
    const y = pad + (1 - (v - min) / range) * (height - pad * 2);
    return `${x.toFixed(1)},${y.toFixed(1)}`;
  });
  const first = points[0] ?? 0;
  const last = points[points.length - 1] ?? 0;
  const up = last >= first;

  return (
    <svg
      aria-hidden="true"
      width={width}
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      className={cn("shrink-0 overflow-visible", up ? "text-up" : "text-down", className)}
    >
      <polyline
        points={coords.join(" ")}
        fill="none"
        stroke="currentColor"
        strokeWidth={1.5}
        strokeLinejoin="round"
        strokeLinecap="round"
      />
    </svg>
  );
}
