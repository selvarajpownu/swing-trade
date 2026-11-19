# 0005. Login-free data first; INDmoney as a fallback

- **Status:** Accepted
- **Date:** 2026-11-19
- **Supersedes:** the provider order in [ADR 0004](0004-pluggable-market-data.md) (the connector design there stands)

## Context

INDmoney's MCP login is a browser sign-in (mobile number, OTP, MPIN). Its saved login stopped working after the app had
been off for about a week ("refresh token does not exist"), and the owner does not want to sign in again and again. Until
then INDmoney was the first provider for every quote, chart and news item, so an expired login turned every stock page
into demo data.

Free sources that need no login were tested from the owner's PC on 19 Nov 2026:

- **NSE's public daily files** (full bhavcopy, index closes, corporate actions): already used for scans and the swing
  screen. Exact end-of-day data; no live prices or intraday bars.
- **Yahoo Finance's chart endpoint** (`/v8/finance/chart`): quotes, 15-minute and daily bars for NSE (`.NS`) and US
  symbols, no key. Unofficial: no guarantee, may throttle, change or lag the exchange. No MCX commodities.
- **NSE's website quote API**: refused automated requests (HTTP 403).
- **Broker APIs** (Upstox, Angel One, Fyers, Dhan): free, but need a fresh login every day.

## Decision

- Default order: `DATA_PROVIDERS=yahoo,indmoney,nse,demo`.
  - `yahoo` answers quotes and charts for NSE and US stocks.
  - `indmoney` (when logged in) is the fallback for those, and the only real source for MCX commodities and for news
    with sentiment.
  - `nse` serves daily and weekly charts and end-of-day quotes for the scan universe from the cached files, so a
    stock page still shows real data when both live sources fail.
  - `demo` stays last, labelled by the DEMO DATA pill.
- Scans and the swing screen keep reading NSE's files only ([architecture](../architecture.md)).
- Stock-page news uses INDmoney when it has headlines, otherwise Google News RSS.
- A provider that does not cover a market, symbol or candle size returns `Error::Unsupported`, which moves on to the
  next provider without a warning. Yahoo's "symbol not found" is final, so demo never invents data for a bad symbol.

## Consequences

- With no INDmoney login the app still shows real NSE and US prices and charts. Only commodities fall to demo.
- Yahoo is unofficial. If it breaks, INDmoney (if logged in) and then NSE's files take over; the page shows which
  source answered next to the quote time.
- Yahoo's NSE prices may be delayed. The delay was not measured when this was written (market closed); the quote
  time on the page is Yahoo's own.
- Requests to Yahoo carry only ticker symbols. They are cached for 30 seconds and rate limited on our side.
- Dropping `demo` from `DATA_PROVIDERS` makes a total failure an error instead of synthetic data.
