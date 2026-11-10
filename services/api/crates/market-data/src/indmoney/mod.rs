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
use std::time::Duration;

use async_trait::async_trait;
use futures::future::try_join_all;
use serde_json::{json, Map, Value};
use swing_core::Candle;
use tokio::sync::RwLock;

use crate::mcp::{snippet, McpClient, TokenSource};
use crate::rate::RateLimiter;
use crate::{names, Error, Interval, Market, MarketData, NewsItem, Quote, Result};

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
/// How long a resolved MCX near-month contract is reused before looking it
/// up again (contracts roll over at expiry).
const MCX_KEY_TTL: Duration = Duration::from_secs(6 * 3_600);
/// Default client-side budget in cost units per minute, a little under
/// INDmoney's 60 so a mis-estimated tool cost does not trip the server limit.
pub const DEFAULT_RATE_PER_MIN: u32 = 55;
/// Default client-side budget per hour (INDmoney allows 500).
pub const DEFAULT_RATE_PER_HOUR: u32 = 450;
/// Longest a call waits for budget before failing with `RateLimited`.
const MAX_LIMITER_WAIT: Duration = Duration::from_secs(8);
/// INDmoney allows 60 cost units per minute across all tools (OHLC costs 2).
/// A throttled call is retried this many times...
const RATE_LIMIT_RETRIES: u32 = 2;
/// ...when the server asks to wait no longer than this.
const MAX_RATE_LIMIT_WAIT_SECS: u64 = 5;

/// Quotes and candles from INDmoney. Read-only; never places orders.
pub struct IndmoneyProvider {
    mcp: McpClient,
    /// Shared by every tool call, so page requests and scans share one budget.
    limiter: Arc<RateLimiter>,
    /// Tool costs learned from INDmoney's rate-limit replies (tool → units).
    learned_costs: std::sync::Mutex<HashMap<String, u32>>,
    /// Upper-case symbol → ind_key.
    keys: RwLock<HashMap<String, String>>,
    /// `MCX:SYMBOL` → (near-month future's ind_key, when resolved).
    mcx_keys: RwLock<HashMap<String, (String, std::time::Instant)>>,
}

impl IndmoneyProvider {
    /// Connects lazily to the MCP server at `mcp_url` (see [`DEFAULT_MCP_URL`]).
    pub fn new(mcp_url: impl Into<String>, tokens: Arc<dyn TokenSource>) -> Result<Self> {
        Ok(Self {
            mcp: McpClient::new(ID, mcp_url, tokens)?,
            limiter: Arc::new(
                RateLimiter::per_minute(DEFAULT_RATE_PER_MIN)
                    .with_window(DEFAULT_RATE_PER_HOUR, Duration::from_secs(3_600)),
            ),
            learned_costs: std::sync::Mutex::new(HashMap::new()),
            keys: RwLock::new(HashMap::new()),
            mcx_keys: RwLock::new(HashMap::new()),
        })
    }

    /// Replaces the default budgets ([`DEFAULT_RATE_PER_MIN`] units per
    /// minute, [`DEFAULT_RATE_PER_HOUR`] per hour).
    pub fn with_rate_limit(mut self, per_minute: u32, per_hour: u32) -> Self {
        self.limiter = Arc::new(
            RateLimiter::per_minute(per_minute).with_window(per_hour, Duration::from_secs(3_600)),
        );
        self
    }

