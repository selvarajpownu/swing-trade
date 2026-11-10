/**
 * Parsing and validation of user-supplied parameters (URL search params and
 * route segments). Pure functions: safe on both server and client.
 *
 * Within the web app, `risk` is a PERCENT (1 = 1% of capital). The backend
 * expects a fraction (0.01); the conversion happens in lib/api/server.ts.
 */
import type { Interval } from "./types";

export type SearchParamValue = string | string[] | undefined;
export type SearchParamsRecord = Record<string, SearchParamValue>;

export const DEFAULT_CAPITAL = 500_000;
export const MIN_CAPITAL = 1_000;
export const MAX_CAPITAL = 10_000_000_000;

export const DEFAULT_RISK_PCT = 1;
export const MIN_RISK_PCT = 0.1;
export const MAX_RISK_PCT = 10;

export const INTERVALS: readonly Interval[] = ["1d", "1w", "15m"];
export const DEFAULT_INTERVAL: Interval = "1d";

/** NSE-style symbols: letters, digits, and a few separators (M&M, BAJAJ-AUTO). */
const SYMBOL_RE = /^[A-Z0-9][A-Z0-9&._-]{0,24}$/;

export function firstParam(value: SearchParamValue): string | undefined {
  if (Array.isArray(value)) return value[0];
  return value;
}

/** Returns the normalised symbol, or null when it is not a valid symbol. */
export function parseSymbol(raw: string | undefined | null): string | null {
  if (!raw) return null;
  let decoded = raw;
  try {
    decoded = decodeURIComponent(raw);
  } catch {
    // Malformed escape sequence: keep the raw value, the regex will reject it.
  }
  const symbol = decoded.trim().toUpperCase();
  return SYMBOL_RE.test(symbol) ? symbol : null;
}

export function isInterval(value: unknown): value is Interval {
  return typeof value === "string" && (INTERVALS as readonly string[]).includes(value);
}

export function parseInterval(raw: string | undefined | null): Interval {
  const value = raw?.trim().toLowerCase();
  return isInterval(value) ? value : DEFAULT_INTERVAL;
}

function parseNumberInRange(
  raw: string | undefined | null,
  fallback: number,
  min: number,
  max: number,
): number {
  if (raw === undefined || raw === null || raw.trim() === "") return fallback;
  const value = Number(raw.replace(/,/g, ""));
  if (!Number.isFinite(value)) return fallback;
  return Math.min(max, Math.max(min, value));
}

export function parseCapital(raw: string | undefined | null): number {
  return Math.round(parseNumberInRange(raw, DEFAULT_CAPITAL, MIN_CAPITAL, MAX_CAPITAL));
}

export function parseRiskPct(raw: string | undefined | null): number {
  const value = parseNumberInRange(raw, DEFAULT_RISK_PCT, MIN_RISK_PCT, MAX_RISK_PCT);
  return Math.round(value * 100) / 100;
}

/** Comma-separated symbol list; invalid entries are dropped. Max 100 symbols. */
export function parseSymbolList(raw: string | undefined | null): string[] | undefined {
  if (!raw) return undefined;
  const symbols = raw
    .split(",")
    .map((s) => parseSymbol(s))
    .filter((s): s is string => s !== null);
  const unique = Array.from(new Set(symbols)).slice(0, 100);
  return unique.length > 0 ? unique : undefined;
}

export interface PlanParams {
  capital: number;
  riskPct: number;
}

export function parsePlanParams(get: (key: string) => string | undefined | null): PlanParams {
  return {
    capital: parseCapital(get("capital")),
    riskPct: parseRiskPct(get("risk")),
  };
}

/** True when the plan params differ from the defaults (so they belong in a URL). */
export function isCustomPlan({ capital, riskPct }: PlanParams): boolean {
  return capital !== DEFAULT_CAPITAL || riskPct !== DEFAULT_RISK_PCT;
}
