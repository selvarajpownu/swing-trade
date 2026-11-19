//! Shared application state: configuration, providers and caches.

use std::sync::Arc;
use std::time::Duration;

use anyhow::bail;
use market_data::{
    indmoney, nse, yahoo, DemoProvider, Error as DataError, FallbackProvider, IndmoneyProvider,
    Interval, MarketData, NseEodProvider, Quote, YahooProvider,
};
use swing_core::Candle;

use crate::cache::TtlCache;
use crate::config::Config;
use crate::news::{GoogleNews, NEWS_TTL};
use crate::scan::Scanner;

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
    /// Background scan results for `GET /v1/scan`.
    pub scanner: Arc<Scanner>,
    /// Provider headlines per market-qualified symbol, with their source.
    pub news: Arc<TtlCache<String, CachedNews>>,
    /// Google News RSS client for the Stock news page.
    pub google: Arc<GoogleNews>,
    /// NSE end-of-day files, when enabled and a real provider is configured.
    pub eod: Option<Arc<market_data::NseEod>>,
    /// NSE catalysts (results dates, filings, deals); enabled with `eod`.
    pub events: Option<Arc<market_data::NseEvents>>,
}

/// Cached provider headlines and the provider that served them.
pub type CachedNews = (Arc<Vec<market_data::NewsItem>>, &'static str);

impl AppState {
    /// State with caches sized from `config`.
    pub fn new(config: Config, mut market: FallbackProvider) -> Self {
        let quotes = TtlCache::new(
            Duration::from_secs(config.quote_ttl_secs),
            QUOTE_CACHE_CAPACITY,
        );
        let candles = TtlCache::new(
            Duration::from_secs(config.candle_ttl_secs),
            CANDLE_CACHE_CAPACITY,
        );
        // Demo-only mode stays offline: no NSE downloads.
        let listed = config.providers();
        let wants_nse = listed.iter().any(|id| id == nse::ID);
        let real = market.ids().iter().any(|id| *id != "demo") || wants_nse;
        if wants_nse && !config.nse_eod {
            tracing::warn!("DATA_PROVIDERS lists `nse` but NSE_EOD is off; skipping it");
        }
        let eod = (config.nse_eod && real)
            .then(|| market_data::NseEod::new(&config.nse_eod_dir, config.nse_eod_days))
            .and_then(|built| match built {
                Ok(eod) => Some(Arc::new(eod)),
                Err(e) => {
                    tracing::warn!(error = %e, "NSE end-of-day data disabled");
                    None
                }
            });
        let events =
            eod.as_ref()
                .and_then(|_| match market_data::NseEvents::new(&config.nse_eod_dir) {
                    Ok(events) => Some(Arc::new(events)),
                    Err(e) => {
                        tracing::warn!(error = %e, "NSE catalysts disabled");
                        None
                    }
                });
        // `nse` in DATA_PROVIDERS: serve daily charts from the same files, at
        // its place in the order among the providers that were built.
        if let (true, Some(eod)) = (wants_nse, &eod) {
            let before = listed
                .iter()
                .take_while(|id| *id != nse::ID)
                .filter(|id| market.get(id).is_some())
                .count();
            market.insert(before, Arc::new(NseEodProvider::new(eod.clone())));
        }
        // Scans fall back to the first provider only when NSE files are not in use.
        let primary = market.ids().first().copied().unwrap_or("demo");
        Self {
            eod,
            events,
            news: Arc::new(TtlCache::new(NEWS_TTL, 1_000)),
            google: Arc::new(GoogleNews::new()),
            scanner: Arc::new(Scanner::new(primary)),
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

    /// Real candles keep the full candle TTL. Demo candles served because
    /// every real source failed live only as long as a quote, so real data
    /// returns as soon as a source recovers. (With demo as the only provider
    /// there is nothing to recover to, so it keeps the full TTL.)
    fn candle_ttl(&self, source: &'static str) -> Duration {
        let demo_only = self.market.ids().iter().all(|id| *id == "demo");
        if source == "demo" && !demo_only {
            self.quotes.ttl()
        } else {
            self.candles.ttl()
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
/// fresh checkout still runs. `nse` is added by [`AppState::new`], which
/// owns the NSE files it reads.
pub fn build_market(config: &Config) -> anyhow::Result<FallbackProvider> {
    let mut providers: Vec<Arc<dyn MarketData>> = Vec::new();
    for id in config.providers() {
        match id.as_str() {
            indmoney::ID => match indmoney::token_source(&config.indmoney_token_file) {
                Some(tokens) => {
                    let provider = IndmoneyProvider::new(config.indmoney_mcp_url.clone(), tokens)?
                        .with_rate_limit(
                            config.indmoney_rate_per_min,
                            config.indmoney_rate_per_hour,
                        );
                    providers.push(Arc::new(provider));
                }
                None => tracing::warn!(
                    token_file = %config.indmoney_token_file.display(),
                    "INDmoney is not logged in (no token file and no {}); skipping it. \
                     Run `swing-api login indmoney` to connect.",
                    indmoney::oauth::ACCESS_TOKEN_ENV
                ),
            },
            yahoo::ID => providers.push(Arc::new(YahooProvider::new()?)),
            nse::ID => {}
            "demo" => providers.push(Arc::new(DemoProvider::new())),
            other => bail!(
                "unknown data provider `{other}` in DATA_PROVIDERS (expected yahoo, indmoney, nse or demo)"
            ),
        }
    }
    let lists_nse = config.providers().iter().any(|id| id == nse::ID);
    if providers.is_empty() && !(lists_nse && config.nse_eod) {
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
        // `test_config` lists only demo: nothing better to wait for.
        assert_eq!(state.candle_ttl("demo"), state.candles.ttl());
        assert_eq!(state.candle_ttl("yahoo"), state.candles.ttl());
        assert!(state.quotes.ttl() < state.candles.ttl());

        // With a real provider in the chain, demo candles are short-lived.
        let dir = std::env::temp_dir().join(format!("swing-state-test-{}", std::process::id()));
        let dir = dir.to_string_lossy().into_owned();
        let c = config_from(&[
            "--api-token",
            "test-token-0123456789",
            "--data-providers",
            "yahoo,nse,demo",
            "--nse-eod-dir",
            &dir,
        ]);
        let state = AppState::new(c.clone(), build_market(&c).unwrap());
        assert_eq!(state.market.ids(), vec!["yahoo", "nse", "demo"]);
        assert!(state.eod.is_some() && state.events.is_some());
        assert_eq!(state.candle_ttl("demo"), state.quotes.ttl());
        assert_eq!(state.candle_ttl("yahoo"), state.candles.ttl());
        assert_eq!(state.candle_ttl("nse"), state.candles.ttl());
        assert_eq!(state.scanner.provider(), "yahoo");
    }

    #[test]
    fn places_nse_by_its_position_in_the_list() {
        let dir = std::env::temp_dir().join(format!("swing-state-test2-{}", std::process::id()));
        let dir = dir.to_string_lossy().into_owned();
        let build = |providers: &str, extra: &[&str]| {
            let mut args = vec![
                "--api-token",
                "test-token-0123456789",
                "--data-providers",
                providers,
                "--nse-eod-dir",
                &dir,
                "--indmoney-token-file",
                "/nonexistent/swing-trade/token.json",
            ];
            args.extend_from_slice(extra);
            let c = config_from(&args);
            build_market(&c).map(|m| AppState::new(c, m))
        };
        // INDmoney has no token here, so it is skipped and `nse` moves up.
        if std::env::var(indmoney::oauth::ACCESS_TOKEN_ENV).is_err() {
            let state = build("yahoo,indmoney,nse,demo", &[]).unwrap();
            assert_eq!(state.market.ids(), vec!["yahoo", "nse", "demo"]);
        }
        assert_eq!(
            build("nse,demo", &[]).unwrap().market.ids(),
            vec!["nse", "demo"]
        );
        assert_eq!(build("nse", &[]).unwrap().market.ids(), vec!["nse"]);
        // Switched off, `nse` is left out; alone it leaves nothing to run on.
        let off = build("yahoo,nse,demo", &["--nse-eod", "false"]).unwrap();
        assert_eq!(off.market.ids(), vec!["yahoo", "demo"]);
        assert!(off.eod.is_none());
        assert!(build("nse", &["--nse-eod", "false"]).is_err());
    }

    #[test]
    fn rejects_unknown_providers() {
        let c = config_from(&[
            "--api-token",
            "test-token-0123456789",
            "--data-providers",
            "demo,bloomberg",
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
