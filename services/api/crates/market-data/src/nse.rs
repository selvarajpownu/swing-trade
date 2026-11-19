//! End-of-day data for the whole NSE cash market from NSE's public files.
//!
//! One "full bhavcopy" CSV per session carries open/high/low/close, volume
//! and delivery percentage for every listed stock, so a universe-wide scan
//! needs one small download per day instead of one API call per stock.
//!
//! Facts this module relies on (checked against real files, Nov 2026):
//! - On a holiday the archive serves the previous session's file again, so
//!   the date **inside** the file (`DATE1`) is the truth, not the URL.
//! - Special sessions happen on weekends (Diwali Muhurat trading), so every
//!   calendar day is tried.
//! - Prices are not adjusted for bonuses and splits. They are adjusted here
//!   from NSE's corporate-actions list, cross-checked against the price gap
//!   on the ex-date; demergers use the observed gap. Dividends are left
//!   unadjusted, as on INDmoney's charts.
//!
//! Files are cached under `<dir>/bhav/YYYY-MM-DD.csv` and
//! `<dir>/index/YYYY-MM-DD.csv`; a `YYYY-MM-DD.none` marker remembers a day
//! without a session.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Duration as Days, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use swing_core::Candle;
use tokio::sync::RwLock;

use crate::{Error, Interval, Market, MarketData, Quote, Result};

/// Id reported for data read from NSE's files.
pub const ID: &str = "nse";

const BHAV_URL: &str = "https://nsearchives.nseindia.com/products/content/sec_bhavdata_full_";
const INDEX_URL: &str = "https://nsearchives.nseindia.com/content/indices/ind_close_all_";
const ACTIONS_URL: &str = "https://www.nseindia.com/api/corporates-corporateActions";
/// NSE's servers refuse clients without a browser-like user agent.
const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36";
/// Pause between requests, to stay polite.
const REQUEST_GAP: Duration = Duration::from_millis(150);
/// A missing file this recent may simply not be published yet.
const RECENT_DAYS: i64 = 3;
/// Indices kept from the daily index file.
const INDICES: [&str; 2] = ["Nifty 50", "Nifty 500"];
/// IST is UTC+05:30.
const IST_OFFSET_MINS: i64 = 330;
/// Corporate actions are requested in windows of this many days.
const ACTIONS_WINDOW_DAYS: i64 = 90;

/// One stock's adjusted daily history.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EodSeries {
    /// Oldest first; `ts` is 00:00 UTC of the session date.
    pub candles: Vec<Candle>,
    /// Delivery percentage per bar, parallel to `candles`.
    pub delivery: Vec<Option<f64>>,
    /// True when the latest bar traded in the normal `EQ` series
    /// (false for trade-to-trade `BE`).
    pub main_series: bool,
}

/// Everything loaded from the cached files.
#[derive(Debug, Clone, Default)]
pub struct EodData {
    pub series: HashMap<String, EodSeries>,
    /// Index name → `(ts, close)`, oldest first.
    pub indices: HashMap<String, Vec<(i64, f64)>>,
    /// Sessions loaded.
    pub days: usize,
    pub last_day: Option<NaiveDate>,
    /// Bonus, split and demerger adjustments applied.
    pub adjustments: usize,
}

impl EodData {
    /// Closing values of an index (e.g. `Nifty 50`), oldest first.
    pub fn index_closes(&self, name: &str) -> Option<Vec<f64>> {
        self.indices
            .get(name)
            .map(|v| v.iter().map(|(_, c)| *c).collect())
    }
}

/// Where a sync stands, for status displays.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncStatus {
    pub syncing: bool,
    /// Session files on disk.
    pub days_cached: usize,
    pub days_target: usize,
    pub last_day: Option<NaiveDate>,
    /// Last problem, if the latest sync hit one.
    pub error: Option<String>,
}

/// One row of a bhavcopy file (`EQ` and `BE` series only).
#[derive(Debug, Clone, PartialEq)]
pub struct BhavRow {
    pub symbol: String,
    pub main_series: bool,
    pub prev_close: f64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub delivery_pct: Option<f64>,
}

/// A corporate action that changes the share count or splits the company.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    /// Earlier prices are multiplied by this (0.5 for a 1:1 bonus).
    Factor(f64),
    /// No fixed ratio: the price gap on the ex-date is used.
    Demerger,
}

/// `(symbol, ex-date)` → action.
pub type Actions = HashMap<(String, NaiveDate), Action>;

/// Saved copy of the corporate-actions list. Dates are `YYYY-MM-DD` strings.
#[derive(Debug, Serialize, Deserialize)]
struct ActionsCache {
    fetched: String,
    from: String,
    /// `(symbol, ex-date, subject)`.
    rows: Vec<(String, String, String)>,
}

impl ActionsCache {
    fn rows(&self) -> Vec<(String, NaiveDate, String)> {
        self.rows
            .iter()
            .filter_map(|(symbol, date, subject)| {
                let date = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
                Some((symbol.clone(), date, subject.clone()))
            })
            .collect()
    }

    /// True when it was fetched on `today` and reaches back to `from`.
    fn covers(&self, from: NaiveDate, today: NaiveDate) -> bool {
        let parse = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok();
        parse(&self.fetched) == Some(today) && parse(&self.from).is_some_and(|f| f <= from)
    }
}

