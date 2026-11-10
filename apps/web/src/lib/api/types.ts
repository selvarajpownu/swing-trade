/**
 * Types mirroring docs/api.md (API contract v1).
 *
 * Numbers are rupees / shares. `time` fields are Unix seconds (UTC); fields
 * ending in `_at` are ISO 8601 strings.
 */

/** Data provider name reported by the backend, e.g. "demo" or "indmoney". */
export type ProviderName = string;

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
  quantity: number;
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
  quote: Quote;
  candles: Candle[];
  overlays: Overlays;
  snapshot: Snapshot;
  /** Nearest-to-price first. */
  zones: Zone[];
  setup: SetupDetail | null;
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
