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

/// Which market an instrument trades in.
///
/// Providers receive market-qualified keys: NSE symbols stay bare (`ACE`),
/// others carry a prefix (`US:AAPL`, `MCX:GOLD`). See [`Market::key`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Market {
    /// NSE equities.
    #[default]
    In,
    /// US equities and ETFs (prices in USD).
    Us,
    /// MCX commodity futures, near-month contract.
    Mcx,
}

impl Market {
    pub const ALL: [Market; 3] = [Market::In, Market::Us, Market::Mcx];

    /// The value used in the public API (`in`, `us`, `mcx`).
    pub fn as_str(&self) -> &'static str {
        match self {
            Market::In => "in",
            Market::Us => "us",
            Market::Mcx => "mcx",
        }
    }

    /// ISO currency of prices in this market.
    pub fn currency(&self) -> &'static str {
        match self {
            Market::In | Market::Mcx => "INR",
            Market::Us => "USD",
        }
    }

    /// Default exchange label.
    pub fn exchange(&self) -> &'static str {
        match self {
            Market::In => "NSE",
            Market::Us => "US",
            Market::Mcx => "MCX",
        }
    }

    fn prefix(&self) -> &'static str {
        match self {
            Market::In => "",
            Market::Us => "US:",
            Market::Mcx => "MCX:",
        }
    }

    /// Provider-level key for `symbol` in this market.
    pub fn key(&self, symbol: &str) -> String {
        format!("{}{}", self.prefix(), symbol.trim().to_ascii_uppercase())
    }

    /// Splits a provider-level key into its market and bare symbol.
    pub fn split(key: &str) -> (Market, &str) {
        for m in [Market::Us, Market::Mcx] {
            if let Some(rest) = key.strip_prefix(m.prefix()) {
                return (m, rest);
            }
        }
        (Market::In, key)
    }

    /// Whether providers serve this candle size here (US data is daily only).
    pub fn supports(&self, interval: Interval) -> bool {
        !(matches!(self, Market::Us) && interval == Interval::Min15)
    }
}

impl fmt::Display for Market {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Market {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "" | "in" | "nse" => Ok(Market::In),
            "us" => Ok(Market::Us),
            "mcx" | "commodity" => Ok(Market::Mcx),
            other => Err(format!("unknown market `{other}` (expected in, us or mcx)")),
        }
    }
}

/// One news headline about an instrument.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewsItem {
    pub title: String,
    pub summary: Option<String>,
    /// `positive`, `neutral` or `negative`, when the source rates it.
    pub sentiment: Option<String>,
    /// ISO 8601 date or date-time, when known.
    pub published_at: Option<String>,
    /// Publisher name, when known.
    pub source: Option<String>,
    /// Link to the article, when known.
    pub url: Option<String>,
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
    fn qualifies_symbols_by_market() {
        assert_eq!(Market::In.key("ace"), "ACE");
        assert_eq!(Market::Us.key("aapl"), "US:AAPL");
        assert_eq!(Market::split("US:AAPL"), (Market::Us, "AAPL"));
        assert_eq!(Market::split("MCX:GOLD"), (Market::Mcx, "GOLD"));
        assert_eq!(Market::split("M&M"), (Market::In, "M&M"));
        assert_eq!("MCX".parse::<Market>(), Ok(Market::Mcx));
        assert_eq!("".parse::<Market>(), Ok(Market::In));
        assert!("lse".parse::<Market>().is_err());
        assert!(!Market::Us.supports(Interval::Min15));
        assert!(Market::Mcx.supports(Interval::Min15));
        assert_eq!(Market::Us.currency(), "USD");
    }

    #[test]
    fn maps_to_indmoney_params() {
        assert_eq!(Interval::Day.indmoney_params(), ("1day", "1y"));
        assert_eq!(Interval::Week.indmoney_params(), ("1week", "1y"));
        assert_eq!(Interval::Min15.indmoney_params(), ("15minute", "7d"));
    }
}
