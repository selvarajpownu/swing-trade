# Roadmap

The app stays view and analysis only in every phase ([ADR 0003](decisions/0003-view-only-no-orders.md)).

## Phase 1 — List & chart MVP (done, 10 Nov 2026)

- Rust API with `/health`, `/v1/stocks`, `/v1/stocks/{symbol}` ([api.md](api.md)), bearer-token auth, in-memory caches.
- Market-data connectors: INDmoney MCP (OAuth login via `swing-api login indmoney`) and demo, with fallback.
- Web: watchlist table with sparklines; stock page with candles (1d / 1w / 15m), SMA 20/50/200, zones and the setup card.
- Docker Compose, CI, free hosting docs.

## Phase 2 — Swing engine (in progress)

Already in phase 1: the four presets, the 0-100 score with reasons, the trade-plan card, capital and risk from the UI.

Done:
- Background scanner over a universe (`UNIVERSE_FILE` / `UNIVERSE` / `WATCHLIST`; NSE constituent CSVs such as the
  Nifty 500 work as downloaded), `GET /v1/scan` with preset / score / R:R / state filters, Scanner page, Home "Top setups".
- Daily-candle file store (`.data/candles`, refreshed once per session) and a shared INDmoney rate limiter that learns
  tool costs from the server's rate-limit replies.
- Golden preset tests on recorded real candles (`swing-core/tests/presets_real.rs`).

- Navigation: **IN Stock**, **US Stock**, **Commodity** (MCX near-month futures), **Stock news**, with Google
  Finance-style market tabs and compact news rows. US and MCX have the same list, chart, setups and plan (USD capital
  for US; no quantity for MCX futures).
- News: INDmoney headlines with sentiment on each stock page; Google News RSS headlines with links on Stock news.

Next:
- Done 11 Nov: scanner universe = NSE Nifty 500 list (`UNIVERSE_FILE`) plus the watchlist (503 stocks); a full
  refresh takes ~3.3 h at INDmoney's hourly limit, once per trading day, with hits shown as they are found.
- Google Finance-style range tabs (1D / 5D / 1M / 6M / YTD / 1Y) on the chart.
- Scanner for US and MCX (today it scans NSE only).

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
