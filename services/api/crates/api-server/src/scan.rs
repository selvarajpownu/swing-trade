//! Background scanner: walks the universe on daily candles and keeps the
//! stocks with a firing setup in memory for `GET /v1/scan`.
//!
//! - Reads only the primary provider (the first in `DATA_PROVIDERS`). A scan
//!   never falls back to demo, so ranked lists never mix in synthetic prices.
//! - Daily candles come from the [`CandleStore`] when fresh for the current
//!   session; network fetches are paced at `SCAN_SYMBOLS_PER_HOUR`, leaving
//!   the rest of the provider's hourly budget for page requests. When the
//!   provider is rate limited the scan waits and retries the same stock.
//! - With `NSE_EOD` (the default for real data) daily candles come from
//!   NSE's end-of-day files instead: the whole universe loads in seconds
//!   and no provider budget is spent. The response then says `nse`.
//! - Each scan also builds the cross-sectional swing screen (`swing.rs`).
//! - Results of the last completed scan stay readable while the next runs.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use market_data::{EodData, Interval, MarketData};
use swing_core::{AnalysisParams, Candle, Setup};
use tokio::sync::RwLock;
use tokio::time::Instant;

use crate::dto;
use crate::params;
use crate::state::AppState;
use crate::store::{session_key, CandleStore};
use crate::swing::{self, SwingSnapshot};
use crate::universe::{Universe, UniverseEntry};

/// Where the scanner is in its cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScanPhase {
    /// No scan has finished yet.
    #[default]
    Pending,
    Running,
    Idle,
}

impl ScanPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            ScanPhase::Pending => "pending",
            ScanPhase::Running => "running",
            ScanPhase::Idle => "idle",
        }
    }
}

/// Progress and timing of the current or last scan.
#[derive(Debug, Clone, Default)]
pub struct ScanStatus {
    pub phase: ScanPhase,
    pub universe_size: usize,
    pub scanned: usize,
    pub failed: Vec<String>,
    pub excluded: Vec<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub next_run_at: Option<DateTime<Utc>>,
    /// Where the candles came from: `nse` or a provider id. Empty before the first scan.
    pub source: &'static str,
}

/// One stock with at least one firing setup.
#[derive(Debug, Clone)]
pub struct ScanRow {
    pub symbol: String,
    pub name: String,
    pub industry: Option<String>,
    pub exchange: String,
    /// Latest daily close.
    pub price: f64,
    pub change: f64,
    pub change_pct: f64,
    pub high_52w: f64,
    pub low_52w: f64,
    pub volume: f64,
    /// Unix seconds of the latest daily bar.
    pub bar_time: i64,
    pub sparkline: Vec<f64>,
    /// Every firing preset, best score first. Never empty.
    pub setups: Vec<Setup>,
}

#[derive(Debug, Default)]
struct Inner {
    status: ScanStatus,
    /// Rows of the last completed scan.
    rows: Arc<Vec<ScanRow>>,
    /// Swing screen of the last completed scan.
    swing: Option<Arc<SwingSnapshot>>,
}

/// Shared scan results; cheap to read from request handlers.
#[derive(Debug)]
pub struct Scanner {
    provider: &'static str,
    inner: RwLock<Inner>,
}

impl Scanner {
    /// A scanner reading from the provider with id `provider`.
    pub fn new(provider: &'static str) -> Self {
        Self {
            provider,
            inner: RwLock::new(Inner::default()),
        }
    }

    /// The provider scans read from.
    pub fn provider(&self) -> &'static str {
        self.provider
    }

    /// Current status and the rows of the last completed scan.
    pub async fn snapshot(&self) -> (ScanStatus, Arc<Vec<ScanRow>>) {
        let inner = self.inner.read().await;
        (inner.status.clone(), inner.rows.clone())
    }

    /// The swing screen of the last completed scan, if any.
    pub async fn swing(&self) -> Option<Arc<SwingSnapshot>> {
        self.inner.read().await.swing.clone()
    }

    async fn update(&self, f: impl FnOnce(&mut ScanStatus)) {
        f(&mut self.inner.write().await.status);
    }
}

