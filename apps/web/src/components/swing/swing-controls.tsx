"use client";

import { useRouter } from "next/navigation";
import { useEffect, useTransition } from "react";

import type { SwingResponse, SwingStatus } from "@/lib/api/types";
import { SWING_GATES, SWING_TABS, writeSwingView, type SwingView } from "@/lib/swing-view";
import { cn } from "@/lib/utils";

interface SwingControlsProps {
  view: SwingView;
  counts: SwingResponse["counts"] | null;
}

/** List tabs (with counts) and gate toggles, mirrored in the URL. */
export function SwingControls({ view, counts }: SwingControlsProps) {
  const router = useRouter();
  const [pending, startTransition] = useTransition();

  const update = (next: SwingView) => {
    const query = new URLSearchParams(window.location.search);
    writeSwingView(next, query);
    const qs = query.toString();
    startTransition(() => {
      router.replace(qs ? `/swing?${qs}` : "/swing", { scroll: false });
    });
  };

  return (
    <div
      className={cn(
        "flex flex-col gap-3 xl:flex-row xl:items-center xl:justify-between",
        pending && "opacity-70",
      )}
      aria-busy={pending}
    >
      <div
        role="group"
        aria-label="List"
        className="-mx-4 flex gap-2 overflow-x-auto px-4 pb-1 xl:mx-0 xl:px-0 xl:pb-0"
      >
        {SWING_TABS.map((tab) => {
          const active = view.tab === tab.id;
          return (
            <button
              key={tab.id}
              type="button"
              aria-pressed={active}
              title={tab.blurb}
              onClick={() => update({ ...view, tab: tab.id })}
              className={cn(
                "flex shrink-0 items-center gap-1.5 rounded-full border px-3 py-1.5 text-xs font-medium transition-colors",
                active
                  ? "border-accent/50 bg-accent/15 text-fg"
                  : "border-line bg-surface text-muted hover:border-line-strong hover:text-fg",
              )}
            >
              {tab.label}
              {counts ? (
                <span className={cn("tabular-nums", active ? "text-accent" : "text-subtle")}>
                  {counts[tab.id]}
                </span>
              ) : null}
            </button>
          );
        })}
      </div>

      <fieldset className="flex flex-wrap items-center gap-x-4 gap-y-2 text-sm text-muted">
        <legend className="sr-only">Gates a stock must pass</legend>
        <span className="text-xs text-subtle">Must pass:</span>
        {SWING_GATES.map((gate) => (
          <label key={gate.id} className="flex items-center gap-2" title={gate.hint}>
            <input
              type="checkbox"
              className="size-4 accent-accent"
              checked={view.gates[gate.id]}
              onChange={(e) =>
                update({ ...view, gates: { ...view.gates, [gate.id]: e.target.checked } })
              }
            />
            {gate.label}
          </label>
        ))}
      </fieldset>
    </div>
  );
}

/** Re-renders the page every few seconds until the first screen is ready. */
export function RefreshUntilReady({ state }: { state: SwingStatus["state"] }) {
  const router = useRouter();
  useEffect(() => {
    if (state === "ready") return;
    const id = window.setInterval(() => router.refresh(), 8_000);
    return () => window.clearInterval(id);
  }, [state, router]);
  return null;
}
