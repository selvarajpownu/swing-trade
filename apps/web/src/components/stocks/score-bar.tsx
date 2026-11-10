import { clamp, cn } from "@/lib/utils";

interface ScoreBarProps {
  score: number | null | undefined;
  muted?: boolean;
  className?: string;
}

/** 0-100 score with a thin bar underneath. */
export function ScoreBar({ score, muted = false, className }: ScoreBarProps) {
  if (score === null || score === undefined || !Number.isFinite(score)) {
    return <span className={cn("text-subtle", className)}>—</span>;
  }
  const value = clamp(Math.round(score), 0, 100);
  const tone = muted ? "bg-subtle" : value >= 70 ? "bg-up" : value >= 50 ? "bg-warn" : "bg-down";
  return (
    <div className={cn("w-16", className)}>
      <span className={cn("text-sm font-semibold tabular-nums", muted && "text-muted")}>
        {value}
      </span>
      <div
        role="meter"
        aria-label="Setup score"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={value}
        className="mt-1 h-1 w-full overflow-hidden rounded-full bg-surface-3"
      >
        <div className={cn("h-full rounded-full", tone)} style={{ width: `${value}%` }} />
      </div>
    </div>
  );
}
