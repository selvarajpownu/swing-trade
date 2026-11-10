//! `GET /v1/stocks` and `GET /v1/stocks/{symbol}`.

use std::collections::HashMap;

use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::Json;
use futures::stream::{self, StreamExt};
use market_data::Interval;
use serde::Deserialize;
use swing_core::AnalysisParams;

use crate::dto::{self, SetupSummary, StockDetailResponse, StockItem, StocksResponse};
use crate::error::ApiError;
use crate::params;
use crate::state::AppState;

/// Concurrent candle fetches for the list.
const CANDLE_CONCURRENCY: usize = 6;

/// Query for the list. Values are parsed by hand for contract-shaped errors.
#[derive(Debug, Default, Deserialize)]
pub struct ListQuery {
    pub symbols: Option<String>,
    pub capital: Option<String>,
    pub risk: Option<String>,
}

/// Query for one stock.
#[derive(Debug, Default, Deserialize)]
pub struct DetailQuery {
    pub interval: Option<String>,
    pub capital: Option<String>,
    pub risk: Option<String>,
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
    let symbols = match q.symbols.as_deref().map(str::trim) {
        Some(s) if !s.is_empty() => params::parse_symbols(s)?,
        _ => state.config.watchlist(),
    };
    let capital = params::parse_capital(q.capital.as_deref(), state.config.default_capital)?;
    let risk = params::parse_risk(q.risk.as_deref(), state.config.default_risk)?;
    let analysis_params = params::analysis_params(capital, risk);

    let (quotes, source) = state.quotes(&symbols).await?;

    // Owned futures (no borrows) keep the handler future `Send` without
    // tripping higher-ranked lifetime inference.
    let jobs: Vec<_> = quotes.iter().map(|quote| {
        let state = state.clone();
        let params = analysis_params.clone();
        let symbol = quote.symbol.clone();
        async move {
            let derived = match state.candles_from(source, &symbol, Interval::Day).await {
                Ok(candles) => summarise(&candles, &params),
                Err(e) => {
                    tracing::warn!(%symbol, provider = source, error = %e, "no candles; skipping setup");
                    (Vec::new(), None)
                }
            };
            (symbol, derived)
        }
    }).collect();
    let mut derived: HashMap<String, (Vec<f64>, Option<SetupSummary>)> = stream::iter(jobs)
        .buffer_unordered(CANDLE_CONCURRENCY)
        .collect()
        .await;

    let items = quotes
        .iter()
        .map(|quote| {
            let (sparkline, setup) = derived.remove(&quote.symbol).unwrap_or_default();
            StockItem::new(quote, sparkline, setup)
        })
        .collect();

    Ok(Json(StocksResponse {
        provider: source,
        as_of: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        items,
    }))
}

/// Sparkline and best setup from daily candles.
fn summarise(
    candles: &[swing_core::Candle],
    params: &AnalysisParams,
) -> (Vec<f64>, Option<SetupSummary>) {
    let setup = swing_core::analyze(candles, params)
        .and_then(|a| a.setup)
        .map(|s| SetupSummary::from(&s));
    (dto::sparkline(candles), setup)
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
    let interval = params::parse_interval(q.interval.as_deref())?;
    let capital = params::parse_capital(q.capital.as_deref(), state.config.default_capital)?;
    let risk = params::parse_risk(q.risk.as_deref(), state.config.default_risk)?;

    let (candles, source) = state.candles(&symbol, interval).await?;
    if candles.is_empty() {
        return Err(ApiError::NotFound(format!("no price history for {symbol}")));
    }

    // Keep the quote on the same provider as the candles.
    let quote = match state.quote_from(source, &symbol).await {
        Ok(Some(q)) => q,
        Ok(None) => dto::quote_from_candles(&symbol, &candles),
        Err(e) => {
            tracing::warn!(%symbol, provider = source, error = %e, "quote failed; deriving from candles");
            dto::quote_from_candles(&symbol, &candles)
        }
    };

    let analysis = swing_core::analyze(&candles, &params::analysis_params(capital, risk));
    Ok(Json(StockDetailResponse::build(
        source,
        &quote,
        &candles,
        analysis.as_ref(),
    )))
}
