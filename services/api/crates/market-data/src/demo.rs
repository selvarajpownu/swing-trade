//! Deterministic synthetic market data.
//!
//! [`DemoProvider`] invents believable prices for any symbol so the app can
//! run with no credentials. The data is **not real**: responses carry the
//! provider id `"demo"` and the UI should label them as such.
//!
//! The series is a seeded random walk. The seed is a hash of the symbol and
//! the generator is a small built-in SplitMix64, so the same symbol always
//! produces the same shape on every machine and every library version.

use async_trait::async_trait;
use swing_core::Candle;

use crate::{names, Interval, MarketData, Quote, Result};

/// Provider id reported for demo data.
pub const ID: &str = "demo";

const DAY_SECS: i64 = 86_400;
/// IST is UTC+05:30.
const IST_OFFSET_SECS: i64 = 19_800;
/// NSE opens at 09:15 IST, i.e. 03:45 UTC.
const SESSION_OPEN_UTC_SECS: i64 = 9 * 3_600 + 15 * 60 - IST_OFFSET_SECS;
const DAILY_BARS: usize = 300;
const INTRADAY_DAYS: usize = 7;
/// 09:15 to 15:30 is 375 minutes, i.e. 25 fifteen-minute bars.
const BARS_PER_SESSION: usize = 25;
const BARS_52W: usize = 250;

/// Synthetic data for any symbol. See the module docs.
#[derive(Debug, Clone, Default)]
pub struct DemoProvider {
    /// Last session as days since the Unix epoch; `None` means "today".
    end_day: Option<i64>,
}

impl DemoProvider {
    /// Demo data ending at the most recent weekday.
    pub fn new() -> Self {
        Self::default()
    }

    /// Demo data whose last bar falls on (or just before) `day`, given as
    /// days since the Unix epoch. Useful for reproducible tests.
    pub fn with_end_day(day: i64) -> Self {
        Self { end_day: Some(day) }
    }

    fn last_session_day(&self) -> i64 {
        let today = self
            .end_day
            .unwrap_or_else(|| chrono::Utc::now().timestamp().div_euclid(DAY_SECS));
        previous_weekday(today)
    }

    fn daily(&self, symbol: &str) -> Vec<Candle> {
        generate_daily(symbol, self.last_session_day(), DAILY_BARS)
    }
}

#[async_trait]
impl MarketData for DemoProvider {
    fn id(&self) -> &'static str {
        ID
    }

    async fn quotes(&self, symbols: &[String]) -> Result<Vec<Quote>> {
        Ok(symbols
            .iter()
            .filter_map(|s| {
                let symbol = s.trim().to_ascii_uppercase();
                let candles = self.daily(&symbol);
                quote_from_daily(&symbol, &candles)
            })
            .collect())
    }

    async fn candles(&self, symbol: &str, interval: Interval) -> Result<Vec<Candle>> {
        let symbol = symbol.trim().to_ascii_uppercase();
        let daily = self.daily(&symbol);
        Ok(match interval {
            Interval::Day => daily,
            Interval::Week => weekly(&daily),
            Interval::Min15 => {
                let seed = fnv1a(symbol.as_bytes());
                let start = daily.len().saturating_sub(INTRADAY_DAYS);
                daily[start..]
                    .iter()
                    .flat_map(|d| intraday(d, seed ^ (d.ts as u64)))
                    .collect()
            }
        })
    }
}

/// Builds a quote from the last two daily bars.
fn quote_from_daily(symbol: &str, candles: &[Candle]) -> Option<Quote> {
    let last = candles.last()?;
    let prev_close = candles
        .len()
        .checked_sub(2)
        .map_or(last.open, |i| candles[i].close);
    let year = &candles[candles.len().saturating_sub(BARS_52W)..];
    let change = last.close - prev_close;
    Some(Quote {
        symbol: symbol.to_owned(),
        name: names::company_name(symbol).unwrap_or(symbol).to_owned(),
        exchange: "NSE".to_owned(),
        price: last.close,
        change: round2(change),
        change_pct: if prev_close > 0.0 {
            round2(change / prev_close * 100.0)
        } else {
            0.0
        },
        day_high: last.high,
        day_low: last.low,
        high_52w: year.iter().map(|c| c.high).fold(f64::MIN, f64::max),
        low_52w: year.iter().map(|c| c.low).fold(f64::MAX, f64::min),
        volume: last.volume,
        updated_at: Some(format!(
            "{}T15:30:00+05:30",
            iso_date(last.ts.div_euclid(DAY_SECS))
        )),
    })
}