/// Downloads, caches and serves NSE end-of-day data.
pub struct NseEod {
    dir: PathBuf,
    http: reqwest::Client,
    target_days: usize,
    data: RwLock<Arc<EodData>>,
    status: RwLock<SyncStatus>,
    /// Symbol → company name, for end-of-day quotes.
    names: RwLock<HashMap<String, String>>,
}

impl NseEod {
    /// Keeps `target_days` sessions under `dir`.
    pub fn new(dir: impl Into<PathBuf>, target_days: usize) -> Result<Self> {
        Ok(Self {
            dir: dir.into(),
            http: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(40))
                .user_agent(USER_AGENT)
                .build()?,
            target_days: target_days.max(30),
            data: RwLock::new(Arc::new(EodData::default())),
            status: RwLock::new(SyncStatus::default()),
            names: RwLock::new(HashMap::new()),
        })
    }

    /// Company names to show on end-of-day quotes (the files carry none).
    pub async fn set_names(&self, names: HashMap<String, String>) {
        *self.names.write().await = names;
    }

    /// The data of the last completed sync.
    pub async fn data(&self) -> Arc<EodData> {
        self.data.read().await.clone()
    }

    pub async fn status(&self) -> SyncStatus {
        self.status.read().await.clone()
    }

    /// Downloads missing sessions, then reloads the series of `symbols`.
    /// A download problem is reported, but whatever is cached still loads.
    pub async fn sync(&self, symbols: &HashSet<String>) -> Result<Arc<EodData>> {
        {
            let mut s = self.status.write().await;
            s.syncing = true;
            s.days_target = self.target_days;
            s.error = None;
        }
        let today = (Utc::now() + Days::minutes(IST_OFFSET_MINS)).date_naive();
        let mut problem = None;
        let mut data = EodData::default();
        // A second pass replaces cached files that turned out to be holiday copies.
        for _ in 0..2 {
            if let Err(e) = self.download_missing(today).await {
                tracing::warn!(error = %e, "NSE download incomplete; using cached sessions");
                problem = Some(e.to_string());
            }
            let first_day = today - Days::days(self.calendar_span());
            let actions = self.actions(first_day, today).await;
            let (dir, wanted, keep) = (self.dir.clone(), symbols.clone(), self.target_days);
            let (built, removed) =
                tokio::task::spawn_blocking(move || load(&dir, &wanted, &actions, keep))
                    .await
                    .map_err(|e| Error::Io(std::io::Error::other(e)))??;
            data = built;
            if removed == 0 || problem.is_some() {
                break;
            }
        }
        let data = Arc::new(data);
        *self.data.write().await = data.clone();
        let mut s = self.status.write().await;
        s.syncing = false;
        s.days_cached = data.days;
        s.last_day = data.last_day;
        s.error = problem;
        Ok(data)
    }

    /// Calendar days that comfortably cover `target_days` sessions.
    fn calendar_span(&self) -> i64 {
        (self.target_days as i64) * 17 / 10 + 40
    }

    async fn download_missing(&self, today: NaiveDate) -> Result<()> {
        let bhav = self.dir.join("bhav");
        let index = self.dir.join("index");
        tokio::fs::create_dir_all(&bhav).await?;
        tokio::fs::create_dir_all(&index).await?;

        let mut have = 0usize;
        for back in 0..self.calendar_span() {
            if have >= self.target_days {
                break;
            }
            let day = today - Days::days(back);
            let name = day.format("%Y-%m-%d").to_string();
            let file = bhav.join(format!("{name}.csv"));
            let none = bhav.join(format!("{name}.none"));
            let recent = back <= RECENT_DAYS;
            if exists(&file).await {
                have += 1;
            } else if exists(&none).await {
                continue;
            } else {
                let url = format!("{BHAV_URL}{}.csv", day.format("%d%m%Y"));
                match self.fetch(&url).await? {
                    Some(text) if bhav_date(&text) == Some(day) => {
                        write_atomic(&file, text.as_bytes()).await?;
                        have += 1;
                        let mut s = self.status.write().await;
                        s.days_cached = s.days_cached.max(have);
                    }
                    // A holiday copy of an earlier session, or nothing at all.
                    _ => {
                        if !recent {
                            tokio::fs::write(&none, b"").await?;
                        }
                        continue;
                    }
                }
            }
            // The index file of a session, best effort.
            let ifile = index.join(format!("{name}.csv"));
            let inone = index.join(format!("{name}.none"));
            if !exists(&ifile).await && !exists(&inone).await {
                let url = format!("{INDEX_URL}{}.csv", day.format("%d%m%Y"));
                match self.fetch(&url).await {
                    Ok(Some(text)) if text.starts_with("Index Name") => {
                        write_atomic(&ifile, text.as_bytes()).await?;
                    }
                    Ok(_) if !recent => tokio::fs::write(&inone, b"").await?,
                    Ok(_) => {}
                    Err(e) => tracing::debug!(%day, error = %e, "index file not fetched"),
                }
            }
        }
        Ok(())
    }

    /// GETs a text file; `None` for 404.
    async fn fetch(&self, url: &str) -> Result<Option<String>> {
        tokio::time::sleep(REQUEST_GAP).await;
        let resp = self.http.get(url).send().await?;
        let status = resp.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !status.is_success() {
            return Err(Error::Upstream {
                provider: ID,
                message: format!("{url} returned {status}"),
            });
        }
        Ok(Some(resp.text().await?))
    }

    /// Bonus, split and demerger events since `from`, refreshed once a day.
    /// On failure the last saved list is used (or none, with a warning).
    async fn actions(&self, from: NaiveDate, today: NaiveDate) -> Actions {
        let path = self.dir.join("corporate-actions.json");
        let cached: Option<ActionsCache> = tokio::fs::read(&path)
            .await
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok());
        if let Some(c) = cached.as_ref().filter(|c| c.covers(from, today)) {
            return to_actions(&c.rows());
        }
        match self.fetch_actions(from, today).await {
            Ok(rows) => {
                let iso = |d: &NaiveDate| d.format("%Y-%m-%d").to_string();
                let cache = ActionsCache {
                    fetched: iso(&today),
                    from: iso(&from),
                    rows: rows
                        .iter()
                        .map(|(s, d, subject)| (s.clone(), iso(d), subject.clone()))
                        .collect(),
                };
                match serde_json::to_vec(&cache) {
                    Ok(json) => {
                        if let Err(e) = write_atomic(&path, &json).await {
                            tracing::warn!(error = %e, "could not save corporate actions");
                        }
                    }
                    Err(e) => tracing::warn!(error = %e, "could not encode corporate actions"),
                }
                to_actions(&rows)
            }
            Err(e) => {
                tracing::warn!(error = %e, "NSE corporate actions not refreshed; bonus/split adjustment may be stale");
                cached.map(|c| to_actions(&c.rows())).unwrap_or_default()
            }
        }
    }

    async fn fetch_actions(
        &self,
        from: NaiveDate,
        today: NaiveDate,
    ) -> Result<Vec<(String, NaiveDate, String)>> {
        let mut rows = Vec::new();
        let mut start = from;
        while start <= today {
            let end = (start + Days::days(ACTIONS_WINDOW_DAYS - 1)).min(today);
            let url = format!(
                "{ACTIONS_URL}?index=equities&from_date={}&to_date={}",
                start.format("%d-%m-%Y"),
                end.format("%d-%m-%Y")
            );
            let text = self.fetch(&url).await?.ok_or_else(|| Error::Upstream {
                provider: ID,
                message: "corporate actions API returned 404".to_owned(),
            })?;
            let json: Value = serde_json::from_str(&text).map_err(|e| Error::Decode {
                provider: ID,
                message: format!("corporate actions: {e}"),
            })?;
            rows.extend(parse_actions(&json));
            start = end + Days::days(1);
        }
        Ok(rows)
    }
}

