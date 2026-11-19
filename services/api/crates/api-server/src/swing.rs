//! Builds the "Swing Stocks" snapshot from the candles one scan collected.
//!
//! The scan already holds every universe stock's daily history, so the
//! cross-sectional screen (`swing_core::screen`) costs no extra data calls.

use std::collections::HashMap;

use swing_core::screen::{
    self, Breadth, Regime, ScreenInput, ScreenParams, ScreenRow, SectorStrength,
};

use crate::config::Config;
use crate::dto;
use crate::scan::Daily;
use crate::universe::UniverseEntry;

/// Benchmark index for the market regime (from NSE's daily index file).
pub const INDEX: &str = "Nifty 50";
/// A stock whose latest bar is older than this against the newest bar in
/// the universe is left out (suspended, or not traded lately).
const MAX_STALE_SECS: i64 = 5 * 86_400;

/// One screened stock with what the page needs to show it.
#[derive(Debug, Clone)]
pub struct SwingRow {
    pub row: ScreenRow,
    pub name: String,
    pub exchange: String,
    pub sparkline: Vec<f64>,
}

/// The swing screen of one completed scan.
#[derive(Debug, Clone)]
pub struct SwingSnapshot {
    /// `nse` or a provider id.
    pub source: &'static str,
    /// Unix seconds of the newest daily bar.
    pub bar_time: Option<i64>,
    /// Benchmark index name, when index closes were available.
    pub index: Option<&'static str>,
    pub regime: Regime,
    pub breadth: Breadth,
    pub sectors: Vec<SectorStrength>,
    pub rows: Vec<SwingRow>,
    /// Stocks left out because their data was stale.
    pub stale: usize,
}

/// Screen thresholds from the server configuration.
pub fn screen_params(config: &Config) -> ScreenParams {
    ScreenParams {
        min_turnover: config.swing_min_turnover_cr * 1.0e7,
        capital: config.default_capital,
        risk_per_trade: config.default_risk,
        ..ScreenParams::default()
    }
}

/// Runs the screen over everything a scan collected.
pub fn build(
    source: &'static str,
    entries: &[UniverseEntry],
    collected: &[Daily],
    names: &HashMap<String, (String, String)>,
    index_closes: Option<&[f64]>,
    params: &ScreenParams,
) -> SwingSnapshot {
    let industry: HashMap<&str, Option<&str>> = entries
        .iter()
        .map(|e| (e.symbol.as_str(), e.industry.as_deref()))
        .collect();
    let last_ts = |d: &Daily| d.candles.last().map(|c| c.ts);
    let bar_time = collected.iter().filter_map(last_ts).max();
    let fresh = |d: &&Daily| match (last_ts(d), bar_time) {
        (Some(ts), Some(newest)) => newest - ts <= MAX_STALE_SECS,
        _ => false,
    };

    let inputs: Vec<ScreenInput<'_>> = collected
        .iter()
        .filter(fresh)
        .map(|d| ScreenInput {
            symbol: &d.symbol,
            industry: industry.get(d.symbol.as_str()).copied().flatten(),
            candles: &d.candles,
            delivery: &d.delivery,
            main_series: d.main_series,
        })
        .collect();
    let stale = collected.len() - inputs.len();
    let screened = screen::run(&inputs, params);
    let regime = screen::regime(index_closes, &screened.breadth);

    let candles: HashMap<&str, &Daily> = collected.iter().map(|d| (d.symbol.as_str(), d)).collect();
    let rows = screened
        .rows
        .into_iter()
        .map(|row| {
            let (name, exchange) = names
                .get(&row.symbol)
                .cloned()
                .unwrap_or_else(|| (row.symbol.clone(), "NSE".to_owned()));
            let sparkline = candles
                .get(row.symbol.as_str())
                .map(|d| dto::sparkline(&d.candles))
                .unwrap_or_default();
            SwingRow {
                row,
                name,
                exchange,
                sparkline,
            }
        })
        .collect();

    SwingSnapshot {
        source,
        bar_time,
        index: index_closes.map(|_| INDEX),
        regime,
        breadth: screened.breadth,
        sectors: screened.sectors,
        rows,
        stale,
    }
}
