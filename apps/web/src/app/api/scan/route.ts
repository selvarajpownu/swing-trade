import type { NextRequest } from "next/server";

import { parsePlanParams } from "@/lib/api/params";
import { apiErrorResponse, jsonNoStore } from "@/lib/api/proxy";
import { getScan } from "@/lib/api/server";
import { parseScanFilters, scanApiParams } from "@/lib/scan-view";

export const dynamic = "force-dynamic";

/**
 * GET /api/scan?preset=&min_score=&min_rr=&state=&limit=&capital=&risk=
 * Proxies to the Rust API's GET /v1/scan, adding the token server-side.
 * `risk` is a percent here (1 = 1%).
 */
export async function GET(request: NextRequest) {
  const search = request.nextUrl.searchParams;
  const plan = parsePlanParams((key) => search.get(key));
  const filters = parseScanFilters((key) => search.get(key));
  const limit = Number(search.get("limit"));

  try {
    const data = await getScan({
      ...scanApiParams(filters),
      limit: Number.isInteger(limit) && limit >= 1 && limit <= 500 ? limit : undefined,
      capital: plan.capital,
      riskPct: plan.riskPct,
    });
    return jsonNoStore(data);
  } catch (err) {
    return apiErrorResponse(err, "api/scan");
  }
}
