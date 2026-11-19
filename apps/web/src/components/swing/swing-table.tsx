import Link from "next/link";

import type { PlanParams } from "@/lib/api/params";
import type { SwingItem } from "@/lib/api/types";
import { formatINR, formatInteger, formatNumber } from "@/lib/format";
import { stockHref } from "@/lib/links";
import { cn } from "@/lib/utils";

import { ScoreBar } from "@/components/stocks/score-bar";
import { Sparkline } from "@/components/stocks/sparkline";
import { Badge } from "@/components/ui/badge";
import { PriceChange } from "@/components/ui/price-change";
import { SymbolAvatar } from "@/components/ui/symbol-avatar";

import { CatalystBadges } from "./catalysts";
import { formatBarDate } from "./regime-banner";

const TH = "px-3 py-3 text-xs font-medium whitespace-nowrap text-muted";
const TD = "px-3 py-3 align-middle whitespace-nowrap";

function rsTone(rs: number | null): string {
  if (rs === null) return "text-subtle";
  return rs >= 80 ? "text-up" : rs >= 70 ? "text-fg" : "text-muted";
}

/** Flags worth a glance: failed gates and a delivery jump. */
function Flags({ item }: { item: SwingItem }) {
  const jump =
    item.delivery_pct !== null &&
    item.delivery_avg20 !== null &&
    item.delivery_avg20 > 0 &&
    item.delivery_pct >= item.delivery_avg20 * 1.2 &&
    item.change > 0;
  return (
    <span className="flex flex-wrap gap-1">
      {item.setup ? (
        <Badge tone={item.setup.kind === "pullback" ? "accent" : "up"}>{item.setup.label}</Badge>
      ) : null}
      {jump ? (
        <Badge tone="up" title="Delivery share at least 20% above its 20-day average on an up day">
          Delivery ↑
        </Badge>
      ) : null}
      {!item.gates.tradable ? (
        <Badge
          tone="warn"
          title="Fails the Tradable gate: low turnover, a restricted series, or on a surveillance list"
        >
          Thin / restricted
        </Badge>
      ) : null}
      {!item.gates.leader ? <Badge tone="muted">Not a leader</Badge> : null}
      {item.gates.sector === false ? <Badge tone="muted">Weak sector</Badge> : null}
    </span>
  );
}

interface SwingTableProps {
  rows: SwingItem[];
  plan: PlanParams;
}

