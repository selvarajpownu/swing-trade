//! INDmoney market data through its MCP server.
//!
//! Symbols are resolved to INDmoney `ind_key`s with `lookup_ind_keys`.
//! That search is fuzzy and returns a flat candidate list (for `ACE` it
//! returns AceVector and others, but not Action Construction Equipment), so
//! every candidate is checked against `entity_basic.symbol` from
//! `get_indian_stocks_details`. Symbols still unresolved get a second search
//! by company name when one is known. Resolved keys are cached for the life
//! of the process.

pub mod oauth;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use futures::future::try_join_all;
use serde_json::{json, Map, Value};
use swing_core::Candle;
use tokio::sync::RwLock;

use crate::mcp::{McpClient, TokenSource};
use crate::{names, Error, Interval, MarketData, Quote, Result};

pub use oauth::{login, token_source, FileTokenSource, LoginOptions, StoredToken};

/// Provider id.
pub const ID: &str = "indmoney";
/// Public INDmoney MCP endpoint.
pub const DEFAULT_MCP_URL: &str = "https://mcp.indmoney.com/mcp";

/// The details tool accepts at most this many keys per call.
const MAX_KEYS_PER_CALL: usize = 10;
/// The lookup tool accepts at most this many names per call.
const MAX_NAMES_PER_CALL: usize = 10;
/// IST is UTC+05:30.
const IST_OFFSET_SECS: i64 = 19_800;

/// Quotes and candles from INDmoney. Read-only; never places orders.
pub struct IndmoneyProvider {
    mcp: McpClient,
    /// Upper-case symbol → ind_key.
    keys: RwLock<HashMap<String, String>>,
}

impl IndmoneyProvider {
    /// Connects lazily to the MCP server at `mcp_url` (see [`DEFAULT_MCP_URL`]).
    pub fn new(mcp_url: impl Into<String>, tokens: Arc<dyn TokenSource>) -> Result<Self> {
        Ok(Self {
            mcp: McpClient::new(ID, mcp_url, tokens)?,
            keys: RwLock::new(HashMap::new()),
        })
    }

    /// Resolves upper-case symbols to ind_keys. Unknown symbols are left out.
    async fn resolve(&self, symbols: &[String]) -> Result<HashMap<String, String>> {
        let mut found = HashMap::new();
        let mut missing = Vec::new();
        {
            let cache = self.keys.read().await;
            for s in symbols {
                match cache.get(s) {
                    Some(k) => {
                        found.insert(s.clone(), k.clone());
                    }
                    None if !missing.contains(s) => missing.push(s.clone()),
                    None => {}
                }
            }
        }
        if missing.is_empty() {
            return Ok(found);
        }

        // Round 1: search by ticker.
        let queries: Vec<(String, String)> =
            missing.iter().map(|s| (s.clone(), s.clone())).collect();
        let mut resolved = self.resolve_round(&queries).await?;

        // Round 2: search by company name for tickers the first round missed.
        let by_name: Vec<(String, String)> = missing
            .iter()
            .filter(|s| !resolved.contains_key(*s))
            .filter_map(|s| names::company_name(s).map(|n| (s.clone(), n.to_owned())))
            .collect();
        if !by_name.is_empty() {
            resolved.extend(self.resolve_round(&by_name).await?);
        }

        for s in missing.iter().filter(|s| !resolved.contains_key(*s)) {
            tracing::debug!(symbol = %s, "no INDmoney instrument matches symbol");
        }
        if !resolved.is_empty() {
            let mut cache = self.keys.write().await;
            for (s, k) in &resolved {
                cache.insert(s.clone(), k.clone());
            }
        }
        found.extend(resolved);
        Ok(found)
    }

