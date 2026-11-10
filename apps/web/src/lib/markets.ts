/**
 * The three markets the app shows, and everything that differs between them.
 * Pure: safe on server and client.
 */
import type { Interval, Market } from "./api/types";

export type Currency = "INR" | "USD";

export interface MarketInfo {
  id: Market;
  /** Sidebar label. */
  label: string;
  /** Mobile tab-bar label. */
  shortLabel: string;
  /** Label in the market tabs strip. */
  tab: string;
  /** List page; stock pages live under it. */
  path: string;
  currency: Currency;
  /** Default capital for position sizing, in `currency`. */
  defaultCapital: number;
  intervals: readonly Interval[];
  /** "stocks" / "commodities", for sentences. */
  noun: string;
  /** Page description. */
  description: string;
}

export const MARKETS: Record<Market, MarketInfo> = {
  in: {
    id: "in",
    label: "IN Stock",
    shortLabel: "IN",
    tab: "India",
    path: "/stocks",
    currency: "INR",
    defaultCapital: 500_000,
    intervals: ["1d", "1w", "15m"],
    noun: "stocks",
    description: "NSE stocks in your watchlist",
  },
  us: {
    id: "us",
    label: "US Stock",
    shortLabel: "US",
    tab: "US",
    path: "/us",
    currency: "USD",
    defaultCapital: 5_000,
    intervals: ["1d", "1w"],
    noun: "stocks",
    description: "US stocks in your watchlist (prices in US dollars)",
  },
  mcx: {
    id: "mcx",
    label: "Commodity",
    shortLabel: "MCX",
    tab: "Commodities",
    path: "/commodities",
    currency: "INR",
    defaultCapital: 500_000,
    intervals: ["1d", "1w", "15m"],
    noun: "commodities",
    description: "MCX commodity futures (near-month contract)",
  },
};

export const MARKET_ORDER: readonly Market[] = ["in", "us", "mcx"];

export function isMarket(value: unknown): value is Market {
  return value === "in" || value === "us" || value === "mcx";
}

/** Market of a quote's exchange label: `US`, `MCX`, anything else is NSE. */
export function marketForExchange(exchange: string | null | undefined): Market {
  const value = exchange?.trim().toUpperCase();
  return value === "US" ? "us" : value === "MCX" ? "mcx" : "in";
}

/** Parses `market` from a URL; anything unknown is India. */
export function parseMarket(raw: string | null | undefined): Market {
  const value = raw?.trim().toLowerCase();
  return isMarket(value) ? value : "in";
}