    /// Calls an MCP tool, waiting out short rate-limit replies. Any other
    /// error object becomes [`Error::Upstream`], so it is never mistaken for
    /// an empty result (which would turn into "symbol not found").
    async fn call(&self, tool: &str, arguments: Value) -> Result<Value> {
        let mut retries = 0;
        loop {
            // Pages must not hang on a spent budget: give up after a short wait.
            if let Err(wait) = self
                .limiter
                .acquire_within(self.cost(tool), MAX_LIMITER_WAIT)
                .await
            {
                return Err(Error::RateLimited {
                    provider: ID,
                    retry_after: wait,
                });
            }
            let v = self.mcp.call_tool(tool, arguments.clone()).await?;
            match tool_error(&v) {
                None => return Ok(v),
                Some(ToolError::RateLimited {
                    retry_after,
                    cost,
                    message,
                }) => {
                    if let Some(cost) = cost {
                        self.learn_cost(tool, cost);
                    }
                    // Hold every caller (scanner and pages) until the server is ready.
                    let wait = Duration::from_secs(retry_after.max(1));
                    self.limiter.pause(wait).await;
                    if retries >= RATE_LIMIT_RETRIES || retry_after > MAX_RATE_LIMIT_WAIT_SECS {
                        tracing::warn!(tool, retry_after, %message, "INDmoney rate limit");
                        return Err(Error::RateLimited {
                            provider: ID,
                            retry_after: wait,
                        });
                    }
                    retries += 1;
                    tracing::debug!(tool, retry_after, %message, "INDmoney rate limit; waiting");
                }
                Some(ToolError::Other(message)) => {
                    return Err(Error::Upstream {
                        provider: ID,
                        message: format!("{tool} failed: {message}"),
                    })
                }
            }
        }
    }

    /// Units to reserve for one call of `tool`.
    fn cost(&self, tool: &str) -> u32 {
        let learned = self
            .learned_costs
            .lock()
            .ok()
            .and_then(|m| m.get(tool).copied());
        learned.unwrap_or_else(|| tool_cost(tool))
    }

