//! Validation of path and query parameters.

use market_data::Interval;
use swing_core::{AnalysisParams, PlanParams};

use crate::error::ApiError;

/// Longest accepted symbol.
pub const MAX_SYMBOL_LEN: usize = 20;
/// Most symbols accepted in one `/v1/stocks` request.
pub const MAX_SYMBOLS: usize = 50;
/// Largest accepted risk fraction per trade.
pub const MAX_RISK: f64 = 0.05;
/// Largest accepted capital, a sanity bound (₹1 lakh crore).
pub const MAX_CAPITAL: f64 = 1e12;

/// Upper-cases and validates one symbol: `A-Z`, `0-9`, `&`, `-`, at most 20 characters.
pub fn normalize_symbol(raw: &str) -> Result<String, ApiError> {
    let symbol = raw.trim().to_ascii_uppercase();
    if symbol.is_empty() {
        return Err(ApiError::BadRequest("symbol is empty".to_owned()));
    }
    if symbol.len() > MAX_SYMBOL_LEN {
        return Err(ApiError::BadRequest(format!(
            "symbol is longer than {MAX_SYMBOL_LEN} characters"
        )));
    }
    if !symbol
        .bytes()
        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'&' || b == b'-')
    {
        return Err(ApiError::BadRequest(format!(
            "invalid symbol `{}` (allowed: A-Z, 0-9, & and -)",
            raw.trim()
        )));
    }
    Ok(symbol)
}

/// Parses a comma-separated symbol list, de-duplicated in order.
pub fn parse_symbols(raw: &str) -> Result<Vec<String>, ApiError> {
    let mut out: Vec<String> = Vec::new();
    for part in raw.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let symbol = normalize_symbol(part)?;
        if !out.contains(&symbol) {
            out.push(symbol);
        }
    }
    if out.is_empty() {
        return Err(ApiError::BadRequest("no symbols given".to_owned()));
    }
    if out.len() > MAX_SYMBOLS {
        return Err(ApiError::BadRequest(format!(
            "at most {MAX_SYMBOLS} symbols per request"
        )));
    }
    Ok(out)
}

/// Like [`parse_symbols`] but for server configuration: an empty list is
/// fine and there is no per-request cap.
pub fn parse_symbols_lenient(raw: &str) -> Result<Vec<String>, ApiError> {
    let mut out: Vec<String> = Vec::new();
    for part in raw.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let symbol = normalize_symbol(part)?;
        if !out.contains(&symbol) {
            out.push(symbol);
        }
    }
    Ok(out)
}

/// `in` (default), `us` or `mcx`.
pub fn parse_market(raw: Option<&str>) -> Result<market_data::Market, ApiError> {
    raw.unwrap_or("").parse().map_err(ApiError::BadRequest)
}

/// Checks a capital amount in rupees.
pub fn check_capital(capital: f64) -> Result<f64, ApiError> {
    if capital.is_finite() && capital > 0.0 && capital <= MAX_CAPITAL {
        Ok(capital)
    } else {
        Err(ApiError::BadRequest(
            "capital must be a positive number of rupees".to_owned(),
        ))
    }
}

/// Checks a risk fraction: `0 < risk <= 0.05`.
pub fn check_risk(risk: f64) -> Result<f64, ApiError> {
    if risk.is_finite() && risk > 0.0 && risk <= MAX_RISK {
        Ok(risk)
    } else {
        Err(ApiError::BadRequest(format!(
            "risk must be a fraction greater than 0 and at most {MAX_RISK}"
        )))
    }
}

/// `capital` query parameter, or the default when absent or blank.
pub fn parse_capital(raw: Option<&str>, default: f64) -> Result<f64, ApiError> {
    match raw.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(default),
        Some(s) => s
            .parse::<f64>()
            .map_err(|_| ApiError::BadRequest(format!("capital `{s}` is not a number")))
            .and_then(check_capital),
    }
}

/// `risk` query parameter, or the default when absent or blank.
pub fn parse_risk(raw: Option<&str>, default: f64) -> Result<f64, ApiError> {
    match raw.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(default),
        Some(s) => s
            .parse::<f64>()
            .map_err(|_| ApiError::BadRequest(format!("risk `{s}` is not a number")))
            .and_then(check_risk),
    }
}

