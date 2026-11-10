# Configuration

Everything is configured with environment variables. For local work, put them in `.env` at the repo root (copy `.env.example`). `.env` is git-ignored.

How each process gets them:

| How you run it | Where variables come from |
|---|---|
| `scripts/dev.sh`, `scripts/dev.ps1`, `make dev-*` | Root `.env`, exported to both processes |
| `cargo run` in `services/api` | Your shell environment, plus `.env` if the API loads it |
| `npm run dev` in `apps/web` | Your shell, or `apps/web/.env.local` (Next.js does not read the root `.env`) |
| `docker compose` | API: `env_file: .env`. Web: `API_URL=http://api:8080` and `API_TOKEN` from `.env` |
| Vercel / a VM | The platform's environment settings |

Use `KEY=value`, one per line, and double-quote values that contain spaces (`NEXT_PUBLIC_APP_NAME="Swing Trade"`). The scripts, `make`, Docker Compose and Next.js all read that format.

## API (`services/api`)

| Variable | Default | Description |
|---|---|---|
| `BIND_ADDR` | `0.0.0.0:8080` | Address and port to listen on. |
| `API_TOKEN` | — | Shared secret for `Authorization: Bearer ...` on `/v1/*`. Required unless `ALLOW_NO_TOKEN=true`. Generate with `openssl rand -hex 32`. |
| `ALLOW_NO_TOKEN` | `false` | Lets the API start without `API_TOKEN`. Local experiments only. |
| `DATA_PROVIDERS` | `indmoney,demo` | Comma-separated provider ids, tried in order. `demo` alone needs no account. |
| `INDMONEY_MCP_URL` | `https://mcp.indmoney.com/mcp` | INDmoney MCP endpoint. |
| `INDMONEY_TOKEN_FILE` | `.data/indmoney-token.json` | Where `swing-api login indmoney` saves the OAuth token and where `serve` reads it. Relative to the working directory. Docker Compose sets `/app/.data/indmoney-token.json` (host `./.data/`). |
| `INDMONEY_ACCESS_TOKEN` | *(empty)* | If set, used instead of the token file. |
| `CORS_ORIGINS` | `http://localhost:3000` | Comma-separated origins allowed for browser calls. The web app calls the API server-side, so this matters only for direct browser use. |
| `WATCHLIST` | `COFORGE,MCX,CAMS,IEX,ACE,AJAXENGG,ASKAUTOLTD,PERSISTENT,OFSS,BSE` | Default symbols for `GET /v1/stocks`. |
| `DEFAULT_CAPITAL` | `500000` | Capital in rupees for position sizing, when the request has no `capital`. |
| `DEFAULT_RISK` | `0.01` | Fraction of capital risked per trade, when the request has no `risk`. |
| `QUOTE_TTL_SECS` | `15` | Quote cache lifetime. |
| `CANDLE_TTL_SECS` | `600` | Candle cache lifetime for the first provider in `DATA_PROVIDERS`. Candles from a fallback provider use `QUOTE_TTL_SECS`. |
| `INDMONEY_RATE_PER_MIN` | `55` | Client-side INDmoney budget in cost units per minute (the server allows 60; OHLC costs 2). Shared by page requests and scans. |
| `UNIVERSE_FILE` | *(empty)* | Symbols for the background scanner: one per line, or a CSV with a `Symbol` column. NSE's index constituent CSVs (e.g. Nifty 500) work as downloaded and add company names and industries. |
| `UNIVERSE` | *(empty)* | Comma-separated scan symbols, used when `UNIVERSE_FILE` is not set. Empty means `WATCHLIST`. |
| `SCAN_EXCLUDE` | *(empty)* | Extra symbols never scanned. `INDRAMEDCO` (already held) is always excluded. |
| `SCAN_INTERVAL_MINS` | `30` | Minutes between scans. `0` scans once at startup. Fresh stored candles make repeat scans cheap. |
| `SCAN_SYMBOLS_PER_MIN` | `15` | Network candle fetches per minute during a scan (2 INDmoney units each), leaving budget for pages. 500 symbols take about 33 minutes once per trading day. |
| `CANDLE_STORE_DIR` | `.data/candles` | Daily candles saved by scans, re-fetched once per session (after 15:45 IST). Relative to the working directory. |
| `RUST_LOG` | `info` | `tracing` filter, e.g. `info,tower_http=debug`. |
| `LOG_FORMAT` | `pretty` | `pretty` (human-readable) or `json` (one object per line, for servers). The Docker image defaults to `json`. |

## Web (`apps/web`)

| Variable | Default | Description |
|---|---|---|
| `API_URL` | `http://localhost:8080` | Base URL of the API, as reached from the Next.js server. |
| `API_TOKEN` | — | Same value as the API's `API_TOKEN`. Server-side only; never prefix with `NEXT_PUBLIC_`. |
| `NEXT_PUBLIC_APP_NAME` | `Swing Trade` | Display name. Inlined at build time, so changing it needs a rebuild (Docker build arg). |

## Command-line

```text
swing-api                    # same as `swing-api serve`
swing-api serve              # start the HTTP server
swing-api login indmoney     # OAuth login, writes INDMONEY_TOKEN_FILE
```

From source: `cargo run -p api-server -- <subcommand>` in `services/api`.