/// A seeded random walk with slowly changing drift, ending on `end_day`.
fn generate_daily(symbol: &str, end_day: i64, bars: usize) -> Vec<Candle> {
    let mut rng = SplitMix64::new(fnv1a(symbol.as_bytes()));
    let base = log_uniform(&mut rng, 100.0, 5_000.0);
    let vol = 0.012 + rng.next_f64() * 0.016;
    let base_volume = log_uniform(&mut rng, 200_000.0, 3_000_000.0);

    let mut drift = 0.0;
    let mut close = base;
    let mut out = Vec::with_capacity(bars);
    for (i, day) in trading_days(end_day, bars).into_iter().enumerate() {
        if i % 60 == 0 {
            // A new regime roughly every quarter, mildly biased upwards.
            drift = (rng.next_f64() - 0.42) * 0.004;
        }
        let open = (close * (1.0 + rng.normal() * vol * 0.25)).max(1.0);
        let next = (open * (drift + rng.normal() * vol).exp()).max(1.0);
        let high = open.max(next) * (1.0 + rng.normal().abs() * vol * 0.4);
        let low = open.min(next) * (1.0 - (rng.normal().abs() * vol * 0.4).min(0.5));
        let move_size = ((next - close) / close).abs() / vol;
        let volume = (base_volume * (0.35 * rng.normal()).exp() * (1.0 + 0.5 * move_size)).round();
        out.push(Candle::new(
            day * DAY_SECS,
            tick(open),
            tick(high),
            tick(low),
            tick(next),
            volume,
        ));
        close = next;
    }
    out
}

/// Aggregates daily bars into Monday-anchored weekly bars.
fn weekly(daily: &[Candle]) -> Vec<Candle> {
    let mut out: Vec<Candle> = Vec::new();
    for c in daily {
        let day = c.ts.div_euclid(DAY_SECS);
        let week_start = (day - weekday(day)) * DAY_SECS;
        match out.last_mut() {
            Some(w) if w.ts == week_start => {
                w.high = w.high.max(c.high);
                w.low = w.low.min(c.low);
                w.close = c.close;
                w.volume += c.volume;
            }
            _ => out.push(Candle::new(
                week_start, c.open, c.high, c.low, c.close, c.volume,
            )),
        }
    }
    out
}

/// Splits one daily bar into 15-minute bars along a Brownian bridge from
/// the day's open to its close, kept inside the day's range.
fn intraday(day_bar: &Candle, seed: u64) -> Vec<Candle> {
    let mut rng = SplitMix64::new(seed);
    let range = (day_bar.high - day_bar.low).max(0.05);
    let n = BARS_PER_SESSION;

    let mut walk = vec![0.0; n + 1];
    for k in 1..=n {
        walk[k] = walk[k - 1] + rng.normal() * range * 0.15;
    }
    let path: Vec<f64> = (0..=n)
        .map(|k| {
            let t = k as f64 / n as f64;
            let bridge = walk[k] - t * walk[n];
            (day_bar.open + (day_bar.close - day_bar.open) * t + bridge)
                .clamp(day_bar.low, day_bar.high)
        })
        .collect();

    let session_open = day_bar.ts + SESSION_OPEN_UTC_SECS;
    (1..=n)
        .map(|k| {
            let open = path[k - 1];
            let close = if k == n { day_bar.close } else { path[k] };
            let high = (open.max(close) + rng.normal().abs() * range * 0.05).min(day_bar.high);
            let low = (open.min(close) - rng.normal().abs() * range * 0.05).max(day_bar.low);
            // U-shaped volume: busy at the open and close. Mean of the shape is 1.5.
            let x = 2.0 * (k as f64 - 0.5) / n as f64 - 1.0;
            let share = (1.0 + 1.5 * x * x) / 1.5 / n as f64;
            Candle::new(
                session_open + (k as i64 - 1) * 900,
                tick(open),
                tick(high),
                tick(low),
                tick(close),
                (day_bar.volume * share).round(),
            )
        })
        .collect()
}

/// `count` weekdays ending at `end_day` (inclusive), oldest first.
fn trading_days(end_day: i64, count: usize) -> Vec<i64> {
    let mut days = Vec::with_capacity(count);
    let mut day = end_day;
    while days.len() < count {
        if weekday(day) < 5 {
            days.push(day);
        }
        day -= 1;
    }
    days.reverse();
    days
}

/// Monday = 0 … Sunday = 6. Day 0 (1970-01-01) was a Thursday.
fn weekday(day: i64) -> i64 {
    (day + 3).rem_euclid(7)
}

fn previous_weekday(mut day: i64) -> i64 {
    while weekday(day) >= 5 {
        day -= 1;
    }
    day
}