    /// Remembers a higher cost reported by the server for `tool`.
    fn learn_cost(&self, tool: &str, cost: u32) {
        let Ok(mut costs) = self.learned_costs.lock() else {
            return;
        };
        let known = costs.get(tool).copied().unwrap_or_else(|| tool_cost(tool));
        if cost > known {
            tracing::info!(tool, cost, "learned INDmoney tool cost");
            costs.insert(tool.to_owned(), cost);
        }
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
            self.call(
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
        self.details_with(keys, &[]).await
    }

    /// Like [`details`](Self::details), with extra `segments` such as `news`.
    async fn details_with(&self, keys: &[String], segments: &[&str]) -> Result<Map<String, Value>> {
        self.keyed_calls("get_indian_stocks_details", "ind_keys", keys, segments)
            .await
    }

    /// Calls a details tool keyed by `arg` in batches and merges the keyed replies.
    async fn keyed_calls(
        &self,
        tool: &'static str,
        arg: &str,
        keys: &[String],
        segments: &[&str],
    ) -> Result<Map<String, Value>> {
        let parts = try_join_all(keys.chunks(MAX_KEYS_PER_CALL).map(|chunk| {
            let mut args = Map::new();
            args.insert(arg.to_owned(), json!(chunk));
            if !segments.is_empty() {
                args.insert("segments".to_owned(), json!(segments));
            }
            self.call(tool, Value::Object(args))
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
                // Plain text instead of JSON is how the server reports problems
                // such as rate limits; fail so the fallback provider says why.
                Value::String(text) => return Err(text_reply(tool, &text)),
                other => tracing::warn!(tool, response = %other, "unexpected details response"),
            }
        }
        Ok(out)
    }

    /// NSE quotes for bare symbols.
    async fn in_quotes(&self, wanted: &[String]) -> Result<Vec<Quote>> {
        let keys = self.resolve(wanted).await?;
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
                let quote = details
                    .get(key)
                    .and_then(|d| parse_quote_for(symbol, d, Market::In));
                if quote.is_none() {
                    let reply = details.get(key).map_or_else(
                        || "no entry for this key".to_owned(),
                        |d| snippet(&d.to_string()),
                    );
                    tracing::warn!(%symbol, %key, %reply, "INDmoney returned no usable quote");
                }
                quote
            })
            .collect())
    }

    /// US quotes for `US:`-prefixed keys.
    async fn us_quotes(&self, keys: &[String]) -> Result<Vec<Quote>> {
        let bare: Vec<String> = keys.iter().map(|k| Market::split(k).1.to_owned()).collect();
        let details = self
            .keyed_calls("get_us_stocks_details", "symbols", &bare, &[])
            .await?;
        Ok(keys
            .iter()
            .zip(&bare)
            .filter_map(|(key, symbol)| {
                let entry = details.get(symbol)?;
                if let Some(e) = entry.get("error") {
                    tracing::warn!(%symbol, error = %e, "INDmoney has no US quote");
                    return None;
                }
                parse_quote_for(key, entry, Market::Us)
            })
            .collect())
    }

    /// MCX quotes for `MCX:`-prefixed keys (near-month futures).
    async fn mcx_quotes(&self, keys: &[String]) -> Result<Vec<Quote>> {
        let resolved = self.resolve_mcx(keys).await?;
        let mut ind_keys: Vec<String> = Vec::new();
        for k in resolved.values() {
            if !ind_keys.contains(k) {
                ind_keys.push(k.clone());
            }
        }
        if ind_keys.is_empty() {
            return Ok(Vec::new());
        }
        let details = self.details(&ind_keys).await?;
        Ok(keys
            .iter()
            .filter_map(|key| {
                let entry = details.get(resolved.get(key)?)?;
                parse_quote_for(key, entry, Market::Mcx)
            })
            .collect())
    }

    /// `MCX:GOLD` → ind_key of the near-month GOLD future. Cached for
    /// [`MCX_KEY_TTL`] so contracts roll over after expiry.
    async fn resolve_mcx(&self, keys: &[String]) -> Result<HashMap<String, String>> {
        let mut found = HashMap::new();
        let mut missing: Vec<&String> = Vec::new();
        {
            let cache = self.mcx_keys.read().await;
            for k in keys {
                match cache.get(k) {
                    Some((ik, at)) if at.elapsed() < MCX_KEY_TTL => {
                        found.insert(k.clone(), ik.clone());
                    }
                    _ if !missing.contains(&k) => missing.push(k),
                    _ => {}
                }
            }
        }
        if missing.is_empty() {
            return Ok(found);
        }
        let names: Vec<&str> = missing.iter().map(|k| Market::split(k).1).collect();
        let lookups = try_join_all(names.chunks(MAX_NAMES_PER_CALL).map(|chunk| {
            self.call(
                "lookup_ind_keys",
                json!({ "names": chunk, "filter_type": "IN_COMMODITY" }),
            )
        }))
        .await?;
        let mut fresh = Vec::new();
        for item in lookups.iter().flat_map(lookup_items) {
            let (Some(ik), Some(name)) = (
                item.get("ind_key").and_then(Value::as_str),
                item.get("name").and_then(Value::as_str),
            ) else {
                continue;
            };
            let name = name.trim().to_ascii_uppercase();
            for k in &missing {
                let bare = Market::split(k).1;
                if !found.contains_key(*k) && name.starts_with(&format!("{bare} ")) {
                    found.insert((*k).clone(), ik.to_owned());
                    fresh.push(((*k).clone(), ik.to_owned()));
                }
            }
        }
        if !fresh.is_empty() {
            let mut cache = self.mcx_keys.write().await;
            for (k, ik) in fresh {
                cache.insert(k, (ik, std::time::Instant::now()));
            }
        }
        Ok(found)
    }

    /// Daily (or weekly, aggregated) US candles; there is no intraday data.
    async fn us_candles(&self, symbol: &str, interval: Interval) -> Result<Vec<Candle>> {
        if interval == Interval::Min15 {
            return Err(Error::Upstream {
                provider: ID,
                message: "15-minute candles are not available for US stocks".to_owned(),
            });
        }
        let v = self
            .call(
                "get_us_stocks_ohlc",
                json!({ "symbols": [symbol], "duration": "1y" }),
            )
            .await?;
        let daily = parse_us_candles(&v, symbol)?;
        Ok(match interval {
            Interval::Week => crate::demo::weekly(&daily),
            _ => daily,
        })
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
        let mut by_market: Vec<(Market, Vec<String>)> = Vec::new();
        for key in &wanted {
            let market = Market::split(key).0;
            match by_market.iter_mut().find(|(m, _)| *m == market) {
                Some((_, keys)) => keys.push(key.clone()),
                None => by_market.push((market, vec![key.clone()])),
            }
        }
        let mut quotes = Vec::new();
        for (market, keys) in by_market {
            quotes.extend(match market {
                Market::In => self.in_quotes(&keys).await?,
                Market::Us => self.us_quotes(&keys).await?,
                Market::Mcx => self.mcx_quotes(&keys).await?,
            });
        }
        quotes.sort_by_key(|q| wanted.iter().position(|w| *w == q.symbol));
        Ok(quotes)
    }

    async fn candles(&self, symbol: &str, interval: Interval) -> Result<Vec<Candle>> {
        let key = symbol.trim().to_ascii_uppercase();
        let (market, bare) = Market::split(&key);
        let ind_key = match market {
            Market::Us => return self.us_candles(bare, interval).await,
            Market::In => self.resolve(std::slice::from_ref(&key)).await?.remove(&key),
            Market::Mcx => self
                .resolve_mcx(std::slice::from_ref(&key))
                .await?
                .remove(&key),
        }
        .ok_or_else(|| Error::NotFound(key.clone()))?;
        let (iv, lookback) = interval.indmoney_params();
        let v = self
            .call(
                "get_indian_stocks_ohlc",
                json!({ "ind_key": ind_key, "interval": iv, "lookback": lookback }),
            )
            .await?;
        parse_candles(&v)
    }

    async fn news(&self, symbol: &str) -> Result<Vec<NewsItem>> {
        let key = symbol.trim().to_ascii_uppercase();
        let (market, bare) = Market::split(&key);
        let entry = match market {
            Market::Mcx => return Ok(Vec::new()),
            Market::Us => self
                .keyed_calls(
                    "get_us_stocks_details",
                    "symbols",
                    &[bare.to_owned()],
                    &["news"],
                )
                .await?
                .remove(bare),
            Market::In => {
                let ind_key = self
                    .resolve(std::slice::from_ref(&key))
                    .await?
                    .remove(&key)
                    .ok_or_else(|| Error::NotFound(key.clone()))?;
                self.details_with(std::slice::from_ref(&ind_key), &["news"])
                    .await?
                    .remove(&ind_key)
            }
        };
        Ok(entry.as_ref().map(parse_news).unwrap_or_default())
    }
}

