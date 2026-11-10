import "server-only";

import { ApiError } from "./errors";
import type { Interval, HealthResponse, StockDetailResponse, StocksResponse } from "./types";

/**
 * Server-side client for the Rust API (services/api).
 *
 * Only import this from server components and route handlers: it reads
 * API_TOKEN, which must never reach the browser. The `server-only` import
 * makes the build fail if a client component imports this module.
 */

const DEFAULT_API_URL = "http://localhost:8080";
const DEFAULT_TIMEOUT_MS = 10_000;
const HEALTH_TIMEOUT_MS = 4_000;

type QueryValue = string | number | undefined;

interface RequestOptions {
  query?: Record<string, QueryValue>;
  /** Send the bearer token (all /v1/* routes). */
  auth?: boolean;
  timeoutMs?: number;
}

export interface GetStocksParams {
  /** NSE symbols; omit for the server's default WATCHLIST. */
  symbols?: string[];
  capital?: number;
  /** Risk per trade in PERCENT (1 = 1%). Converted to a fraction for the API. */
  riskPct?: number;
}

export interface GetStockParams {
  interval?: Interval;
  capital?: number;
  /** Risk per trade in PERCENT (1 = 1%). Converted to a fraction for the API. */
  riskPct?: number;
}

export function apiBaseUrl(): string {
  const raw = process.env.API_URL?.trim() || DEFAULT_API_URL;
  return raw.replace(/\/+$/, "");
}

function isTimeoutError(err: unknown): boolean {
  if (typeof err !== "object" || err === null) return false;
  const name = (err as { name?: unknown }).name;
  return name === "TimeoutError" || name === "AbortError";
}

function riskFraction(riskPct: number | undefined): string | undefined {
  if (riskPct === undefined) return undefined;
  // Avoid float noise like 0.015000000000000001 in the query string.
  return String(Math.round(riskPct * 1000) / 100_000);
}

async function request<T>(path: string, options: RequestOptions = {}): Promise<T> {
  const { query, auth = true, timeoutMs = DEFAULT_TIMEOUT_MS } = options;

  let url: URL;
  try {
    url = new URL(`${apiBaseUrl()}${path}`);
  } catch {
    throw new ApiError("not_configured", "API_URL is not a valid URL.", 500);
  }
  if (query) {
    for (const [key, value] of Object.entries(query)) {
      if (value !== undefined && value !== "") url.searchParams.set(key, String(value));
    }
  }

  const headers: Record<string, string> = { Accept: "application/json" };
  if (auth) {
    const token = process.env.API_TOKEN?.trim();
    if (token) headers.Authorization = `Bearer ${token}`;
  }

  let response: Response;
  try {
    response = await fetch(url, {
      method: "GET",
      headers,
      cache: "no-store",
      signal: AbortSignal.timeout(timeoutMs),
    });
  } catch (err) {
    if (isTimeoutError(err)) {
      throw new ApiError("timeout", `The API did not respond within ${timeoutMs / 1000}s.`, 504);
    }
    throw new ApiError("network", `Could not reach the API at ${url.origin}.`, 502);
  }

  let body: unknown = null;
  try {
    body = await response.json();
  } catch {
    if (response.ok) {
      throw new ApiError("invalid_response", "The API returned a non-JSON response.", 502);
    }
  }

  if (!response.ok) {
    throw ApiError.fromResponse(response.status, body);
  }
  if (typeof body !== "object" || body === null) {
    throw new ApiError("invalid_response", "The API returned an empty response.", 502);
  }
  return body as T;
}

function assertShape(condition: boolean, what: string): void {
  if (!condition) {
    throw new ApiError("invalid_response", `The API response is missing ${what}.`, 502);
  }
}

/** GET /v1/stocks */
export async function getStocks(params: GetStocksParams = {}): Promise<StocksResponse> {
  const data = await request<StocksResponse>("/v1/stocks", {
    query: {
      symbols: params.symbols?.length ? params.symbols.join(",") : undefined,
      capital: params.capital,
      risk: riskFraction(params.riskPct),
    },
  });
  assertShape(Array.isArray(data.items), "`items`");
  return data;
}

/** GET /v1/stocks/{symbol} */
export async function getStock(
  symbol: string,
  params: GetStockParams = {},
): Promise<StockDetailResponse> {
  const data = await request<StockDetailResponse>(`/v1/stocks/${encodeURIComponent(symbol)}`, {
    query: {
      interval: params.interval,
      capital: params.capital,
      risk: riskFraction(params.riskPct),
    },
  });
  assertShape(typeof data.quote === "object" && data.quote !== null, "`quote`");
  assertShape(Array.isArray(data.candles), "`candles`");
  return {
    ...data,
    overlays: {
      sma20: data.overlays?.sma20 ?? [],
      sma50: data.overlays?.sma50 ?? [],
      sma200: data.overlays?.sma200 ?? [],
    },
    zones: Array.isArray(data.zones) ? data.zones : [],
  };
}

/** GET /health (no auth). */
export async function getHealth(): Promise<HealthResponse> {
  return request<HealthResponse>("/health", { auth: false, timeoutMs: HEALTH_TIMEOUT_MS });
}

/** Whether API_TOKEN is set (for the settings page; never returns the token). */
export function isApiTokenConfigured(): boolean {
  return Boolean(process.env.API_TOKEN?.trim());
}

export { ApiError, isApiError } from "./errors";