/// Scans now, then every `SCAN_INTERVAL_MINS` (once only when it is 0).
pub async fn run_forever(state: AppState, universe: Universe) {
    let every = state.config.scan_interval_mins;
    let symbols: HashSet<String> = universe.entries.iter().map(|e| e.symbol.clone()).collect();
    if let Some(eod) = &state.eod {
        // Company names for end-of-day quotes (NSE's price files carry none).
        let names = universe
            .entries
            .iter()
            .filter_map(|e| Some((e.symbol.clone(), e.name.clone()?)))
            .collect();
        eod.set_names(names).await;
    }
    loop {
        if let Some(eod) = &state.eod {
            match eod.sync(&symbols).await {
                Ok(data) => tracing::info!(
                    days = data.days,
                    last_day = ?data.last_day,
                    adjustments = data.adjustments,
                    "NSE end-of-day data ready"
                ),
                Err(e) => tracing::warn!(error = %e, "NSE end-of-day sync failed"),
            }
        }
        if let Some(events) = &state.events {
            let data = events.refresh().await;
            tracing::info!(
                symbols = data.catalysts.len(),
                results_dates = data.results.len(),
                surveillance = data.surveillance.len(),
                "NSE catalysts refreshed"
            );
            for problem in &data.errors {
                tracing::warn!(%problem, "NSE catalyst source not refreshed");
            }
        }
        run_once(&state, &universe).await;
        if every == 0 {
            return;
        }
        let next = Utc::now() + chrono::Duration::minutes(i64::try_from(every).unwrap_or(60));
        state.scanner.update(|s| s.next_run_at = Some(next)).await;
        tokio::time::sleep(Duration::from_secs(every.saturating_mul(60))).await;
    }
}

/// Runs one full scan and publishes its rows.
pub async fn run_once(state: &AppState, universe: &Universe) {
    let scanner = &state.scanner;
    let Some(provider) = state.market.get(scanner.provider()) else {
        tracing::error!(
            provider = scanner.provider(),
            "scan provider is not configured"
        );
        return;
    };
    // Demo data is generated locally: no pacing, nothing worth storing.
    let networked = provider.id() != "demo";
    let pace = networked.then(|| {
        Duration::from_secs_f64(3_600.0 / f64::from(state.config.scan_symbols_per_hour.max(1)))
    });
    // NSE files are used once at least a few months of sessions are cached.
    let eod = match &state.eod {
        Some(eod) => Some(eod.data().await).filter(|d| d.days >= MIN_EOD_DAYS),
        None => None,
    };
    let source = if eod.is_some() {
        market_data::nse::ID
    } else {
        provider.id()
    };
    let index_closes = eod.as_ref().and_then(|d| d.index_closes(swing::INDEX));
    let events = match &state.events {
        Some(events) => Some(events.data().await),
        None => None,
    };
    let fetcher = Fetcher {
        eod,
        provider: provider.clone(),
        store: networked.then(|| CandleStore::new(&state.config.candle_store_dir)),
        session: session_key(Utc::now()),
        pace,
        last_fetch: None,
    };
    let params = params::analysis_params(state.config.default_capital, state.config.default_risk);

    let started = Utc::now();
    scanner
        .update(|s| {
            *s = ScanStatus {
                phase: ScanPhase::Running,
                universe_size: universe.entries.len(),
                excluded: universe.excluded.clone(),
                started_at: Some(started),
                source,
                // Keep showing when the previous scan finished.
                finished_at: s.finished_at,
                ..ScanStatus::default()
            };
        })
        .await;
    tracing::info!(source, symbols = universe.entries.len(), "scan started");

    let (mut rows, failed, collected) =
        scan_entries(fetcher, &universe.entries, &params, scanner).await;
    let names = resolve_names(provider.as_ref(), &universe.entries, &rows).await;
    for row in &mut rows {
        if let Some((name, exchange)) = names.get(&row.symbol) {
            row.name = name.clone();
            row.exchange = exchange.clone();
        }
    }
    rows.sort_by(|a, b| best_score(b).total_cmp(&best_score(a)));
    let screen_params = swing::screen_params(&state.config);
    let snapshot = swing::build(
        source,
        &universe.entries,
        &collected,
        &names,
        index_closes.as_deref(),
        events.as_deref(),
        &screen_params,
    );

    let finished = Utc::now();
    tracing::info!(
        hits = rows.len(),
        failed = failed.len(),
        secs = (finished - started).num_seconds(),
        "scan finished"
    );
    let mut inner = scanner.inner.write().await;
    inner.rows = Arc::new(rows);
    inner.swing = Some(Arc::new(snapshot));
    inner.status.phase = ScanPhase::Idle;
    inner.status.failed = failed;
    inner.status.finished_at = Some(finished);
}

