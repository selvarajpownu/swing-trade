import type { NextRequest } from "next/server";

import { isInterval, parsePlanParams, parseSymbol } from "@/lib/api/params";
import { apiErrorResponse, badRequest, jsonNoStore } from "@/lib/api/proxy";
import { getStock } from "@/lib/api/server";
import type { Interval } from "@/lib/api/types";

export const dynamic = "force-dynamic";

/**
 * GET /api/stocks/{symbol}?interval=1d&capital=500000&risk=1
 * Proxies to the Rust API's GET /v1/stocks/{symbol}. `risk` is a percent here.
 */
export async function GET(
  request: NextRequest,
  { params }: { params: Promise<{ symbol: string }> },
) {
  const { symbol: rawSymbol } = await params;
  const symbol = parseSymbol(rawSymbol);
  if (!symbol) return badRequest("Invalid symbol.");

  const search = request.nextUrl.searchParams;
  const rawInterval = search.get("interval");
  let interval: Interval | undefined;
  if (rawInterval !== null) {
    if (!isInterval(rawInterval)) return badRequest("`interval` must be one of 1d, 1w, 15m.");
    interval = rawInterval;
  }
  const plan = parsePlanParams((key) => search.get(key));

  try {
    const data = await getStock(symbol, {
      interval,
      capital: plan.capital,
      riskPct: plan.riskPct,
    });
    return jsonNoStore(data);
  } catch (err) {
    return apiErrorResponse(err, `api/stocks/${symbol}`);
  }
}