/// Bars in roughly one trading year.
const BARS_52W: usize = 250;

/// Daily and weekly candles, and end-of-day quotes, for NSE stocks from the
/// cached files. Meant as a fallback behind live sources: its "quote" is the
/// last session's close, and it has no intraday bars.
pub struct NseEodProvider {
    eod: Arc<NseEod>,
}

impl NseEodProvider {
    pub fn new(eod: Arc<NseEod>) -> Self {
        Self { eod }
    }

    /// The series of an NSE key. Errors are never `NotFound`: the files only
    /// hold the scan universe, so a miss says nothing about other sources.
    async fn series(&self, key: &str) -> Result<(Arc<EodData>, String)> {
        let unavailable = |message: String| Error::Unsupported {
            provider: ID,
            message,
        };
        let (market, bare) = Market::split(key);
        if market != Market::In {
            return Err(unavailable(format!(
                "{market} symbols are not in NSE's files"
            )));
        }
        let data = self.eod.data().await;
        if data.days == 0 {
            return Err(unavailable(
                "NSE end-of-day files are not loaded yet".to_owned(),
            ));
        }
        if !data.series.contains_key(bare) {
            return Err(unavailable(format!(
                "{bare} is not in the scan universe kept from NSE's files"
            )));
        }
        Ok((data, bare.to_owned()))
    }
}

#[async_trait]
impl MarketData for NseEodProvider {
    fn id(&self) -> &'static str {
        ID
    }

    async fn quotes(&self, symbols: &[String]) -> Result<Vec<Quote>> {
        let names = self.names_snapshot().await;
        let mut quotes = Vec::with_capacity(symbols.len());
        for symbol in symbols {
            let key = symbol.trim().to_ascii_uppercase();
            let (data, bare) = self.series(&key).await?;
            if let Some(q) = data
                .series
                .get(&bare)
                .and_then(|s| eod_quote(&key, names.get(&bare), &s.candles))
            {
                quotes.push(q);
            }
        }
        Ok(quotes)
    }

    async fn candles(&self, symbol: &str, interval: Interval) -> Result<Vec<Candle>> {
        let key = symbol.trim().to_ascii_uppercase();
        if interval == Interval::Min15 {
            return Err(Error::Unsupported {
                provider: ID,
                message: "NSE's end-of-day files have no intraday bars".to_owned(),
            });
        }
        let (data, bare) = self.series(&key).await?;
        let daily = data
            .series
            .get(&bare)
            .map(|s| s.candles.clone())
            .unwrap_or_default();
        Ok(match interval {
            Interval::Week => crate::demo::weekly(&daily),
            _ => daily,
        })
    }
}

impl NseEodProvider {
    async fn names_snapshot(&self) -> HashMap<String, String> {
        self.eod.names.read().await.clone()
    }
}

