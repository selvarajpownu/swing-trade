//! `GET /v1/scan`: ranked setups from the latest background scan.

use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;
use swing_core::{Preset, Setup, SetupState};

use crate::dto::{ScanItem, ScanResponse};
use crate::error::ApiError;
use crate::params;
use crate::state::AppState;

const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 500;

/// Query for the scan. Values are parsed by hand for contract-shaped errors.
#[derive(Debug, Default, Deserialize)]
pub struct ScanQuery {
    pub preset: Option<String>,
    pub min_score: Option<String>,
    pub min_rr: Option<String>,
    pub state: Option<String>,
    pub limit: Option<String>,
    pub capital: Option<String>,
    pub risk: Option<String>,
}

/// Filters every row's setups must pass.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScanFilter {
    pub preset: Option<Preset>,
    pub min_score: f64,
    pub min_rr: f64,
    pub state: Option<SetupState>,
}

impl ScanFilter {
    pub fn accepts(&self, s: &Setup) -> bool {
        self.preset.is_none_or(|p| s.preset == p)
            && self.state.is_none_or(|st| s.state == st)
            && s.score.total() >= self.min_score
            && s.plan.risk_reward >= self.min_rr
    }
}

pub async fn list(
    State(state): State<AppState>,
    query: Result<Query<ScanQuery>, QueryRejection>,
) -> Result<Json<ScanResponse>, ApiError> {
    let Query(q) = query.map_err(|e| ApiError::BadRequest(e.body_text()))?;
    let filter = parse_filter(&q)?;
    let limit = parse_limit(q.limit.as_deref())?;
    let capital = params::parse_capital(q.capital.as_deref(), state.config.default_capital)?;
    let risk = params::parse_risk(q.risk.as_deref(), state.config.default_risk)?;

    let (status, rows) = state.scanner.snapshot().await;
    let mut items: Vec<ScanItem> = rows
        .iter()
        .filter_map(|row| {
            // Setups are best-first, so the first accepted one is the best match.
            let setup = row.setups.iter().find(|s| filter.accepts(s))?;
            Some(ScanItem::new(row, setup, capital, risk))
        })
        .collect();
    items.sort_by(|a, b| {
        b.setup
            .score
            .cmp(&a.setup.score)
            .then(b.setup.risk_reward.total_cmp(&a.setup.risk_reward))
            .then_with(|| a.symbol.cmp(&b.symbol))
    });
    let total = items.len();
    items.truncate(limit);

    Ok(Json(ScanResponse {
        provider: state.scanner.provider(),
        status: (&status).into(),
        total,
        items,
    }))
}

fn parse_filter(q: &ScanQuery) -> Result<ScanFilter, ApiError> {
    let preset = match q.preset.as_deref().map(str::trim) {
        None | Some("") | Some("any") => None,
        Some("pullback") => Some(Preset::Pullback),
        Some("breakout") => Some(Preset::Breakout),
        Some("support_bounce") => Some(Preset::SupportBounce),
        Some("oversold") => Some(Preset::Oversold),
        Some(other) => {
            return Err(ApiError::BadRequest(format!(
                "unknown preset `{other}` (expected pullback, breakout, support_bounce or oversold)"
            )))
        }
    };
    let state = match q.state.as_deref().map(str::trim) {
        None | Some("") | Some("any") => None,
        Some("watching") => Some(SetupState::Watching),
        Some("low_risk_reward") => Some(SetupState::LowRiskReward),
        Some(other) => {
            return Err(ApiError::BadRequest(format!(
                "unknown state `{other}` (expected watching or low_risk_reward)"
            )))
        }
    };
    Ok(ScanFilter {
        preset,
        state,
        min_score: parse_number(q.min_score.as_deref(), "min_score", 0.0, 100.0)?,
        min_rr: parse_number(q.min_rr.as_deref(), "min_rr", 0.0, 100.0)?,
    })
}

fn parse_number(raw: Option<&str>, name: &str, min: f64, max: f64) -> Result<f64, ApiError> {
    let Some(raw) = raw.map(str::trim).filter(|r| !r.is_empty()) else {
        return Ok(min);
    };
    match raw.parse::<f64>() {
        Ok(v) if v.is_finite() && (min..=max).contains(&v) => Ok(v),
        _ => Err(ApiError::BadRequest(format!(
            "`{name}` must be a number from {min} to {max}"
        ))),
    }
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

    fn query(pairs: &[(&str, &str)]) -> ScanQuery {
        let mut q = ScanQuery::default();
        for (k, v) in pairs {
            let v = Some((*v).to_owned());
            match *k {
                "preset" => q.preset = v,
                "min_score" => q.min_score = v,
                "min_rr" => q.min_rr = v,
                "state" => q.state = v,
                "limit" => q.limit = v,
                _ => unreachable!(),
            }
        }
        q
    }

    #[test]
    fn parses_filters() {
        let f = parse_filter(&query(&[
            ("preset", "support_bounce"),
            ("min_score", "60"),
            ("min_rr", "2"),
            ("state", "watching"),
        ]))
        .unwrap();
        assert_eq!(f.preset, Some(Preset::SupportBounce));
        assert_eq!(f.state, Some(SetupState::Watching));
        assert_eq!((f.min_score, f.min_rr), (60.0, 2.0));
        assert_eq!(parse_filter(&query(&[])).unwrap(), ScanFilter::default());

        for bad in [
            ("preset", "cup_handle"),
            ("state", "bought"),
            ("min_score", "101"),
            ("min_rr", "-1"),
            ("min_score", "NaN"),
        ] {
            assert!(parse_filter(&query(&[bad])).is_err(), "{bad:?}");
        }
        assert_eq!(parse_limit(None).unwrap(), 50);
        assert_eq!(parse_limit(Some("500")).unwrap(), 500);
        assert!(parse_limit(Some("0")).is_err());
        assert!(parse_limit(Some("501")).is_err());
    }
}
