# Roadmap

The app stays view and analysis only in every phase ([ADR 0003](decisions/0003-view-only-no-orders.md)).

## Phase 1 — List & chart MVP (current)

- Rust API with `/health`, `/v1/stocks`, `/v1/stocks/{symbol}` ([api.md](api.md)), bearer-token auth, in-memory caches.
- Market-data connectors: INDmoney MCP (OAuth login via `swing-api login indmoney`) and demo, with fallback.
- Web: watchlist table with sparklines; stock page with candles (1d / 1w / 15m), SMA 20/50/200, zones and the setup card.
- Docker Compose, CI, free hosting docs.

## Phase 2 — Swing engine

- Scanner over the presets: pullback, breakout, support bounce, oversold.
- 0-100 score (30 trend + 30 closeness + 15 volume + 15 risk-reward + 10 quality) with plain-language reasons.
- Trade plan card: entry just above the setup bar high, stop below nearest support (at least 1 ATR), target at next resistance or 2R, quantity = capital × risk ÷ (entry − stop).
- Capital and risk adjustable from the UI.

## Phase 3 — Data

- Nightly Nifty 500 sync (daily candles and quotes).
- Postgres + TimescaleDB for candle history and scan results (the `db` service is already sketched in `docker-compose.yml`).
- CSV import of fundamentals to fill the 10-point quality score.

## Phase 4 — News & AI

- News per stock.
- AI gateway for summaries and setup explanations:
  - OpenAI-compatible endpoints, plus native Claude and Gemini APIs.
  - Local models through Ollama / LM Studio.
  - Per-job model routing and monthly spend caps.

## Phase 5 — Live, journal, backtests

- Live ticks via Kite Connect (a new `MarketData` connector).
- Trade journal (manual entries; still no order placement).
- Backtests using `swing-core`. If an analytics library is only available in Python, `services/quant` is added at this point.

## Hosting path

Free first (Vercel Hobby + Oracle Always Free or laptop + Cloudflare Tunnel), then AWS Lightsail Mumbai when an always-on server with a database is worth paying for. See [deployment.md](deployment.md).
