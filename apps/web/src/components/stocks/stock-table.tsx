import Link from "next/link";
import { ArrowDown, ArrowUp, ArrowUpDown } from "lucide-react";

import type { PlanParams } from "@/lib/api/params";
import type { StockListItem } from "@/lib/api/types";
import { formatINR, formatNumber, formatVolume } from "@/lib/format";
import { stockHref } from "@/lib/links";
import type { ListView, SortKey } from "@/lib/stock-list-view";
import { cn } from "@/lib/utils";

import { PriceChange } from "@/components/ui/price-change";
import { SymbolAvatar } from "@/components/ui/symbol-avatar";

import { ScoreBar } from "./score-bar";
import { SetupBadge } from "./setup-badge";
import { Sparkline } from "./sparkline";

interface StockTableProps {
  rows: StockListItem[];
  view: ListView;
  onSort: (key: SortKey) => void;
  plan: PlanParams;
}

type AriaSort = "ascending" | "descending" | undefined;

function ariaSort(view: ListView, keys: SortKey[]): AriaSort {
  if (!keys.includes(view.sort)) return undefined;
  return view.dir === "asc" ? "ascending" : "descending";
}

interface SortButtonProps {
  sortKey: SortKey;
  label: string;
  view: ListView;
  onSort: (key: SortKey) => void;
}

function SortButton({ sortKey, label, view, onSort }: SortButtonProps) {
  const active = view.sort === sortKey;
  const Icon = !active ? ArrowUpDown : view.dir === "asc" ? ArrowUp : ArrowDown;
  return (
    <button
      type="button"
      onClick={() => onSort(sortKey)}
      className={cn(
        "inline-flex items-center gap-1 rounded px-0.5 font-medium transition-colors hover:text-fg",
        active ? "text-fg" : "text-muted",
      )}
    >
      {label}
      <Icon aria-hidden="true" className={cn("size-3.5", !active && "opacity-60")} />
      <span className="sr-only">
        {active
          ? `, sorted ${view.dir === "asc" ? "ascending" : "descending"}. Click to reverse.`
          : ", click to sort"}
      </span>
    </button>
  );
}

const TH = "px-4 py-3 text-xs font-medium whitespace-nowrap text-muted";
const TD = "px-4 py-3 align-middle";

/** Desktop table (md and up). */
export function StockTable({ rows, view, onSort, plan }: StockTableProps) {
  const sortProps = { view, onSort };

  return (
    <div className="hidden overflow-x-auto md:block">
      <table className="w-full min-w-[1080px] border-collapse text-sm">
        <caption className="sr-only">
          Stocks in the watchlist with their swing setups. Columns with buttons can be sorted.
        </caption>
        <thead>
          <tr className="border-b border-line bg-surface-2/50 text-left">
            <th scope="col" className={TH} aria-sort={ariaSort(view, ["name"])}>
              <SortButton sortKey="name" label="Stock Name" {...sortProps} />
            </th>
            <th
              scope="col"
              className={cn(TH, "text-right")}
              aria-sort={ariaSort(view, ["price", "change"])}
            >
              <SortButton sortKey="price" label="Price" {...sortProps} />
              <span aria-hidden="true" className="px-1 text-subtle">
                /
              </span>
              <SortButton sortKey="change" label="1D Change" {...sortProps} />
            </th>
            <th
              scope="col"
              className={cn(TH, "text-right")}
              aria-sort={ariaSort(view, ["high_52w"])}
            >
              <SortButton sortKey="high_52w" label="52W High" {...sortProps} />
            </th>
            <th scope="col" className={cn(TH, "text-right")} aria-sort={ariaSort(view, ["volume"])}>
              <SortButton sortKey="volume" label="Market Volume" {...sortProps} />
            </th>
            <th scope="col" className={TH}>
              Setup
            </th>
            <th scope="col" className={TH} aria-sort={ariaSort(view, ["score"])}>
              <SortButton sortKey="score" label="Score" {...sortProps} />
            </th>
            <th scope="col" className={cn(TH, "text-right")}>
              SL / Entry / Target
            </th>
            <th scope="col" className={cn(TH, "text-right")} aria-sort={ariaSort(view, ["rr"])}>
              <SortButton sortKey="rr" label="R:R" {...sortProps} />
            </th>
          </tr>
        </thead>
        <tbody>
          {rows.map((item) => {
            const setup = item.setup;
            const lowRR = setup?.state === "low_risk_reward";
            return (
              <tr
                key={item.symbol}
                className="relative border-b border-line/70 transition-colors last:border-0 focus-within:bg-surface-2 hover:bg-surface-2"
              >
                <td className={TD}>
                  <div className="flex max-w-[300px] items-center gap-3">
                    <SymbolAvatar symbol={item.symbol} name={item.name} />
                    <div className="min-w-0 flex-1">
                      <Link
                        href={stockHref(item.symbol, plan)}
                        title={item.name}
                        className="block truncate font-semibold text-fg after:absolute after:inset-0 after:content-[''] focus-visible:outline-none focus-visible:after:ring-2 focus-visible:after:ring-accent focus-visible:after:ring-inset"
                      >
                        {item.name}
                      </Link>
                      <span className="block text-xs text-muted">{item.symbol}</span>
                    </div>
                    <Sparkline values={item.sparkline} />
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
                <td className={cn(TD, "text-right tabular-nums")}>{formatINR(item.high_52w)}</td>
                <td className={cn(TD, "text-right tabular-nums")}>{formatVolume(item.volume)}</td>
                <td className={TD}>
                  <SetupBadge setup={setup} />
                </td>
                <td className={TD}>
                  <ScoreBar score={setup?.score} muted={lowRR} />
                </td>
                <td className={cn(TD, "text-right")}>
                  {setup ? (
                    <dl
                      className={cn(
                        "ml-auto grid w-max grid-cols-[auto_auto] gap-x-2 text-xs leading-5 tabular-nums",
                        lowRR && "opacity-60",
                      )}
                    >
                      <dt className="text-down">SL</dt>
                      <dd>{formatNumber(setup.stop)}</dd>
                      <dt className="text-muted">Entry</dt>
                      <dd>{formatNumber(setup.entry)}</dd>
                      <dt className="text-up">TP</dt>
                      <dd className="text-up">{formatNumber(setup.target)}</dd>
                    </dl>
                  ) : (
                    <span className="text-subtle">—</span>
                  )}
                </td>
                <td className={cn(TD, "text-right font-semibold tabular-nums")}>
                  {setup ? (
                    <span
                      className={cn(
                        lowRR ? "text-subtle" : setup.risk_reward >= 2 ? "text-up" : "text-fg",
                      )}
                    >
                      {setup.risk_reward.toFixed(2)}
                    </span>
                  ) : (
                    <span className="font-normal text-subtle">—</span>
                  )}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