/// A quote as of the last session's close.
fn eod_quote(key: &str, name: Option<&String>, candles: &[Candle]) -> Option<Quote> {
    let last = candles.last()?;
    let prev_close = candles.len().checked_sub(2).map(|i| candles[i].close);
    let change = prev_close.map_or(0.0, |p| last.close - p);
    let year = &candles[candles.len().saturating_sub(BARS_52W)..];
    let date = DateTime::from_timestamp(last.ts, 0)?.format("%Y-%m-%d");
    Some(Quote {
        symbol: key.to_owned(),
        name: name
            .cloned()
            .unwrap_or_else(|| Market::split(key).1.to_owned()),
        exchange: "NSE".to_owned(),
        price: last.close,
        change,
        change_pct: prev_close
            .filter(|p| *p > 0.0)
            .map_or(0.0, |p| change / p * 100.0),
        day_high: last.high,
        day_low: last.low,
        high_52w: year.iter().map(|c| c.high).fold(f64::MIN, f64::max),
        low_52w: year.iter().map(|c| c.low).fold(f64::MAX, f64::min),
        volume: last.volume,
        // NSE's closing session ends at 15:30 IST.
        updated_at: Some(format!("{date}T15:30:00+05:30")),
    })
}

async fn exists(path: &Path) -> bool {
    tokio::fs::try_exists(path).await.unwrap_or(false)
}

async fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    tokio::fs::write(&tmp, bytes).await?;
    tokio::fs::rename(&tmp, path).await
}

/// The session date inside a bhavcopy file (`DATE1` of its first row).
pub fn bhav_date(text: &str) -> Option<NaiveDate> {
    let mut lines = text.lines();
    let header = lines.next()?;
    if !header.trim_start_matches('\u{feff}').starts_with("SYMBOL") {
        return None;
    }
    let col = header.split(',').position(|h| h.trim() == "DATE1")?;
    let field = lines.next()?.split(',').nth(col)?;
    NaiveDate::parse_from_str(field.trim(), "%d-%b-%Y").ok()
}

/// `EQ` and `BE` rows of a bhavcopy file, optionally only for `wanted`.
pub fn parse_bhavcopy(text: &str, wanted: Option<&HashSet<String>>) -> Vec<BhavRow> {
    let mut lines = text.lines();
    let Some(header) = lines.next() else {
        return Vec::new();
    };
    let cols: Vec<&str> = header
        .trim_start_matches('\u{feff}')
        .split(',')
        .map(str::trim)
        .collect();
    let col = |name: &str| cols.iter().position(|c| *c == name);
    let (
        Some(sym),
        Some(ser),
        Some(prev),
        Some(open),
        Some(high),
        Some(low),
        Some(close),
        Some(vol),
    ) = (
        col("SYMBOL"),
        col("SERIES"),
        col("PREV_CLOSE"),
        col("OPEN_PRICE"),
        col("HIGH_PRICE"),
        col("LOW_PRICE"),
        col("CLOSE_PRICE"),
        col("TTL_TRD_QNTY"),
    )
    else {
        return Vec::new();
    };
    let deliv = col("DELIV_PER");

    let mut rows: Vec<BhavRow> = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    for line in lines {
        let f: Vec<&str> = line.split(',').map(str::trim).collect();
        let (Some(symbol), Some(series)) = (f.get(sym), f.get(ser)) else {
            continue;
        };
        let main_series = *series == "EQ";
        if !main_series && *series != "BE" {
            continue;
        }
        if wanted.is_some_and(|w| !w.contains(*symbol)) {
            continue;
        }
        let num = |i: usize| f.get(i).and_then(|v| v.parse::<f64>().ok());
        let (Some(prev_close), Some(o), Some(h), Some(l), Some(c), Some(v)) = (
            num(prev),
            num(open),
            num(high),
            num(low),
            num(close),
            num(vol),
        ) else {
            continue;
        };
        let row = BhavRow {
            symbol: (*symbol).to_owned(),
            main_series,
            prev_close,
            open: o,
            high: h,
            low: l,
            close: c,
            volume: v,
            delivery_pct: deliv.and_then(num),
        };
        // A symbol can appear in both series on one day; EQ wins.
        match seen.get(*symbol) {
            Some(&i) if main_series && !rows[i].main_series => rows[i] = row,
            Some(_) => {}
            None => {
                seen.insert(row.symbol.clone(), rows.len());
                rows.push(row);
            }
        }
    }
    rows
}

/// `(index name, date, close)` rows of a daily index file.
pub fn parse_index(text: &str) -> Vec<(String, NaiveDate, f64)> {
    let mut lines = text.lines();
    let Some(header) = lines.next() else {
        return Vec::new();
    };
    let cols: Vec<&str> = header.split(',').map(str::trim).collect();
    let col = |name: &str| cols.iter().position(|c| *c == name);
    let (Some(name), Some(date), Some(close)) = (
        col("Index Name"),
        col("Index Date"),
        col("Closing Index Value"),
    ) else {
        return Vec::new();
    };
    lines
        .filter_map(|line| {
            let f: Vec<&str> = line.split(',').map(str::trim).collect();
            Some((
                (*f.get(name)?).to_owned(),
                NaiveDate::parse_from_str(f.get(date)?, "%d-%m-%Y").ok()?,
                f.get(close)?.parse().ok()?,
            ))
        })
        .collect()
}

