# 0003. View only; never place orders

- **Status:** Accepted
- **Date:** 2026-11-10

## Context

The app helps one person find and plan swing trades. Placing orders through a broker API would bring regulatory duties, operational risk (bugs that trade real money) and extra security requirements.

- SEBI's framework for retail algorithmic trading, fully in force from **1 April 2026**, covers orders placed through broker APIs: it brings broker-issued API access, static IP whitelisting, order tagging and, above the order-rate threshold, algorithm registration through the broker.
- Sharing buy/sell recommendations or trade plans with other people can count as research analysis, which may require registration under the SEBI (Research Analysts) Regulations.

## Decision

- The app is strictly **view and analysis only**. No connector, endpoint or UI places, modifies or cancels orders. Market-data connectors only read data.
- Trade plans are shown to the owner for manual decisions. Orders, if any, are placed by the owner by hand in their broker app.
- The app is for personal use. Its outputs are not published or shared as recommendations.

## Consequences

- The API-order rules above do not apply, because the app sends no orders.
- Bugs cannot cause unintended trades.
- No order-related credentials are ever stored.
- Phase 5 (Kite Connect live ticks, journal) uses market-data and manual journal entries only. Adding order placement would need a new ADR and a review of the SEBI rules in force at that time.
- If the owner ever wants to share trade plans with others, check the research-analyst rules first.

This ADR is not legal advice.
