/**
 * Types mirroring docs/api.md (API contract v1).
 *
 * Numbers are rupees / shares. `time` fields are Unix seconds (UTC); fields
 * ending in `_at` are ISO 8601 strings.
 */

/** Data provider name reported by the backend, e.g. "demo" or "indmoney". */
export type ProviderName = string;

/** `in` (NSE), `us` (US stocks, USD) or `mcx` (MCX commodity futures). */
export type Market = "in" | "us" | "mcx";

export type SetupPreset = "pullback" | "breakout" | "support_bounce" | "oversold";

export type SetupState = "watching" | "low_risk_reward";

export type Interval = "1d" | "1w" | "15m";

export type ZoneKind = "support" | "resistance";

/** Setup as returned in the stock list. */
export interface SetupSummary {
  preset: SetupPreset;
  label: string;
  state: SetupState;
  /** 0-100 */
  score: number;
  entry: number;
  stop: number;
  target: number;
  risk_reward: number;
  /** Shares to trade; null where it cannot be sized (MCX futures trade in lots). */
  quantity: number | null;
}

export interface ScoreBreakdown {
  trend: number;
  closeness: number;
  volume: number;
  risk_reward: number;
  quality: number;
}

/** Setup as returned on the single-stock endpoint. */
export interface SetupDetail extends SetupSummary {
  risk_per_share: number;
  reward_per_share: number;
  target_from_zone: boolean;
  score_breakdown: ScoreBreakdown;
  reasons: string[];
}

export interface StockListItem {
  symbol: string;
  name: string;
  exchange: string;
  price: number;
  change: number;
  change_pct: number;
  high_52w: number;
  low_52w: number;
  volume: number;
  /** Last 30 daily closes, oldest first. */
  sparkline: number[];
  setup: SetupSummary | null;
}

export interface StocksResponse {
  provider: ProviderName;
  market: Market;
  /** Every price in the response is in this currency. */
  currency: "INR" | "USD";
  as_of: string;
  items: StockListItem[];
}

export interface Quote {
  symbol: string;
  name: string;
  exchange: string;
  price: number;
  change: number;
  change_pct: number;
  day_high: number;
  day_low: number;
  high_52w: number;
  low_52w: number;
  volume: number;
  updated_at: string;
}

export interface Candle {
  time: number;
  open: number;
  high: number;
  low: number;
  close: number;
  volume: number;
}

export interface LinePoint {
  time: number;
  value: number;
}

export interface Overlays {
  sma20: LinePoint[];
  sma50: LinePoint[];
  sma200: LinePoint[];
}

/** Indicator fields may be null when history is too short. */
export interface Snapshot {
  close: number;
  sma20: number | null;
  sma50: number | null;
  sma200: number | null;
  rsi14: number | null;
  atr14: number | null;
  volume_avg20: number | null;
  high_52w: number | null;
  low_52w: number | null;
}

export interface Zone {
  low: number;
  high: number;
  touches: number;
  kind: ZoneKind;
  /** Unix seconds of the most recent touch (may be omitted by the API). */
  last_touch?: number;
}

export interface StockDetailResponse {
  provider: ProviderName;
  market: Market;
  currency: "INR" | "USD";
  quote: Quote;
  candles: Candle[];
  overlays: Overlays;
  snapshot: Snapshot;
  /** Nearest-to-price first. */
  zones: Zone[];
  setup: SetupDetail | null;
}

/** GET /v1/scan */
export type ScanPhase = "pending" | "running" | "idle";

export interface ScanStatus {
  state: ScanPhase;
  universe_size: number;
  scanned: number;
  failed: string[];
  excluded: string[];
  started_at: string | null;
  finished_at: string | null;
  next_run_at: string | null;
}

/** A scanner row: list fields from the latest daily bar plus its best matching setup. */
export interface ScanItem extends StockListItem {
  industry: string | null;
  /** Unix seconds of the latest daily bar. */
  bar_time: number;
  /** Every preset firing on the latest bar. */
  presets: SetupPreset[];
  setup: SetupSummary;
}

export interface ScanResponse {
  provider: ProviderName;
  status: ScanStatus;
  /** Matching rows before `limit`. */
  total: number;
  items: ScanItem[];
}

/** A headline: INDmoney (summary + sentiment) or Google News (publisher + link). */
export interface NewsItem {
  title: string;
  summary: string | null;
  sentiment: "positive" | "neutral" | "negative" | string | null;
  /** ISO 8601 date or date-time. */
  published_at: string | null;
  source: string | null;
  url: string | null;
}

/** GET /v1/stocks/{symbol}/news */
export interface StockNewsResponse {
  provider: ProviderName;
  market: Market;
  symbol: string;
  items: NewsItem[];
}

export interface Headline extends NewsItem {
  symbol: string;
  name: string;
}

/** GET /v1/news */
export interface HeadlinesResponse {
  source: "google_news";
  market: Market;
  as_of: string;
  items: Headline[];
}

export interface HealthResponse {
  status: string;
  version: string;
  providers: ProviderName[];
}

export type ApiErrorCodeFromServer =
  "not_found" | "unauthorized" | "bad_request" | "upstream" | "internal";

export interface ApiErrorBody {
  error: {
    code: ApiErrorCodeFromServer;
    message: string;
  };
}