/// `(symbol, ex-date, subject)` for equity rows of the corporate-actions API.
fn parse_actions(json: &Value) -> Vec<(String, NaiveDate, String)> {
    let items = json
        .as_array()
        .or_else(|| json.get("data").and_then(Value::as_array));
    items
        .into_iter()
        .flatten()
        .filter_map(|r| {
            let series = r.get("series").and_then(Value::as_str)?;
            if series != "EQ" && series != "BE" {
                return None;
            }
            let subject = r.get("subject").and_then(Value::as_str)?;
            action_for(subject)?;
            let date = r.get("exDate").and_then(Value::as_str)?;
            Some((
                r.get("symbol").and_then(Value::as_str)?.trim().to_owned(),
                NaiveDate::parse_from_str(date.trim(), "%d-%b-%Y").ok()?,
                subject.to_owned(),
            ))
        })
        .collect()
}

fn to_actions(rows: &[(String, NaiveDate, String)]) -> Actions {
    let mut out = Actions::new();
    for (symbol, date, subject) in rows {
        let Some(action) = action_for(subject) else {
            continue;
        };
        let key = (symbol.clone(), *date);
        // Two actions on one day (bonus and split) multiply.
        let merged = match (out.get(&key), action) {
            (Some(Action::Factor(a)), Action::Factor(b)) => Action::Factor(a * b),
            (Some(Action::Factor(a)), Action::Demerger) => Action::Factor(*a),
            _ => action,
        };
        out.insert(key, merged);
    }
    out
}

/// Reads NSE's subject line: `Bonus 1:2`, `Face Value Split (Sub-Division) -
/// From Rs 10/- Per Share To Re 1/- Per Share`, `Demerger`. Bonus issues of
/// preference shares or debentures do not change the share count.
pub fn action_for(subject: &str) -> Option<Action> {
    let s = subject.to_ascii_lowercase();
    if let Some(rest) = s.split("bonus").nth(1) {
        if s.contains("ncrps") || s.contains("debenture") || s.contains("preference") {
            return None;
        }
        let ratio: String = rest
            .trim_start()
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == ':' || *c == ' ')
            .filter(|c| *c != ' ')
            .collect();
        let (a, b) = ratio.split_once(':')?;
        let (a, b): (f64, f64) = (a.parse().ok()?, b.parse().ok()?);
        return (a > 0.0 && b > 0.0).then(|| Action::Factor(b / (a + b)));
    }
    if s.contains("split") || s.contains("sub-division") || s.contains("consolidat") {
        let (from, to) = s.split_once("from")?.1.split_once(" to ")?;
        let (from, to) = (rupees(from)?, rupees(to)?);
        return (from > 0.0 && to > 0.0).then(|| Action::Factor(to / from));
    }
    s.contains("demerg").then_some(Action::Demerger)
}

/// The first number in text like ` rs 10/- per share`.
fn rupees(text: &str) -> Option<f64> {
    let start = text.find(|c: char| c.is_ascii_digit())?;
    let number: String = text[start..]
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    number.trim_end_matches('.').parse().ok()
}

/// A gap this large at the open cannot be an ordinary move.
const IMPOSSIBLE_GAP: (f64, f64) = (0.72, 1.45);

/// Builds one adjusted series from a symbol's rows, oldest first.
/// Returns the series and how many adjustments were applied.
pub fn adjust(
    symbol: &str,
    rows: &[(NaiveDate, BhavRow)],
    actions: &Actions,
) -> (EodSeries, usize) {
    let mut out = EodSeries {
        candles: Vec::with_capacity(rows.len()),
        delivery: Vec::with_capacity(rows.len()),
        main_series: rows.last().is_some_and(|(_, r)| r.main_series),
    };
    let mut applied = 0;
    for (date, row) in rows {
        if let Some(last) = out.candles.last().map(|c| c.close).filter(|c| *c > 0.0) {
            let observed = if row.open > 0.0 { row.open } else { row.close } / last;
            let nse_ratio = row.prev_close / last;
            let ratio = match actions.get(&(symbol.to_owned(), *date)) {
                // The listed ratio, when the price gap agrees with it.
                Some(Action::Factor(f)) if (0.7..=1.4).contains(&(observed / f)) => Some(*f),
                Some(Action::Demerger) if observed < 0.97 => Some(observed),
                // NSE already adjusted its own previous close (consolidations).
                _ if row.prev_close > 0.0 && (nse_ratio - 1.0).abs() > 0.005 => Some(nse_ratio),
                _ if observed < IMPOSSIBLE_GAP.0 || observed > IMPOSSIBLE_GAP.1 => {
                    tracing::debug!(symbol, %date, observed, "unlisted price gap treated as a corporate action");
                    Some(observed)
                }
                _ => None,
            };
            if let Some(ratio) = ratio.filter(|r| r.is_finite() && *r > 0.0) {
                for c in &mut out.candles {
                    c.open *= ratio;
                    c.high *= ratio;
                    c.low *= ratio;
                    c.close *= ratio;
                    c.volume /= ratio;
                }
                applied += 1;
            }
        }
        let ts = date
            .and_hms_opt(0, 0, 0)
            .map_or(0, |d| d.and_utc().timestamp());
        out.candles.push(Candle::new(
            ts, row.open, row.high, row.low, row.close, row.volume,
        ));
        out.delivery.push(row.delivery_pct);
    }
    (out, applied)
}