/// `interval` query parameter (`1d` default, `1w`, `15m`).
pub fn parse_interval(raw: Option<&str>) -> Result<Interval, ApiError> {
    match raw.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(Interval::Day),
        Some(s) => s
            .parse()
            .map_err(|e: market_data::ParseIntervalError| ApiError::BadRequest(e.to_string())),
    }
}

/// Analysis parameters with the caller's position sizing.
pub fn analysis_params(capital: f64, risk: f64) -> AnalysisParams {
    AnalysisParams {
        plan: PlanParams {
            capital,
            risk_per_trade: risk,
            ..PlanParams::default()
        },
        ..AnalysisParams::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bad<T: std::fmt::Debug>(r: Result<T, ApiError>) -> bool {
        matches!(r, Err(ApiError::BadRequest(_)))
    }

    #[test]
    fn validates_symbols() {
        assert_eq!(normalize_symbol(" coforge ").unwrap(), "COFORGE");
        assert_eq!(normalize_symbol("m&m").unwrap(), "M&M");
        assert_eq!(normalize_symbol("BAJAJ-AUTO").unwrap(), "BAJAJ-AUTO");
        assert_eq!(normalize_symbol("NIFTY50").unwrap(), "NIFTY50");
        assert!(bad(normalize_symbol("")));
        assert!(bad(normalize_symbol("ABC DEF")));
        assert!(bad(normalize_symbol("ABC.NS")));
        assert!(bad(normalize_symbol("../etc")));
        assert!(bad(normalize_symbol("ÄBC")));
        assert!(bad(normalize_symbol(&"A".repeat(21))));
        assert!(normalize_symbol(&"A".repeat(20)).is_ok());
    }

    #[test]
    fn parses_symbol_lists() {
        assert_eq!(
            parse_symbols("mcx, IEX,,mcx ,cams").unwrap(),
            vec!["MCX", "IEX", "CAMS"]
        );
        assert!(bad(parse_symbols(" , ")));
        assert!(bad(parse_symbols("MCX,B@D")));
        let many = (0..51)
            .map(|i| format!("S{i}"))
            .collect::<Vec<_>>()
            .join(",");
        assert!(bad(parse_symbols(&many)));
    }

    #[test]
    fn validates_capital_and_risk() {
        assert_eq!(parse_capital(None, 5e5).unwrap(), 5e5);
        assert_eq!(parse_capital(Some(""), 5e5).unwrap(), 5e5);
        assert_eq!(parse_capital(Some("100000"), 5e5).unwrap(), 1e5);
        assert!(bad(parse_capital(Some("0"), 5e5)));
        assert!(bad(parse_capital(Some("-5"), 5e5)));
        assert!(bad(parse_capital(Some("abc"), 5e5)));
        assert!(bad(parse_capital(Some("NaN"), 5e5)));
        assert!(bad(parse_capital(Some("inf"), 5e5)));

        assert_eq!(parse_risk(None, 0.01).unwrap(), 0.01);
        assert_eq!(parse_risk(Some("0.05"), 0.01).unwrap(), 0.05);
        assert!(bad(parse_risk(Some("0"), 0.01)));
        assert!(bad(parse_risk(Some("0.051"), 0.01)));
        assert!(bad(parse_risk(Some("1"), 0.01)));
    }

    #[test]
    fn parses_intervals() {
        assert_eq!(parse_interval(None).unwrap(), Interval::Day);
        assert_eq!(parse_interval(Some("1w")).unwrap(), Interval::Week);
        assert_eq!(parse_interval(Some("15m")).unwrap(), Interval::Min15);
        assert!(bad(parse_interval(Some("1h"))));
    }

    #[test]
    fn builds_analysis_params() {
        let p = analysis_params(250_000.0, 0.02);
        assert_eq!(p.plan.capital, 250_000.0);
        assert_eq!(p.plan.risk_per_trade, 0.02);
        assert_eq!(p.min_risk_reward, AnalysisParams::default().min_risk_reward);
    }
}
