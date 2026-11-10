/**
 * Stock list view state (search, filter chip, sort) and how it maps to URL
 * search params, so views are bookmarkable. Pure: used on server and client.
 */
import type { SearchParamsRecord } from "./api/params";
import { firstParam } from "./api/params";
import type { SetupPreset, StockListItem } from "./api/types";

export const FILTERS = [
  { id: "all", label: "All" },
  { id: "setup", label: "Has setup" },
  { id: "pullback", label: "Pullback" },
  { id: "breakout", label: "Breakout" },
  { id: "support_bounce", label: "Support bounce" },
  { id: "oversold", label: "Oversold" },
  { id: "rr2", label: "R:R ≥ 2" },
] as const;

export type FilterId = (typeof FILTERS)[number]["id"];

export const SORT_KEYS = ["name", "price", "change", "high_52w", "volume", "score", "rr"] as const;
export type SortKey = (typeof SORT_KEYS)[number];
export type SortDir = "asc" | "desc";

export interface ListView {
  q: string;
  filter: FilterId;
  sort: SortKey;
  dir: SortDir;
}

export const DEFAULT_VIEW: ListView = { q: "", filter: "all", sort: "score", dir: "desc" };

const PRESET_FILTERS: readonly SetupPreset[] = [
  "pullback",
  "breakout",
  "support_bounce",
  "oversold",
];

function isFilter(value: unknown): value is FilterId {
  return typeof value === "string" && FILTERS.some((f) => f.id === value);
}

function isSortKey(value: unknown): value is SortKey {
  return typeof value === "string" && (SORT_KEYS as readonly string[]).includes(value);
}

/** Default direction when a column is first clicked: text A-Z, numbers high-low. */
export function defaultDirFor(key: SortKey): SortDir {
  return key === "name" ? "asc" : "desc";
}

export function parseListView(params: SearchParamsRecord): ListView {
  const q = (firstParam(params.q) ?? "").slice(0, 64);
  const filter = firstParam(params.filter);
  const sort = firstParam(params.sort);
  const dir = firstParam(params.dir);
  const sortKey = isSortKey(sort) ? sort : DEFAULT_VIEW.sort;
  return {
    q,
    filter: isFilter(filter) ? filter : DEFAULT_VIEW.filter,
    sort: sortKey,
    dir:
      dir === "asc" || dir === "desc"
        ? dir
        : isSortKey(sort)
          ? defaultDirFor(sortKey)
          : DEFAULT_VIEW.dir,
  };
}

/** Writes the view into `query`, omitting defaults to keep URLs short. */
export function writeListView(view: ListView, query: URLSearchParams): void {
  const q = view.q.trim();
  if (q) query.set("q", q);
  else query.delete("q");

  if (view.filter !== DEFAULT_VIEW.filter) query.set("filter", view.filter);
  else query.delete("filter");

  if (view.sort !== DEFAULT_VIEW.sort || view.dir !== DEFAULT_VIEW.dir) {
    query.set("sort", view.sort);
    query.set("dir", view.dir);
  } else {
    query.delete("sort");
    query.delete("dir");
  }
}

function matchesFilter(item: StockListItem, filter: FilterId): boolean {
  switch (filter) {
    case "all":
      return true;
    case "setup":
      return item.setup !== null;
    case "rr2":
      return item.setup !== null && item.setup.risk_reward >= 2;
    default:
      return (
        (PRESET_FILTERS as readonly string[]).includes(filter) && item.setup?.preset === filter
      );
  }
}

function matchesQuery(item: StockListItem, q: string): boolean {
  if (!q) return true;
  const needle = q.trim().toLowerCase();
  if (!needle) return true;
  return item.symbol.toLowerCase().includes(needle) || item.name.toLowerCase().includes(needle);
}

function sortValue(item: StockListItem, key: SortKey): number | string | null {
  switch (key) {
    case "name":
      return item.name.toLowerCase();
    case "price":
      return item.price;
    case "change":
      return item.change_pct;
    case "high_52w":
      return item.high_52w;
    case "volume":
      return item.volume;
    case "score":
      return item.setup?.score ?? null;
    case "rr":
      return item.setup?.risk_reward ?? null;
  }
}

/** Filters and sorts; items without a value for the sort key always go last. */
export function applyListView(items: readonly StockListItem[], view: ListView): StockListItem[] {
  const factor = view.dir === "asc" ? 1 : -1;
  return items
    .filter((item) => matchesFilter(item, view.filter) && matchesQuery(item, view.q))
    .sort((a, b) => {
      const va = sortValue(a, view.sort);
      const vb = sortValue(b, view.sort);
      if (va === null && vb === null) return a.symbol.localeCompare(b.symbol);
      if (va === null) return 1;
      if (vb === null) return -1;
      let cmp: number;
      if (typeof va === "string" && typeof vb === "string") cmp = va.localeCompare(vb);
      else cmp = (va as number) - (vb as number);
      return cmp !== 0 ? cmp * factor : a.symbol.localeCompare(b.symbol);
    });
}

export function countByFilter(items: readonly StockListItem[]): Record<FilterId, number> {
  const counts = {} as Record<FilterId, number>;
  for (const f of FILTERS) counts[f.id] = items.filter((i) => matchesFilter(i, f.id)).length;
  return counts;
}
