/**
 * Browser-side fetchers. They call our own route handlers under /api, which
 * proxy to the Rust API and add the token on the server.
 */
import { ApiError } from "./errors";
import { MARKETS } from "../markets";
import type { PlanParams } from "./params";
import { writePlanParams } from "./params";
import type { Interval, Market, ScanResponse, StockDetailResponse, StocksResponse } from "./types";

async function fetchJson<T>(url: string, signal?: AbortSignal): Promise<T> {
  let response: Response;
  try {
    response = await fetch(url, {
      signal,
      cache: "no-store",
      headers: { Accept: "application/json" },
    });
  } catch (err) {
    if (signal?.aborted) throw err;
    throw new ApiError("network", "Network error while refreshing data.", 0);
  }
  let body: unknown = null;
  try {
    body = await response.json();
  } catch {
    // handled below
  }
  if (!response.ok) throw ApiError.fromResponse(response.status, body);
  if (typeof body !== "object" || body === null) {
    throw new ApiError("invalid_response", "Empty response.", response.status);
  }
  return body as T;
}

export function stocksUrl(plan?: PlanParams, market: Market = "in"): string {
  const query = new URLSearchParams();
  if (market !== "in") query.set("market", market);
  writePlanParams(plan, query, MARKETS[market].defaultCapital);
  const qs = query.toString();
  return qs ? `/api/stocks?${qs}` : "/api/stocks";
}

export function stockUrl(
  symbol: string,
  interval: Interval,
  plan?: PlanParams,
  market: Market = "in",
): string {
  const query = new URLSearchParams({ interval });
  if (market !== "in") query.set("market", market);
  writePlanParams(plan, query, MARKETS[market].defaultCapital);
  return `/api/stocks/${encodeURIComponent(symbol)}?${query.toString()}`;
}

export function fetchStocks(url: string, signal?: AbortSignal): Promise<StocksResponse> {
  return fetchJson<StocksResponse>(url, signal);
}

export function fetchScan(url: string, signal?: AbortSignal): Promise<ScanResponse> {
  return fetchJson<ScanResponse>(url, signal);
}

export function fetchStock(url: string, signal?: AbortSignal): Promise<StockDetailResponse> {
  return fetchJson<StockDetailResponse>(url, signal);
}
