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

/** GET /v1/swing */
export type SwingTab = "breakout" | "pullback" | "results_gap" | "leaders";

export type CatalystKind =
  | "results"
  | "dividend"
  | "bonus"
  | "split"
  | "buyback"
  | "demerger"
  | "rights"
  | "fundraise"
  | "order_win"
  | "expansion"
  | "deal"
  | "credit_rating"
  | "management"
  | "legal"
  | "exchange_query"
  | "stake_change"
  | "bulk_deal"
  | "block_deal"
  | "surveillance"
  | "fno_ban";

/** One dated NSE event for a stock. `risk` marks event risk or a trading restriction. */
export interface Catalyst {
  kind: CatalystKind;
  tone: "positive" | "negative" | "neutral" | "risk";
  /** YYYY-MM-DD */
  date: string;
  /** Days from the reference date (latest bar, or today); negative is past. */
  days: number;
  title: string;
  detail: string | null;
  url: string | null;
}

/** GET /v1/stocks/{symbol}/events */
export interface StockEventsResponse {
  symbol: string;
  as_of: string;
  /** False when NSE catalysts are not in use (demo data). */
  available: boolean;
  items: Catalyst[];
}
export type SwingGate = "tradable" | "leader" | "sector";
export type RegimeState = "normal" | "caution" | "defensive";

export interface SwingStatus {
  /** `syncing`: NSE files are still downloading and there is nothing to show yet. */
  state: "pending" | "syncing" | "ready";
  scan: ScanPhase;
  universe_size: number;
  failed: number;
  excluded: string[];
  /** NSE end-of-day sessions cached; null when NSE files are not used. */
  days_cached: number | null;
  days_target: number | null;
  error: string | null;
  /** Catalyst sources that could not be refreshed. */
  catalyst_errors: string[];
}

export interface SwingRegime {
  state: RegimeState;
  index: string | null;
  index_close: number | null;
  index_sma50: number | null;
  index_sma200: number | null;
  breadth_50_pct: number;
  breadth_200_pct: number;
  universe: number;
}

export interface SwingSector {
  industry: string;
  members: number;
  ret_3m_pct: number;
  rank: number;
  strong: boolean;
}

export interface SwingSetup {
  kind: "breakout" | "pullback" | "results_gap";
  label: string;
  /** 0-100 */
  score: number;
  score_breakdown: {
    rs: number;
    trend: number;
    setup: number;
    sector: number;
    delivery: number;
    quality: number;
  };
  /** Breakout level, or the swing high a pullback came from. */
  level: number;
  extension_pct: number;
  volume_ratio: number;
  entry: number;
  stop: number;
  risk_per_share: number;
  target_2r: number;
  target_3r: number;
  quantity: number;
  /** Six weeks after the setup bar, YYYY-MM-DD. */
  review_by: string | null;
  reasons: string[];
}

export interface SwingItem {
  symbol: string;
  name: string;
  exchange: string;
  industry: string | null;
  /** Latest daily close. */
  price: number;
  change: number;
  change_pct: number;
  volume: number;
  bar_time: number;
  sparkline: number[];
  /** Median 20-day turnover in crore rupees. */
  turnover_cr: number;
  /** Relative-strength rating, 0-100. */
  rs: number | null;
  ret_1m_pct: number | null;
  ret_3m_pct: number | null;
  ret_6m_pct: number | null;
  below_high_pct: number;
  delivery_pct: number | null;
  delivery_avg20: number | null;
  gates: { tradable: boolean; leader: boolean; sector: boolean | null };
  setup: SwingSetup | null;
  /** Events from 7 days before to 14 days after the latest bar; risks first. */
  catalysts: Catalyst[];
}

export interface SwingResponse {
  /** `nse` (NSE end-of-day files) or a provider id; null before the first scan. */
  source: ProviderName | null;
  /** Date of the newest daily bar, YYYY-MM-DD. */
  as_of: string | null;
  status: SwingStatus;
  regime: SwingRegime | null;
  sectors: SwingSector[];
  counts: Record<SwingTab, number>;
  total: number;
  items: SwingItem[];
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
