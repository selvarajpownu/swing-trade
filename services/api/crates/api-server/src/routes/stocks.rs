//! `GET /v1/stocks`, `GET /v1/stocks/{symbol}` and `GET /v1/stocks/{symbol}/news`.
//!
//! Every route takes `market` (`in` default, `us`, `mcx`). Providers see
//! market-qualified keys (`US:AAPL`); responses carry the bare symbol.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::Json;
use futures::stream::{self, StreamExt};
use market_data::{Interval, Market};
use serde::Deserialize;
use swing_core::AnalysisParams;

use crate::dto::{
    self, round2, NewsResponse, SetupSummary, StockDetailResponse, StockItem, StocksResponse,
};
use crate::error::ApiError;
use crate::params;
use crate::state::AppState;

/// Concurrent candle fetches for the list.
const CANDLE_CONCURRENCY: usize = 6;
/// Bars in roughly one trading year.
const BARS_52W: usize = 250;

/// Query for the list. Values are parsed by hand for contract-shaped errors.
#[derive(Debug, Default, Deserialize)]
pub struct ListQuery {
    pub market: Option<String>,
    pub symbols: Option<String>,
    pub capital: Option<String>,
    pub risk: Option<String>,
}

/// Query for one stock.
#[derive(Debug, Default, Deserialize)]
pub struct DetailQuery {
    pub market: Option<String>,
    pub interval: Option<String>,
    pub capital: Option<String>,
    pub risk: Option<String>,
}

/// Query for one stock's news.
#[derive(Debug, Default, Deserialize)]
pub struct NewsQuery {
    pub market: Option<String>,
}

/// What the list derives from each stock's daily candles.
#[derive(Debug, Default)]
struct Derived {
    sparkline: Vec<f64>,
    setup: Option<SetupSummary>,
    /// 52-week (high, low) from candles, used where quotes lack it (MCX).
    range: Option<(f64, f64)>,
}