    /// One lookup + verify pass. `queries` holds `(symbol, search text)`.
    async fn resolve_round(&self, queries: &[(String, String)]) -> Result<HashMap<String, String>> {
        let search: Vec<&str> = queries.iter().map(|(_, q)| q.as_str()).collect();
        let lookups = try_join_all(search.chunks(MAX_NAMES_PER_CALL).map(|chunk| {
            self.mcp.call_tool(
                "lookup_ind_keys",
                json!({ "names": chunk, "filter_type": "IN_STOCKS" }),
            )
        }))
        .await?;

        let mut candidates: Vec<String> = Vec::new();
        for v in &lookups {
            for key in candidate_keys(v) {
                if !candidates.contains(&key) {
                    candidates.push(key);
                }
            }
        }
        if candidates.is_empty() {
            return Ok(HashMap::new());
        }
        let details = self.details(&candidates).await?;
        let wanted: Vec<&str> = queries.iter().map(|(s, _)| s.as_str()).collect();
        Ok(match_symbols(&wanted, &details))
    }

    /// `get_indian_stocks_details` for any number of keys, merged into one map.
    async fn details(&self, keys: &[String]) -> Result<Map<String, Value>> {
        let parts = try_join_all(keys.chunks(MAX_KEYS_PER_CALL).map(|chunk| {
            self.mcp
                .call_tool("get_indian_stocks_details", json!({ "ind_keys": chunk }))
        }))
        .await?;
        let mut out = Map::new();
        for part in parts {
            match part {
                Value::Object(mut map) => {
                    // Tolerate a `{"data": {...}}` wrapper around the keyed object.
                    let wrapped = !map.keys().any(|k| looks_like_key(k))
                        && matches!(map.get("data"), Some(Value::Object(_)));
                    match map.remove("data") {
                        Some(Value::Object(inner)) if wrapped => out.extend(inner),
                        Some(data) => {
                            map.insert("data".to_owned(), data);
                            out.extend(map);
                        }
                        None => out.extend(map),
                    }
                }
                other => tracing::warn!(response = %other, "unexpected details response"),
            }
        }
        Ok(out)
    }
}

#[async_trait]
impl MarketData for IndmoneyProvider {
    fn id(&self) -> &'static str {
        ID
    }

    async fn quotes(&self, symbols: &[String]) -> Result<Vec<Quote>> {
        let mut wanted: Vec<String> = Vec::new();
        for s in symbols {
            let s = s.trim().to_ascii_uppercase();
            if !s.is_empty() && !wanted.contains(&s) {
                wanted.push(s);
            }
        }
        if wanted.is_empty() {
            return Ok(Vec::new());
        }
        let keys = self.resolve(&wanted).await?;
        let ordered: Vec<(&String, &String)> = wanted
            .iter()
            .filter_map(|s| keys.get(s).map(|k| (s, k)))
            .collect();
        if ordered.is_empty() {
            return Ok(Vec::new());
        }
        let mut key_list: Vec<String> = Vec::with_capacity(ordered.len());
        for (_, k) in &ordered {
            if !key_list.contains(*k) {
                key_list.push((*k).clone());
            }
        }
        let details = self.details(&key_list).await?;
        Ok(ordered
            .into_iter()
            .filter_map(|(symbol, key)| {
                let quote = details.get(key).and_then(|d| parse_quote(symbol, d));
                if quote.is_none() {
                    tracing::warn!(%symbol, %key, "INDmoney returned no usable quote");
                }
                quote
            })
            .collect())
    }

    async fn candles(&self, symbol: &str, interval: Interval) -> Result<Vec<Candle>> {
        let symbol = symbol.trim().to_ascii_uppercase();
        let keys = self.resolve(std::slice::from_ref(&symbol)).await?;
        let key = keys
            .get(&symbol)
            .ok_or_else(|| Error::NotFound(symbol.clone()))?;
        let (iv, lookback) = interval.indmoney_params();
        let v = self
            .mcp
            .call_tool(
                "get_indian_stocks_ohlc",
                json!({ "ind_key": key, "interval": iv, "lookback": lookback }),
            )
            .await?;
        parse_candles(&v)
    }
}

/// True for strings shaped like an ind_key (`INDS03131`, `INDI00339`).
fn looks_like_key(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() > 4 && b.starts_with(b"IND") && b[4..].iter().all(u8::is_ascii_digit)
}

