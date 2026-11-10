import { isCustomPlan, type PlanParams } from "./api/params";
import type { Interval, Market } from "./api/types";
import { MARKETS } from "./markets";

/** Link to a stock page, carrying non-default plan params (and interval) along. */
export function stockHref(
  symbol: string,
  plan?: PlanParams,
  interval?: Interval,
  market: Market = "in",
): string {
  const base = `${MARKETS[market].path}/${encodeURIComponent(symbol)}`;
  const query = new URLSearchParams();
  if (interval && interval !== "1d") query.set("interval", interval);
  if (plan && isCustomPlan(plan, MARKETS[market].defaultCapital)) {
    query.set("capital", String(plan.capital));
    query.set("risk", String(plan.riskPct));
  }
  const qs = query.toString();
  return qs ? `${base}?${qs}` : base;
}
