//! Pure swing-trading analytics.
//!
//! Everything here works on plain [`Candle`] slices and has no I/O, so the
//! same code runs in the API, in background scans and in back-tests.
//!
//! Entry point: [`analyze`].

pub mod candle;
pub mod indicators;
pub mod plan;
pub mod setup;
pub mod zones;

pub use candle::Candle;
pub use plan::{PlanParams, TradePlan};
pub use setup::{Preset, ScoreBreakdown, Setup, SetupState};
pub use zones::{Zone, ZoneKind};

/// Tunable inputs for a full analysis. Defaults mirror the design doc.
#[derive(Debug, Clone)]
pub struct AnalysisParams {
    /// Bars on each side a swing high/low must beat.
    pub pivot_window: usize,
    /// Zones closer than this many ATRs are merged.
    pub zone_merge_atr: f64,
    /// Plans with a lower risk-reward are flagged as not tradable.
    pub min_risk_reward: f64,
    pub plan: PlanParams,
}

impl Default for AnalysisParams {
    fn default() -> Self {
        Self {
            pivot_window: 5,
            zone_merge_atr: 0.5,
            min_risk_reward: 2.0,
            plan: PlanParams::default(),
        }
    }
}

/// Latest values of the indicators the UI shows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub close: f64,
    pub sma20: Option<f64>,
    pub sma50: Option<f64>,
    pub sma200: Option<f64>,
    pub rsi14: Option<f64>,
    pub atr14: Option<f64>,
    pub volume_avg20: Option<f64>,
    pub high_52w: f64,
    pub low_52w: f64,
}

/// Everything the stock page needs, computed from one candle series.
#[derive(Debug, Clone)]
pub struct Analysis {
    pub snapshot: Snapshot,
    pub sma20: Vec<Option<f64>>,
    pub sma50: Vec<Option<f64>>,
    pub sma200: Vec<Option<f64>>,
    pub zones: Vec<Zone>,
    /// Best matching setup, if any preset fires.
    pub setup: Option<Setup>,
}

/// Bars in roughly one trading year.
const BARS_52W: usize = 250;

/// Run the full pipeline on daily candles (oldest first).
///
/// Returns `None` when there is too little history to say anything useful.
pub fn analyze(candles: &[Candle], params: &AnalysisParams) -> Option<Analysis> {
    if candles.len() < 30 {
        return None;
    }
    let closes: Vec<f64> = candles.iter().map(|c| c.close).collect();
    let volumes: Vec<f64> = candles.iter().map(|c| c.volume).collect();

    let sma20 = indicators::sma(&closes, 20);
    let sma50 = indicators::sma(&closes, 50);
    let sma200 = indicators::sma(&closes, 200);
    let rsi14 = indicators::rsi(&closes, 14);
    let atr14 = indicators::atr(candles, 14);
    let vol20 = indicators::sma(&volumes, 20);

    let last = candles.len() - 1;
    let year = &candles[candles.len().saturating_sub(BARS_52W)..];
    let snapshot = Snapshot {
        close: closes[last],
        sma20: sma20[last],
        sma50: sma50[last],
        sma200: sma200[last],
        rsi14: rsi14[last],
        atr14: atr14[last],
        volume_avg20: vol20[last],
        high_52w: year.iter().map(|c| c.high).fold(f64::MIN, f64::max),
        low_52w: year.iter().map(|c| c.low).fold(f64::MAX, f64::min),
    };

    let zones = match snapshot.atr14 {
        Some(atr) => zones::find_zones(candles, params.pivot_window, atr * params.zone_merge_atr),
        None => Vec::new(),
    };

    let setup = setup::detect(candles, &snapshot, &rsi14, &zones, params);

    Some(Analysis {
        snapshot,
        sma20,
        sma50,
        sma200,
        zones,
        setup,
    })
}