/// Every non-null `ind_key` in a lookup response, in order.
/// Accepts a bare array or an object wrapping one.
fn candidate_keys(v: &Value) -> Vec<String> {
    let items: Vec<&Value> = match v {
        Value::Array(items) => items.iter().collect(),
        Value::Object(map) => map.values().filter_map(Value::as_array).flatten().collect(),
        _ => Vec::new(),
    };
    items
        .into_iter()
        .filter_map(|i| i.get("ind_key").and_then(Value::as_str))
        .map(str::to_owned)
        .collect()
}

/// Maps wanted symbols to the candidate whose listed symbol matches,
/// preferring NSE listings over others.
fn match_symbols(wanted: &[&str], details: &Map<String, Value>) -> HashMap<String, String> {
    let wanted: HashSet<String> = wanted.iter().map(|s| s.to_ascii_uppercase()).collect();
    let mut best: HashMap<String, (String, bool)> = HashMap::new();
    for (key, entry) in details {
        let basic = entry.get("entity_basic");
        let Some(symbol) = basic
            .and_then(|b| b.get("symbol"))
            .and_then(Value::as_str)
            .map(|s| s.trim().to_ascii_uppercase())
        else {
            continue;
        };
        if !wanted.contains(&symbol) {
            continue;
        }
        let nse = basic
            .and_then(|b| b.get("exchange"))
            .and_then(Value::as_str)
            .is_some_and(|e| e.eq_ignore_ascii_case("NSE"));
        match best.get(&symbol) {
            Some((_, true)) => {}
            Some((_, false)) if !nse => {}
            _ => {
                best.insert(symbol, (key.clone(), nse));
            }
        }
    }
    best.into_iter().map(|(s, (k, _))| (s, k)).collect()
}

/// A number that may arrive as a JSON number or a numeric string.
fn num(v: &Value, key: &str) -> Option<f64> {
    let field = v.get(key)?;
    let n = match field {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().replace(',', "").parse().ok(),
        _ => None,
    }?;
    n.is_finite().then_some(n)
}

fn text<'a>(v: Option<&'a Value>, key: &str) -> Option<&'a str> {
    v?.get(key)?
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// Builds a [`Quote`] from one `get_indian_stocks_details` entry.
fn parse_quote(symbol: &str, entry: &Value) -> Option<Quote> {
    let basic = entry.get("entity_basic");
    let stats = entry.get("entity_stats")?;
    let price = num(stats, "live_price")?;
    let prev_close = num(stats, "prev_close");
    let change = num(stats, "day_change")
        .or_else(|| prev_close.map(|p| price - p))
        .unwrap_or(0.0);
    let change_pct = num(stats, "day_change_percentage")
        .or_else(|| {
            prev_close
                .filter(|p| *p != 0.0)
                .map(|p| (price - p) / p * 100.0)
        })
        .unwrap_or(0.0);
    let day_high = num(stats, "day_high").unwrap_or(price);
    let day_low = num(stats, "day_low").unwrap_or(price);
    Some(Quote {
        symbol: symbol.to_owned(),
        name: text(basic, "short_name")
            .or_else(|| text(basic, "display_name"))
            .or_else(|| text(basic, "name"))
            .unwrap_or(symbol)
            .to_owned(),
        exchange: text(basic, "exchange").unwrap_or("NSE").to_owned(),
        price,
        change,
        change_pct,
        day_high,
        day_low,
        high_52w: num(stats, "52week_high").unwrap_or(day_high),
        low_52w: num(stats, "52week_low").unwrap_or(day_low),
        volume: num(stats, "volume").unwrap_or(0.0),
        updated_at: text(Some(stats), "price_updated_at_ist").map(str::to_owned),
    })
}

/// Parses an IST timestamp into Unix seconds.
///
/// `2025-11-11 05:30:00` (IST wall clock, no offset) is the observed shape;
/// RFC 3339 strings with an explicit offset and bare dates are accepted too.
fn parse_ist(s: &str) -> Option<i64> {
    let s = s.trim();
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(dt.timestamp());
    }
    const FORMATS: [&str; 4] = [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%d %H:%M",
    ];
    let naive = FORMATS
        .iter()
        .find_map(|f| chrono::NaiveDateTime::parse_from_str(s, f).ok())
        .or_else(|| {
            chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .ok()
                .and_then(|d| d.and_hms_opt(0, 0, 0))
        })?;
    Some(naive.and_utc().timestamp() - IST_OFFSET_SECS)
}

