//! `GET /v1/swing`: the Swing Stocks screen (regime, sectors, gated setups).
//!
//! Reads the snapshot the background scan keeps in memory; never calls a
//! data provider. Response shapes are documented in `docs/api.md`.

use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::{DateTime, Duration, NaiveDate, Utc};
use market_data::Catalyst;
use serde::{Deserialize, Serialize};
use swing_core::screen::{Gates, SwingKind, SwingSetup};

use crate::dto::round2;
use crate::error::ApiError;
use crate::params;
use crate::scan::ScanPhase;
use crate::state::AppState;
use crate::swing::{self, SwingRow, SwingSnapshot};

const DEFAULT_LIMIT: usize = 100;
const MAX_LIMIT: usize = 500;
/// A plan is reviewed this long after its setup bar (the six-week time stop).
const REVIEW_DAYS: i64 = 42;

#[derive(Debug, Default, Deserialize)]
pub struct SwingQuery {
    pub setup: Option<String>,
    pub gates: Option<String>,
    pub limit: Option<String>,
    pub capital: Option<String>,
    pub risk: Option<String>,
}

/// Which list to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Setup(SwingKind),
    /// Leaders without regard to a setup, strongest first.
    Leaders,
}

/// Gates a row must pass. An unknown sector never excludes a stock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Required {
    tradable: bool,
    leader: bool,
    sector: bool,
}

impl Required {
    fn accepts(&self, g: &Gates) -> bool {
        (!self.tradable || g.tradable)
            && (!self.leader || g.leader)
            && (!self.sector || g.sector != Some(false))
    }
}

#[derive(Debug, Serialize)]
pub struct SwingResponse {
    /// `nse` (NSE end-of-day files) or a provider id; `null` before the first scan.
    pub source: Option<&'static str>,
    /// Date of the newest daily bar, `YYYY-MM-DD`.
    pub as_of: Option<String>,
    pub status: StatusDto,
    pub regime: Option<RegimeDto>,
    pub sectors: Vec<SectorDto>,
    pub counts: CountsDto,
    /// Rows in the chosen list before `limit`.
    pub total: usize,
    pub items: Vec<SwingItem>,
}

