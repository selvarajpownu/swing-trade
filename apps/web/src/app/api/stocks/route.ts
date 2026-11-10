import type { NextRequest } from "next/server";

import { parsePlanParams, parseSymbolList } from "@/lib/api/params";
import { apiErrorResponse, badRequest, jsonNoStore } from "@/lib/api/proxy";
import { getStocks } from "@/lib/api/server";
import { MARKETS, parseMarket } from "@/lib/markets";

export const dynamic = "force-dynamic";

/**
 * GET /api/stocks?symbols=A,B&capital=500000&risk=1
 * Proxies to the Rust API's GET /v1/stocks, adding the token server-side.
 * `risk` is a percent here (1 = 1%).
 */
export async function GET(request: NextRequest) {
  const search = request.nextUrl.searchParams;
  const market = parseMarket(search.get("market"));
  const plan = parsePlanParams((key) => search.get(key), MARKETS[market].defaultCapital);

  const rawSymbols = search.get("symbols");
  const symbols = parseSymbolList(rawSymbols);
  if (rawSymbols && !symbols) return badRequest("No valid symbols in `symbols`.");

  try {
    const data = await getStocks({
      market,
      symbols,
      capital: plan.capital,
      riskPct: plan.riskPct,
    });
    return jsonNoStore(data);
  } catch (err) {
    return apiErrorResponse(err, "api/stocks");
  }
}
