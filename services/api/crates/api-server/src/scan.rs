//! Background scanner: walks the universe on daily candles and keeps the
//! stocks with a firing setup in memory for `GET /v1/scan`.
//!
//! - Reads only the primary provider (the first in `DATA_PROVIDERS`). A scan
//!   never falls back to demo, so ranked lists never mix in synthetic prices.
//! - Daily candles come from the [`CandleStore`] when fresh for the current
//!   session; network fetches are paced at `SCAN_SYMBOLS_PER_HOUR`, leaving
//!   the rest of the provider's hourly budget for page requests. When the
//!   provider is rate limited the scan waits and retries the same stock.
//! - Results of the last completed scan stay readable while the next runs.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use market_data::{Interval, MarketData};
use swing_core::{AnalysisParams, Candle, Setup};
use tokio::sync::RwLock;
use tokio::time::Instant;

use crate::dto;
use crate::params;
use crate::state::AppState;
use crate::store::{session_key, CandleStore};
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

    async fn update(&self, f: impl FnOnce(&mut ScanStatus)) {
        f(&mut self.inner.write().await.status);
    }
}

/// Scans now, then every `SCAN_INTERVAL_MINS` (once only when it is 0).
pub async fn run_forever(state: AppState, universe: Universe) {
    let every = state.config.scan_interval_mins;
    loop {
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
    let fetcher = Fetcher {
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
                // Keep showing when the previous scan finished.
                finished_at: s.finished_at,
                ..ScanStatus::default()
            };
        })
        .await;
    tracing::info!(
        provider = provider.id(),
        symbols = universe.entries.len(),
        "scan started"
    );

    let (mut rows, failed) = scan_entries(fetcher, &universe.entries, &params, scanner).await;
    fill_names(provider.as_ref(), &mut rows).await;
    rows.sort_by(|a, b| best_score(b).total_cmp(&best_score(a)));

    let finished = Utc::now();
    tracing::info!(
        hits = rows.len(),
        failed = failed.len(),
        secs = (finished - started).num_seconds(),
        "scan finished"
    );
    let mut inner = scanner.inner.write().await;
    inner.rows = Arc::new(rows);
    inner.status.phase = ScanPhase::Idle;
    inner.status.failed = failed;
    inner.status.finished_at = Some(finished);
}

async fn scan_entries(
    mut fetcher: Fetcher,
    entries: &[UniverseEntry],
    params: &AnalysisParams,
    scanner: &Scanner,
) -> (Vec<ScanRow>, Vec<String>) {
    let mut rows = Vec::new();
    let mut failed = Vec::new();
    for (i, entry) in entries.iter().enumerate() {
        match fetcher.daily(&entry.symbol).await {
            Ok(candles) => {
                if let Some(row) = build_row(entry, &candles, params) {
                    rows.push(row);
                    publish_partial(scanner, &rows).await;
                }
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
    (rows, failed)
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

/// Daily candles from the store, or from the provider at the scan's pace.
struct Fetcher {
    provider: Arc<dyn MarketData>,
    store: Option<CandleStore>,
    session: String,
    pace: Option<Duration>,
    last_fetch: Option<Instant>,
}

impl Fetcher {
    async fn daily(&mut self, symbol: &str) -> market_data::Result<Vec<Candle>> {
        let id = self.provider.id();
        if let Some(store) = &self.store {
            if let Some(candles) = store.load(id, symbol, &self.session).await {
                return Ok(candles);
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
        Ok(candles)
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

/// Company names for rows whose universe entry had none (one quote batch).
async fn fill_names(provider: &dyn MarketData, rows: &mut [ScanRow]) {
    let missing: Vec<String> = rows
        .iter()
        .filter(|r| r.name == r.symbol)
        .map(|r| r.symbol.clone())
        .collect();
    if missing.is_empty() {
        return;
    }
    match provider.quotes(&missing).await {
        Ok(quotes) => {
            for row in rows.iter_mut() {
                if let Some(q) = quotes.iter().find(|q| q.symbol == row.symbol) {
                    row.name = q.name.clone();
                    row.exchange = q.exchange.clone();
                }
            }
        }
        Err(e) => tracing::warn!(error = %e, "scan: could not fetch company names"),
    }
}

fn best_score(row: &ScanRow) -> f64 {
    row.setups.first().map_or(0.0, |s| s.score.total())
}
