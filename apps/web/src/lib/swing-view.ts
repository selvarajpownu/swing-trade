/**
 * Swing Stocks page state (list tab and gate toggles) and its URL form
 * (`setup`, `gates`). Pure: used on the server and in the browser.
 */
import type { SwingGate, SwingTab } from "./api/types";

export const SWING_TABS: readonly { id: SwingTab; label: string; blurb: string }[] = [
  {
    id: "breakout",
    label: "Breakouts",
    blurb:
      "Closed above the 20-day high on at least 1.5× volume, from a base no deeper than 15%, and not more than 5% past the level.",
  },
  {
    id: "pullback",
    label: "Pullbacks",
    blurb:
      "Dipped 3–10% over 3–10 days on lighter volume, touched the 20/50-day average, then closed above the prior day's high.",
  },
  {
    id: "leaders",
    label: "Leaders",
    blurb: "Every stock passing the Leader gate, strongest first. A watchlist, not a setup.",
  },
];

export const SWING_GATES: readonly { id: SwingGate; label: string; hint: string }[] = [
  {
    id: "tradable",
    label: "Tradable",
    hint: "Normal EQ series and median 20-day turnover of ₹10 crore or more",
  },
  {
    id: "leader",
    label: "Leader",
    hint: "Relative strength 70+, price above rising 50/200-day averages, within 25% of the 52-week high",
  },
  {
    id: "sector",
    label: "Strong sector",
    hint: "Industry in the stronger half by 3-month performance",
  },
];

export interface SwingView {
  tab: SwingTab;
  gates: Record<SwingGate, boolean>;
}

export const DEFAULT_SWING_VIEW: SwingView = {
  tab: "breakout",
  gates: { tradable: true, leader: true, sector: true },
};

function isTab(value: unknown): value is SwingTab {
  return SWING_TABS.some((t) => t.id === value);
}

export function parseSwingView(get: (key: string) => string | undefined | null): SwingView {
  const setup = get("setup");
  const raw = get("gates");
  const listed = raw
    ?.split(",")
    .map((g) => g.trim())
    .filter(Boolean);
  const on = (gate: SwingGate) => (listed ? listed.includes(gate) : true);
  return {
    tab: isTab(setup) ? setup : DEFAULT_SWING_VIEW.tab,
    gates: { tradable: on("tradable"), leader: on("leader"), sector: on("sector") },
  };
}

/** The `gates` value for the API and URLs: undefined when all are on. */
export function gatesParam(gates: SwingView["gates"]): string | undefined {
  const on = SWING_GATES.filter((g) => gates[g.id]).map((g) => g.id);
  if (on.length === SWING_GATES.length) return undefined;
  return on.length === 0 ? "none" : on.join(",");
}

/** Writes the non-default parts of the view into `query`. */
export function writeSwingView(view: SwingView, query: URLSearchParams): void {
  if (view.tab === DEFAULT_SWING_VIEW.tab) query.delete("setup");
  else query.set("setup", view.tab);
  const gates = gatesParam(view.gates);
  if (gates === undefined) query.delete("gates");
  else query.set("gates", gates);
}