/// Reads the cached files into adjusted series for `wanted`, keeping the
/// latest `keep` sessions. Cached files whose inner date differs from their
/// name (holiday copies) are replaced by markers; their count is returned.
fn load(
    dir: &Path,
    wanted: &HashSet<String>,
    actions: &Actions,
    keep: usize,
) -> Result<(EodData, usize)> {
    let bhav = dir.join("bhav");
    let mut names: Vec<String> = match std::fs::read_dir(&bhav) {
        Ok(entries) => entries
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter_map(|n| n.strip_suffix(".csv").map(str::to_owned))
            .collect(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(e.into()),
    };
    names.sort();

    let mut removed = 0;
    let mut sessions: Vec<(NaiveDate, Vec<BhavRow>)> = Vec::new();
    for name in &names {
        let Ok(day) = NaiveDate::parse_from_str(name, "%Y-%m-%d") else {
            continue;
        };
        let path = bhav.join(format!("{name}.csv"));
        let text = std::fs::read_to_string(&path)?;
        if bhav_date(&text) != Some(day) {
            std::fs::remove_file(&path)?;
            std::fs::write(bhav.join(format!("{name}.none")), b"")?;
            removed += 1;
            continue;
        }
        sessions.push((day, parse_bhavcopy(&text, Some(wanted))));
    }
    if sessions.len() > keep {
        sessions.drain(..sessions.len() - keep);
    }

    let mut by_symbol: HashMap<String, Vec<(NaiveDate, BhavRow)>> = HashMap::new();
    for (day, rows) in &sessions {
        for row in rows {
            by_symbol
                .entry(row.symbol.clone())
                .or_default()
                .push((*day, row.clone()));
        }
    }
    let mut data = EodData {
        days: sessions.len(),
        last_day: sessions.last().map(|(d, _)| *d),
        ..EodData::default()
    };
    for (symbol, rows) in by_symbol {
        let (series, applied) = adjust(&symbol, &rows, actions);
        data.adjustments += applied;
        data.series.insert(symbol, series);
    }

    let first = sessions.first().map(|(d, _)| *d);
    let index = dir.join("index");
    let mut index_names: Vec<String> = std::fs::read_dir(&index)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok()?.file_name().into_string().ok())
                .filter(|n| n.ends_with(".csv"))
                .collect()
        })
        .unwrap_or_default();
    index_names.sort();
    for name in index_names {
        let Ok(text) = std::fs::read_to_string(index.join(&name)) else {
            continue;
        };
        for (index_name, day, close) in parse_index(&text) {
            if !INDICES.contains(&index_name.as_str()) || first.is_some_and(|f| day < f) {
                continue;
            }
            let ts = day
                .and_hms_opt(0, 0, 0)
                .map_or(0, |d| d.and_utc().timestamp());
            let series = data.indices.entry(index_name).or_default();
            // Holiday copies repeat the previous session; keep one row per day.
            if series.last().is_none_or(|(last, _)| *last < ts) {
                series.push((ts, close));
            }
        }
    }
    Ok((data, removed))
}

#[cfg(test)]
mod tests {
    use super::*;

    const BHAV: &str = "SYMBOL, SERIES, DATE1, PREV_CLOSE, OPEN_PRICE, HIGH_PRICE, LOW_PRICE, LAST_PRICE, CLOSE_PRICE, AVG_PRICE, TTL_TRD_QNTY, TURNOVER_LACS, NO_OF_TRADES, DELIV_QTY, DELIV_PER\n\
20MICRONS, EQ, 18-Nov-2026, 181.96, 179.86, 181.01, 177.20, 177.70, 177.98, 178.45, 47679, 85.09, 1920, 23085, 48.42\n\
21STCENMGM, BE, 18-Nov-2026, 50.16, 50.16, 51.00, 49.00, 50.50, 50.40, 50.30, 1200, 0.60, 12, -, -\n\
ACE, BL, 18-Nov-2026, 1166.65, 1170.00, 1170.00, 1170.00, 1170.00, 1170.00, 1170.00, 50000, 585.00, 1, 50000, 100.00\n\
ACE, EQ, 18-Nov-2026, 1166.65, 1170.00, 1185.00, 1160.10, 1180.00, 1179.50, 1175.00, 155303, 1824.81, 9000, 80000, 51.51\n\
GS2030, GS, 18-Nov-2026, 100, 100, 100, 100, 100, 100, 100, 10, 0.01, 1, 10, 100\n";

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn parses_bhavcopy_rows() {
        assert_eq!(bhav_date(BHAV), Some(day(2026, 11, 18)));
        assert_eq!(bhav_date("<html>not found</html>"), None);

        let rows = parse_bhavcopy(BHAV, None);
        let symbols: Vec<&str> = rows.iter().map(|r| r.symbol.as_str()).collect();
        assert_eq!(symbols, vec!["20MICRONS", "21STCENMGM", "ACE"]);
        let ace = &rows[2];
        assert!(
            ace.main_series,
            "the EQ row is kept, not the BL block-deal row"
        );
        assert_eq!(
            (ace.open, ace.close, ace.volume),
            (1170.0, 1179.5, 155_303.0)
        );
        assert_eq!(ace.delivery_pct, Some(51.51));
        assert!(!rows[1].main_series);
        assert_eq!(rows[1].delivery_pct, None);

        let wanted: HashSet<String> = ["ACE".to_owned()].into();
        assert_eq!(parse_bhavcopy(BHAV, Some(&wanted)).len(), 1);
    }

