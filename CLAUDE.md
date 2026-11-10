# CLAUDE.md — swing-trade

Read this first. It carries the context from the planning chat (10 Nov 2026) where this project was designed.

## What this is

A personal **swing-trading web app for Indian stocks** (NSE). It shortlists stocks in a swing setup, shows a live
chart with support/resistance, a trade-plan card (stop-loss / entry / target / quantity) and, later, news + an AI
summary. Owner: selvarajpownu (GitHub). Repo: https://github.com/selvarajpownu/swing-trade

Design doc (living): https://claude.ai/artifact/MPfWPrQPgf93TtCydTrLr8

## Hard rules (decided by the owner — do not change without asking)

- **View and analysis only. Never place orders.** (Keeps clear of SEBI's 1 Apr 2026 retail-algo rules.)
- **No login / registration.** The Rust API is protected by one shared secret: `Authorization: Bearer $API_TOKEN`.
  The web app calls the API **only server-side**; the token must never reach the browser.
- **Stack:** Next.js (App Router, TS, Tailwind v4, lightweight-charts v5) + **Rust** (Axum/Tokio).
  Python only if a specific library truly needs it (would live in `services/quant`, not created yet).
- **Hosting:** free setup first (Vercel Hobby for web; Oracle Always Free ARM VM or own laptop + Cloudflare Tunnel
  for the API). No database in phase 1 (in-memory TTL caches).
- **Data:** INDmoney MCP first (`https://mcp.indmoney.com/mcp`, OAuth 2.1 + PKCE, read-only), demo provider as
  fallback. Connectors are pluggable behind the `MarketData` trait; Kite/Upstox/Dhan come later.
- Trade plans are for the owner only; sharing them publicly may need SEBI research-analyst registration.
- The owner already holds Indraprastha Medical Corporation — never include it in "top" lists.

## Layout

```
apps/web/                 Next.js front end (npm). Server-only API client in src/lib/api/server.ts
services/api/             Rust workspace
  crates/swing-core/      pure analytics (std-only): indicators, zones, setups, trade plan  ← tested
  crates/market-data/     MarketData trait, DemoProvider, FallbackProvider, MCP client, INDmoney + OAuth
  crates/api-server/      Axum binary `swing-api` (serve | login indmoney)
docs/                     api.md (contract), architecture, configuration, deployment, roadmap, decisions/, research-notes.md
scripts/                  dev.sh, dev.ps1 (Windows)
```

## Run

```bash
cp .env.example .env            # set API_TOKEN (openssl rand -hex 32); DATA_PROVIDERS=demo works without accounts
cd services/api && cargo run -p api-server -- serve          # :8080
cd services/api && cargo run -p api-server -- login indmoney # one-time browser login for real data
cd apps/web && npm install && npm run dev                    # :3000
# or: docker compose up --build
```

Windows: `scripts\dev.ps1`. Tests: `cargo test` (services/api), `npm run typecheck && npm run lint` (apps/web).

## Status (end of planning chat)

Phase 1 (list + chart MVP) code is **written but never built**: the planning environment had no network, so no
crates/npm packages could be downloaded. Only `swing-core` was compiled and tested (8 tests pass, incl. the owner's
example card: SL 371.90 / entry 392 / target 440 → R:R 2.39, 248 shares at Rs 5,00,000 × 1%).

**First job in Claude Code: make it build.**
0. If a `setup-pending/` folder exists, follow its README (moves the Makefile and `.github/` files into place), then delete it.
1. `cd services/api && cargo build && cargo test && cargo clippy -- -D warnings`. Least-certain spots (fix as needed):
   tower-http 0.6 `TimeoutLayer::new` (deprecated), `DefaultMakeSpan/OnResponse .level()`, `CorsLayer::max_age`;
   clap `global = true` + `env` on flattened args; axum 0.8 `Result<Path<_>, PathRejection>` extractors;
   reqwest 0.12 `Response::chunk()`; chrono `and_utc()`; `try_join_all` inside `#[async_trait]` methods (Send).
2. `cd apps/web && npm install && npm run typecheck && npm run lint && npm run build`. Least-certain:
   `src/components/stock/candle-chart.tsx` (lightweight-charts v5 API: `seriesData.get`, `priceFormatter`,
   histogram `priceScale().applyOptions`), `src/components/ui/button.tsx` (`ButtonLink` props from `Link`),
   lucide-react icon names, Tailwind v4 `@keyframes` inside `@theme`.
3. Run end-to-end with `DATA_PROVIDERS=demo`, then `login indmoney` and real data.
4. Commit and push to `origin main`.

Known decisions/quirks:
- INDmoney `lookup_ind_keys` is fuzzy ("ACE" returns AceVector first). The provider fetches details for all
  candidates and keeps the exact `entity_basic.symbol` match, with a company-name fallback in `market-data/src/names.rs`.
- Web URLs use `risk` as a **percent** (`risk=1` = 1%); `server.ts` converts to the API's fraction (0.01).
- `INDMONEY_TOKEN_FILE` is relative: `cargo run` from `services/api` writes `services/api/.data/`; Docker uses `./.data/`.
- The demo provider's prices are synthetic; the UI shows a "DEMO DATA" pill when `provider == "demo"`.

## Next phases (docs/roadmap.md)

2 swing engine at scale (nightly Nifty 500 sync, Postgres+TimescaleDB, filters, Screener CSV quality import) ·
3 news + AI gateway (OpenAI-compatible + native Claude/Gemini, local Ollama/LM Studio, per-job routing, spend caps) ·
4 live ticks (Kite Connect WebSocket), setup states, notifications · 5 journal + back-tests of each preset.

## Conventions

Conventional commits. Rust: rustfmt, clippy `-D warnings`, no `unwrap()` outside tests. Web: strict TS, no client
import of server-only modules. Keep `docs/api.md` as the single source of truth for the API shape.
Wording in any research output: never "buy/sell/target price/multibagger" style advice; mark figures fetched vs CSV.