/// `YYYY-MM-DD` for days since the Unix epoch (Howard Hinnant's algorithm).
fn iso_date(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Rounds to the NSE tick of ₹0.05, then to two decimals so values print cleanly.
fn tick(price: f64) -> f64 {
    round2((price / 0.05).round() * 0.05)
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn log_uniform(rng: &mut SplitMix64, lo: f64, hi: f64) -> f64 {
    (lo.ln() + rng.next_f64() * (hi.ln() - lo.ln())).exp()
}

/// 64-bit FNV-1a: tiny, stable across platforms and releases.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// SplitMix64 PRNG (Steele, Lea & Flood). Deterministic for a given seed.
struct SplitMix64(u64);

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, 1)`.
    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Standard normal via Box–Muller.
    fn normal(&mut self) -> f64 {
        let u1 = self.next_f64().max(f64::MIN_POSITIVE);
        let u2 = self.next_f64();
        (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-11-10, a Tuesday.
    const END: i64 = 20_767;

    fn provider() -> DemoProvider {
        DemoProvider::with_end_day(END)
    }

    #[test]
    fn calendar_helpers() {
        assert_eq!(iso_date(0), "1970-01-01");
        assert_eq!(iso_date(END), "2026-11-10");
        assert_eq!(weekday(END), 1);
        // Saturday 2026-11-14 rolls back to Friday 2026-11-13.
        assert_eq!(previous_weekday(END + 4), END + 3);
    }

    #[tokio::test]
    async fn daily_series_is_deterministic_and_well_formed() {
        let a = provider().candles("COFORGE", Interval::Day).await.unwrap();
        let b = provider().candles("coforge", Interval::Day).await.unwrap();
        assert_eq!(a, b);
        assert_eq!(a.len(), DAILY_BARS);
        assert_eq!(a.last().unwrap().ts, END * DAY_SECS);
        for w in a.windows(2) {
            assert!(w[0].ts < w[1].ts);
        }
        for c in &a {
            assert!(c.low > 0.0);
            assert!(c.high >= c.open.max(c.close), "{c:?}");
            assert!(c.low <= c.open.min(c.close), "{c:?}");
            assert!(c.volume > 0.0);
            assert!(weekday(c.ts / DAY_SECS) < 5);
        }
        let other = provider().candles("MCX", Interval::Day).await.unwrap();
        assert_ne!(a, other);
    }

    #[tokio::test]
    async fn weekly_and_intraday_are_consistent() {
        let p = provider();
        let daily = p.candles("IEX", Interval::Day).await.unwrap();
        let weeks = p.candles("IEX", Interval::Week).await.unwrap();
        assert!(weeks.len() >= DAILY_BARS / 5);
        assert_eq!(weeks.last().unwrap().close, daily.last().unwrap().close);
        let vol_d: f64 = daily.iter().map(|c| c.volume).sum();
        let vol_w: f64 = weeks.iter().map(|c| c.volume).sum();
        assert!((vol_d - vol_w).abs() < 1e-6);

        let bars = p.candles("IEX", Interval::Min15).await.unwrap();
        assert_eq!(bars.len(), INTRADAY_DAYS * BARS_PER_SESSION);
        let last_day = daily.last().unwrap();
        let today = &bars[bars.len() - BARS_PER_SESSION..];
        assert_eq!(today[0].ts, last_day.ts + SESSION_OPEN_UTC_SECS);
        assert_eq!(today.last().unwrap().close, last_day.close);
        for c in today {
            assert!(c.high <= last_day.high + 1e-9 && c.low >= last_day.low - 1e-9);
        }
    }

    #[tokio::test]
    async fn quotes_follow_the_last_bars() {
        let p = provider();
        let quotes = p
            .quotes(&["ACE".to_owned(), "NEWCO".to_owned()])
            .await
            .unwrap();
        assert_eq!(quotes.len(), 2);
        let ace = &quotes[0];
        assert_eq!(ace.symbol, "ACE");
        assert_eq!(ace.name, "Action Construction Equipment");
        assert_eq!(ace.exchange, "NSE");
        assert_eq!(quotes[1].name, "NEWCO");

        let daily = p.candles("ACE", Interval::Day).await.unwrap();
        let last = daily[daily.len() - 1];
        let prev = daily[daily.len() - 2];
        assert_eq!(ace.price, last.close);
        assert!((ace.change - (last.close - prev.close)).abs() < 0.006);
        assert!(ace.high_52w >= ace.price && ace.low_52w <= ace.price);
        assert_eq!(ace.updated_at.as_deref(), Some("2026-11-10T15:30:00+05:30"));
    }

    #[test]
    fn analysis_runs_on_demo_data() {
        let candles = generate_daily("PERSISTENT", END, DAILY_BARS);
        let analysis = swing_core::analyze(&candles, &swing_core::AnalysisParams::default());
        assert!(analysis.is_some());
    }
}
