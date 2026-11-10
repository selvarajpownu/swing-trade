import type { ComponentPropsWithoutRef } from "react";

import { cn } from "@/lib/utils";

export type BadgeTone = "neutral" | "up" | "down" | "accent" | "warn" | "muted";

const TONES: Record<BadgeTone, string> = {
  neutral: "border-line-strong bg-surface-3 text-fg",
  up: "border-up/30 bg-up/10 text-up",
  down: "border-down/30 bg-down/10 text-down",
  accent: "border-accent/30 bg-accent/10 text-accent",
  warn: "border-warn/30 bg-warn/10 text-warn",
  muted: "border-line bg-surface-2 text-subtle",
};

interface BadgeProps extends ComponentPropsWithoutRef<"span"> {
  tone?: BadgeTone;
}

export function Badge({ tone = "neutral", className, ...props }: BadgeProps) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-[11px] leading-4 font-medium whitespace-nowrap",
        TONES[tone],
        className,
      )}
      {...props}
    />
  );
}
