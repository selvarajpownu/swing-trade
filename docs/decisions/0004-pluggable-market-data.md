# 0004. Pluggable market-data connectors

- **Status:** Accepted
- **Date:** 2026-11-10

## Context

Indian market data for individuals comes from several sources with different access methods, limits and costs: INDmoney (MCP server with OAuth), Kite Connect (needed for live ticks later), CSV exports, and so on. Any one source can be down, rate-limited or logged out. The app must also work with no account at all, for development, CI and demos.

## Decision

- All sources implement one trait in the `market-data` crate:
  ```rust
  #[async_trait]
  pub trait MarketData: Send + Sync {
      fn id(&self) -> &'static str;
      async fn quotes(&self, symbols: &[String]) -> Result<Vec<Quote>>;
      async fn candles(&self, symbol: &str, interval: Interval) -> Result<Vec<Candle>>;
  }
  ```
- The API only depends on `dyn MarketData`. At startup it builds a `FallbackProvider` from `DATA_PROVIDERS` (e.g. `indmoney,demo`), which tries each source in order.
- Phase 1 connectors: `indmoney` (INDmoney MCP; token from `swing-api login indmoney`, `INDMONEY_TOKEN_FILE` or `INDMONEY_ACCESS_TOKEN`) and `demo` (generated data).
- API responses include a `provider` field so the UI can show which source answered.
- Analytics (`swing-core`) take plain `Candle` slices and know nothing about providers.

## Consequences

- New sources (Kite Connect in phase 5, CSV/DB-backed history in phase 3) are added without touching the API handlers or the swing engine.
- `DATA_PROVIDERS=demo` gives a fully working app with no credentials, which CI and new setups rely on.
- Fallback can silently switch to demo data when INDmoney fails. The `provider` field must be shown in the UI so demo prices are never mistaken for real ones.
- Providers differ in symbol naming and candle coverage; each connector is responsible for mapping to NSE symbols and returning candles oldest first.