#[derive(Debug, Serialize)]
pub struct StatusDto {
    /// `pending` (nothing yet), `syncing` (downloading NSE files, nothing to show yet) or `ready`.
    pub state: &'static str,
    /// The background scan: `pending`, `running` or `idle`.
    pub scan: &'static str,
    pub universe_size: usize,
    /// Universe stocks without usable data in the last scan.
    pub failed: usize,
    pub excluded: Vec<String>,
    /// NSE end-of-day sessions cached, of the target; `null` when NSE files are not used.
    pub days_cached: Option<usize>,
    pub days_target: Option<usize>,
    pub error: Option<String>,
    /// Catalyst sources that could not be refreshed (their last saved copy is used).
    pub catalyst_errors: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct RegimeDto {
    /// `normal`, `caution` or `defensive`.
    pub state: &'static str,
    pub index: Option<&'static str>,
    pub index_close: Option<f64>,
    pub index_sma50: Option<f64>,
    pub index_sma200: Option<f64>,
    /// Share of the universe above its 50- and 200-day averages, in percent.
    pub breadth_50_pct: f64,
    pub breadth_200_pct: f64,
    pub universe: usize,
}

#[derive(Debug, Serialize)]
pub struct SectorDto {
    pub industry: String,
    pub members: usize,
    pub ret_3m_pct: f64,
    pub rank: usize,
    pub strong: bool,
}

/// Rows in each list under the requested gates.
#[derive(Debug, Default, Serialize)]
pub struct CountsDto {
    pub breakout: usize,
    pub pullback: usize,
    pub results_gap: usize,
    pub leaders: usize,
}

#[derive(Debug, Serialize)]
pub struct GatesDto {
    pub tradable: bool,
    pub leader: bool,
    pub sector: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct SwingItem {
    pub symbol: String,
    pub name: String,
    pub exchange: String,
    pub industry: Option<String>,
    /// Latest daily close.
    pub price: f64,
    pub change: f64,
    pub change_pct: f64,
    pub volume: u64,
    pub bar_time: i64,
    pub sparkline: Vec<f64>,
    /// Median 20-day turnover in crore rupees.
    pub turnover_cr: f64,
    /// Relative-strength rating, 0-100.
    pub rs: Option<u32>,
    pub ret_1m_pct: Option<f64>,
    pub ret_3m_pct: Option<f64>,
    pub ret_6m_pct: Option<f64>,
    pub below_high_pct: f64,
    pub delivery_pct: Option<f64>,
    pub delivery_avg20: Option<f64>,
    pub gates: GatesDto,
    pub setup: Option<SwingSetupDto>,
    /// Events from 7 days before to 14 days after the latest bar: risks first, then newest first.
    pub catalysts: Vec<CatalystDto>,
}

/// One dated event for a stock.
#[derive(Debug, Serialize)]
pub struct CatalystDto {
    /// e.g. `results`, `dividend`, `order_win`, `bulk_deal`, `surveillance`.
    pub kind: &'static str,
    /// `positive`, `negative`, `neutral` or `risk`.
    pub tone: &'static str,
    /// `YYYY-MM-DD`.
    pub date: String,
    /// Days from the reference date; negative is past.
    pub days: i64,
    pub title: String,
    pub detail: Option<String>,
    pub url: Option<String>,
}

impl CatalystDto {
    pub fn new(c: &Catalyst, reference: NaiveDate) -> Self {
        Self {
            kind: c.kind.as_str(),
            tone: c.tone.as_str(),
            date: c.date.format("%Y-%m-%d").to_string(),
            days: (c.date - reference).num_days(),
            title: c.title.clone(),
            detail: c.detail.clone(),
            url: c.url.clone(),
        }
    }
}

/// `GET /v1/stocks/{symbol}/events`.
#[derive(Debug, Serialize)]
pub struct EventsResponse {
    pub symbol: String,
    /// The date `days` is counted from (today in IST), `YYYY-MM-DD`.
    pub as_of: String,
    /// False when NSE catalysts are not in use (demo data, or `NSE_EOD=false`).
    pub available: bool,
    pub items: Vec<CatalystDto>,
}

#[derive(Debug, Serialize)]
pub struct SwingSetupDto {
    /// `breakout` or `pullback`.
    pub kind: &'static str,
    pub label: &'static str,
    pub score: u32,
    pub score_breakdown: ScoreDto,
    /// Breakout level, or the swing high a pullback came from.
    pub level: f64,
    pub extension_pct: f64,
    pub volume_ratio: f64,
    pub entry: f64,
    pub stop: f64,
    pub risk_per_share: f64,
    pub target_2r: f64,
    pub target_3r: f64,
    pub quantity: u64,
    /// Six weeks after the setup bar, `YYYY-MM-DD`.
    pub review_by: Option<String>,
    pub reasons: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ScoreDto {
    pub rs: f64,
    pub trend: f64,
    pub setup: f64,
    pub sector: f64,
    pub delivery: f64,
    pub quality: f64,
}

pub async fn list(
    State(state): State<AppState>,
    query: Result<Query<SwingQuery>, QueryRejection>,
) -> Result<Json<SwingResponse>, ApiError> {
    let Query(q) = query.map_err(|e| ApiError::BadRequest(e.body_text()))?;
    let tab = parse_tab(q.setup.as_deref())?;
    let required = parse_gates(q.gates.as_deref())?;
    let limit = parse_limit(q.limit.as_deref())?;
    let capital = params::parse_capital(q.capital.as_deref(), state.config.default_capital)?;
    let risk = params::parse_risk(q.risk.as_deref(), state.config.default_risk)?;

    let (scan, _) = state.scanner.snapshot().await;
    let snapshot = state.scanner.swing().await;
    let eod = match &state.eod {
        Some(eod) => Some(eod.status().await),
        None => None,
    };
    let syncing = eod.as_ref().is_some_and(|s| s.syncing);
    let status = StatusDto {
        state: match (&snapshot, syncing) {
            (Some(_), _) => "ready",
            (None, true) => "syncing",
            (None, false) => "pending",
        },
        scan: scan.phase.as_str(),
        universe_size: scan.universe_size,
        failed: if scan.phase == ScanPhase::Running {
            0
        } else {
            scan.failed.len()
        },
        excluded: scan.excluded.clone(),
        days_cached: eod.as_ref().map(|s| s.days_cached),
        days_target: eod.as_ref().map(|s| s.days_target),
        error: eod.and_then(|s| s.error),
        catalyst_errors: snapshot
            .as_ref()
            .map(|s| s.catalyst_errors.clone())
            .unwrap_or_default(),
    };

    let Some(snap) = snapshot else {
        return Ok(Json(SwingResponse {
            source: None,
            as_of: None,
            status,
            regime: None,
            sectors: Vec::new(),
            counts: CountsDto::default(),
            total: 0,
            items: Vec::new(),
        }));
    };

    let counts = CountsDto {
        breakout: select(&snap, Tab::Setup(SwingKind::Breakout), required).len(),
        pullback: select(&snap, Tab::Setup(SwingKind::Pullback), required).len(),
        results_gap: select(&snap, Tab::Setup(SwingKind::ResultsGap), required).len(),
        leaders: select(&snap, Tab::Leaders, required).len(),
    };
    let chosen = select(&snap, tab, required);
    let total = chosen.len();
    let items = chosen
        .into_iter()
        .take(limit)
        .map(|(row, setup)| item(row, setup, capital, risk))
        .collect();

    Ok(Json(SwingResponse {
        source: Some(snap.source),
        as_of: snap.bar_time.and_then(date),
        status,
        regime: Some(RegimeDto {
            state: snap.regime.state.as_str(),
            index: snap.index,
            index_close: snap.regime.index_close.map(round2),
            index_sma50: snap.regime.index_sma50.map(round2),
            index_sma200: snap.regime.index_sma200.map(round2),
            breadth_50_pct: round1(snap.breadth.above_sma50_pct),
            breadth_200_pct: round1(snap.breadth.above_sma200_pct),
            universe: snap.breadth.universe,
        }),
        sectors: snap
            .sectors
            .iter()
            .map(|s| SectorDto {
                industry: s.industry.clone(),
                members: s.members,
                ret_3m_pct: round1(s.ret_3m_pct),
                rank: s.rank,
                strong: s.strong,
            })
            .collect(),
        counts,
        total,
        items,
    }))
}

/// Recent and upcoming NSE events for one stock (results dates, corporate
/// actions, tagged filings, bulk and block deals, surveillance).
pub async fn events(
    State(state): State<AppState>,
    path: Result<Path<String>, PathRejection>,
) -> Result<Json<EventsResponse>, ApiError> {
    let Path(raw) = path.map_err(|e| ApiError::BadRequest(e.body_text()))?;
    let symbol = params::normalize_symbol(&raw)?;
    // Today in IST.
    let today = (Utc::now() + Duration::minutes(330)).date_naive();
    let items = match &state.events {
        Some(events) => swing::catalysts_around(&*events.data().await, &symbol, today)
            .iter()
            .map(|c| CatalystDto::new(c, today))
            .collect(),
        None => Vec::new(),
    };
    Ok(Json(EventsResponse {
        symbol,
        as_of: today.format("%Y-%m-%d").to_string(),
        available: state.events.is_some(),
        items,
    }))
}

/// Rows of a tab under the required gates, best first, each with the setup to show.
fn select(
    snap: &SwingSnapshot,
    tab: Tab,
    required: Required,
) -> Vec<(&SwingRow, Option<&SwingSetup>)> {
    let mut rows: Vec<(&SwingRow, Option<&SwingSetup>)> = snap
        .rows
        .iter()
        .filter(|r| required.accepts(&r.row.gates))
        .filter_map(|r| match tab {
            Tab::Setup(kind) => {
                let setup = r.row.setups.iter().find(|s| s.kind == kind)?;
                Some((r, Some(setup)))
            }
            Tab::Leaders => r.row.gates.leader.then(|| (r, r.row.setups.first())),
        })
        .collect();
    let rs = |r: &SwingRow| r.row.rs.unwrap_or(0.0);
    rows.sort_by(|(a, sa), (b, sb)| {
        let by_score = match tab {
            Tab::Setup(_) => {
                let score = |s: &Option<&SwingSetup>| s.map_or(0.0, |s| s.score.total());
                score(sb).total_cmp(&score(sa))
            }
            Tab::Leaders => std::cmp::Ordering::Equal,
        };
        by_score
            .then(rs(b).total_cmp(&rs(a)))
            .then_with(|| a.row.symbol.cmp(&b.row.symbol))
    });
    rows
}

fn item(r: &SwingRow, setup: Option<&SwingSetup>, capital: f64, risk: f64) -> SwingItem {
    let row = &r.row;
    let pct = |v: Option<f64>| v.map(round1);
    SwingItem {
        symbol: row.symbol.clone(),
        name: r.name.clone(),
        exchange: r.exchange.clone(),
        industry: row.industry.clone(),
        price: round2(row.close),
        change: round2(row.close - row.prev_close),
        change_pct: if row.prev_close > 0.0 {
            round2((row.close / row.prev_close - 1.0) * 100.0)
        } else {
            0.0
        },
        volume: row.volume.max(0.0).round() as u64,
        bar_time: row.bar_time,
        sparkline: r.sparkline.clone(),
        turnover_cr: round1(row.turnover / 1.0e7),
        rs: row.rs.map(|v| v.round().clamp(0.0, 100.0) as u32),
        ret_1m_pct: pct(row.ret_1m_pct),
        ret_3m_pct: pct(row.ret_3m_pct),
        ret_6m_pct: pct(row.ret_6m_pct),
        below_high_pct: round1(row.below_high_pct),
        delivery_pct: pct(row.delivery_pct),
        delivery_avg20: pct(row.delivery_avg20),
        gates: GatesDto {
            tradable: row.gates.tradable,
            leader: row.gates.leader,
            sector: row.gates.sector,
        },
        setup: setup.map(|s| setup_dto(s, row.bar_time, capital, risk)),
        catalysts: match swing::bar_date(row.bar_time) {
            Some(day) => r
                .catalysts
                .iter()
                .map(|c| CatalystDto::new(c, day))
                .collect(),
            None => Vec::new(),
        },
    }
}

fn setup_dto(s: &SwingSetup, bar_time: i64, capital: f64, risk: f64) -> SwingSetupDto {
    let p = &s.plan;
    // The scan sized plans with the server defaults; re-size for this request.
    let quantity = if p.risk_per_share > 0.0 {
        (capital * risk / p.risk_per_share).floor().max(0.0) as u64
    } else {
        0
    };
    SwingSetupDto {
        kind: s.kind.as_str(),
        label: s.kind.label(),
        score: s.score.total().clamp(0.0, 100.0) as u32,
        score_breakdown: ScoreDto {
            rs: round1(s.score.rs),
            trend: round1(s.score.trend),
            setup: round1(s.score.setup),
            sector: round1(s.score.sector),
            delivery: round1(s.score.delivery),
            quality: round1(s.score.quality),
        },
        level: round2(s.level),
        extension_pct: round1(s.extension_pct),
        volume_ratio: round2(s.volume_ratio),
        entry: round2(p.entry),
        stop: round2(p.stop),
        risk_per_share: round2(p.risk_per_share),
        target_2r: round2(p.target_2r),
        target_3r: round2(p.target_3r),
        quantity,
        review_by: date(bar_time + Duration::days(REVIEW_DAYS).num_seconds()),
        reasons: s.reasons.clone(),
    }
}

/// `YYYY-MM-DD` of a daily bar (bars are stamped 00:00 UTC of their session date).
fn date(ts: i64) -> Option<String> {
    DateTime::from_timestamp(ts, 0).map(|d| d.format("%Y-%m-%d").to_string())
}

fn round1(v: f64) -> f64 {
    if v.is_finite() {
        (v * 10.0).round() / 10.0
    } else {
        0.0
    }
}

fn parse_tab(raw: Option<&str>) -> Result<Tab, ApiError> {
    match raw.map(str::trim) {
        None | Some("") | Some("breakout") => Ok(Tab::Setup(SwingKind::Breakout)),
        Some("pullback") => Ok(Tab::Setup(SwingKind::Pullback)),
        Some("results_gap") => Ok(Tab::Setup(SwingKind::ResultsGap)),
        Some("leaders") => Ok(Tab::Leaders),
        Some(other) => Err(ApiError::BadRequest(format!(
            "unknown setup `{other}` (expected breakout, pullback, results_gap or leaders)"
        ))),
    }
}

/// `gates` is a comma-separated list of the gates to require; the default
/// is all three, and `none` requires nothing.
fn parse_gates(raw: Option<&str>) -> Result<Required, ApiError> {
    let Some(raw) = raw.map(str::trim).filter(|r| !r.is_empty()) else {
        return Ok(Required {
            tradable: true,
            leader: true,
            sector: true,
        });
    };
    let mut required = Required {
        tradable: false,
        leader: false,
        sector: false,
    };
    for gate in raw.split(',').map(str::trim).filter(|g| !g.is_empty()) {
        match gate {
            "tradable" => required.tradable = true,
            "leader" => required.leader = true,
            "sector" => required.sector = true,
            "none" => {}
            other => {
                return Err(ApiError::BadRequest(format!(
                    "unknown gate `{other}` (expected tradable, leader, sector or none)"
                )))
            }
        }
    }
    Ok(required)
}

fn parse_limit(raw: Option<&str>) -> Result<usize, ApiError> {
    let Some(raw) = raw.map(str::trim).filter(|r| !r.is_empty()) else {
        return Ok(DEFAULT_LIMIT);
    };
    match raw.parse::<usize>() {
        Ok(n) if (1..=MAX_LIMIT).contains(&n) => Ok(n),
        _ => Err(ApiError::BadRequest(format!(
            "`limit` must be a whole number from 1 to {MAX_LIMIT}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tabs_and_gates() {
        assert_eq!(parse_tab(None).unwrap(), Tab::Setup(SwingKind::Breakout));
        assert_eq!(
            parse_tab(Some("pullback")).unwrap(),
            Tab::Setup(SwingKind::Pullback)
        );
        assert_eq!(parse_tab(Some("leaders")).unwrap(), Tab::Leaders);
        assert_eq!(
            parse_tab(Some("results_gap")).unwrap(),
            Tab::Setup(SwingKind::ResultsGap)
        );
        assert!(parse_tab(Some("oversold")).is_err());

        let all = parse_gates(None).unwrap();
        assert!(all.tradable && all.leader && all.sector);
        let some = parse_gates(Some("tradable, sector")).unwrap();
        assert!(some.tradable && !some.leader && some.sector);
        let none = parse_gates(Some("none")).unwrap();
        assert!(!none.tradable && !none.leader && !none.sector);
        assert!(parse_gates(Some("cheap")).is_err());

        let gates = |tradable, leader, sector| Gates {
            tradable,
            leader,
            sector,
        };
        assert!(all.accepts(&gates(true, true, Some(true))));
        assert!(
            all.accepts(&gates(true, true, None)),
            "unknown sector never excludes"
        );
        assert!(!all.accepts(&gates(true, true, Some(false))));
        assert!(!all.accepts(&gates(true, false, Some(true))));
        assert!(none.accepts(&gates(false, false, Some(false))));
    }

    #[test]
    fn formats_dates_and_rounding() {
        assert_eq!(date(1_794_182_400).as_deref(), Some("2026-11-09"));
        assert_eq!(round1(12.345), 12.3);
        assert_eq!(round1(f64::NAN), 0.0);
        assert_eq!(parse_limit(None).unwrap(), 100);
        assert!(parse_limit(Some("0")).is_err());
    }
}