/// Every object in a lookup reply (a bare array or an object wrapping arrays).
fn lookup_items(v: &Value) -> Vec<&Value> {
    match v {
        Value::Array(items) => items.iter().collect(),
        Value::Object(map) => map.values().filter_map(Value::as_array).flatten().collect(),
        _ => Vec::new(),
    }
}

/// Parses `get_us_stocks_ohlc` (`prices.<SYMBOL>.candles[{date, ...}]`) into
/// daily candles at 00:00 UTC, oldest first. Today's in-progress `today`
/// row is left out: setups are read on completed daily bars.
fn parse_us_candles(v: &Value, symbol: &str) -> Result<Vec<Candle>> {
    if let Value::String(text) = v {
        return Err(text_reply("get_us_stocks_ohlc", text));
    }
    let rows = v
        .get("prices")
        .and_then(|p| p.get(symbol))
        .and_then(|s| s.get("candles"))
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Decode {
            provider: ID,
            message: format!(
                "US OHLC response has no candles for {symbol}: {}",
                snippet(&v.to_string())
            ),
        })?;
    let mut out: Vec<Candle> = rows
        .iter()
        .filter_map(|r| {
            let date = r.get("date").and_then(Value::as_str)?;
            let day = chrono::NaiveDate::parse_from_str(date.trim(), "%Y-%m-%d").ok()?;
            let ts = day.and_hms_opt(0, 0, 0)?.and_utc().timestamp();
            Some(Candle::new(
                ts,
                num(r, "open")?,
                num(r, "high")?,
                num(r, "low")?,
                num(r, "close")?,
                num(r, "volume").unwrap_or(0.0),
            ))
        })
        .collect();
    out.sort_by_key(|c| c.ts);
    out.dedup_by_key(|c| c.ts);
    Ok(out)
}

