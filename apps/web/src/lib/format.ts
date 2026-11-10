/**
 * Number and date formatting for Indian markets (en-IN grouping, IST).
 * Formatters are created once and reused.
 */

const LOCALE = "en-IN";
const IST = "Asia/Kolkata";

const fixed2 = new Intl.NumberFormat(LOCALE, {
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
});

const integer = new Intl.NumberFormat(LOCALE, { maximumFractionDigits: 0 });

const dateTime = new Intl.DateTimeFormat(LOCALE, {
  timeZone: IST,
  day: "2-digit",
  month: "short",
  hour: "2-digit",
  minute: "2-digit",
  hour12: false,
});

const EM_DASH = "—";

function isNum(value: number | null | undefined): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

/** 1234567.8 -> "12,34,567.80" */
export function formatNumber(value: number | null | undefined): string {
  return isNum(value) ? fixed2.format(value) : EM_DASH;
}

/** 1234567.8 -> "₹12,34,567.80" */
export function formatINR(value: number | null | undefined): string {
  if (!isNum(value)) return EM_DASH;
  const sign = value < 0 ? "-" : "";
  return `${sign}₹${fixed2.format(Math.abs(value))}`;
}

const usd2 = new Intl.NumberFormat("en-US", {
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
});
const usdWhole = new Intl.NumberFormat("en-US", { maximumFractionDigits: 0 });

/** Price in its currency: ₹12,34,567.80 (Indian grouping) or $1,234,567.80. */
export function formatPrice(value: number | null | undefined, currency: string = "INR"): string {
  if (currency !== "USD") return formatINR(value);
  if (!isNum(value)) return EM_DASH;
  const sign = value < 0 ? "-" : "";
  return `${sign}$${usd2.format(Math.abs(value))}`;
}

/** Whole amounts for capital: ₹5,00,000 or $5,000. */
export function formatMoneyWhole(
  value: number | null | undefined,
  currency: string = "INR",
): string {
  if (currency !== "USD") return formatINRWhole(value);
  return isNum(value) ? `$${usdWhole.format(value)}` : EM_DASH;
}

/** "3h ago", "2d ago", or a date for anything older than a week. */
export function formatTimeAgo(iso: string | null | undefined, now = Date.now()): string {
  if (!iso) return "";
  const t = new Date(iso).getTime();
  if (Number.isNaN(t)) return "";
  const mins = Math.max(0, Math.round((now - t) / 60_000));
  if (mins < 60) return `${Math.max(1, mins)}m ago`;
  const hours = Math.round(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.round(hours / 24);
  if (days <= 7) return `${days}d ago`;
  return new Intl.DateTimeFormat(LOCALE, { day: "numeric", month: "short", timeZone: IST }).format(
    t,
  );
}

/** Whole rupees, for capital: 500000 -> "₹5,00,000" */
export function formatINRWhole(value: number | null | undefined): string {
  return isNum(value) ? `₹${integer.format(value)}` : EM_DASH;
}

/** 13461950 -> "1,34,61,950.00" (market volume, as in the reference design). */
export function formatVolume(value: number | null | undefined, currency: string = "INR"): string {
  if (!isNum(value)) return EM_DASH;
  // US volumes use US grouping (1,090,629), Indian ones lakh/crore grouping.
  return currency === "USD" ? usd2.format(value) : fixed2.format(value);
}

export function formatInteger(value: number | null | undefined): string {
  return isNum(value) ? integer.format(value) : EM_DASH;
}

/** 5.3 -> "5.30%" */
export function formatPercent(value: number | null | undefined, digits = 2): string {
  return isNum(value) ? `${value.toFixed(digits)}%` : EM_DASH;
}

export type Direction = "up" | "down" | "flat";

export function direction(value: number | null | undefined): Direction {
  if (!isNum(value) || value === 0) return "flat";
  return value > 0 ? "up" : "down";
}

/**
 * Change line as in the reference design:
 *   (26.93, 5.3)  -> "+26.93 (▲5.30%)"
 *   (-4.1, -0.82) -> "-4.10 (▼0.82%)"
 */
export function formatChange(
  change: number | null | undefined,
  pct: number | null | undefined,
): string {
  if (!isNum(change) || !isNum(pct)) return EM_DASH;
  const dir = direction(change);
  const sign = dir === "up" ? "+" : dir === "down" ? "-" : "";
  const arrow = dir === "up" ? "▲" : dir === "down" ? "▼" : "";
  return `${sign}${fixed2.format(Math.abs(change))} (${arrow}${Math.abs(pct).toFixed(2)}%)`;
}

/** 1.94 -> "1 : 1.94" */
export function formatRiskReward(value: number | null | undefined): string {
  return isNum(value) ? `1 : ${value.toFixed(2)}` : EM_DASH;
}

/** Ratio of two numbers, e.g. volume vs its 20-day average -> "1.16×". */
export function formatRatio(
  numerator: number | null | undefined,
  denominator: number | null | undefined,
): string {
  if (!isNum(numerator) || !isNum(denominator) || denominator === 0) return EM_DASH;
  return `${(numerator / denominator).toFixed(2)}×`;
}

/** ISO string -> "10 Nov, 15:30 IST" */
export function formatDateTimeIST(iso: string | null | undefined): string {
  if (!iso) return EM_DASH;
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return EM_DASH;
  return `${dateTime.format(date)} IST`;
}

/** Percentage distance of `value` from `reference` ("+1.20%"). */
export function formatDistancePct(value: number, reference: number): string {
  if (!isNum(value) || !isNum(reference) || reference === 0) return EM_DASH;
  const pct = ((value - reference) / reference) * 100;
  const sign = pct > 0 ? "+" : pct < 0 ? "-" : "";
  return `${sign}${Math.abs(pct).toFixed(2)}%`;
}
