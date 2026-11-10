//! Shared application state: configuration, providers and caches.

use std::sync::Arc;
use std::time::Duration;

use anyhow::bail;
use market_data::{
    indmoney, DemoProvider, Error as DataError, FallbackProvider, IndmoneyProvider, Interval,
    MarketData, Quote,
};
use swing_core::Candle;

use crate::cache::TtlCache;
use crate::config::Config;

/// A cached quote lookup: `None` remembers that the provider has no such symbol.
pub type CachedQuote = (Option<Quote>, &'static str);
/// Cached candles and the provider that served them.
pub type CachedCandles = (Arc<Vec<Candle>>, &'static str);

const QUOTE_CACHE_CAPACITY: usize = 2_000;
const CANDLE_CACHE_CAPACITY: usize = 1_000;

/// Cheap to clone; everything inside is shared.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub market: Arc<FallbackProvider>,
    /// Upper-case symbol → quote.
    pub quotes: Arc<TtlCache<String, CachedQuote>>,
    /// (symbol, interval) → candles.
    pub candles: Arc<TtlCache<(String, Interval), CachedCandles>>,
}

impl AppState {
    /// State with caches sized from `config`.
    pub fn new(config: Config, market: FallbackProvider) -> Self {
        let quotes = TtlCache::new(
            Duration::from_secs(config.quote_ttl_secs),
            QUOTE_CACHE_CAPACITY,
        );
        let candles = TtlCache::new(
            Duration::from_secs(config.candle_ttl_secs),
            CANDLE_CACHE_CAPACITY,
        );
        Self {
            config: Arc::new(config),
            market: Arc::new(market),
            quotes: Arc::new(quotes),
            candles: Arc::new(candles),
        }
    }

    /// Quotes for `symbols` in one batch, all from the same provider.
    /// Served from cache when every symbol is cached from one provider.
    pub async fn quotes(
        &self,
        symbols: &[String],
    ) -> Result<(Vec<Quote>, &'static str), DataError> {
        let mut cached = Vec::with_capacity(symbols.len());
        let mut source: Option<&'static str> = None;
        let mut complete = true;
        for s in symbols {
            match self.quotes.get(s).await {
                Some((quote, src)) if source.is_none_or(|known| known == src) => {
                    source = Some(src);
                    cached.extend(quote);
                }
                _ => {
                    complete = false;
                    break;
                }
            }
        }
        if let (true, Some(src)) = (complete, source) {
            return Ok((cached, src));
        }

        let (quotes, src) = self.market.quotes_with_source(symbols).await?;
        for s in symbols {
            let quote = quotes.iter().find(|q| &q.symbol == s).cloned();
            self.quotes.insert(s.clone(), (quote, src)).await;
        }
        Ok((quotes, src))
    }

    /// One quote from a specific provider (so it matches that provider's candles).
    pub async fn quote_from(
        &self,
        source: &'static str,
        symbol: &str,
    ) -> Result<Option<Quote>, DataError> {
        if let Some((quote, src)) = self.quotes.get(&symbol.to_owned()).await {
            if src == source {
                return Ok(quote);
            }
        }
        let provider = self.provider(source)?;
        let quote = provider
            .quotes(&[symbol.to_owned()])
            .await?
            .into_iter()
            .find(|q| q.symbol == symbol);
        self.quotes
            .insert(symbol.to_owned(), (quote.clone(), source))
            .await;
        Ok(quote)
    }

    /// Candles from whichever provider answers first.
    pub async fn candles(
        &self,
        symbol: &str,
        interval: Interval,
    ) -> Result<CachedCandles, DataError> {
        let key = (symbol.to_owned(), interval);
        if let Some(hit) = self.candles.get(&key).await {
            return Ok(hit);
        }
        let (candles, src) = self.market.candles_with_source(symbol, interval).await?;
        let entry = (Arc::new(candles), src);
        self.candles
            .insert_for(key, entry.clone(), self.candle_ttl(src))
            .await;
        Ok(entry)
    }

    /// Candles from a specific provider (used to keep a list on one source).
    pub async fn candles_from(
        &self,
        source: &'static str,
        symbol: &str,
        interval: Interval,
    ) -> Result<Arc<Vec<Candle>>, DataError> {
        let key = (symbol.to_owned(), interval);
        if let Some((candles, src)) = self.candles.get(&key).await {
            if src == source {
                return Ok(candles);
            }
        }
        let candles = Arc::new(self.provider(source)?.candles(symbol, interval).await?);
        self.candles
            .insert_for(key, (candles.clone(), source), self.candle_ttl(source))
            .await;
        Ok(candles)
    }

    /// Candles from the first provider in `DATA_PROVIDERS` keep the full
    /// candle TTL. Anything served by a fallback (usually demo, after an
    /// INDmoney hiccup) lives only as long as a quote, so real data returns
    /// as soon as the primary source recovers.
    fn candle_ttl(&self, source: &'static str) -> Duration {
        let primary = self.market.ids().first().copied();
        if primary == Some(source) {
            self.candles.ttl()
        } else {
            self.quotes.ttl()
        }
    }

    fn provider(&self, id: &'static str) -> Result<Arc<dyn MarketData>, DataError> {
        self.market.get(id).ok_or_else(|| {
            DataError::AllProvidersFailed(format!("provider `{id}` is not configured"))
        })
    }
}

/// Builds the provider chain from `DATA_PROVIDERS`.
///
/// INDmoney is skipped with a warning when it has no credentials, so a
/// fresh checkout still runs on demo data.
pub fn build_market(config: &Config) -> anyhow::Result<FallbackProvider> {
    let mut providers: Vec<Arc<dyn MarketData>> = Vec::new();
    for id in config.providers() {
        match id.as_str() {
            indmoney::ID => match indmoney::token_source(&config.indmoney_token_file) {
                Some(tokens) => {
                    let provider = IndmoneyProvider::new(config.indmoney_mcp_url.clone(), tokens)?;
                    providers.push(Arc::new(provider));
                }
                None => tracing::warn!(
                    token_file = %config.indmoney_token_file.display(),
                    "INDmoney is not logged in (no token file and no {}); skipping it. \
                     Run `swing-api login indmoney` to connect.",
                    indmoney::oauth::ACCESS_TOKEN_ENV
                ),
            },
            "demo" => providers.push(Arc::new(DemoProvider::new())),
            other => bail!(
                "unknown data provider `{other}` in DATA_PROVIDERS (expected indmoney or demo)"
            ),
        }
    }
    if providers.is_empty() {
        bail!(
            "no usable data provider: run `swing-api login indmoney` or add `demo` to DATA_PROVIDERS"
        );
    }
    if providers.iter().any(|p| p.id() == "demo") {
        tracing::info!("demo provider enabled: responses with provider \"demo\" are synthetic");
    }
    Ok(FallbackProvider::new(providers))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::tests::{config_from, test_config};

    #[tokio::test]
    async fn caches_quotes_and_candles() {
        let state = AppState::new(test_config(), build_market(&test_config()).unwrap());
        let symbols = vec!["MCX".to_owned(), "IEX".to_owned()];
        let (quotes, src) = state.quotes(&symbols).await.unwrap();
        assert_eq!(src, "demo");
        assert_eq!(quotes.len(), 2);
        assert_eq!(state.quotes.len().await, 2);
        let (again, _) = state.quotes(&symbols).await.unwrap();
        assert_eq!(again, quotes);

        let (candles, src) = state.candles("MCX", Interval::Day).await.unwrap();
        assert_eq!(src, "demo");
        let same = state
            .candles_from("demo", "MCX", Interval::Day)
            .await
            .unwrap();
        assert!(
            Arc::ptr_eq(&candles, &same),
            "second read should hit the cache"
        );
        assert!(state
            .candles_from("indmoney", "MCX", Interval::Day)
            .await
            .is_err());

        let q = state.quote_from("demo", "MCX").await.unwrap().unwrap();
        assert_eq!(q.symbol, "MCX");
    }

    #[test]
    fn fallback_candles_expire_with_quotes() {
        let state = AppState::new(test_config(), build_market(&test_config()).unwrap());
        // `test_config` lists only demo, so demo is the primary source here.
        assert_eq!(state.candle_ttl("demo"), state.candles.ttl());
        assert_eq!(state.candle_ttl("indmoney"), state.quotes.ttl());
        assert!(state.quotes.ttl() < state.candles.ttl());
    }

    #[test]
    fn rejects_unknown_providers() {
        let c = config_from(&[
            "--api-token",
            "test-token-0123456789",
            "--data-providers",
            "demo,yahoo",
        ]);
        assert!(build_market(&c).is_err());
    }

    #[test]
    fn skips_indmoney_without_credentials() {
        if std::env::var(indmoney::oauth::ACCESS_TOKEN_ENV).is_ok() {
            return;
        }
        let c = config_from(&[
            "--api-token",
            "test-token-0123456789",
            "--data-providers",
            "indmoney,demo",
            "--indmoney-token-file",
            "/nonexistent/swing-trade/token.json",
        ]);
        assert_eq!(build_market(&c).unwrap().ids(), vec!["demo"]);

        let only = config_from(&[
            "--api-token",
            "test-token-0123456789",
            "--data-providers",
            "indmoney",
            "--indmoney-token-file",
            "/nonexistent/swing-trade/token.json",
        ]);
        assert!(build_market(&only).is_err());
    }
}
