/**
 * Scanner filters and their URL form (`preset`, `min_score`, `min_rr`, `state`).
 * Pure: used on the server (page, route handler) and in the browser.
 */
import type { PlanParams } from "./api/params";
import { writePlanParams } from "./api/params";
import type { SetupPreset, SetupState } from "./api/types";

export const SCAN_PRESETS: readonly { id: SetupPreset | "any"; label: string }[] = [
  { id: "any", label: "Any setup" },
  { id: "pullback", label: "Pullback" },
  { id: "breakout", label: "Breakout" },
  { id: "support_bounce", label: "Support bounce" },
  { id: "oversold", label: "Oversold" },
];

/** Rows the Scanner page asks for; the scan itself may find more. */
export const SCAN_PAGE_LIMIT = 200;

export const MIN_SCORE_OPTIONS = [0, 40, 50, 60, 70, 80] as const;
export const MIN_RR_OPTIONS = [0, 1.5, 2, 2.5, 3] as const;

export interface ScanFilters {
  preset: SetupPreset | "any";
  minScore: number;
  minRr: number;
  /** Only `watching` setups (hide plans with risk-reward below the minimum). */
  watchingOnly: boolean;
}

export const DEFAULT_SCAN_FILTERS: ScanFilters = {
  preset: "any",
  minScore: 0,
  minRr: 0,
  watchingOnly: false,
};

/** Filters for the Home page's "Top setups". */
export const TOP_SETUP_FILTERS: ScanFilters = {
  preset: "any",
  minScore: 0,
  minRr: 2,
  watchingOnly: true,
};

function isPreset(value: unknown): value is SetupPreset | "any" {
  return typeof value === "string" && SCAN_PRESETS.some((p) => p.id === value);
}

function pick<T extends number>(raw: string | null | undefined, options: readonly T[]): T | null {
  if (!raw) return null;
  const n = Number(raw);
  return (options as readonly number[]).includes(n) ? (n as T) : null;
}

export function parseScanFilters(get: (key: string) => string | undefined | null): ScanFilters {
  const preset = get("preset");
  return {
    preset: isPreset(preset) ? preset : DEFAULT_SCAN_FILTERS.preset,
    minScore: pick(get("min_score"), MIN_SCORE_OPTIONS) ?? DEFAULT_SCAN_FILTERS.minScore,
    minRr: pick(get("min_rr"), MIN_RR_OPTIONS) ?? DEFAULT_SCAN_FILTERS.minRr,
    watchingOnly: get("state") === "watching",
  };
}

/** Writes non-default filters into `query`. */
export function writeScanFilters(filters: ScanFilters, query: URLSearchParams): void {
  const set = (key: string, value: string | null) => {
    if (value === null) query.delete(key);
    else query.set(key, value);
  };
  set("preset", filters.preset === "any" ? null : filters.preset);
  set("min_score", filters.minScore > 0 ? String(filters.minScore) : null);
  set("min_rr", filters.minRr > 0 ? String(filters.minRr) : null);
  set("state", filters.watchingOnly ? "watching" : null);
}

/** Arguments for `getScan` in lib/api/server.ts. */
export function scanApiParams(filters: ScanFilters): {
  preset?: SetupPreset;
  minScore?: number;
  minRr?: number;
  state?: SetupState;
} {
  return {
    preset: filters.preset === "any" ? undefined : filters.preset,
    minScore: filters.minScore || undefined,
    minRr: filters.minRr || undefined,
    state: filters.watchingOnly ? "watching" : undefined,
  };
}

/** Browser URL of our /api/scan proxy for these filters. */
export function scanUrl(filters: ScanFilters, plan?: PlanParams, limit?: number): string {
  const query = new URLSearchParams();
  writeScanFilters(filters, query);
  writePlanParams(plan, query);
  if (limit) query.set("limit", String(limit));
  const qs = query.toString();
  return qs ? `/api/scan?${qs}` : "/api/scan";
}