async fn scan_entries(
    mut fetcher: Fetcher,
    entries: &[UniverseEntry],
    params: &AnalysisParams,
    scanner: &Scanner,
) -> (Vec<ScanRow>, Vec<String>, Vec<Daily>) {
    let mut rows = Vec::new();
    let mut failed = Vec::new();
    let mut collected = Vec::with_capacity(entries.len());
    for (i, entry) in entries.iter().enumerate() {
        match fetcher.daily(&entry.symbol).await {
            Ok(daily) => {
                if let Some(row) = build_row(entry, &daily.candles, params) {
                    rows.push(row);
                    publish_partial(scanner, &rows).await;
                }
                collected.push(daily);
            }
            Err(e) => {
                tracing::warn!(symbol = %entry.symbol, error = %e, "scan: no candles");
                failed.push(entry.symbol.clone());
            }
        }
        let failed_now = failed.clone();
        scanner
            .update(|s| {
                s.scanned = i + 1;
                s.failed = failed_now;
            })
            .await;
    }
    (rows, failed, collected)
}

/// During the very first scan there are no earlier results to show (a full
/// Nifty 500 run takes hours), so hits are published as they are found.
/// Later scans keep showing the previous complete results until they finish.
async fn publish_partial(scanner: &Scanner, rows: &[ScanRow]) {
    let mut inner = scanner.inner.write().await;
    if inner.status.finished_at.is_none() {
        let mut sorted = rows.to_vec();
        sorted.sort_by(|a, b| best_score(b).total_cmp(&best_score(a)));
        inner.rows = Arc::new(sorted);
    }
}

/// Times a scan retries one stock after the provider reports a rate limit.
const RATE_LIMIT_ATTEMPTS: u32 = 3;
/// Longest single wait for the provider's budget during a scan.
const MAX_RATE_LIMIT_WAIT: Duration = Duration::from_secs(20 * 60);

/// NSE end-of-day data is used for scans once this many sessions are cached.
const MIN_EOD_DAYS: usize = 60;

/// One stock's daily history as collected by a scan.
#[derive(Debug, Clone)]
pub struct Daily {
    pub symbol: String,
    pub candles: Vec<Candle>,
    /// Delivery percentage per bar; empty when the source has none.
    pub delivery: Vec<Option<f64>>,
    /// False for trade-to-trade series.
    pub main_series: bool,
}

/// Daily candles from NSE's files, the store, or the provider at the scan's pace.
struct Fetcher {
    eod: Option<Arc<EodData>>,
    provider: Arc<dyn MarketData>,
    store: Option<CandleStore>,
    session: String,
    pace: Option<Duration>,
    last_fetch: Option<Instant>,
}

