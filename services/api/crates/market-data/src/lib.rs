//! Market-data connectors.
//!
//! Every source implements [`MarketData`]. The API only ever talks to a
//! `dyn MarketData`, usually a [`FallbackProvider`] that tries sources in the
//! order set in configuration.

mod demo;
mod error;
mod fallback;
pub mod indmoney;
pub mod mcp;
mod names;
mod types;

pub use demo::DemoProvider;
pub use error::{Error, Result};
pub use fallback::FallbackProvider;
pub use indmoney::IndmoneyProvider;
pub use mcp::{StaticToken, TokenSource};
pub use types::{Interval, ParseIntervalError, Quote};

use async_trait::async_trait;
use swing_core::Candle;

/// A source of quotes and candles for NSE/BSE equities.
#[async_trait]
pub trait MarketData: Send + Sync {
    /// Short stable id used in config and logs, e.g. `"indmoney"`.
    fn id(&self) -> &'static str;

    /// Latest quote for each symbol. Unknown symbols are skipped, not errors.
    async fn quotes(&self, symbols: &[String]) -> Result<Vec<Quote>>;

    /// OHLCV candles, oldest first.
    async fn candles(&self, symbol: &str, interval: Interval) -> Result<Vec<Candle>>;
}
