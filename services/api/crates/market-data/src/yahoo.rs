//! Quotes and candles from Yahoo Finance's public chart endpoint.
//!
//! No account and no key: `GET /v8/finance/chart/<SYMBOL>` answers with the
//! latest quote (in `meta`) and OHLCV bars in one response. NSE symbols take
//! a `.NS` suffix; US tickers are used as they are. MCX commodities are not
//! covered, so those requests fail over to the next provider.
//!
//! The endpoint is unofficial: it can change, throttle or lag the exchange
//! without notice. To stay polite, responses are cached briefly, requests
//! pass a rate limiter, and a 429 backs every caller off. Daily bars are
//! split-adjusted by Yahoo but not dividend-adjusted, like the other sources.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, FixedOffset};
use futures::stream::{self, StreamExt};
use serde_json::Value;
use swing_core::Candle;
use tokio::sync::RwLock;
use tokio::time::Instant;

use crate::rate::RateLimiter;
use crate::{Error, Interval, Market, MarketData, Quote, Result};

/// Provider id.
pub const ID: &str = "yahoo";

const BASE_URL: &str = "https://query1.finance.yahoo.com/v8/finance/chart";
/// Yahoo answers 429 or 403 to clients without a browser-like user agent.
const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36";
/// How long one chart response is reused (it carries the quote too).
const CHART_TTL: Duration = Duration::from_secs(30);
/// Requests allowed per minute and per hour; guesses kept well below the
/// point where Yahoo is known to throttle.
const RATE_PER_MIN: u32 = 90;
const RATE_PER_HOUR: u32 = 1_500;
/// Longest a call waits for budget before failing over to the next provider.
const MAX_LIMITER_WAIT: Duration = Duration::from_secs(5);
/// Back-off after Yahoo itself says "too many requests".
const THROTTLE_PAUSE: Duration = Duration::from_secs(60);
/// Symbols fetched at once for a list.
const CONCURRENCY: usize = 5;
/// Cached charts kept; old ones are dropped beyond this.
const CACHE_CAPACITY: usize = 600;

/// One parsed chart response.
#[derive(Debug, Clone)]
struct Chart {
    quote: Quote,
    /// Bars as sent (daily bars normalised to 00:00 UTC of the session date).
    candles: Vec<Candle>,
}

/// `(key, intraday)` → when the chart was fetched, and the chart.
type ChartCache = HashMap<(String, bool), (Instant, Arc<Chart>)>;

/// Quotes and candles from Yahoo Finance. Read-only; never places orders.
pub struct YahooProvider {
    http: reqwest::Client,
    limiter: RateLimiter,
    cache: RwLock<ChartCache>,
}

impl YahooProvider {
    pub fn new() -> Result<Self> {
        Ok(Self {
            http: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(15))
                .user_agent(USER_AGENT)
                .build()?,
            limiter: RateLimiter::per_minute(RATE_PER_MIN)
                .with_window(RATE_PER_HOUR, Duration::from_secs(3_600)),
            cache: RwLock::new(HashMap::new()),
        })
    }

    /// The chart of `key` (a market-qualified symbol), cached for [`CHART_TTL`].
    async fn chart(&self, key: &str, intraday: bool) -> Result<Arc<Chart>> {
        let cache_key = (key.to_owned(), intraday);
        if let Some((at, chart)) = self.cache.read().await.get(&cache_key) {
            if at.elapsed() < CHART_TTL {
                return Ok(chart.clone());
            }
        }
        let (market, bare) = Market::split(key);
        let ticker = ticker(market, bare).ok_or_else(|| Error::Unsupported {
            provider: ID,
            message: format!("{market} symbols are not available from Yahoo"),
        })?;
        if let Err(wait) = self.limiter.acquire_within(1, MAX_LIMITER_WAIT).await {
            return Err(Error::RateLimited {
                provider: ID,
                retry_after: wait,
            });
        }
        let (interval, range) = if intraday {
            ("15m", "5d")
        } else {
            ("1d", "1y")
        };
        let url = reqwest::Url::parse(&format!("{BASE_URL}/"))
            .and_then(|base| base.join(&urlencode(&ticker)))
            .map_err(|e| Error::Upstream {
                provider: ID,
                message: format!("bad symbol `{ticker}`: {e}"),
            })?;
        let resp = self
            .http
            .get(url)
            .query(&[("interval", interval), ("range", range)])
            .send()
            .await?;
        let status = resp.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            self.limiter.pause(THROTTLE_PAUSE).await;
            return Err(Error::RateLimited {
                provider: ID,
                retry_after: THROTTLE_PAUSE,
            });
        }
        let text = resp.text().await?;
        let json: Value = serde_json::from_str(&text).map_err(|e| Error::Decode {
            provider: ID,
            message: format!("{ticker} ({status}): {e}"),
        })?;
        let chart = Arc::new(parse_chart(key, market, &json, intraday)?);

        let mut cache = self.cache.write().await;
        if cache.len() >= CACHE_CAPACITY {
            cache.retain(|_, (at, _)| at.elapsed() < CHART_TTL);
        }
        cache.insert(cache_key, (Instant::now(), chart.clone()));
        Ok(chart)
    }
}