impl Fetcher {
    async fn daily(&mut self, symbol: &str) -> market_data::Result<Daily> {
        if let Some(eod) = &self.eod {
            // A symbol missing from NSE's files is not listed there (or is
            // suspended); asking the provider instead would only be slow.
            return match eod.series.get(symbol) {
                Some(s) => Ok(Daily {
                    symbol: symbol.to_owned(),
                    candles: s.candles.clone(),
                    delivery: s.delivery.clone(),
                    main_series: s.main_series,
                }),
                None => Err(market_data::Error::NotFound(symbol.to_owned())),
            };
        }
        let plain = |candles: Vec<Candle>| Daily {
            symbol: symbol.to_owned(),
            candles,
            delivery: Vec::new(),
            main_series: true,
        };
        let id = self.provider.id();
        if let Some(store) = &self.store {
            if let Some(candles) = store.load(id, symbol, &self.session).await {
                return Ok(plain(candles));
            }
        }
        if let (Some(pace), Some(last)) = (self.pace, self.last_fetch) {
            tokio::time::sleep_until(last + pace).await;
        }
        self.last_fetch = Some(Instant::now());
        let mut attempts = 0;
        let candles = loop {
            match self.provider.candles(symbol, Interval::Day).await {
                Err(market_data::Error::RateLimited { retry_after, .. })
                    if attempts < RATE_LIMIT_ATTEMPTS =>
                {
                    attempts += 1;
                    let wait = (retry_after + Duration::from_secs(1)).min(MAX_RATE_LIMIT_WAIT);
                    tracing::info!(%symbol, secs = wait.as_secs(), "scan: provider rate limited; waiting");
                    tokio::time::sleep(wait).await;
                }
                other => break other?,
            }
        };
        if let Some(store) = &self.store {
            if !candles.is_empty() {
                if let Err(e) = store.save(id, symbol, &self.session, &candles).await {
                    tracing::warn!(%symbol, error = %e, "could not save candles");
                }
            }
        }
        Ok(plain(candles))
    }
}

/// A row when at least one preset fires on the latest bar.
fn build_row(
    entry: &UniverseEntry,
    candles: &[Candle],
    params: &AnalysisParams,
) -> Option<ScanRow> {
    let analysis = swing_core::analyze(candles, params)?;
    if analysis.setups.is_empty() {
        return None;
    }
    let last = candles.last()?;
    let prev_close = candles
        .len()
        .checked_sub(2)
        .map(|i| candles[i].close)
        .filter(|p| *p > 0.0);
    let change = prev_close.map_or(0.0, |p| last.close - p);
    let change_pct = prev_close.map_or(0.0, |p| change / p * 100.0);
    Some(ScanRow {
        symbol: entry.symbol.clone(),
        name: entry.name.clone().unwrap_or_else(|| entry.symbol.clone()),
        industry: entry.industry.clone(),
        exchange: "NSE".to_owned(),
        price: last.close,
        change,
        change_pct,
        high_52w: analysis.snapshot.high_52w,
        low_52w: analysis.snapshot.low_52w,
        volume: last.volume,
        bar_time: last.ts,
        sparkline: dto::sparkline(candles),
        setups: analysis.setups,
    })
}

/// Most unnamed symbols looked up in one go (quotes come in batches of 10).
const MAX_NAME_LOOKUPS: usize = 40;

/// `symbol → (company name, exchange)`: from the universe file where it has
/// names, otherwise from one batch of provider quotes. When too many
/// symbols lack a name, only those with a scan hit are looked up.
async fn resolve_names(
    provider: &dyn MarketData,
    entries: &[UniverseEntry],
    rows: &[ScanRow],
) -> HashMap<String, (String, String)> {
    let mut names: HashMap<String, (String, String)> = entries
        .iter()
        .filter_map(|e| Some((e.symbol.clone(), (e.name.clone()?, "NSE".to_owned()))))
        .collect();
    let mut missing: Vec<String> = entries
        .iter()
        .filter(|e| e.name.is_none())
        .map(|e| e.symbol.clone())
        .collect();
    if missing.len() > MAX_NAME_LOOKUPS {
        missing.retain(|s| rows.iter().any(|r| &r.symbol == s));
        missing.truncate(MAX_NAME_LOOKUPS);
    }
    if missing.is_empty() {
        return names;
    }
    match provider.quotes(&missing).await {
        Ok(quotes) => {
            for q in quotes {
                names.insert(q.symbol, (q.name, q.exchange));
            }
        }
        Err(e) => tracing::warn!(error = %e, "scan: could not fetch company names"),
    }
    names
}

fn best_score(row: &ScanRow) -> f64 {
    row.setups.first().map_or(0.0, |s| s.score.total())
}
