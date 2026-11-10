//! Plain data types shared by every provider.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Latest quote for one listed equity. Prices are in rupees.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Quote {
    /// Exchange ticker, upper case (e.g. `COFORGE`).
    pub symbol: String,
    /// Human readable company name.
    pub name: String,
    /// Listing exchange, e.g. `NSE`.
    pub exchange: String,
    /// Last traded price.
    pub price: f64,
    /// Change against the previous close, in rupees.
    pub change: f64,
    /// Change against the previous close, in percent.
    pub change_pct: f64,
    pub day_high: f64,
    pub day_low: f64,
    pub high_52w: f64,
    pub low_52w: f64,
    /// Shares traded in the current (or last) session.
    pub volume: f64,
    /// When the price was last updated (ISO 8601), if the source says.
    pub updated_at: Option<String>,
}

/// Candle size supported by the API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Interval {
    /// One bar per trading day.
    #[default]
    Day,
    /// One bar per week.
    Week,
    /// Fifteen-minute intraday bars.
    Min15,
}

impl Interval {
    /// The value used in the public API (`1d`, `1w`, `15m`).
    pub fn as_str(&self) -> &'static str {
        match self {
            Interval::Day => "1d",
            Interval::Week => "1w",
            Interval::Min15 => "15m",
        }
    }

    /// The `(interval, lookback)` pair the INDmoney OHLC tool expects.
    pub fn indmoney_params(&self) -> (&'static str, &'static str) {
        match self {
            Interval::Day => ("1day", "1y"),
            Interval::Week => ("1week", "1y"),
            Interval::Min15 => ("15minute", "7d"),
        }
    }
}

impl fmt::Display for Interval {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Returned when an interval string is not one of `1d`, `1w`, `15m`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseIntervalError(pub String);

impl fmt::Display for ParseIntervalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unsupported interval `{}` (expected 1d, 1w or 15m)",
            self.0
        )
    }
}

impl std::error::Error for ParseIntervalError {}

impl FromStr for Interval {
    type Err = ParseIntervalError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "1d" => Ok(Interval::Day),
            "1w" => Ok(Interval::Week),
            "15m" => Ok(Interval::Min15),
            _ => Err(ParseIntervalError(s.to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_api_intervals() {
        assert_eq!("1d".parse::<Interval>(), Ok(Interval::Day));
        assert_eq!("1W".parse::<Interval>(), Ok(Interval::Week));
        assert_eq!(" 15m ".parse::<Interval>(), Ok(Interval::Min15));
        assert!("1h".parse::<Interval>().is_err());
        assert!("".parse::<Interval>().is_err());
    }

    #[test]
    fn round_trips_through_display() {
        for i in [Interval::Day, Interval::Week, Interval::Min15] {
            assert_eq!(i.to_string().parse::<Interval>(), Ok(i));
        }
    }

    #[test]
    fn maps_to_indmoney_params() {
        assert_eq!(Interval::Day.indmoney_params(), ("1day", "1y"));
        assert_eq!(Interval::Week.indmoney_params(), ("1week", "1y"));
        assert_eq!(Interval::Min15.indmoney_params(), ("15minute", "7d"));
    }
}