/// Quotes, sparklines and setups for a list of symbols.
///
/// One symbol failing (e.g. its candles) only drops its sparkline and
/// setup; the rest of the list is still returned.
pub async fn list(
    State(state): State<AppState>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<StocksResponse>, ApiError> {
    let Query(q) = query.map_err(|e| ApiError::BadRequest(e.body_text()))?;
    let market = params::parse_market(q.market.as_deref())?;
    let symbols = match q.symbols.as_deref().map(str::trim) {
        Some(s) if !s.is_empty() => params::parse_symbols(s)?,
        _ => state.config.watchlist_for(market),
    };
    let capital = params::parse_capital(
        q.capital.as_deref(),
        state.config.default_capital_for(market),
    )?;
    let risk = params::parse_risk(q.risk.as_deref(), state.config.default_risk)?;
    let analysis_params = params::analysis_params(capital, risk);

    let keys: Vec<String> = symbols.iter().map(|s| market.key(s)).collect();
    let (quotes, source) = state.quotes(&keys).await?;

    // Owned futures (no borrows) keep the handler future `Send` without
    // tripping higher-ranked lifetime inference.
    let jobs: Vec<_> = quotes
        .iter()
        .map(|quote| {
            let state = state.clone();
            let params = analysis_params.clone();
            let key = quote.symbol.clone();
            async move {
                let derived = match state.candles_from(source, &key, Interval::Day).await {
                    Ok(candles) => summarise(&candles, &params),
                    Err(e) => {
                        tracing::warn!(symbol = %key, provider = source, error = %e, "no candles; skipping setup");
                        Derived::default()
                    }
                };
                (key, derived)
            }
        })
        .collect();
    let mut derived: HashMap<String, Derived> = stream::iter(jobs)
        .buffer_unordered(CANDLE_CONCURRENCY)
        .collect()
        .await;

    let items = quotes
        .iter()
        .map(|quote| {
            let d = derived.remove(&quote.symbol).unwrap_or_default();
            let mut item = StockItem::new(quote, d.sparkline, d.setup);
            item.symbol = Market::split(&item.symbol).1.to_owned();
            if market == Market::Mcx {
                if let Some((high, low)) = d.range {
                    item.high_52w = round2(high);
                    item.low_52w = round2(low);
                }
                if let Some(s) = item.setup.as_mut() {
                    s.quantity = None;
                }
            }
            item
        })
        .collect();

    Ok(Json(StocksResponse {
        provider: source,
        market: market.as_str(),
        currency: market.currency(),
        as_of: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        items,
    }))
}

/// Sparkline, best setup and 52-week range from daily candles.
fn summarise(candles: &[swing_core::Candle], params: &AnalysisParams) -> Derived {
    let setup = swing_core::analyze(candles, params)
        .and_then(|a| a.setup)
        .map(|s| SetupSummary::from(&s));
    Derived {
        sparkline: dto::sparkline(candles),
        setup,
        range: range_52w(candles),
    }
}

fn range_52w(candles: &[swing_core::Candle]) -> Option<(f64, f64)> {
    let year = &candles[candles.len().saturating_sub(BARS_52W)..];
    let high = year.iter().map(|c| c.high).fold(f64::MIN, f64::max);
    let low = year.iter().map(|c| c.low).fold(f64::MAX, f64::min);
    (!year.is_empty()).then_some((high, low))
}

/// Everything the stock page needs for one symbol.
pub async fn detail(
    State(state): State<AppState>,
    path: Result<Path<String>, PathRejection>,
    query: Result<Query<DetailQuery>, QueryRejection>,
) -> Result<Json<StockDetailResponse>, ApiError> {
    let Path(raw) = path.map_err(|e| ApiError::BadRequest(e.body_text()))?;
    let symbol = params::normalize_symbol(&raw)?;
    let Query(q) = query.map_err(|e| ApiError::BadRequest(e.body_text()))?;
    let market = params::parse_market(q.market.as_deref())?;
    let interval = params::parse_interval(q.interval.as_deref())?;
    if !market.supports(interval) {
        return Err(ApiError::BadRequest(format!(
            "interval `{interval}` is not available for market `{market}` (daily data only)"
        )));
    }
    let capital = params::parse_capital(
        q.capital.as_deref(),
        state.config.default_capital_for(market),
    )?;
    let risk = params::parse_risk(q.risk.as_deref(), state.config.default_risk)?;
    let key = market.key(&symbol);

    let (candles, source) = state.candles(&key, interval).await?;
    if candles.is_empty() {
        return Err(ApiError::NotFound(format!("no price history for {symbol}")));
    }

    // Keep the quote on the same provider as the candles.
    let mut quote = match state.quote_from(source, &key).await {
        Ok(Some(q)) => q,
        Ok(None) => dto::quote_from_candles(&key, &candles),
        Err(e) => {
            tracing::warn!(%symbol, provider = source, error = %e, "quote failed; deriving from candles");
            dto::quote_from_candles(&key, &candles)
        }
    };
    if market == Market::Mcx && interval == Interval::Day {
        if let Some((high, low)) = range_52w(&candles) {
            quote.high_52w = high;
            quote.low_52w = low;
        }
    }

    let analysis = swing_core::analyze(&candles, &params::analysis_params(capital, risk));
    Ok(Json(StockDetailResponse::build(
        source,
        market,
        &quote,
        &candles,
        analysis.as_ref(),
    )))
}

/// Recent headlines for one stock from the data provider (INDmoney news).
pub async fn news(
    State(state): State<AppState>,
    path: Result<Path<String>, PathRejection>,
    query: Result<Query<NewsQuery>, QueryRejection>,
) -> Result<Json<NewsResponse>, ApiError> {
    let Path(raw) = path.map_err(|e| ApiError::BadRequest(e.body_text()))?;
    let symbol = params::normalize_symbol(&raw)?;
    let Query(q) = query.map_err(|e| ApiError::BadRequest(e.body_text()))?;
    let market = params::parse_market(q.market.as_deref())?;
    let key = market.key(&symbol);

    let (items, source) = match state.news.get(&key).await {
        Some(hit) => hit,
        None => {
            let (items, source) = state.market.news_with_source(&key).await?;
            let entry = (Arc::new(items), source);
            state.news.insert(key, entry.clone()).await;
            entry
        }
    };
    Ok(Json(NewsResponse {
        provider: source,
        market: market.as_str(),
        symbol,
        items: items.as_ref().clone(),
    }))
}
