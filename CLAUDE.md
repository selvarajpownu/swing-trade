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

## Status (10 Nov 2026, first Claude Code session)

**Phase 2 progress (same day):** background scanner (`GET /v1/scan`, `api-server/src/{scan,store,universe}.rs`), Scanner
page, Home "Top setups" (never "recommended": wording rule), shared INDmoney rate limiter that learns tool costs
(`market-data/src/rate.rs`), golden preset tests on real candles. Next: IN Stock / US Stock / Commodity / Stock news
sections (see docs/roadmap.md Phase 2 "Next").

Phase 1 (list + chart MVP) **builds, passes all checks and runs end to end on real INDmoney data** (Windows 11,
Rust 1.90, Node 24).
- Rust: `cargo build`, `cargo test` (76 tests: 34 api-server, 34 market-data, 8 swing-core incl. the owner's card
  SL 371.90 / entry 392 / target 440 → R:R 2.39, 248 shares), `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`.
- Web: `npm run typecheck`, `lint`, `build`, `format` all clean.
- Verified by hand: `/health` open; `/v1/*` 401 without/with a wrong token; list page (sparklines, en-IN numbers,
  setup badges, filters, sorting); stock page (candles 1D/1W/15m, SMA 20/50/200, zones, SL/entry/target lines,
  trade-plan card, key levels, sizing via `?capital=&risk=`); DEMO DATA pill on demo, absent on INDmoney; token
  absent from HTML and static JS.
- INDmoney: `login indmoney` works (dynamic client registration, PKCE, loopback :8765, refresh token). All 10
  watchlist symbols resolve; ACE → Action Construction Equipment (INDS00830), not AceVector.
- Git: work is on `main` at github.com/selvarajpownu/swing-trade. The repo was created with a `master` branch
  ("Initial commit"); `main` sits on top of it. Switch the GitHub default branch to `main`, then delete `master`.

Fixed in this session: two api-server tests (clap repeated flag, clippy excessive_precision), INDmoney rate-limit
handling (see quirks), fallback candles no longer cached for 10 min, login no longer requests write scopes, and
the trade-plan card's 2R wording.

Open items:
- `npm audit`: 1 high (postcss bundled inside `next`) whose only fix is Next 16 (breaking; stack says Next 15),
  plus moderate minimatch issues in lint tooling. Revisit when Next 15.5.x ships a patched postcss.
- `/stocks/<bad symbol>` renders "Symbol not found" but with HTTP 200 (Next streams `loading.tsx` first).
- Falling back to demo for a real symbol mixes synthetic prices into the UI (only the pill warns). Consider
  returning 503 "INDmoney busy, retry in Ns" instead of demo when INDmoney is configured.

Known decisions/quirks:
- INDmoney `lookup_ind_keys` is fuzzy ("ACE" returns AceVector first). The provider fetches details for all
  candidates and keeps the exact `entity_basic.symbol` match, with a company-name fallback in `market-data/src/names.rs`.
- INDmoney MCP rate limit: **60 cost units/min, global** (OHLC = 2). Throttled calls return a JSON error object
  inside a successful tool result; `IndmoneyProvider::call` waits ≤ 5 s and retries, else `Upstream` → fallback.
  Bursts (e.g. scripted checks of 30 charts) will hit it; normal browsing doesn't.
- INDmoney access tokens last ~1 h; the stored refresh token renews them automatically.
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
