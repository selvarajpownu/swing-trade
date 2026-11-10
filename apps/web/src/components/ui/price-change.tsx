import { direction, formatChange } from "@/lib/format";
import { cn } from "@/lib/utils";

interface PriceChangeProps {
  change: number;
  changePct: number;
  className?: string;
}

/** "+26.93 (▲5.30%)" in green, "-4.10 (▼0.82%)" in red. */
export function PriceChange({ change, changePct, className }: PriceChangeProps) {
  const dir = direction(change);
  const label = dir === "up" ? "up" : dir === "down" ? "down" : "unchanged";
  return (
    <span
      className={cn(
        "tabular-nums",
        dir === "up" && "text-up",
        dir === "down" && "text-down",
        dir === "flat" && "text-muted",
        className,
      )}
    >
      <span className="sr-only">{`Change today, ${label}: `}</span>
      {formatChange(change, changePct)}
    </span>
  );
}
