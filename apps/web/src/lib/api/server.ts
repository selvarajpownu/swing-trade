import "server-only";

import { ApiError } from "./errors";
import type {
  HeadlinesResponse,
  Interval,
  HealthResponse,
  Market,
  StockNewsResponse,
  ScanResponse,
  SetupPreset,
  SetupState,
  StockDetailResponse,
  StocksResponse,
} from "./types";

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
const NEWS_TIMEOUT_MS = 20_000;

/** `in` is the API default, so it stays out of URLs. */
function marketParam(market: Market | undefined): string | undefined {
  return market && market !== "in" ? market : undefined;
}

type QueryValue = string | number | undefined;

interface RequestOptions {
  query?: Record<string, QueryValue>;
  /** Send the bearer token (all /v1/* routes). */
  auth?: boolean;
  timeoutMs?: number;
}

export interface GetStocksParams {
  market?: Market;
  /** Symbols; omit for the market's default watchlist. */
  symbols?: string[];
  capital?: number;
  /** Risk per trade in PERCENT (1 = 1%). Converted to a fraction for the API. */
  riskPct?: number;
}

export interface GetStockParams {
  market?: Market;
  interval?: Interval;
  capital?: number;
  /** Risk per trade in PERCENT (1 = 1%). Converted to a fraction for the API. */
  riskPct?: number;
}

export interface GetScanParams {
  preset?: SetupPreset;
  minScore?: number;
  minRr?: number;
  state?: SetupState;
  limit?: number;
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
      market: marketParam(params.market),
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
      market: marketParam(params.market),
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

/** GET /v1/stocks/{symbol}/news (INDmoney headlines with sentiment). */
export async function getStockNews(
  symbol: string,
  market: Market = "in",
): Promise<StockNewsResponse> {
  const data = await request<StockNewsResponse>(`/v1/stocks/${encodeURIComponent(symbol)}/news`, {
    query: { market: marketParam(market) },
  });
  assertShape(Array.isArray(data.items), "`items`");
  return data;
}

/** GET /v1/news (Google News headlines for a market's watchlist or one stock). */
export async function getHeadlines(
  market: Market = "in",
  symbol?: string,
): Promise<HeadlinesResponse> {
  const data = await request<HeadlinesResponse>("/v1/news", {
    query: { market: marketParam(market), symbol },
    // One feed per watchlist stock, three at a time, on a cold cache.
    timeoutMs: NEWS_TIMEOUT_MS,
  });
  assertShape(Array.isArray(data.items), "`items`");
  return data;
}

/** GET /v1/scan */
export async function getScan(params: GetScanParams = {}): Promise<ScanResponse> {
  const data = await request<ScanResponse>("/v1/scan", {
    query: {
      preset: params.preset,
      min_score: params.minScore || undefined,
      min_rr: params.minRr || undefined,
      state: params.state,
      limit: params.limit,
      capital: params.capital,
      risk: riskFraction(params.riskPct),
    },
  });
  assertShape(Array.isArray(data.items), "`items`");
  assertShape(typeof data.status === "object" && data.status !== null, "`status`");
  return data;
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
