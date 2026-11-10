import type { SetupSummary } from "@/lib/api/types";
import { cn } from "@/lib/utils";

import { Badge, type BadgeTone } from "@/components/ui/badge";

const PRESET_TONE: Record<SetupSummary["preset"], BadgeTone> = {
  pullback: "accent",
  breakout: "up",
  support_bounce: "warn",
  oversold: "down",
};

const STATE_LABEL: Record<SetupSummary["state"], string> = {
  watching: "Watching",
  low_risk_reward: "Low R:R",
};

interface SetupBadgeProps {
  setup: Pick<SetupSummary, "preset" | "label" | "state"> | null;
  /** Show the state next to the label. */
  showState?: boolean;
  className?: string;
}

/** Preset label + state. Greyed out when the risk-reward is too low. */
export function SetupBadge({ setup, showState = true, className }: SetupBadgeProps) {
  if (!setup) {
    return <span className={cn("text-xs text-subtle", className)}>No setup</span>;
  }
  const lowRR = setup.state === "low_risk_reward";
  return (
    <Badge
      tone={lowRR ? "muted" : PRESET_TONE[setup.preset]}
      className={cn(lowRR && "opacity-70", className)}
      title={lowRR ? "Risk-reward below the minimum" : undefined}
    >
      <span>{setup.label}</span>
      {showState ? (
        <>
          <span aria-hidden="true" className="opacity-50">
            ·
          </span>
          <span className="font-normal opacity-80">{STATE_LABEL[setup.state]}</span>
        </>
      ) : null}
    </Badge>
  );
}