    #[test]
    fn builds_end_of_day_quotes() {
        let candles = vec![
            Candle::new(1_794_960_000, 100.0, 104.0, 99.0, 102.0, 1_000.0),
            Candle::new(1_795_046_400, 102.0, 110.0, 101.0, 108.0, 2_000.0),
        ];
        let name = "Action Construction Equipment Ltd.".to_owned();
        let q = eod_quote("ACE", Some(&name), &candles).unwrap();
        assert_eq!((q.price, q.change), (108.0, 6.0));
        assert!((q.change_pct - 6.0 / 102.0 * 100.0).abs() < 1e-9);
        assert_eq!(
            (q.day_high, q.day_low, q.high_52w, q.low_52w),
            (110.0, 101.0, 110.0, 99.0)
        );
        assert_eq!(q.name, name);
        assert_eq!(q.updated_at.as_deref(), Some("2026-11-19T15:30:00+05:30"));
        assert_eq!(eod_quote("ACE", None, &candles).unwrap().name, "ACE");
        assert!(eod_quote("ACE", None, &[]).is_none());
    }

    #[tokio::test]
    async fn eod_provider_never_claims_a_symbol_is_missing() {
        let dir = std::env::temp_dir().join(format!("swing-nse-prov-{}", std::process::id()));
        let provider = NseEodProvider::new(Arc::new(NseEod::new(&dir, 300).unwrap()));
        // Nothing loaded yet, another market, intraday: all fail over, none is NotFound.
        for result in [
            provider.candles("ACE", Interval::Day).await,
            provider.candles("US:AAPL", Interval::Day).await,
            provider.candles("ACE", Interval::Min15).await,
        ] {
            assert!(
                matches!(result, Err(Error::Unsupported { .. })),
                "{result:?}"
            );
        }
        assert!(matches!(
            provider.quotes(&["MCX:GOLD".to_owned()]).await,
            Err(Error::Unsupported { .. })
        ));
    }

