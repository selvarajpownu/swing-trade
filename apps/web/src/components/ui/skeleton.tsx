import { cn } from "@/lib/utils";

/** Placeholder block. Pulses only when the user allows motion. */
export function Skeleton({ className }: { className?: string }) {
  return (
    <div
      aria-hidden="true"
      className={cn("rounded-md bg-surface-3 motion-safe:animate-pulse", className)}
    />
  );
}
