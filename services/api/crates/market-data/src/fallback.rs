//! Tries several providers in order.

use std::sync::Arc;

use async_trait::async_trait;
use swing_core::Candle;

use crate::{Error, Interval, MarketData, NewsItem, Quote, Result};

/// Wraps several providers and serves each request from the first one that
/// succeeds, in the configured order.
///
/// [`Error::NotFound`] is treated as a definitive answer and returned at
/// once: a later provider (typically demo) would otherwise invent data for a
/// symbol that a real source has just said does not exist.
pub struct FallbackProvider {
    providers: Vec<Arc<dyn MarketData>>,
}

impl FallbackProvider {
    /// Providers are tried in the order given.
    pub fn new(providers: Vec<Arc<dyn MarketData>>) -> Self {
        Self { providers }
    }

    /// Ids of the wrapped providers, in order.
    pub fn ids(&self) -> Vec<&'static str> {
        self.providers.iter().map(|p| p.id()).collect()
    }

    /// True when no provider is configured.
    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }

    /// The wrapped provider with this id, if any. Lets callers keep follow-up
    /// requests on the provider that answered the first one.
    pub fn get(&self, id: &str) -> Option<Arc<dyn MarketData>> {
        self.providers.iter().find(|p| p.id() == id).cloned()
    }

    /// Quotes plus the id of the provider that served them.
    pub async fn quotes_with_source(
        &self,
        symbols: &[String],
    ) -> Result<(Vec<Quote>, &'static str)> {
        let mut failures = Vec::new();
        for p in &self.providers {
            match p.quotes(symbols).await {
                Ok(quotes) => return Ok((quotes, p.id())),
                Err(e @ Error::NotFound(_)) => return Err(e),
                Err(e) => {
                    tracing::warn!(provider = p.id(), error = %e, "quotes failed; trying next provider");
                    failures.push((p.id(), e));
                }
            }
        }
        Err(combine(failures))
    }

    /// Candles plus the id of the provider that served them.
    pub async fn candles_with_source(
        &self,
        symbol: &str,
        interval: Interval,
    ) -> Result<(Vec<Candle>, &'static str)> {
        let mut failures = Vec::new();
        for p in &self.providers {
            match p.candles(symbol, interval).await {
                Ok(candles) => return Ok((candles, p.id())),
                Err(e @ Error::NotFound(_)) => return Err(e),
                Err(e) => {
                    tracing::warn!(provider = p.id(), symbol, error = %e, "candles failed; trying next provider");
                    failures.push((p.id(), e));
                }
            }
        }
        Err(combine(failures))
    }

    /// Headlines plus the id of the provider that served them. Demo has no
    /// news, so after an INDmoney failure the list is simply empty.
    pub async fn news_with_source(&self, symbol: &str) -> Result<(Vec<NewsItem>, &'static str)> {
        let mut failures = Vec::new();
        for p in &self.providers {
            match p.news(symbol).await {
                Ok(items) => return Ok((items, p.id())),
                Err(e @ Error::NotFound(_)) => return Err(e),
                Err(e) => {
                    tracing::warn!(provider = p.id(), symbol, error = %e, "news failed; trying next provider");
                    failures.push((p.id(), e));
                }
            }
        }
        Err(combine(failures))
    }
}

/// One failure is returned as is (keeps e.g. `Unauthorized` intact);
/// several are joined into [`Error::AllProvidersFailed`].
fn combine(mut failures: Vec<(&'static str, Error)>) -> Error {
    match failures.len() {
        0 => Error::AllProvidersFailed("no providers configured".to_owned()),
        1 => failures.remove(0).1,
        _ => Error::AllProvidersFailed(
            failures
                .iter()
                .map(|(id, e)| format!("{id}: {e}"))
                .collect::<Vec<_>>()
                .join("; "),
        ),
    }
}

#[async_trait]
impl MarketData for FallbackProvider {
    fn id(&self) -> &'static str {
        "fallback"
    }

    async fn quotes(&self, symbols: &[String]) -> Result<Vec<Quote>> {
        self.quotes_with_source(symbols).await.map(|(q, _)| q)
    }

    async fn candles(&self, symbol: &str, interval: Interval) -> Result<Vec<Candle>> {
        self.candles_with_source(symbol, interval)
            .await
            .map(|(c, _)| c)
    }

    async fn news(&self, symbol: &str) -> Result<Vec<NewsItem>> {
        self.news_with_source(symbol).await.map(|(n, _)| n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DemoProvider;

    struct Failing(&'static str, fn() -> Error);

    #[async_trait]
    impl MarketData for Failing {
        fn id(&self) -> &'static str {
            self.0
        }
        async fn quotes(&self, _: &[String]) -> Result<Vec<Quote>> {
            Err((self.1)())
        }
        async fn candles(&self, _: &str, _: Interval) -> Result<Vec<Candle>> {
            Err((self.1)())
        }
    }

    fn unauthorized() -> Error {
        Error::Unauthorized {
            provider: "indmoney",
        }
    }

    fn not_found() -> Error {
        Error::NotFound("NOPE".to_owned())
    }

    fn upstream() -> Error {
        Error::Upstream {
            provider: "other",
            message: "boom".to_owned(),
        }
    }

    #[tokio::test]
    async fn falls_through_to_the_next_provider() {
        let f = FallbackProvider::new(vec![
            Arc::new(Failing("indmoney", unauthorized)),
            Arc::new(DemoProvider::new()),
        ]);
        assert_eq!(f.ids(), vec!["indmoney", "demo"]);
        let (quotes, source) = f.quotes_with_source(&["MCX".to_owned()]).await.unwrap();
        assert_eq!(source, "demo");
        assert_eq!(quotes.len(), 1);
        let (_, source) = f.candles_with_source("MCX", Interval::Day).await.unwrap();
        assert_eq!(source, "demo");
        assert!(f.get("demo").is_some());
        assert!(f.get("nope").is_none());
    }

    #[tokio::test]
    async fn not_found_is_definitive() {
        let f = FallbackProvider::new(vec![
            Arc::new(Failing("indmoney", not_found)),
            Arc::new(DemoProvider::new()),
        ]);
        let err = f
            .candles_with_source("NOPE", Interval::Day)
            .await
            .unwrap_err();
        assert!(matches!(err, Error::NotFound(_)));
    }

    #[tokio::test]
    async fn reports_every_failure() {
        let single = FallbackProvider::new(vec![Arc::new(Failing("indmoney", unauthorized))]);
        assert!(matches!(
            single.quotes(&[]).await.unwrap_err(),
            Error::Unauthorized { .. }
        ));

        let both = FallbackProvider::new(vec![
            Arc::new(Failing("indmoney", unauthorized)),
            Arc::new(Failing("other", upstream)),
        ]);
        match both.quotes(&[]).await.unwrap_err() {
            Error::AllProvidersFailed(msg) => {
                assert!(msg.contains("indmoney:"), "{msg}");
                assert!(msg.contains("other: upstream other error: boom"), "{msg}");
            }
            e => panic!("unexpected {e:?}"),
        }

        let none = FallbackProvider::new(Vec::new());
        assert!(none.is_empty());
        assert!(matches!(
            none.quotes(&[]).await.unwrap_err(),
            Error::AllProvidersFailed(_)
        ));
    }
}
