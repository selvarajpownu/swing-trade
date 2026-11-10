"use client";

import { useRouter } from "next/navigation";
import { useTransition } from "react";

import {
  MIN_RR_OPTIONS,
  MIN_SCORE_OPTIONS,
  SCAN_PRESETS,
  writeScanFilters,
  type ScanFilters as Filters,
} from "@/lib/scan-view";
import { cn } from "@/lib/utils";

interface ScanFiltersProps {
  filters: Filters;
}

const SELECT =
  "h-9 rounded-lg border border-line bg-surface px-2 text-sm text-fg focus:border-accent focus:outline-none";

/** Preset chips plus score / risk-reward / state controls, mirrored in the URL. */
export function ScanFilters({ filters }: ScanFiltersProps) {
  const router = useRouter();
  const [pending, startTransition] = useTransition();

  const update = (patch: Partial<Filters>) => {
    const query = new URLSearchParams(window.location.search);
    writeScanFilters({ ...filters, ...patch }, query);
    const qs = query.toString();
    startTransition(() => {
      router.replace(qs ? `/scan?${qs}` : "/scan", { scroll: false });
    });
  };

  return (
    <div
      className={cn(
        "flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between",
        pending && "opacity-70",
      )}
      aria-busy={pending}
    >
      <div
        role="group"
        aria-label="Setup preset"
        className="-mx-4 flex gap-2 overflow-x-auto px-4 pb-1 lg:mx-0 lg:px-0 lg:pb-0"
      >
        {SCAN_PRESETS.map((p) => {
          const active = filters.preset === p.id;
          return (
            <button
              key={p.id}
              type="button"
              aria-pressed={active}
              onClick={() => update({ preset: p.id })}
              className={cn(
                "shrink-0 rounded-full border px-3 py-1.5 text-xs font-medium transition-colors",
                active
                  ? "border-accent/50 bg-accent/15 text-fg"
                  : "border-line bg-surface text-muted hover:border-line-strong hover:text-fg",
              )}
            >
              {p.label}
            </button>
          );
        })}
      </div>

      <div className="flex flex-wrap items-center gap-3 text-sm text-muted">
        <label className="flex items-center gap-2">
          Min score
          <select
            className={SELECT}
            value={filters.minScore}
            onChange={(e) => update({ minScore: Number(e.target.value) })}
          >
            {MIN_SCORE_OPTIONS.map((v) => (
              <option key={v} value={v}>
                {v === 0 ? "Any" : `${v}+`}
              </option>
            ))}
          </select>
        </label>
        <label className="flex items-center gap-2">
          Min R:R
          <select
            className={SELECT}
            value={filters.minRr}
            onChange={(e) => update({ minRr: Number(e.target.value) })}
          >
            {MIN_RR_OPTIONS.map((v) => (
              <option key={v} value={v}>
                {v === 0 ? "Any" : `${v}+`}
              </option>
            ))}
          </select>
        </label>
        <label className="flex items-center gap-2">
          <input
            type="checkbox"
            className="size-4 accent-accent"
            checked={filters.watchingOnly}
            onChange={(e) => update({ watchingOnly: e.target.checked })}
          />
          Hide low R:R plans
        </label>
      </div>
    </div>
  );
}