    #[test]
    fn parses_index_files() {
        let text = "Index Name,Index Date,Open Index Value,High Index Value,Low Index Value,Closing Index Value,Points Change\n\
Nifty 50,18-11-2026,25323.15,25390.8,25141.8,25227.95,-159.45\n\
Nifty 500,18-11-2026,23765.6,23811.6,23568.85,23630.25,-192.05\n\
Broken,xx,1,2,3,4,5\n";
        let rows = parse_index(text);
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[0],
            ("Nifty 50".to_owned(), day(2026, 11, 18), 25_227.95)
        );
    }

    #[test]
    fn reads_corporate_action_subjects() {
        let f = |s: &str| action_for(s);
        assert_eq!(f("Bonus 1:1"), Some(Action::Factor(0.5)));
        assert_eq!(f("Bonus 1:2"), Some(Action::Factor(2.0 / 3.0)));
        assert_eq!(f("Bonus 10:1"), Some(Action::Factor(1.0 / 11.0)));
        assert_eq!(
            f("Face Value Split (Sub-Division) - From Rs 10/- Per Share To Re 1/- Per Share"),
            Some(Action::Factor(0.1))
        );
        assert_eq!(
            f("Face Value Split (Sub-Division) - From Rs 5/- Per Share To Rs 2/- Per Share"),
            Some(Action::Factor(0.4))
        );
        assert_eq!(f("Demerger"), Some(Action::Demerger));
        assert_eq!(f("Scheme Of Arrangement - Bonus Ncrps 4:1"), None);
        assert_eq!(f("Interim Dividend - Rs 24 Per Share"), None);
        assert_eq!(f("Rights 1:5 @ Premium Rs 90/-"), None);

        let json = serde_json::json!([
            {"symbol": "ANANDRATHI", "series": "EQ", "exDate": "03-Jun-2026", "subject": "Bonus 1:1"},
            {"symbol": "COLPAL", "series": "EQ", "exDate": "01-Jun-2026", "subject": "Interim Dividend - Rs 24 Per Share"},
            {"symbol": "XYZ", "series": "N1", "exDate": "01-Jun-2026", "subject": "Bonus 1:1"}
        ]);
        let rows = parse_actions(&json);
        assert_eq!(rows.len(), 1);
        let actions = to_actions(&rows);
        assert_eq!(
            actions.get(&("ANANDRATHI".to_owned(), day(2026, 6, 3))),
            Some(&Action::Factor(0.5))
        );
    }

    fn row(prev_close: f64, open: f64, close: f64, volume: f64) -> BhavRow {
        BhavRow {
            symbol: "X".to_owned(),
            main_series: true,
            prev_close,
            open,
            high: open.max(close) * 1.01,
            low: open.min(close) * 0.99,
            close,
            volume,
            delivery_pct: Some(40.0),
        }
    }

    #[test]
    fn adjusts_for_bonus_consolidation_and_demerger() {
        // Real shape of ANANDRATHI's 1:1 bonus: NSE's PREV_CLOSE is NOT adjusted.
        let rows = vec![
            (day(2026, 6, 2), row(3502.4, 3513.4, 3539.3, 881_607.0)),
            (day(2026, 6, 3), row(3539.3, 1779.8, 1785.2, 686_802.0)),
            (day(2026, 6, 4), row(1785.2, 1780.0, 1750.6, 289_842.0)),
        ];
        let mut actions = Actions::new();
        actions.insert(("X".to_owned(), day(2026, 6, 3)), Action::Factor(0.5));
        let (s, applied) = adjust("X", &rows, &actions);
        assert_eq!(applied, 1);
        assert!((s.candles[0].close - 1769.65).abs() < 1e-9);
        assert_eq!(s.candles[0].volume, 1_763_214.0);
        assert_eq!(s.candles[2].close, 1750.6);
        assert_eq!(s.candles[0].ts, 1_780_358_400); // 2026-06-02 00:00 UTC
        assert_eq!(s.delivery.len(), 3);

        // Without the listing the halving is still caught as an impossible gap.
        let (s, applied) = adjust("X", &rows, &Actions::new());
        assert_eq!(applied, 1);
        assert!((s.candles[0].close - 1779.8).abs() < 1e-6);

        // A wrong listed ratio (price did not move) is ignored.
        let flat = vec![
            (day(2026, 6, 2), row(100.0, 100.0, 100.0, 1_000.0)),
            (day(2026, 6, 3), row(100.0, 101.0, 102.0, 1_000.0)),
        ];
        let (s, applied) = adjust("X", &flat, &actions);
        assert_eq!((applied, s.candles[0].close), (0, 100.0));

        // Real shape of CCAVENUE's 10-for-1 consolidation: PREV_CLOSE is adjusted.
        let consolidation = vec![
            (day(2026, 10, 16), row(13.9, 13.9, 13.83, 5_000_000.0)),
            (day(2026, 10, 19), row(138.3, 135.65, 137.0, 400_000.0)),
        ];
        let (s, applied) = adjust("X", &consolidation, &Actions::new());
        assert_eq!(applied, 1);
        assert!((s.candles[0].close - 138.3).abs() < 1e-9);

        // A demerger uses the observed gap.
        let demerger = vec![
            (day(2026, 7, 21), row(400.0, 400.0, 400.0, 1_000.0)),
            (day(2026, 7, 22), row(400.0, 250.0, 255.0, 1_000.0)),
        ];
        let mut actions = Actions::new();
        actions.insert(("X".to_owned(), day(2026, 7, 22)), Action::Demerger);
        let (s, applied) = adjust("X", &demerger, &actions);
        assert_eq!(applied, 1);
        assert!((s.candles[0].close - 250.0).abs() < 1e-9);
    }

    #[test]
    fn loads_cached_sessions_and_drops_holiday_copies() {
        let dir = std::env::temp_dir().join(format!("swing-nse-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("bhav")).unwrap();
        std::fs::create_dir_all(dir.join("index")).unwrap();
        let session = |date: &str, close: f64| {
            format!(
                "SYMBOL, SERIES, DATE1, PREV_CLOSE, OPEN_PRICE, HIGH_PRICE, LOW_PRICE, LAST_PRICE, CLOSE_PRICE, AVG_PRICE, TTL_TRD_QNTY, TURNOVER_LACS, NO_OF_TRADES, DELIV_QTY, DELIV_PER\n\
ACE, EQ, {date}, {prev}, {close}, {close}, {close}, {close}, {close}, {close}, 1000, 1, 1, 500, 50.00\n\
OTHER, EQ, {date}, 10, 10, 10, 10, 10, 10, 10, 1000, 1, 1, 500, 50.00\n",
                prev = close - 1.0
            )
        };
        std::fs::write(
            dir.join("bhav/2026-11-17.csv"),
            session("17-Nov-2026", 100.0),
        )
        .unwrap();
        std::fs::write(
            dir.join("bhav/2026-11-18.csv"),
            session("18-Nov-2026", 101.0),
        )
        .unwrap();
        // A holiday: the archive served the 18th again under the 19th's name.
        std::fs::write(
            dir.join("bhav/2026-11-19.csv"),
            session("18-Nov-2026", 101.0),
        )
        .unwrap();
        std::fs::write(
            dir.join("index/2026-11-18.csv"),
            "Index Name,Index Date,Open Index Value,High Index Value,Low Index Value,Closing Index Value\nNifty 50,18-11-2026,1,2,3,25227.95\n",
        )
        .unwrap();

        let wanted: HashSet<String> = ["ACE".to_owned()].into();
        let (data, removed) = load(&dir, &wanted, &Actions::new(), 300).unwrap();
        assert_eq!(removed, 1);
        assert!(dir.join("bhav/2026-11-19.none").exists());
        assert!(!dir.join("bhav/2026-11-19.csv").exists());
        assert_eq!(data.days, 2);
        assert_eq!(data.last_day, Some(day(2026, 11, 18)));
        assert_eq!(data.series.len(), 1, "only wanted symbols are kept");
        let ace = &data.series["ACE"];
        assert_eq!(ace.candles.len(), 2);
        assert!(ace.main_series);
        assert_eq!(ace.delivery, vec![Some(50.0), Some(50.0)]);
        assert_eq!(data.index_closes("Nifty 50"), Some(vec![25_227.95]));

        // Keeping one session drops the oldest.
        let (data, _) = load(&dir, &wanted, &Actions::new(), 1).unwrap();
        assert_eq!((data.days, data.series["ACE"].candles.len()), (1, 1));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