/// Headlines from a details entry's `news` array. Titles end in
/// `" - 09 Nov, 2026"`, which becomes `published_at`.
fn parse_news(entry: &Value) -> Vec<NewsItem> {
    let Some(items) = entry.get("news").and_then(Value::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|n| {
            let raw = text(Some(n), "title")?;
            let (title, published_at) = match raw.rsplit_once(" - ") {
                Some((head, tail)) => {
                    match chrono::NaiveDate::parse_from_str(tail.trim(), "%d %b, %Y") {
                        Ok(d) => (
                            head.trim().to_owned(),
                            Some(d.format("%Y-%m-%d").to_string()),
                        ),
                        Err(_) => (raw.to_owned(), None),
                    }
                }
                None => (raw.to_owned(), None),
            };
            Some(NewsItem {
                title,
                summary: text(Some(n), "description").map(str::to_owned),
                sentiment: text(Some(n), "sentiment").map(str::to_ascii_lowercase),
                published_at,
                source: None,
                url: None,
            })
        })
        .collect()
}

/// Cost units INDmoney charges per call. OHLC's cost (2) is what its
/// rate-limit replies report; the others are assumed to cost 1.
fn tool_cost(tool: &str) -> u32 {
    match tool {
        "get_indian_stocks_ohlc" | "get_us_stocks_ohlc" => 2,
        _ => 1,
    }
}

/// An error reported inside a successful `tools/call` result.
#[derive(Debug, PartialEq)]
enum ToolError {
    /// `{"error":"rate_limit_exceeded","retry_after_seconds":1,"message":...}`
    RateLimited {
        retry_after: u64,
        /// What the server says the rejected call costs.
        cost: Option<u32>,
        message: String,
    },
    Other(String),
}