/// Yahoo's ticker for a symbol, or `None` where Yahoo has no such market.
fn ticker(market: Market, symbol: &str) -> Option<String> {
    match market {
        Market::In => Some(format!("{symbol}.NS")),
        Market::Us => Some(symbol.to_owned()),
        Market::Mcx => None,
    }
}

/// Percent-encodes everything but letters, digits, `.`, `-` and `_` (for `M&M.NS`).
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn num(v: &Value, key: &str) -> Option<f64> {
    v.get(key).and_then(Value::as_f64).filter(|n| n.is_finite())
}

/// Reads one chart response into a quote and candles.
fn parse_chart(key: &str, market: Market, json: &Value, intraday: bool) -> Result<Chart> {
    let chart = json.get("chart").unwrap_or(&Value::Null);
    let Some(result) = chart
        .get("result")
        .and_then(Value::as_array)
        .and_then(|r| r.first())
    else {
        let error = chart.get("error").unwrap_or(&Value::Null);
        let code = error.get("code").and_then(Value::as_str).unwrap_or("");
        return Err(if code.eq_ignore_ascii_case("Not Found") {
            Error::NotFound(key.to_owned())
        } else {
            Error::Upstream {
                provider: ID,
                message: error
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("empty chart response")
                    .to_owned(),
            }
        });
    };
    let meta = result.get("meta").unwrap_or(&Value::Null);
    let offset = meta.get("gmtoffset").and_then(Value::as_i64).unwrap_or(0);
    let stamps: Vec<i64> = result
        .get("timestamp")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_i64).collect())
        .unwrap_or_default();
    let series = result
        .get("indicators")
        .and_then(|i| i.get("quote"))
        .and_then(Value::as_array)
        .and_then(|q| q.first())
        .unwrap_or(&Value::Null);
    let column = |name: &str| -> Vec<Option<f64>> {
        series
            .get(name)
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|v| v.as_f64().filter(|n| n.is_finite()))
                    .collect()
            })
            .unwrap_or_default()
    };
    let (open, high, low, close, volume) = (
        column("open"),
        column("high"),
        column("low"),
        column("close"),
        column("volume"),
    );

    // Session date in the exchange's own time zone, as 00:00 UTC of that date.
    let session = |ts: i64| (ts + offset).div_euclid(86_400) * 86_400;
    let mut candles: Vec<Candle> = stamps
        .iter()
        .enumerate()
        .filter_map(|(i, ts)| {
            let at = |col: &Vec<Option<f64>>| col.get(i).copied().flatten();
            // Yahoo sends nulls for bars without trades; skip them.
            Some(Candle::new(
                if intraday { *ts } else { session(*ts) },
                at(&open)?,
                at(&high)?,
                at(&low)?,
                at(&close)?,
                at(&volume).unwrap_or(0.0),
            ))
        })
        .collect();
    candles.sort_by_key(|c| c.ts);
    candles.dedup_by_key(|c| c.ts);

    let price = num(meta, "regularMarketPrice").ok_or_else(|| Error::Decode {
        provider: ID,
        message: format!("no price for {key}"),
    })?;
    let market_time = meta.get("regularMarketTime").and_then(Value::as_i64);
    // The previous session's close: the bar before the one the price belongs to.
    let prev_close = if intraday {
        num(meta, "previousClose").or_else(|| num(meta, "chartPreviousClose"))
    } else {
        let n = candles.len();
        let last_is_current = match (candles.last(), market_time) {
            (Some(last), Some(t)) => last.ts == session(t),
            _ => true,
        };
        match (last_is_current, n) {
            (true, n) if n >= 2 => Some(candles[n - 2].close),
            (false, n) if n >= 1 => Some(candles[n - 1].close),
            _ => num(meta, "chartPreviousClose"),
        }
    };
    let change = prev_close.map_or(0.0, |p| price - p);
    let change_pct = prev_close
        .filter(|p| *p > 0.0)
        .map_or(0.0, |p| change / p * 100.0);
    let name = ["longName", "shortName"]
        .iter()
        .find_map(|k| meta.get(*k).and_then(Value::as_str))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(Market::split(key).1)
        .to_owned();
    let updated_at = market_time.and_then(|t| {
        let zone = FixedOffset::east_opt(i32::try_from(offset).ok()?)?;
        Some(
            DateTime::from_timestamp(t, 0)?
                .with_timezone(&zone)
                .to_rfc3339(),
        )
    });
    let day_high = num(meta, "regularMarketDayHigh").unwrap_or(price);
    let day_low = num(meta, "regularMarketDayLow").unwrap_or(price);

    Ok(Chart {
        quote: Quote {
            symbol: key.to_owned(),
            name,
            exchange: market.exchange().to_owned(),
            price,
            change,
            change_pct,
            day_high,
            day_low,
            high_52w: num(meta, "fiftyTwoWeekHigh").unwrap_or(day_high),
            low_52w: num(meta, "fiftyTwoWeekLow").unwrap_or(day_low),
            volume: num(meta, "regularMarketVolume").unwrap_or(0.0),
            updated_at,
        },
        candles,
    })
}