/** Desktop table (md and up) and a compact list for phones. */
export function SwingTable({ rows, plan }: SwingTableProps) {
  return (
    <>
      {/* `relative` contains the sr-only labels so they cannot widen the page. */}
      <div className="relative hidden overflow-x-auto md:block">
        <table className="w-full min-w-[1280px] border-collapse text-sm">
          <caption className="sr-only">
            Swing candidates with relative strength, setup score and trade plan.
          </caption>
          <thead>
            <tr className="border-b border-line bg-surface-2/50 text-left">
              <th scope="col" className={TH}>
                Stock
              </th>
              <th scope="col" className={cn(TH, "text-right")}>
                Close / 1D
              </th>
              <th scope="col" className={cn(TH, "text-right")} title="Relative strength, 0-100">
                RS
              </th>
              <th scope="col" className={cn(TH, "text-right")} title="3-month return">
                3M
              </th>
              <th scope="col" className={cn(TH, "text-right")} title="Median 20-day turnover">
                Turnover
              </th>
              <th scope="col" className={cn(TH, "text-right")} title="Delivery share of volume">
                Delivery
              </th>
              <th scope="col" className={TH} title="Setup, flags and recent or upcoming events">
                Setup / events
              </th>
              <th scope="col" className={TH}>
                Score
              </th>
              <th scope="col" className={cn(TH, "text-right")}>
                Stop / Entry
              </th>
              <th scope="col" className={cn(TH, "text-right")}>
                2R / 3R
              </th>
              <th scope="col" className={cn(TH, "text-right")}>
                Qty
              </th>
              <th
                scope="col"
                className={cn(TH, "text-right")}
                title="Six weeks after the setup bar"
              >
                Review by
              </th>
            </tr>
          </thead>
          <tbody>
            {rows.map((item) => {
              const s = item.setup;
              return (
                <tr
                  key={item.symbol}
                  className="border-b border-line last:border-b-0 hover:bg-surface-2/40"
                >
                  <td className={TD}>
                    <div className="flex items-center gap-3">
                      <SymbolAvatar symbol={item.symbol} name={item.name} />
                      <div className="min-w-0">
                        <Link
                          href={stockHref(item.symbol, plan)}
                          className="block max-w-[220px] truncate font-semibold text-fg hover:text-accent"
                        >
                          {item.name}
                        </Link>
                        <span className="block max-w-[220px] truncate text-xs text-muted">
                          {item.symbol}
                          {item.industry ? ` · ${item.industry}` : ""}
                        </span>
                      </div>
                      <Sparkline values={item.sparkline} width={64} height={22} />
                    </div>
                  </td>
                  <td className={cn(TD, "text-right")}>
                    <div className="font-semibold tabular-nums">{formatINR(item.price)}</div>
                    <PriceChange
                      change={item.change}
                      changePct={item.change_pct}
                      className="text-xs"
                    />
                  </td>
                  <td className={cn(TD, "text-right font-semibold tabular-nums", rsTone(item.rs))}>
                    {item.rs ?? "—"}
                  </td>
                  <td
                    className={cn(
                      TD,
                      "text-right tabular-nums",
                      (item.ret_3m_pct ?? 0) >= 0 ? "text-up" : "text-down",
                    )}
                  >
                    {item.ret_3m_pct === null
                      ? "—"
                      : `${item.ret_3m_pct > 0 ? "+" : ""}${item.ret_3m_pct.toFixed(1)}%`}
                  </td>
                  <td className={cn(TD, "text-right tabular-nums")}>
                    ₹{formatInteger(item.turnover_cr)} cr
                  </td>
                  <td className={cn(TD, "text-right tabular-nums")}>
                    {item.delivery_pct === null ? "—" : `${item.delivery_pct.toFixed(0)}%`}
                  </td>
                  <td className={cn(TD, "min-w-[250px] whitespace-normal")}>
                    <Flags item={item} />
                    {s ? (
                      <span className="mt-1 block max-w-[240px] text-xs whitespace-normal text-muted">
                        {s.reasons[0]}
                      </span>
                    ) : null}
                    {item.catalysts.length > 0 ? (
                      <span className="mt-1.5 block">
                        <CatalystBadges catalysts={item.catalysts} />
                      </span>
                    ) : null}
                  </td>
                  <td className={TD}>
                    {s ? <ScoreBar score={s.score} /> : <span className="text-subtle">—</span>}
                  </td>
                  <td className={cn(TD, "text-right tabular-nums")}>
                    {s ? (
                      <>
                        <span className="block text-down">{formatNumber(s.stop)}</span>
                        <span className="block font-medium">{formatNumber(s.entry)}</span>
                      </>
                    ) : (
                      <span className="text-subtle">—</span>
                    )}
                  </td>
                  <td className={cn(TD, "text-right tabular-nums")}>
                    {s ? (
                      <>
                        <span className="block text-up">{formatNumber(s.target_2r)}</span>
                        <span className="block text-up/80">{formatNumber(s.target_3r)}</span>
                      </>
                    ) : (
                      <span className="text-subtle">—</span>
                    )}
                  </td>
                  <td className={cn(TD, "text-right tabular-nums")}>
                    {s ? formatInteger(s.quantity) : <span className="text-subtle">—</span>}
                  </td>
                  <td className={cn(TD, "text-right text-xs text-muted")}>
                    {s ? formatBarDate(s.review_by) : <span className="text-subtle">—</span>}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>

      <ul className="divide-y divide-line md:hidden" aria-label="Swing candidates">
        {rows.map((item) => {
          const s = item.setup;
          return (
            <li key={item.symbol}>
              <Link
                href={stockHref(item.symbol, plan)}
                className="block px-4 py-3 transition-colors hover:bg-surface-2"
              >
                <div className="flex items-baseline justify-between gap-2">
                  <span className="truncate font-semibold text-fg">{item.name}</span>
                  <span className="shrink-0 font-semibold tabular-nums">
                    {formatINR(item.price)}
                  </span>
                </div>
                <div className="mt-0.5 flex items-center justify-between gap-2 text-xs text-muted">
                  <span className="truncate">
                    {item.symbol} · RS {item.rs ?? "—"}
                    {s ? ` · score ${s.score}` : ""}
                  </span>
                  <PriceChange
                    change={item.change}
                    changePct={item.change_pct}
                    className="text-xs"
                  />
                </div>
                <div className="mt-2 space-y-1.5">
                  <Flags item={item} />
                  <CatalystBadges catalysts={item.catalysts} max={3} />
                </div>
                {s ? (
                  <p className="mt-2 text-xs text-muted tabular-nums">
                    SL {formatNumber(s.stop)} · Entry {formatNumber(s.entry)} · 2R{" "}
                    {formatNumber(s.target_2r)} · 3R {formatNumber(s.target_3r)} · {s.quantity} sh
                  </p>
                ) : null}
              </Link>
            </li>
          );
        })}
      </ul>
    </>
  );
}