/// Recognises a top-level `{"error": "...", "message": "..."}` reply.
/// Data replies are arrays, ind_key maps or objects with `candles`.
fn tool_error(v: &Value) -> Option<ToolError> {
    let map = v.as_object()?;
    let code = map.get("error")?.as_str()?;
    if map.contains_key("candles") || map.keys().any(|k| looks_like_key(k)) {
        return None;
    }
    let message = map
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or(code)
        .to_owned();
    Some(if code == "rate_limit_exceeded" {
        let retry_after = map
            .get("retry_after_seconds")
            .and_then(Value::as_u64)
            .unwrap_or(1);
        let cost = map
            .get("cost")
            .and_then(Value::as_u64)
            .and_then(|c| u32::try_from(c).ok());
        ToolError::RateLimited {
            retry_after,
            cost,
            message,
        }
    } else {
        ToolError::Other(message)
    })
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
fn parse_quote_for(symbol: &str, entry: &Value, market: Market) -> Option<Quote> {
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
        exchange: text(basic, "exchange")
            .or_else(|| text(basic, "exchange_name"))
            .unwrap_or(market.exchange())
            .to_owned(),
        price,
        change,
        change_pct,
        day_high,
        day_low,
        // MCX futures report 0 for both; treat that as missing.
        high_52w: num(stats, "52week_high")
            .filter(|v| *v > 0.0)
            .unwrap_or(day_high),
        low_52w: num(stats, "52week_low")
            .filter(|v| *v > 0.0)
            .unwrap_or(day_low),
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

/// A tool answered with text instead of JSON (e.g. a rate-limit notice).
fn text_reply(tool: &str, text: &str) -> Error {
    Error::Upstream {
        provider: ID,
        message: format!("{tool} replied: {}", snippet(text)),
    }
}

/// Parses a `get_indian_stocks_ohlc` response into candles, oldest first.
fn parse_candles(v: &Value) -> Result<Vec<Candle>> {
    if let Value::String(text) = v {
        return Err(text_reply("get_indian_stocks_ohlc", text));
    }
    let rows = v
        .get("candles")
        .and_then(Value::as_array)
        .or_else(|| v.as_array())
        .ok_or_else(|| Error::Decode {
            provider: ID,
            message: format!(
                "OHLC response has no candles array: {}",
                snippet(&v.to_string())
            ),
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

    fn parse_quote(symbol: &str, entry: &Value) -> Option<Quote> {
        parse_quote_for(symbol, entry, Market::In)
    }

    #[test]
    fn parses_us_and_mcx_replies() {
        let us = json!({"prices": {"AAPL": {"count": 2, "candles": [
            {"date": "2026-11-09", "open": 345.0, "high": 352.33, "low": 344.78, "close": 350.69, "volume": 48716842},
            {"date": "2026-11-06", "open": 338.73, "high": 345.79, "low": 338.3, "close": 342.37, "volume": 44703806}
        ], "today": {"date": "2026-11-10", "open": 351.3, "high": 353.8, "low": 346.48, "last": 348.27}}}});
        let c = parse_us_candles(&us, "AAPL").unwrap();
        assert_eq!(c.len(), 2, "today's partial row is left out");
        assert_eq!(c[1].ts, 1_794_182_400); // 2026-11-09 00:00 UTC
        assert_eq!(c[1].close, 350.69);
        assert!(parse_us_candles(&us, "MSFT").is_err());

        let aapl = json!({"entity_basic": {"short_name": "Apple", "symbol": "AAPL", "exchange_name": "US"},
            "entity_stats": {"live_price": 348.27, "day_change": -2.42, "prev_close": 350.69, "52week_high": 355.38}});
        let q = parse_quote_for("US:AAPL", &aapl, Market::Us).unwrap();
        assert_eq!(
            (q.symbol.as_str(), q.name.as_str(), q.exchange.as_str()),
            ("US:AAPL", "Apple", "US")
        );

        let gold = json!({"entity_basic": {"short_name": "GOLD 04 Dec Fut", "exchange": "MCX"},
            "entity_stats": {"live_price": 140645, "day_high": 142500, "day_low": 140358, "52week_high": 0, "52week_low": 0}});
        let q = parse_quote_for("MCX:GOLD", &gold, Market::Mcx).unwrap();
        assert_eq!((q.high_52w, q.low_52w), (142_500.0, 140_358.0));
    }

    #[test]
    fn parses_news_titles_and_dates() {
        let entry = json!({"news": [
            {"title": "ACE Secures Defence Order, Shares Rise 3% - 30 Sep, 2026", "description": "Order for 74 backhoe loaders.", "sentiment": "Positive"},
            {"title": "No date here", "sentiment": "neutral"},
            {"description": "no title"}
        ]});
        let news = parse_news(&entry);
        assert_eq!(news.len(), 2);
        assert_eq!(news[0].title, "ACE Secures Defence Order, Shares Rise 3%");
        assert_eq!(news[0].published_at.as_deref(), Some("2026-09-30"));
        assert_eq!(news[0].sentiment.as_deref(), Some("positive"));
        assert_eq!(news[1].title, "No date here");
        assert_eq!(news[1].published_at, None);
        assert!(parse_news(&json!({})).is_empty());
    }

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
        assert!(matches!(
            parse_candles(&json!("Too many requests, slow down")),
            Err(Error::Upstream { message, .. }) if message.contains("Too many requests")
        ));
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
    fn recognises_tool_errors() {
        let limited = json!({"cost":2,"current":60,"error":"rate_limit_exceeded","limit":60,
            "message":"Rate limit exceeded: 60 calls per min. Try again in 2s.",
            "retry_after_seconds":2,"scope":"global","tool":"get_indian_stocks_ohlc","window":"min"});
        assert_eq!(
            tool_error(&limited),
            Some(ToolError::RateLimited {
                retry_after: 2,
                cost: Some(2),
                message: "Rate limit exceeded: 60 calls per min. Try again in 2s.".to_owned()
            })
        );
        assert_eq!(
            tool_error(&json!({"error": "bad_key"})),
            Some(ToolError::Other("bad_key".to_owned()))
        );
        assert_eq!(tool_error(&json!({"candles": [], "error": null})), None);
        assert_eq!(tool_error(&json!({"INDS1": {"error": "x"}})), None);
        assert_eq!(
            tool_error(&json!([{"ind_key": null, "error": "retry"}])),
            None
        );
        assert_eq!(tool_error(&mcx_details()), None);
    }

    #[test]
    fn recognises_ind_keys() {
        assert!(looks_like_key("INDS03131"));
        assert!(looks_like_key("INDI00339"));
        assert!(!looks_like_key("data"));
        assert!(!looks_like_key("INDSX"));
    }
}