/// Parses a `get_indian_stocks_ohlc` response into candles, oldest first.
fn parse_candles(v: &Value) -> Result<Vec<Candle>> {
    let rows = v
        .get("candles")
        .and_then(Value::as_array)
        .or_else(|| v.as_array())
        .ok_or_else(|| Error::Decode {
            provider: ID,
            message: "OHLC response has no candles array".to_owned(),
        })?;

    let mut skipped = 0usize;
    let mut out: Vec<Candle> = rows
        .iter()
        .filter_map(|r| {
            let ts = r
                .get("datetime_ist")
                .and_then(Value::as_str)
                .and_then(parse_ist)
                .or_else(|| r.get("timestamp").and_then(Value::as_i64));
            let candle = match (
                ts,
                num(r, "open"),
                num(r, "high"),
                num(r, "low"),
                num(r, "close"),
            ) {
                (Some(ts), Some(o), Some(h), Some(l), Some(c)) => {
                    Some(Candle::new(ts, o, h, l, c, num(r, "volume").unwrap_or(0.0)))
                }
                _ => None,
            };
            if candle.is_none() {
                skipped += 1;
            }
            candle
        })
        .collect();
    if skipped > 0 {
        tracing::warn!(skipped, "skipped malformed INDmoney candles");
    }
    out.sort_by_key(|c| c.ts);
    out.dedup_by_key(|c| c.ts);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ist_timestamps() {
        // 05:30 IST is midnight UTC: 2025-11-11T00:00:00Z.
        assert_eq!(parse_ist("2025-11-11 05:30:00"), Some(1_762_819_200));
        assert_eq!(parse_ist("2025-11-11T05:30:00"), Some(1_762_819_200));
        assert_eq!(parse_ist("2025-11-11 05:30:00.000"), Some(1_762_819_200));
        assert_eq!(parse_ist("2025-11-11 05:30"), Some(1_762_819_200));
        assert_eq!(parse_ist("2026-11-09T15:59:59+05:30"), Some(1_794_220_199));
        assert_eq!(
            parse_ist("2025-11-11"),
            Some(1_762_819_200 - IST_OFFSET_SECS)
        );
        assert_eq!(parse_ist("yesterday"), None);
    }

    #[test]
    fn parses_ohlc_response() {
        let v = json!({
            "ind_key": "INDS03131", "interval": "1day", "count": 3,
            "candles": [
                {"datetime_ist":"2025-11-12 05:30:00","open":1908.7,"high":1950,"low":1900,"close":1940.5,"volume":"1,200"},
                {"datetime_ist":"2025-11-11 05:30:00","open":1768.8,"high":1932.4,"low":1756,"close":1908.7,"volume":20101226},
                {"datetime_ist":"bad","open":1,"high":1,"low":1,"close":1},
                {"datetime_ist":"2025-11-12 05:30:00","open":1,"high":1,"low":1,"close":1}
            ]
        });
        let candles = parse_candles(&v).unwrap();
        assert_eq!(candles.len(), 2);
        assert_eq!(candles[0].ts, 1_762_819_200);
        assert_eq!(candles[0].close, 1908.7);
        assert_eq!(candles[0].volume, 20_101_226.0);
        assert_eq!(candles[1].ts, 1_762_819_200 + 86_400);
        assert_eq!(candles[1].volume, 1_200.0);
        assert!(parse_candles(&json!({"error": "x"})).is_err());
    }

    fn mcx_details() -> Value {
        json!({
            "INDS01940": {
                "entity_basic": {"name":"Multi Commodity Exchange of India Ltd","short_name":"Multi Commodity Exchange of India","symbol":"MCX","exchange":"NSE","market_cap_in_currency":90394.61},
                "entity_stats": {"live_price":3545,"day_change":-12,"day_change_percentage":-0.34,"prev_close":3557,"day_low":3481.3,"day_high":3581.5,"52week_high":3581.5,"52week_low":1762.1,"volume":2199335,"price_updated_at_ist":"2026-11-09T15:59:59+05:30"}
            },
            "INDS03601": {
                "entity_basic": {"short_name":"Ace Integrated Solutions","symbol":"ACEINTEG","exchange":"NSE"},
                "entity_stats": {"live_price":16.89}
            },
            "INDS03947": {
                "entity_basic": {"short_name":"Ace Engitech","symbol":"530669","exchange":"BSE"},
                "entity_stats": {"live_price":65.46}
            }
        })
    }

    #[test]
    fn parses_quote() {
        let d = mcx_details();
        let q = parse_quote("MCX", &d["INDS01940"]).unwrap();
        assert_eq!(q.symbol, "MCX");
        assert_eq!(q.name, "Multi Commodity Exchange of India");
        assert_eq!(q.exchange, "NSE");
        assert_eq!(q.price, 3545.0);
        assert_eq!(q.change, -12.0);
        assert_eq!(q.change_pct, -0.34);
        assert_eq!(q.day_high, 3581.5);
        assert_eq!(q.low_52w, 1762.1);
        assert_eq!(q.volume, 2_199_335.0);
        assert_eq!(q.updated_at.as_deref(), Some("2026-11-09T15:59:59+05:30"));

        let sparse = parse_quote("ACEINTEG", &d["INDS03601"]).unwrap();
        assert_eq!(sparse.change, 0.0);
        assert_eq!(sparse.day_high, 16.89);
        assert_eq!(sparse.volume, 0.0);
        assert!(parse_quote("X", &json!({"entity_basic": {}})).is_none());

        let derived = parse_quote(
            "Y",
            &json!({"entity_stats": {"live_price": "110", "prev_close": 100}}),
        )
        .unwrap();
        assert_eq!(derived.change, 10.0);
        assert!((derived.change_pct - 10.0).abs() < 1e-9);
        assert_eq!(derived.name, "Y");
    }

    #[test]
    fn matches_candidates_by_listed_symbol() {
        let Value::Object(details) = mcx_details() else {
            unreachable!()
        };
        let m = match_symbols(&["MCX", "ACE"], &details);
        assert_eq!(m.get("MCX").map(String::as_str), Some("INDS01940"));
        assert!(!m.contains_key("ACE"), "fuzzy candidates must not match");

        let mut dual = Map::new();
        dual.insert(
            "INDS1".into(),
            json!({"entity_basic": {"symbol": "abc", "exchange": "BSE"}}),
        );
        dual.insert(
            "INDS2".into(),
            json!({"entity_basic": {"symbol": "ABC", "exchange": "NSE"}}),
        );
        dual.insert(
            "INDS3".into(),
            json!({"entity_basic": {"symbol": "ABC", "exchange": "BSE"}}),
        );
        let m = match_symbols(&["ABC"], &dual);
        assert_eq!(m.get("ABC").map(String::as_str), Some("INDS2"));
    }

    #[test]
    fn reads_lookup_candidates() {
        let v = json!([
            {"ind_key": "INDS44947", "name": "AceVector"},
            {"ind_key": null, "name": "x", "error": "retry"},
            {"ind_key": "INDS03601", "name": "Ace Integrated Solutions"}
        ]);
        assert_eq!(candidate_keys(&v), vec!["INDS44947", "INDS03601"]);
        let wrapped = json!({"results": [{"ind_key": "INDS1"}]});
        assert_eq!(candidate_keys(&wrapped), vec!["INDS1"]);
        assert!(candidate_keys(&json!("nothing")).is_empty());
    }

    #[test]
    fn recognises_ind_keys() {
        assert!(looks_like_key("INDS03131"));
        assert!(looks_like_key("INDI00339"));
        assert!(!looks_like_key("data"));
        assert!(!looks_like_key("INDSX"));
    }
}