#[async_trait]
impl MarketData for YahooProvider {
    fn id(&self) -> &'static str {
        ID
    }

    /// One request per symbol, a few at a time. A symbol Yahoo does not
    /// know is skipped; if nothing at all could be fetched the first error
    /// is returned so the next provider gets a turn.
    async fn quotes(&self, symbols: &[String]) -> Result<Vec<Quote>> {
        let mut keys: Vec<String> = Vec::new();
        for s in symbols {
            let s = s.trim().to_ascii_uppercase();
            if !s.is_empty() && !keys.contains(&s) {
                keys.push(s);
            }
        }
        // Owned keys keep the futures free of borrowed arguments (which trips
        // higher-ranked lifetime inference inside `async_trait` methods).
        let jobs: Vec<_> = keys
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, key)| async move { (i, self.chart(&key, false).await) })
            .collect();
        let results: Vec<(usize, Result<Arc<Chart>>)> = stream::iter(jobs)
            .buffer_unordered(CONCURRENCY)
            .collect()
            .await;

        let mut quotes: Vec<(usize, Quote)> = Vec::new();
        let mut first_error = None;
        for (i, result) in results {
            match result {
                Ok(chart) => quotes.push((i, chart.quote.clone())),
                Err(Error::NotFound(symbol)) => {
                    tracing::debug!(%symbol, "Yahoo does not know this symbol");
                }
                Err(e) => {
                    first_error.get_or_insert(e);
                }
            }
        }
        // A partial list would silently drop stocks; let the next provider answer instead.
        if let Some(e) = first_error {
            return Err(e);
        }
        quotes.sort_by_key(|(i, _)| *i);
        Ok(quotes.into_iter().map(|(_, q)| q).collect())
    }

    async fn candles(&self, symbol: &str, interval: Interval) -> Result<Vec<Candle>> {
        let key = symbol.trim().to_ascii_uppercase();
        let chart = self.chart(&key, interval == Interval::Min15).await?;
        Ok(match interval {
            Interval::Week => crate::demo::weekly(&chart.candles),
            Interval::Day | Interval::Min15 => chart.candles.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 2026-11-18 and 2026-11-19 at 09:15 IST, as Yahoo stamps NSE daily bars.
    const D19: i64 = 1_795_059_900;
    const D18: i64 = D19 - 86_400;

    fn daily() -> Value {
        json!({"chart": {"error": null, "result": [{
            "meta": {
                "currency": "INR", "symbol": "COFORGE.NS", "exchangeName": "NSI", "gmtoffset": 19800,
                "longName": "Coforge Limited", "shortName": "COFORGE LTD",
                "regularMarketPrice": 2041.0, "regularMarketTime": D19 + 6 * 3600 + 13 * 60,
                "regularMarketDayHigh": 2054.1, "regularMarketDayLow": 2011.2, "regularMarketVolume": 3451234,
                "fiftyTwoWeekHigh": 2127.9, "fiftyTwoWeekLow": 1008.1, "chartPreviousClose": 1500.0
            },
            "timestamp": [D18 - 86400, D18, D19],
            "indicators": {"quote": [{
                "open": [2000.0, null, 2030.0], "high": [2010.0, null, 2054.1],
                "low": [1990.0, null, 2011.2], "close": [2005.0, null, 2041.0],
                "volume": [1000, null, 3451234]
            }], "adjclose": [{"adjclose": [2005.0, null, 2041.0]}]}
        }]}})
    }

    #[test]
    fn parses_a_daily_chart_into_quote_and_candles() {
        let chart = parse_chart("COFORGE", Market::In, &daily(), false).unwrap();
        // The null bar is skipped; bars sit at 00:00 UTC of their IST date.
        assert_eq!(chart.candles.len(), 2);
        assert_eq!(chart.candles[1].ts, 1_795_046_400); // 2026-11-19 00:00 UTC
        assert_eq!(chart.candles[1].close, 2041.0);
        assert_eq!(chart.candles[0].ts, 1_795_046_400 - 2 * 86_400);

        let q = &chart.quote;
        assert_eq!(
            (q.symbol.as_str(), q.name.as_str(), q.exchange.as_str()),
            ("COFORGE", "Coforge Limited", "NSE")
        );
        assert_eq!(q.price, 2041.0);
        // Change is against the previous bar's close, not Yahoo's range-start close.
        assert_eq!(q.change, 36.0);
        assert!((q.change_pct - 36.0 / 2005.0 * 100.0).abs() < 1e-9);
        assert_eq!(
            (q.day_high, q.day_low, q.high_52w, q.low_52w),
            (2054.1, 2011.2, 2127.9, 1008.1)
        );
        assert_eq!(q.volume, 3_451_234.0);
        assert_eq!(q.updated_at.as_deref(), Some("2026-11-19T15:28:00+05:30"));
    }

    #[test]
    fn uses_the_last_bar_as_previous_close_before_a_new_session_bar_exists() {
        let mut v = daily();
        // The quote is from the 20th, but the bars still end on the 19th.
        v["chart"]["result"][0]["meta"]["regularMarketTime"] = json!(D19 + 86_400 + 3_600);
        v["chart"]["result"][0]["meta"]["regularMarketPrice"] = json!(2050.0);
        let chart = parse_chart("COFORGE", Market::In, &v, false).unwrap();
        assert_eq!(chart.quote.change, 9.0);
    }

    #[test]
    fn parses_intraday_bars_and_us_symbols() {
        let v = json!({"chart": {"error": null, "result": [{
            "meta": {"gmtoffset": -18000, "shortName": "Apple Inc.", "regularMarketPrice": 348.46,
                     "previousClose": 354.88, "regularMarketTime": 1_795_194_000},
            "timestamp": [1_795_190_400, 1_795_191_300],
            "indicators": {"quote": [{"open": [350.0, 349.0], "high": [351.0, 349.5], "low": [348.9, 348.0],
                                      "close": [349.0, 348.46], "volume": [10, 20]}]}
        }]}});
        let chart = parse_chart("US:AAPL", Market::Us, &v, true).unwrap();
        assert_eq!(chart.candles.len(), 2);
        assert_eq!(
            chart.candles[0].ts, 1_795_190_400,
            "intraday bars keep their real time"
        );
        assert_eq!(chart.quote.symbol, "US:AAPL");
        assert_eq!(chart.quote.exchange, "US");
        assert!((chart.quote.change - (348.46 - 354.88)).abs() < 1e-9);
        assert!(chart
            .quote
            .updated_at
            .as_deref()
            .unwrap()
            .ends_with("-05:00"));
    }

    #[test]
    fn maps_errors_and_tickers() {
        let missing = json!({"chart": {"result": null,
            "error": {"code": "Not Found", "description": "No data found, symbol may be delisted"}}});
        assert!(matches!(
            parse_chart("NOPE", Market::In, &missing, false),
            Err(Error::NotFound(s)) if s == "NOPE"
        ));
        let broken = json!({"chart": {"result": null, "error": {"code": "Bad Request", "description": "boom"}}});
        assert!(matches!(
            parse_chart("X", Market::In, &broken, false),
            Err(Error::Upstream { message, .. }) if message == "boom"
        ));
        assert!(parse_chart("X", Market::In, &json!({}), false).is_err());

        assert_eq!(ticker(Market::In, "M&M").as_deref(), Some("M&M.NS"));
        assert_eq!(ticker(Market::Us, "AAPL").as_deref(), Some("AAPL"));
        assert_eq!(ticker(Market::Mcx, "GOLD"), None);
        assert_eq!(urlencode("M&M.NS"), "M%26M.NS");
        assert_eq!(urlencode("BAJAJ-AUTO.NS"), "BAJAJ-AUTO.NS");
    }
}
